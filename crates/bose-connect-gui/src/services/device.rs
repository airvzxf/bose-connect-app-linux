//! Abstract device service.
//!
//! Two implementations:
//!
//!   - [`RealService`] — RFCOMM against a real `BoseDevice`.
//!   - [`MockService`]  — in-process [`crate::transport`] state.
//!
//! The trait is **sync** so the GUI can dispatch every command
//! through a single `Component::worker`. The real implementation
//! internally hops onto a dedicated thread for the actual socket
//! I/O, so it never blocks the GTK main loop.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;

use bose_connect::types::Device as DeviceInfo;
use bose_connect::{
    AutoOff as AutoOffWire, BdAddr, BoseDevice, BoseError, BoseResult, DeviceStatus,
    DeviceStatusReport, NoiseCancelling, PairedDevice, PairedDevices, PartyMode, PromptLanguage,
    SelfVoice,
};

use crate::services::bluetooth::PairedBose;
use crate::transport::{self, MockHandle};

/// Per-device capability table derived from the device id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    pub noise_cancelling: bool,
    pub self_voice: bool,
    pub pairing_toggle: bool,
    /// Party and stereo mode with a second speaker (SoundLink Color II).
    pub party_mode: bool,
}

/// Snapshot of the device's identity and current state.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceSnapshot {
    pub address: BdAddr,
    pub name: String,
    pub firmware: String,
    pub serial: String,
    pub device_id: u16,
    pub battery: u8,
    pub status: DeviceStatusReport,
    pub paired: PairedDevices,
    pub devices: Vec<DeviceInfo>,
    /// The product id of each paired Bose device in `devices`;
    /// phones and computers have none.
    pub products: HashMap<BdAddr, u16>,
    pub capabilities: Capability,
    pub volume: Option<u8>,
    pub active_device: Option<BdAddr>,
    pub device_bd_addr: Option<BdAddr>,
    /// How BlueZ sees the audio link to this computer; `None` when
    /// unknown (simulated device, or BlueZ not reachable).
    pub audio: Option<bose_connect::AudioState>,
    /// Party mode; `None` on devices without it, or when the device
    /// did not answer.
    pub party_mode: Option<PartyMode>,
    /// Sample rate of the SBC audio link from this computer, when
    /// there is one.
    pub audio_rate: Option<u32>,
}

/// Concrete boxed service.
pub type DynDeviceService = Arc<dyn DeviceService>;

pub trait DeviceService: Send + Sync + 'static {
    /// Open / wake the device.
    fn connect(&self, address: &str) -> Result<DeviceSnapshot>;

    /// Re-derive every get_* and return the latest snapshot.
    fn refresh(&self) -> Result<DeviceSnapshot>;

    /// Close the connection, so the device is free for another
    /// client (a phone app, the CLI).
    fn disconnect(&self);

    /// The Bose devices paired with this computer.
    fn known_devices(&self) -> Result<Vec<PairedBose>>;

    fn set_name(&self, name: &str) -> Result<DeviceSnapshot>;
    fn set_noise_cancelling(&self, level: NoiseCancelling) -> Result<DeviceSnapshot>;
    fn set_self_voice(&self, level: SelfVoice) -> Result<DeviceSnapshot>;
    fn set_auto_off(&self, minutes: AutoOffWire) -> Result<DeviceSnapshot>;
    fn set_language(&self, lang: PromptLanguage, voice: bool) -> Result<DeviceSnapshot>;
    /// Toggle voice prompts, keeping the current language.
    fn set_voice_prompts(&self, on: bool) -> Result<DeviceSnapshot>;
    fn set_pairing(&self, on: bool) -> Result<DeviceSnapshot>;
    fn connect_device(&self, addr: BdAddr) -> Result<DeviceSnapshot>;
    fn disconnect_device(&self, addr: BdAddr) -> Result<DeviceSnapshot>;
    fn remove_device(&self, addr: BdAddr) -> Result<DeviceSnapshot>;
    fn set_volume(&self, level: u8) -> Result<DeviceSnapshot>;
    fn send_media_key(&self, key: bose_connect::MediaKey) -> Result<()>;

    /// Join the speaker `peer` to this one (party mode). `source` is
    /// the music source; `None` means this computer.
    fn join_party(&self, peer: BdAddr, source: Option<BdAddr>) -> Result<DeviceSnapshot>;
    /// Separate the speaker `peer` from this one.
    fn leave_party(&self, peer: BdAddr) -> Result<DeviceSnapshot>;
    /// Switch two joined speakers between party and stereo mode.
    fn set_party_mode(&self, mode: PartyMode) -> Result<DeviceSnapshot>;
    /// Restart this computer's audio stream to the device, which a
    /// SoundLink Color II needs after every party-mode change. `false`
    /// when this computer is not sending it audio.
    fn restart_audio_stream(&self) -> Result<bool>;

    fn capabilities(&self, device_id: u16) -> Capability {
        capability_for(device_id)
    }
}

// ---------------------------------------------------------------------------
// Mock
// ---------------------------------------------------------------------------

/// What the simulated computer has paired: lets tests and
/// screenshots reach every state of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockScenario {
    /// The simulated headset is paired and connected; a simulated
    /// speaker is paired but off.
    Connected,
    /// Both are paired, neither is connected.
    Disconnected,
    /// No Bose device is paired.
    NoDevices,
    /// The simulated device is a SoundLink Color II, and a second
    /// one is paired and connected (party mode).
    Speaker,
}

impl MockScenario {
    /// Parse the value of `BOSE_CONNECT_GUI_SIMULATOR`.
    pub fn from_env_value(value: &str) -> Option<Self> {
        match value {
            "1" | "connected" => Some(Self::Connected),
            "disconnected" => Some(Self::Disconnected),
            "no-devices" => Some(Self::NoDevices),
            "speaker" => Some(Self::Speaker),
            _ => None,
        }
    }
}

pub struct MockService {
    handle: MockHandle,
    scenario: MockScenario,
}

impl MockService {
    pub fn new(handle: MockHandle) -> Self {
        Self::with_scenario(handle, MockScenario::Connected)
    }

    pub fn with_scenario(handle: MockHandle, scenario: MockScenario) -> Self {
        Self { handle, scenario }
    }

    fn snapshot(&self) -> DeviceSnapshot {
        Self::snapshot_from(&self.handle.read())
    }

    /// Build a snapshot from a state the caller already holds; a
    /// setter holding the write guard must not call
    /// [`Self::snapshot`], which would deadlock on the read lock.
    fn snapshot_from(state: &transport::MockState) -> DeviceSnapshot {
        let status = transport::snapshot_report(state);
        let paired = transport::paired_devices_snapshot(state);
        let devices: Vec<DeviceInfo> = state
            .paired
            .iter()
            .map(|(addr, name, st)| transport::make_device_info(*addr, name, *st))
            .collect();
        let products = state
            .paired
            .iter()
            .filter_map(|(addr, _, _)| {
                (BdAddr::from_canonical(transport::demo::SPEAKER) == Some(*addr))
                    .then_some((*addr, 0x400d))
            })
            .collect();
        let capabilities = capability_for(state.device_id);
        DeviceSnapshot {
            address: state.address,
            name: state.name.clone(),
            firmware: state.firmware.clone(),
            serial: state.serial.clone(),
            device_id: state.device_id,
            battery: state.battery,
            status,
            paired,
            devices,
            products,
            capabilities,
            volume: Some(state.volume),
            active_device: state.active_device,
            device_bd_addr: state.device_bd_addr,
            audio: None,
            party_mode: bose_connect::has_party_mode(state.device_id).then_some(state.party_mode),
            audio_rate: None,
        }
    }
}

impl DeviceService for MockService {
    fn connect(&self, address: &str) -> Result<DeviceSnapshot> {
        // Only the simulated headset answers; the simulated speaker
        // is "switched off", like a real device out of range.
        if !address.eq_ignore_ascii_case(transport::demo::DEVICE)
            || self.scenario == MockScenario::NoDevices
        {
            bail!("connect {address}: Host is down (simulated)");
        }
        let mut state = self.handle.write();
        state.connected = true;
        Ok(Self::snapshot_from(&state))
    }

    fn disconnect(&self) {
        self.handle.write().connected = false;
    }

    fn known_devices(&self) -> Result<Vec<PairedBose>> {
        let connected = matches!(
            self.scenario,
            MockScenario::Connected | MockScenario::Speaker
        );
        let peer_on = self.scenario == MockScenario::Speaker;
        Ok(match self.scenario {
            MockScenario::NoDevices => Vec::new(),
            _ => vec![
                PairedBose {
                    address: transport::demo::DEVICE.to_owned(),
                    name: self.handle.read().name.clone(),
                    connected,
                    product_id: self.handle.read().device_id,
                },
                PairedBose {
                    address: transport::demo::SPEAKER.to_owned(),
                    name: "Bose SoundLink Color II".to_owned(),
                    connected: peer_on,
                    product_id: 0x400d,
                },
            ],
        })
    }

    fn refresh(&self) -> Result<DeviceSnapshot> {
        Ok(self.snapshot())
    }

    fn set_name(&self, name: &str) -> Result<DeviceSnapshot> {
        transport::validate_name(name).map_err(|e| anyhow!(e))?;
        let mut state = self.handle.write();
        state.name = name.to_string();
        Ok(Self::snapshot_from(&state))
    }

    fn set_noise_cancelling(&self, level: NoiseCancelling) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if bose_connect::has_noise_cancelling(state.device_id) {
            state.noise_cancelling = level;
            Ok(Self::snapshot_from(&state))
        } else {
            bail!("this device does not have noise cancelling");
        }
    }

    fn set_self_voice(&self, level: SelfVoice) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if bose_connect::has_self_voice(state.device_id) {
            state.self_voice = level;
            Ok(Self::snapshot_from(&state))
        } else {
            bail!("this device does not support self-voice");
        }
    }

    fn set_auto_off(&self, minutes: AutoOffWire) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.auto_off = minutes;
        Ok(Self::snapshot_from(&state))
    }

    fn set_language(&self, lang: PromptLanguage, voice: bool) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.language = lang;
        state.voice_prompts_on = voice;
        Ok(Self::snapshot_from(&state))
    }

    fn set_voice_prompts(&self, on: bool) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.voice_prompts_on = on;
        Ok(Self::snapshot_from(&state))
    }

    fn set_pairing(&self, on: bool) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if bose_connect::has_pairing_toggle(state.device_id) {
            state.pairing_on = on;
            Ok(Self::snapshot_from(&state))
        } else {
            bail!("this device does not support pairing-toggle");
        }
    }

    fn connect_device(&self, addr: BdAddr) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if let Some(slot) = state.paired.iter_mut().find(|(a, _, _)| *a == addr) {
            slot.2 = DeviceStatus::Connected;
        }
        Ok(Self::snapshot_from(&state))
    }

    fn disconnect_device(&self, addr: BdAddr) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if let Some(slot) = state.paired.iter_mut().find(|(a, _, _)| *a == addr) {
            slot.2 = DeviceStatus::Disconnected;
        }
        Ok(Self::snapshot_from(&state))
    }

    fn remove_device(&self, addr: BdAddr) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.paired.retain(|(a, _, _)| *a != addr);
        Ok(Self::snapshot_from(&state))
    }

    fn set_volume(&self, level: u8) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.volume = level.min(75);
        Ok(Self::snapshot_from(&state))
    }

    fn send_media_key(&self, _key: bose_connect::MediaKey) -> Result<()> {
        Ok(())
    }

    fn join_party(&self, peer: BdAddr, _source: Option<BdAddr>) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if !bose_connect::has_party_mode(state.device_id) {
            bail!("this device does not support party mode");
        }
        if peer == state.address {
            bail!("a speaker cannot join itself");
        }
        state.party_mode = PartyMode::Party;
        Ok(Self::snapshot_from(&state))
    }

    fn leave_party(&self, _peer: BdAddr) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.party_mode = PartyMode::Off;
        Ok(Self::snapshot_from(&state))
    }

    fn set_party_mode(&self, mode: PartyMode) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if state.party_mode == PartyMode::Off || mode == PartyMode::Off {
            bail!("join a speaker first");
        }
        state.party_mode = mode;
        Ok(Self::snapshot_from(&state))
    }

    fn restart_audio_stream(&self) -> Result<bool> {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Real RFCOMM
// ---------------------------------------------------------------------------

/// Retries for one `get_device_info` while building a snapshot. A
/// SoundLink Color II sometimes misses the 1 s receive timeout for
/// the source it is playing from; the CLI uses the same budget.
const DEVICE_INFO_RETRIES: usize = 3;
const DEVICE_INFO_BACKOFF: Duration = Duration::from_millis(500);

/// One persistent RFCOMM connection to a real device.
///
/// The connection is opened by [`DeviceService::connect`] and kept
/// for every later call, so the GUI does not open a connection per
/// command (a SoundLink Color II switches itself off after its 9th
/// RFCOMM connection since power-on; see `docs/protocol.md`). A
/// transport error drops it; the next call opens a new one.
///
/// Every method blocks on socket I/O, so callers must run them off
/// the GTK main thread.
pub struct RealService {
    inner: Mutex<RealInner>,
}

#[derive(Default)]
struct RealInner {
    address: Option<String>,
    device: Option<BoseDevice>,
    /// Requests in a row that got no reply in time.
    timeouts: u32,
}

impl Default for RealService {
    fn default() -> Self {
        Self::new()
    }
}

impl RealService {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RealInner::default()),
        }
    }

    /// Run `f` on the open connection, opening it first if needed.
    /// A transport error closes the connection so the next call
    /// reconnects; the command itself is not retried, because a
    /// write that reached the device must not be sent twice.
    fn with_device<T>(&self, f: impl FnOnce(&mut BoseDevice) -> BoseResult<T>) -> Result<T> {
        let mut inner = self.inner.lock();
        if inner.device.is_none() {
            let address = inner
                .address
                .clone()
                .ok_or_else(|| anyhow!("no device address; connect first"))?;
            inner.device = Some(BoseDevice::open(&address)?);
        }
        let device = inner.device.as_mut().expect("opened above");
        let result = f(device);
        match &result {
            Ok(_) => inner.timeouts = 0,
            Err(e) if is_transport_error(e) => {
                tracing::warn!(target: "device", "closing the connection after: {e}");
                inner.device = None;
                inner.timeouts = 0;
            }
            Err(e) if is_timeout_error(e) => {
                inner.timeouts += 1;
                if inner.timeouts >= TIMEOUTS_BEFORE_CLOSING {
                    tracing::warn!(target: "device", "closing the connection after {} late replies: {e}", inner.timeouts);
                    inner.device = None;
                    inner.timeouts = 0;
                } else {
                    tracing::warn!(target: "device", "no reply in time ({}/{TIMEOUTS_BEFORE_CLOSING}), keeping the connection: {e}", inner.timeouts);
                }
            }
            Err(_) => {}
        }
        Ok(result?)
    }

    /// Every getter a snapshot needs, over one connection.
    fn read_snapshot(&self) -> Result<DeviceSnapshot> {
        let address = self.inner.lock().address.clone().unwrap_or_default();
        let audio = crate::services::bluetooth::audio_state(&address);
        let audio_rate = crate::services::bluetooth::audio_sample_rate(&address);
        let address = BdAddr::from_canonical(&address).unwrap_or(BdAddr::ANY);
        self.with_device(|dev| {
            let (device_id, _index) = dev.device_id()?;
            let status = dev.device_status()?;
            let firmware = dev.firmware_version()?;
            let serial = dev.serial_number()?;
            let battery = dev.battery_level()?;
            let paired = dev.paired_devices()?;
            let mut devices = Vec::with_capacity(paired.num_devices);
            let mut products = HashMap::new();
            for &addr in &paired.addresses[..paired.num_devices] {
                let info = device_info_with_retry(dev, addr)?;
                if let Some(product_id) = info.product_id {
                    products.insert(addr, product_id);
                }
                devices.push(info.device);
            }
            // Not every product answers these two (an ERROR packet,
            // or no source connected); the snapshot shows them as
            // unknown instead of failing.
            let active_device = optional(dev.active_device())?;
            let device_bd_addr = optional(dev.device_bd_addr())?;
            let party_mode = if bose_connect::has_party_mode(device_id) {
                optional(dev.party_mode())?
            } else {
                None
            };
            Ok(DeviceSnapshot {
                address,
                name: status.name.clone(),
                firmware,
                serial,
                device_id,
                battery,
                status,
                paired,
                devices,
                products,
                capabilities: capability_for(device_id),
                // The protocol has no volume getter.
                volume: None,
                active_device,
                device_bd_addr,
                audio,
                party_mode,
                audio_rate,
            })
        })
    }

    /// Run a setter, then return a fresh snapshot.
    fn set_and_read(
        &self,
        f: impl FnOnce(&mut BoseDevice) -> BoseResult<()>,
    ) -> Result<DeviceSnapshot> {
        self.with_device(f)?;
        self.read_snapshot()
    }
}

/// Errors after which the socket can no longer be trusted.
/// Requests in a row without a reply in time before the connection is
/// taken for gone and closed (see [`is_transport_error`]).
const TIMEOUTS_BEFORE_CLOSING: u32 = 3;

/// True when the connection is gone. A reply that did not arrive in
/// time is not, at first: the device was busy (a SoundLink Color II answered
/// late while another device connected to it), and the next request
/// discards whatever arrives late. Closing the connection then would
/// be worse than the late reply: in party mode with a phone as the
/// source, a SoundLink Color II accepts no new connection.
fn is_transport_error(e: &BoseError) -> bool {
    match e {
        BoseError::Io(io) => !is_timeout(io),
        BoseError::Closed | BoseError::ShortRead { .. } | BoseError::ShortWrite { .. } => true,
        _ => false,
    }
}

/// A reply that did not arrive within the receive timeout.
fn is_timeout_error(e: &BoseError) -> bool {
    matches!(e, BoseError::Io(io) if is_timeout(io))
}

fn is_timeout(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// Map a "this device does not answer that" reply to `None`; keep
/// transport errors, which mean the connection is gone.
fn optional<T>(result: BoseResult<T>) -> BoseResult<Option<T>> {
    match result {
        Ok(v) => Ok(Some(v)),
        Err(e) if is_transport_error(&e) || is_timeout_error(&e) => Err(e),
        Err(e) => {
            tracing::debug!(target: "device", "optional getter failed: {e}");
            Ok(None)
        }
    }
}

fn device_info_with_retry(dev: &mut BoseDevice, addr: BdAddr) -> BoseResult<PairedDevice> {
    let mut attempt = 1;
    loop {
        match dev.paired_device(addr) {
            Ok(info) => return Ok(info),
            Err(e) if attempt < DEVICE_INFO_RETRIES && !is_transport_error(&e) => {
                tracing::debug!(target: "device", "device_info attempt {attempt} failed: {e}");
                std::thread::sleep(DEVICE_INFO_BACKOFF);
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

fn capability_for(device_id: u16) -> Capability {
    Capability {
        noise_cancelling: bose_connect::has_noise_cancelling(device_id),
        self_voice: bose_connect::has_self_voice(device_id),
        pairing_toggle: bose_connect::has_pairing_toggle(device_id),
        party_mode: bose_connect::has_party_mode(device_id),
    }
}

impl DeviceService for RealService {
    fn connect(&self, address: &str) -> Result<DeviceSnapshot> {
        if BdAddr::from_canonical(address).is_none() {
            bail!("invalid Bluetooth address: {address}");
        }
        {
            let mut inner = self.inner.lock();
            if inner.address.as_deref() != Some(address) {
                inner.device = None;
                inner.address = Some(address.to_string());
            }
        }
        self.read_snapshot()
    }

    fn refresh(&self) -> Result<DeviceSnapshot> {
        self.read_snapshot()
    }

    fn disconnect(&self) {
        let mut inner = self.inner.lock();
        // Dropping the device closes the socket.
        inner.device = None;
        inner.address = None;
    }

    fn known_devices(&self) -> Result<Vec<PairedBose>> {
        crate::services::bluetooth::paired_bose_devices()
            .ok_or_else(|| anyhow!("BlueZ is not reachable over the system bus"))
    }

    fn set_name(&self, name: &str) -> Result<DeviceSnapshot> {
        transport::validate_name(name).map_err(|e| anyhow!(e))?;
        self.set_and_read(|dev| dev.set_name(name))
    }

    fn set_noise_cancelling(&self, level: NoiseCancelling) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| dev.set_noise_cancelling(level))
    }

    fn set_self_voice(&self, level: SelfVoice) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| dev.set_self_voice(level))
    }

    fn set_auto_off(&self, minutes: AutoOffWire) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| dev.set_auto_off(minutes))
    }

    fn set_language(&self, lang: PromptLanguage, _voice: bool) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| {
            let (device_id, _) = dev.device_id()?;
            if !bose_connect::has_legacy_settings(device_id) {
                // Same guard as the CLI: the QC Ultra's 7-byte
                // language payload has not been written from here.
                return Err(BoseError::InvalidArgument(
                    "the prompt language cannot be changed on this device yet".into(),
                ));
            }
            // Keeps the voice-prompts bit and the device's other bits.
            dev.set_language_keep_voice_prompts(lang)
        })
    }

    fn set_voice_prompts(&self, on: bool) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| dev.set_voice_prompts(on))
    }

    fn set_pairing(&self, on: bool) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| dev.set_pairing(on))
    }

    fn connect_device(&self, addr: BdAddr) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| dev.connect_device(addr))
    }

    fn disconnect_device(&self, addr: BdAddr) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| dev.disconnect_device(addr))
    }

    fn remove_device(&self, addr: BdAddr) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| dev.remove_device(addr))
    }

    fn set_volume(&self, level: u8) -> Result<DeviceSnapshot> {
        let _top = self.with_device(|dev| dev.set_volume(level))?;
        let mut snap = self.read_snapshot()?;
        snap.volume = Some(level);
        Ok(snap)
    }

    fn send_media_key(&self, key: bose_connect::MediaKey) -> Result<()> {
        self.with_device(|dev| {
            if key == bose_connect::MediaKey::Pause {
                let (device_id, _) = dev.device_id()?;
                if bose_connect::has_audio_modes(device_id) {
                    // Same guard as the CLI: on a QC Ultra, pause never
                    // completes and locks media control, the phone's
                    // included, until the phone reconnects.
                    return Err(BoseError::InvalidArgument(
                        "pause is refused on this device (QC Ultra media-control lock)".into(),
                    ));
                }
            }
            dev.send_media_key(key)
        })
    }

    fn join_party(&self, peer: BdAddr, source: Option<BdAddr>) -> Result<DeviceSnapshot> {
        let source = match source {
            Some(source) => {
                // With another source, the speaker dropped this
                // computer's whole link (control connection included)
                // at the join while this computer had an audio link
                // too; without the audio profiles the link survives.
                let address = self.inner.lock().address.clone().unwrap_or_default();
                match crate::services::bluetooth::disconnect_audio_profiles(&address) {
                    Ok(0) => {}
                    Ok(n) => {
                        tracing::info!(target: "party", "dropped {n} audio profiles before the join");
                        std::thread::sleep(PROFILE_DROP_SETTLE);
                    }
                    Err(e) => tracing::warn!(target: "party", "drop the audio profiles: {e}"),
                }
                source
            }
            None => crate::services::bluetooth::adapter_address()
                .as_deref()
                .and_then(BdAddr::from_canonical)
                .ok_or_else(|| anyhow!("cannot read this computer's Bluetooth address"))?,
        };
        self.set_and_read(|dev| {
            require_party_mode(dev)?;
            let own = retry_late(|| dev.device_bd_addr())?;
            if peer == source || own == peer {
                // A speaker asked to connect to itself or to the
                // source drops another link.
                return Err(BoseError::InvalidArgument(
                    "the other speaker's address is this speaker's or the source's".into(),
                ));
            }
            dev.join_party(peer, source)
        })
    }

    fn leave_party(&self, peer: BdAddr) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| {
            require_party_mode(dev)?;
            dev.leave_party(peer)
        })
    }

    fn set_party_mode(&self, mode: PartyMode) -> Result<DeviceSnapshot> {
        self.set_and_read(|dev| {
            require_party_mode(dev)?;
            dev.set_party_mode(mode)
        })
    }

    fn restart_audio_stream(&self) -> Result<bool> {
        let address = self
            .inner
            .lock()
            .address
            .clone()
            .ok_or_else(|| anyhow!("no device address; connect first"))?;
        crate::services::audio::restart_stream(&address)
    }
}

/// Wait after dropping this computer's audio profiles, before a join.
const PROFILE_DROP_SETTLE: Duration = Duration::from_secs(2);

/// Same guard as the CLI: party-mode commands only on the speakers
/// that have them.
/// Run a getter again when its reply came late (a busy SoundLink
/// Color II in party mode): a getter changes nothing on the device, so
/// it can be sent twice, unlike a setter.
fn retry_late<T>(mut get: impl FnMut() -> BoseResult<T>) -> BoseResult<T> {
    let mut attempt = 1;
    loop {
        match get() {
            Err(e) if is_timeout_error(&e) && attempt < LATE_GETTER_ATTEMPTS => {
                tracing::debug!(target: "device", "late reply, asking again: {e}");
                attempt += 1;
            }
            result => return result,
        }
    }
}

/// How many times [`retry_late`] sends a getter.
const LATE_GETTER_ATTEMPTS: u32 = 3;

fn require_party_mode(dev: &mut BoseDevice) -> BoseResult<()> {
    let (device_id, _) = retry_late(|| dev.device_id())?;
    if bose_connect::has_party_mode(device_id) {
        Ok(())
    } else {
        Err(BoseError::InvalidArgument(
            "this device does not support party mode (SoundLink Color II only)".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bose_connect::VP_ENABLE_BIT;

    use std::sync::mpsc;

    #[test]
    fn a_late_getter_is_asked_again() {
        use std::io::Error;
        let mut calls = 0;
        let got = retry_late(|| {
            calls += 1;
            if calls < 3 {
                Err(BoseError::Io(Error::from_raw_os_error(11)))
            } else {
                Ok(7u8)
            }
        });
        assert_eq!(got.unwrap(), 7);
        // A refusal is not asked again.
        let mut calls = 0;
        let got: BoseResult<u8> = retry_late(|| {
            calls += 1;
            Err(BoseError::AckMismatch)
        });
        assert!(got.is_err());
        assert_eq!(calls, 1);
    }

    #[test]
    fn a_late_reply_keeps_the_connection() {
        use std::io::{Error, ErrorKind};
        // A receive timeout on Linux: EAGAIN (11).
        let late = BoseError::Io(Error::from_raw_os_error(11));
        assert!(!is_transport_error(&late));
        assert!(!is_transport_error(&BoseError::Io(
            ErrorKind::TimedOut.into()
        )));
        // A link that is gone.
        assert!(is_transport_error(&BoseError::Io(
            ErrorKind::ConnectionReset.into()
        )));
        // EHOSTDOWN (112).
        assert!(is_transport_error(&BoseError::Io(
            Error::from_raw_os_error(112)
        )));
        assert!(is_transport_error(&BoseError::Closed));
        // A late reply still fails the snapshot instead of showing
        // the setting as unsupported.
        assert!(optional::<u8>(Err(BoseError::Io(Error::from_raw_os_error(11)))).is_err());
    }

    /// Run `f` on another thread and fail instead of hanging if it
    /// does not return.
    fn within_timeout<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("call did not return: deadlock?")
    }

    fn mock_service() -> MockService {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let (handle, ticker) = rt.block_on(async { transport::spawn(None, None) });
        drop(ticker);
        MockService::new(handle)
    }

    /// Every mock setter used to call `snapshot()` (a read lock)
    /// while holding the write lock, which deadlocks on
    /// `parking_lot::RwLock` and froze the GUI on the first change.
    #[test]
    fn mock_setters_return_instead_of_deadlocking() {
        let svc = mock_service();
        within_timeout(move || {
            svc.set_auto_off(AutoOffWire::Min20).expect("auto-off");
            svc.set_noise_cancelling(NoiseCancelling::Low).expect("nc");
            svc.set_self_voice(SelfVoice::High).expect("self-voice");
            svc.set_language(PromptLanguage::Es, true)
                .expect("language");
            svc.set_voice_prompts(false).expect("voice prompts");
            svc.set_name("Desk").expect("name");
            svc.set_volume(30).expect("volume");
            svc.connect(transport::demo::DEVICE).expect("connect");
            let snap = svc.refresh().expect("refresh");
            assert_eq!(snap.status.minutes, Some(AutoOffWire::Min20 as u16));
            assert_eq!(snap.name, "Desk");
            assert_eq!(snap.volume, Some(30));
        });
    }

    #[test]
    fn mock_voice_prompts_keep_the_language() {
        let svc = mock_service();
        within_timeout(move || {
            svc.set_language(PromptLanguage::Es, true)
                .expect("language");
            let snap = svc.set_voice_prompts(false).expect("voice prompts");
            // `PromptLanguage` values carry the voice-prompts bit.
            assert_eq!(
                snap.status.language,
                PromptLanguage::Es as u8 & !VP_ENABLE_BIT
            );
        });
    }

    #[test]
    fn mock_scenarios_list_their_devices() {
        let svc = mock_service();
        let devices = svc.known_devices().expect("devices");
        assert_eq!(devices.len(), 2);
        assert!(devices[0].connected && !devices[1].connected);
        assert!(svc.connect(transport::demo::SPEAKER).is_err());

        let (handle, scenario) = (svc.handle.clone(), MockScenario::NoDevices);
        let empty = MockService::with_scenario(handle, scenario);
        assert!(empty.known_devices().expect("devices").is_empty());
        assert!(empty.connect(transport::demo::DEVICE).is_err());

        assert_eq!(
            MockScenario::from_env_value("1"),
            Some(MockScenario::Connected)
        );
        assert_eq!(MockScenario::from_env_value("0"), None);
    }

    #[test]
    fn real_service_disconnect_forgets_the_address() {
        let svc = RealService::new();
        svc.disconnect();
        let err = svc.refresh().expect_err("no address after disconnect");
        assert!(err.to_string().contains("connect first"), "{err}");
    }

    #[test]
    fn real_service_needs_an_address_before_any_call() {
        let svc = RealService::new();
        let err = svc.refresh().expect_err("no address yet");
        assert!(err.to_string().contains("connect first"), "{err}");
    }

    #[test]
    fn real_service_rejects_a_malformed_address_without_io() {
        let svc = RealService::new();
        let err = svc.connect("not-an-address").expect_err("malformed");
        assert!(
            err.to_string().contains("invalid Bluetooth address"),
            "{err}"
        );
    }

    #[test]
    fn only_socket_failures_close_the_connection() {
        assert!(is_transport_error(&BoseError::Closed));
        assert!(is_transport_error(&BoseError::ShortRead {
            expected: 4,
            actual: 0
        }));
        assert!(!is_transport_error(&BoseError::AckMismatch));
        assert!(!is_transport_error(&BoseError::InvalidArgument("x".into())));
    }

    #[test]
    fn optional_getters_hide_device_refusals_but_not_transport_errors() {
        assert_eq!(optional::<u8>(Err(BoseError::AckMismatch)).unwrap(), None);
        assert!(optional::<u8>(Err(BoseError::Closed)).is_err());
        assert_eq!(optional(Ok(7u8)).unwrap(), Some(7));
    }
}

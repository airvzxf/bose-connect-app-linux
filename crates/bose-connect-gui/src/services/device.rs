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

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;

use bose_connect::types::Device as DeviceInfo;
use bose_connect::{
    AutoOff as AutoOffWire, BdAddr, BoseDevice, BoseError, BoseResult, DeviceStatus,
    DeviceStatusReport, NoiseCancelling, PairedDevices, PromptLanguage, SelfVoice, VP_ENABLE_BIT,
};

use crate::transport::{self, MockHandle};

/// Per-device capability table derived from the device id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    pub noise_cancelling: bool,
    pub self_voice: bool,
    pub pairing_toggle: bool,
}

/// Snapshot of the device's identity and current state.
#[derive(Debug, Clone)]
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
    pub capabilities: Capability,
    pub volume: Option<u8>,
    pub active_device: Option<BdAddr>,
    pub device_bd_addr: Option<BdAddr>,
}

/// Concrete boxed service.
pub type DynDeviceService = Arc<dyn DeviceService>;

pub trait DeviceService: Send + Sync + 'static {
    /// Open / wake the device.
    fn connect(&self, address: &str) -> Result<DeviceSnapshot>;

    /// Re-derive every get_* and return the latest snapshot.
    fn refresh(&self) -> Result<DeviceSnapshot>;

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

    fn capabilities(&self, device_id: u16) -> Capability {
        Capability {
            noise_cancelling: bose_connect::has_noise_cancelling(device_id),
            self_voice: bose_connect::has_self_voice(device_id),
            pairing_toggle: bose_connect::has_pairing_toggle(device_id),
        }
    }
}

// ---------------------------------------------------------------------------
// Mock
// ---------------------------------------------------------------------------

pub struct MockService {
    handle: MockHandle,
}

impl MockService {
    pub fn new(handle: MockHandle) -> Self {
        Self { handle }
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
        let capabilities = Capability {
            noise_cancelling: bose_connect::has_noise_cancelling(state.device_id),
            self_voice: bose_connect::has_self_voice(state.device_id),
            pairing_toggle: bose_connect::has_pairing_toggle(state.device_id),
        };
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
            capabilities,
            volume: Some(state.volume),
            active_device: state.active_device,
            device_bd_addr: state.device_bd_addr,
        }
    }
}

impl DeviceService for MockService {
    fn connect(&self, address: &str) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if !address.is_empty() {
            if let Some(a) = BdAddr::from_canonical(address) {
                state.address = a;
            }
        }
        state.connected = true;
        Ok(Self::snapshot_from(&state))
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
        if let Err(e) = &result {
            if is_transport_error(e) {
                tracing::warn!(target: "device", "closing the connection after: {e}");
                inner.device = None;
            }
        }
        Ok(result?)
    }

    /// Every getter a snapshot needs, over one connection.
    fn read_snapshot(&self) -> Result<DeviceSnapshot> {
        let address = self.inner.lock().address.clone().unwrap_or_default();
        let address = BdAddr::from_canonical(&address).unwrap_or(BdAddr::ANY);
        self.with_device(|dev| {
            let (device_id, _index) = dev.device_id()?;
            let status = dev.device_status()?;
            let firmware = dev.firmware_version()?;
            let serial = dev.serial_number()?;
            let battery = dev.battery_level()?;
            let paired = dev.paired_devices()?;
            let mut devices = Vec::with_capacity(paired.num_devices);
            for &addr in &paired.addresses[..paired.num_devices] {
                devices.push(device_info_with_retry(dev, addr)?);
            }
            // Not every product answers these two (an ERROR packet,
            // or no source connected); the snapshot shows them as
            // unknown instead of failing.
            let active_device = optional(dev.active_device())?;
            let device_bd_addr = optional(dev.device_bd_addr())?;
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
                capabilities: capability_for(device_id),
                // The protocol has no volume getter.
                volume: None,
                active_device,
                device_bd_addr,
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
fn is_transport_error(e: &BoseError) -> bool {
    matches!(
        e,
        BoseError::Io(_)
            | BoseError::Closed
            | BoseError::ShortRead { .. }
            | BoseError::ShortWrite { .. }
    )
}

/// Map a "this device does not answer that" reply to `None`; keep
/// transport errors, which mean the connection is gone.
fn optional<T>(result: BoseResult<T>) -> BoseResult<Option<T>> {
    match result {
        Ok(v) => Ok(Some(v)),
        Err(e) if is_transport_error(&e) => Err(e),
        Err(e) => {
            tracing::debug!(target: "device", "optional getter failed: {e}");
            Ok(None)
        }
    }
}

fn device_info_with_retry(dev: &mut BoseDevice, addr: BdAddr) -> BoseResult<DeviceInfo> {
    let mut attempt = 1;
    loop {
        match dev.device_info(addr) {
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

    fn set_language(&self, lang: PromptLanguage, voice: bool) -> Result<DeviceSnapshot> {
        let byte = if voice {
            lang as u8 | VP_ENABLE_BIT
        } else {
            lang as u8 & !VP_ENABLE_BIT
        };
        self.set_and_read(|dev| dev.set_prompt_language(byte))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::mpsc;

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
            svc.connect("AA:BB:CC:DD:EE:FF").expect("connect");
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
        assert!(is_transport_error(&BoseError::Io(std::io::Error::from(
            std::io::ErrorKind::TimedOut
        ))));
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

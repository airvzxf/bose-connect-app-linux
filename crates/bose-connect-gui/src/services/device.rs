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

use anyhow::{anyhow, bail, Result};

use bose_connect::types::Device as DeviceInfo;
use bose_connect::{
    AutoOff as AutoOffWire, BdAddr, DeviceStatus, DeviceStatusReport, NoiseCancelling,
    PairedDevices, PromptLanguage, SelfVoice,
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
        let state = self.handle.read();
        let status = transport::snapshot_report(&state);
        let paired = transport::paired_devices_snapshot(&state);
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
        Ok(self.snapshot())
    }

    fn refresh(&self) -> Result<DeviceSnapshot> {
        Ok(self.snapshot())
    }

    fn set_name(&self, name: &str) -> Result<DeviceSnapshot> {
        transport::validate_name(name).map_err(|e| anyhow!(e))?;
        let mut state = self.handle.write();
        state.name = name.to_string();
        Ok(self.snapshot())
    }

    fn set_noise_cancelling(&self, level: NoiseCancelling) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if bose_connect::has_noise_cancelling(state.device_id) {
            state.noise_cancelling = level;
            Ok(self.snapshot())
        } else {
            bail!("this device does not have noise cancelling");
        }
    }

    fn set_self_voice(&self, level: SelfVoice) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if bose_connect::has_self_voice(state.device_id) {
            state.self_voice = level;
            Ok(self.snapshot())
        } else {
            bail!("this device does not support self-voice");
        }
    }

    fn set_auto_off(&self, minutes: AutoOffWire) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.auto_off = minutes;
        Ok(self.snapshot())
    }

    fn set_language(&self, lang: PromptLanguage, voice: bool) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.language = lang;
        state.voice_prompts_on = voice;
        Ok(self.snapshot())
    }

    fn set_pairing(&self, on: bool) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if bose_connect::has_pairing_toggle(state.device_id) {
            state.pairing_on = on;
            Ok(self.snapshot())
        } else {
            bail!("this device does not support pairing-toggle");
        }
    }

    fn connect_device(&self, addr: BdAddr) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if let Some(slot) = state.paired.iter_mut().find(|(a, _, _)| *a == addr) {
            slot.2 = DeviceStatus::Connected;
        }
        Ok(self.snapshot())
    }

    fn disconnect_device(&self, addr: BdAddr) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        if let Some(slot) = state.paired.iter_mut().find(|(a, _, _)| *a == addr) {
            slot.2 = DeviceStatus::Disconnected;
        }
        Ok(self.snapshot())
    }

    fn remove_device(&self, addr: BdAddr) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.paired.retain(|(a, _, _)| *a != addr);
        Ok(self.snapshot())
    }

    fn set_volume(&self, level: u8) -> Result<DeviceSnapshot> {
        let mut state = self.handle.write();
        state.volume = level.min(75);
        Ok(self.snapshot())
    }

    fn send_media_key(&self, _key: bose_connect::MediaKey) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Real RFCOMM
// ---------------------------------------------------------------------------

pub struct RealService;

impl Default for RealService {
    fn default() -> Self {
        Self::new()
    }
}

impl RealService {
    pub fn new() -> Self {
        Self
    }

    #[allow(clippy::too_many_arguments, dead_code)]
    fn snapshot_for_state(
        address: BdAddr,
        device_id: u16,
        name: &str,
        firmware: String,
        serial: String,
        battery: u8,
        status: DeviceStatusReport,
        paired: PairedDevices,
        devices: Vec<DeviceInfo>,
    ) -> DeviceSnapshot {
        let capabilities = Capability {
            noise_cancelling: bose_connect::has_noise_cancelling(device_id),
            self_voice: bose_connect::has_self_voice(device_id),
            pairing_toggle: bose_connect::has_pairing_toggle(device_id),
        };
        DeviceSnapshot {
            address,
            name: name.to_string(),
            firmware,
            serial,
            device_id,
            battery,
            status,
            paired,
            devices,
            capabilities,
            volume: None,
            active_device: None,
            device_bd_addr: None,
        }
    }
}

impl DeviceService for RealService {
    fn connect(&self, _address: &str) -> Result<DeviceSnapshot> {
        // Real RFCOMM is best driven by a blocking thread per the
        // bose_connect API; we don't open an explicit connection
        // here — the first `set_*` opens its own.
        bail!("RealService: not yet wired in this build")
    }

    fn refresh(&self) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn set_name(&self, _name: &str) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn set_noise_cancelling(&self, _level: NoiseCancelling) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn set_self_voice(&self, _level: SelfVoice) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn set_auto_off(&self, _minutes: AutoOffWire) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn set_language(&self, _lang: PromptLanguage, _voice: bool) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn set_pairing(&self, _on: bool) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn connect_device(&self, _addr: BdAddr) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn disconnect_device(&self, _addr: BdAddr) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn remove_device(&self, _addr: BdAddr) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn set_volume(&self, _level: u8) -> Result<DeviceSnapshot> {
        bail!("RealService: not yet wired in this build")
    }

    fn send_media_key(&self, _key: bose_connect::MediaKey) -> Result<()> {
        bail!("RealService: not yet wired in this build")
    }
}

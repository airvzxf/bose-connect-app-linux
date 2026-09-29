//! In-memory mock of the Bose Connect protocol transport.
//!
//! Lets the GUI run end-to-end without a real Bluetooth socket.
//! State is shared behind a `parking_lot::RwLock` so background
//! "battery-drain" ticks and the GUI can coexist without taking a
//! Mutex.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use bose_connect::types::Device as DeviceInfo;
use bose_connect::{
    BdAddr, DeviceStatus, DeviceStatusReport, DevicesConnected, NoiseCancelling, PairedDevices,
    PromptLanguage, SelfVoice, MAX_NAME_LEN, VP_ENABLE_BIT,
};
use parking_lot::RwLock;
use rand::Rng;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::i18n::LANG_NAMES;

/// Events the mock fires asynchronously. The GUI bridges these
/// onto its model when the relevant flag is enabled.
#[derive(Debug, Clone)]
pub enum MockEvent {
    BatteryChanged(u8),
    PairingToggled(bool),
}

#[derive(Debug, Clone)]
pub struct MockSeed {
    pub address: String,
    pub name: String,
    pub battery: u8,
    pub firmware: String,
    pub serial: String,
    pub device_id: u16,
    pub paired: Vec<(String, String, DeviceStatus)>,
    pub history: Vec<u8>,
    pub volume: Option<u8>,
    pub active_device: Option<BdAddr>,
    pub device_bd_addr: Option<BdAddr>,
}

impl Default for MockSeed {
    fn default() -> Self {
        let mut history = Vec::with_capacity(32);
        for i in 0..32u8 {
            let noise = (i.wrapping_mul(7)) % 3;
            history.push(80 + i + noise);
        }
        Self {
            address: "AA:BB:CC:DD:EE:FF".to_string(),
            name: "QuietCompanion".to_string(),
            battery: 85,
            firmware: "1.3.2".to_string(),
            serial: "08AB12CD345678".to_string(),
            device_id: 0x4020,
            paired: vec![
                (
                    "11:22:33:44:55:66".to_string(),
                    "Isra's Phone".to_string(),
                    DeviceStatus::Connected,
                ),
                (
                    "11:22:33:44:55:77".to_string(),
                    "Workstation".to_string(),
                    DeviceStatus::This,
                ),
                (
                    "11:22:33:44:55:88".to_string(),
                    "Tablet (sleep)".to_string(),
                    DeviceStatus::Disconnected,
                ),
            ],
            history,
            volume: Some(45),
            active_device: BdAddr::from_canonical("11:22:33:44:55:77"),
            device_bd_addr: BdAddr::from_canonical("04:52:C7:BA:68:0D"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MockState {
    pub address: BdAddr,
    pub name: String,
    pub battery: u8,
    pub firmware: String,
    pub serial: String,
    pub device_id: u16,
    pub language: PromptLanguage,
    pub voice_prompts_on: bool,
    pub auto_off: bose_connect::AutoOff,
    pub noise_cancelling: NoiseCancelling,
    pub self_voice: SelfVoice,
    pub pairing_on: bool,
    pub connected: bool,
    pub paired: Vec<(BdAddr, String, DeviceStatus)>,
    pub history: VecDeque<u8>,
    pub volume: u8,
    pub active_device: Option<BdAddr>,
    pub device_bd_addr: Option<BdAddr>,
}

impl From<MockSeed> for MockState {
    fn from(seed: MockSeed) -> Self {
        let address = BdAddr::from_canonical(&seed.address).unwrap_or(BdAddr::ANY);
        let paired = seed
            .paired
            .into_iter()
            .filter_map(|(addr, name, status)| {
                BdAddr::from_canonical(&addr).map(|a| (a, name, status))
            })
            .collect();
        let history: VecDeque<u8> = seed.history.into_iter().collect();
        Self {
            address,
            name: seed.name,
            battery: seed.battery.min(100),
            firmware: seed.firmware,
            serial: seed.serial,
            device_id: seed.device_id,
            language: PromptLanguage::En,
            voice_prompts_on: true,
            auto_off: bose_connect::AutoOff::Never,
            noise_cancelling: NoiseCancelling::High,
            self_voice: SelfVoice::Off,
            pairing_on: false,
            connected: true,
            paired,
            history,
            volume: seed.volume.unwrap_or(45),
            active_device: seed.active_device,
            device_bd_addr: seed.device_bd_addr,
        }
    }
}

#[derive(Clone)]
pub struct MockHandle {
    inner: Arc<RwLock<MockState>>,
    tx: mpsc::UnboundedSender<MockEvent>,
}

impl MockHandle {
    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, MockState> {
        self.inner.read()
    }

    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, MockState> {
        self.inner.write()
    }

    pub fn tx(&self) -> mpsc::UnboundedSender<MockEvent> {
        self.tx.clone()
    }
}

pub struct MockTicker {
    _abort: tokio::task::JoinHandle<()>,
}

impl Drop for MockTicker {
    fn drop(&mut self) {
        self._abort.abort();
    }
}

pub fn spawn(seed: Option<MockSeed>, tick_every: Option<Duration>) -> (MockHandle, MockTicker) {
    let seed = seed.unwrap_or_default();
    let state = MockState::from(seed);

    let (tx, mut rx) = mpsc::unbounded_channel::<MockEvent>();
    let inner = Arc::new(RwLock::new(state));
    let handle = MockHandle {
        inner: inner.clone(),
        tx,
    };
    let tick_every = tick_every.unwrap_or(Duration::from_secs(4));
    let ticker_inner = inner.clone();
    let event_tx = handle.tx();

    let abort = tokio::spawn(async move {
        loop {
            sleep(tick_every).await;
            let battery_now: u8 = {
                let mut state = ticker_inner.write();
                let new_battery = if state.battery > 0 {
                    state.battery - 1
                } else {
                    state.battery
                };
                state.battery = new_battery;
                state.history.push_back(new_battery);
                if state.history.len() > 32 {
                    state.history.pop_front();
                }
                new_battery
            };
            let _ = rand::thread_rng().gen_ratio(1, 50);
            let _ = event_tx.send(MockEvent::BatteryChanged(battery_now));
        }
    });

    // Drain channel so producer never blocks.
    tokio::spawn(async move { while rx.recv().await.is_some() {} });

    (handle, MockTicker { _abort: abort })
}

/// Snapshot the current state into the same wire-shaped struct that
/// `bose_connect::protocol::get_device_status` returns, so the GUI
/// can be talked to the same way regardless of transport.
pub fn snapshot_report(state: &MockState) -> DeviceStatusReport {
    let mut byte = state.language as u8;
    if state.voice_prompts_on {
        byte |= VP_ENABLE_BIT;
    } else {
        byte &= !VP_ENABLE_BIT;
    }
    DeviceStatusReport {
        device_id: state.device_id,
        name: state.name.clone(),
        language: byte,
        minutes: Some(state.auto_off as u16),
        level: state.noise_cancelling,
    }
}

/// Re-export some device-id -> capability predicates from the real
/// library so we can keep the GUI logic identical between transports.
pub mod capabilities {
    pub use bose_connect::{has_noise_cancelling, has_pairing_toggle, has_self_voice};
}

/// Helper: validate a name change against the same `MAX_NAME_LEN`
/// limit the real protocol enforces.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("name must not be empty".to_string());
    }
    if name.len() >= MAX_NAME_LEN {
        return Err(format!("name must be < {MAX_NAME_LEN} chars"));
    }
    Ok(())
}

/// Build a [`DeviceInfo`] from a `(addr, name, status)` triple.
pub fn make_device_info(addr: BdAddr, name: &str, status: DeviceStatus) -> DeviceInfo {
    let mut bytes = [0u8; MAX_NAME_LEN];
    let trimmed = name.as_bytes();
    let len = trimmed.len().min(MAX_NAME_LEN - 1);
    bytes[..len].copy_from_slice(&trimmed[..len]);
    DeviceInfo {
        address: addr,
        status,
        name: bytes,
        name_len: len,
    }
}

/// Construct a [`PairedDevices`] from the current state.
pub fn paired_devices_snapshot(state: &MockState) -> PairedDevices {
    let mut addresses = [BdAddr::ANY; bose_connect::MAX_NUM_DEVICES];
    let num = state.paired.len().min(bose_connect::MAX_NUM_DEVICES);
    for (i, (a, _, _)) in state.paired.iter().take(num).enumerate() {
        addresses[i] = *a;
    }
    let connected = state
        .paired
        .iter()
        .filter(|(_, _, st)| *st == DeviceStatus::Connected)
        .count()
        .min(2);
    let connected = match connected {
        0 | 1 => DevicesConnected::One,
        _ => DevicesConnected::Two,
    };
    PairedDevices {
        addresses,
        num_devices: num,
        connected,
    }
}

/// All languages we expose in the dropdown.
pub fn prompt_languages() -> &'static [PromptLanguage] {
    use PromptLanguage::*;
    &[En, Fr, It, De, Es, Pt, Zh, Ko, Ru, Pl, Nl, Ja, Sv]
}

/// All auto-off choices we expose in the dropdown.
pub fn auto_off_choices() -> &'static [bose_connect::AutoOff] {
    use bose_connect::AutoOff::*;
    &[Never, Min5, Min20, Min40, Min60, Min180]
}

/// Friendly name for a prompt language (for the combobox rows).
pub fn prompt_language_label(l: PromptLanguage) -> &'static str {
    LANG_NAMES
        .iter()
        .find(|(k, _)| *k == l as u8)
        .map(|(_, n)| *n)
        .unwrap_or("Unknown")
}

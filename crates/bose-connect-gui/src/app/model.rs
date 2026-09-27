//! The GUI's single source of truth — the `AppModel` and `AppMsg`.
//!
//! We follow the Elm/Relm4 pattern: state lives in the model,
//! messages are dispatched into the `update` function, and the
//! view is a pure projection of the model. Components (the
//! individual widgets) talk to the top-level component via
//! `output` messages so we never lock on shared state.
//!
//! Async work goes through Relm4's `Command::future`, which
//! internally sends the result back as another message. We never
//! block the GTK main loop with a `BoseDevice` call.

use std::sync::Arc;
use std::time::Duration;

use bose_connect::{AutoOff as AutoOffWire, NoiseCancelling, PromptLanguage, SelfVoice};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::services::device::{Capability, DeviceService, DeviceSnapshot};
use crate::services::state::{PersistedState, ProfileName};
use crate::services::tray::TrayCommand;

/// A single entry in the activity log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Connection state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// Address not yet chosen (cold start).
    NotStarted,
    /// A discovery scan is running.
    Discovering,
    /// User picked an address; waiting for `connect` to complete.
    Connecting(String),
    /// Connected; live data is fresh.
    Connected,
    /// Connection attempt failed; show the error inline.
    Error(String),
}

/// Top-level GUI state.
pub struct AppModel {
    /// The Bose device service.
    pub service: Arc<dyn DeviceService>,
    /// The persisted user state (loaded once at startup).
    pub persisted: PersistedState,
    /// Most recent successful device snapshot (or `None` if not connected).
    pub snapshot: Option<DeviceSnapshot>,
    /// The connection state machine.
    pub connection: ConnectionState,
    /// In-flight async ops; the reducer uses this to gate optimistic UI.
    pub in_flight: Vec<String>,
    /// Activity log; newest first.
    pub log: Vec<LogEntry>,
    /// Whether the user has enabled Quiet Mode for the running session.
    pub quiet_mode: bool,
    /// Battery history we keep around for the sparkline.
    pub history: Vec<u8>,
    /// Discovery list (BlueZ events). Empty in mock.
    pub discovery: Vec<(String, String)>,
}

impl AppModel {
    pub fn new(service: Arc<dyn DeviceService>) -> Self {
        Self {
            service,
            persisted: PersistedState::load(),
            snapshot: None,
            connection: ConnectionState::NotStarted,
            in_flight: Vec::new(),
            log: vec![LogEntry {
                timestamp: Utc::now(),
                level: LogLevel::Info,
                message: "Welcome to Bose Connect for Linux".into(),
            }],
            quiet_mode: false,
            history: Vec::new(),
            discovery: Vec::new(),
        }
    }

    /// Push a log entry. Use this from the reducer rather than
    /// formatting strings by hand so the log is consistent.
    pub fn log_info<S: Into<String>>(&mut self, s: S) {
        self.log.push(LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message: s.into(),
        });
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    pub fn log_success<S: Into<String>>(&mut self, s: S) {
        self.log.push(LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Success,
            message: s.into(),
        });
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    pub fn log_error<S: Into<String>>(&mut self, s: S) {
        self.log.push(LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Error,
            message: s.into(),
        });
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    /// Capabilities snapshot.
    pub fn capabilities(&self) -> Capability {
        match &self.snapshot {
            Some(s) => s.capabilities,
            None => Capability {
                noise_cancelling: true,
                self_voice: true,
                pairing_toggle: true,
            },
        }
    }
}

/// A useful "ignored" event we sometimes want to emit when a
/// command resolves without changing state.
pub struct Ignored;

/// All updates that the top-level component can produce. Every
/// UI event is dispatched as one of these. We group them by
/// their source (user action vs. async result) for legibility.
#[derive(Debug)]
pub enum AppMsg {
    /// Cold start: attempt to connect to the persisted address,
    /// or show the discovery screen if there isn't one.
    InitialConnect,

    /// User picked an address from discovery.
    Connect(String),

    /// Result of a successful Connect (incl. the
    /// `BoseDevice::open` round-trip).
    SnapshotReady(DeviceSnapshot),
    SnapshotError(String),

    /// Manual refresh — re-issues every get_* on the device.
    Refresh,
    Refreshed(DeviceSnapshot),
    RefreshFailed(String),

    /// Noise-cancelling change.
    SetNoiseCancelling(NoiseCancelling),
    SelfVoice(SelfVoice),
    SetSelfVoice(SelfVoice),
    SetAutoOff(AutoOffWire),
    SetLanguage {
        language: PromptLanguage,
        voice_prompts: bool,
    },
    SetVoicePrompts(bool),
    SetPairing(bool),
    SetName(String),

    /// Profile selection.
    ApplyProfile(ProfileName),

    /// Quiet mode toggle.
    ToggleQuietMode,

    /// Paired-device actions.
    PairedConnect(bose_connect::BdAddr),
    PairedDisconnect(bose_connect::BdAddr),
    PairedRemove(bose_connect::BdAddr),

    /// Tray-related outputs.
    TrayEvent(TrayCommand),

    /// Connection-state change with a reason (used by
    /// initialConnect / disconnectAll / timeouts).
    SetConnection(ConnectionState),

    /// Periodic battery tick — pushed from a worker.
    BatteryTick(u8),

    /// Discovery tick — pushed when BlueZ finds a new device.
    DiscoveryFound {
        address: String,
        name: String,
    },

    /// Dismiss an error or warning.
    Acknowledge,
}

/// The unified view-state projected from the model. We don't keep
/// this in `AppModel`; the `view` fn computes it. Keep it pure:
/// every member must be derivable from the model.
pub mod view {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BatteryBand {
        Critical,
        Low,
        Ok,
        Full,
    }

    pub fn battery_band(battery: u8) -> BatteryBand {
        match battery {
            0..=5 => BatteryBand::Critical,
            6..=25 => BatteryBand::Low,
            26..=80 => BatteryBand::Ok,
            _ => BatteryBand::Full,
        }
    }

    pub fn safe_band_for(battery: u8) -> BatteryBand {
        battery_band(battery)
    }

    /// True when a command is in flight *for the given tag*.
    pub fn is_in_flight(model: &AppModel, tag: &str) -> bool {
        model.in_flight.iter().any(|t| t == tag)
    }
}

/// The outcome of running one async command. Internal helper —
/// the reducer matches on it to decide whether to short-circuit a
/// re-render.
pub async fn run<F, S, T>(label: &str, f: F) -> (S, Result<T, anyhow::Error>)
where
    F: std::future::Future<Output = anyhow::Result<T>>,
    S: Default,
{
    let _ = label;
    let result = f.await;
    (S::default(), result)
}

/// `Option`-shaped helper that ignores the `_outcome` side-channel.
pub fn outcome_changed(_o: ()) -> bool {
    true
}

/// Number of milliseconds we space background ticks by.
pub const TICK_INTERVAL_MS: u64 = 4_000;

/// Discovery scan cooldown; we re-emit a fresh scan after this
/// much user-idle.
pub const DISCOVERY_REFRESH_MS: u64 = 60_000;

/// A constant used to gate debouncing of the name-editor input.
pub const NAME_DEBOUNCE_MS: u64 = 350;

/// Convenience trait alias for the layers that want to be able to
/// `clone()` and stay `Send` regardless of the underlying service.
pub trait CloneableSend: Send + Sync {
    fn clone_box(&self) -> Box<dyn CloneableSend>;
}

#[allow(dead_code)]
fn _ensure_duration_compiles() -> Duration {
    Duration::from_millis(TICK_INTERVAL_MS)
}

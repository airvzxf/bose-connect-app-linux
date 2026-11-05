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

use crate::services::bluetooth::PairedBose;
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
    /// No device chosen, or the user disconnected.
    NotStarted,
    /// User picked an address; waiting for `connect` to complete.
    Connecting(String),
    /// Connected; live data is fresh.
    Connected,
    /// Connection attempt failed; show the error inline.
    Error(String),
}

/// Identifies which page of the main view the user is looking at.
/// We store it as a string so the persisted state file is
/// human-readable; the actual rendering looks up the matching
/// `Page` value via `Page::from_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    MyBose,
}

impl Page {
    pub const ALL: &'static [Page] = &[Page::Overview, Page::MyBose];

    pub fn key(self) -> &'static str {
        match self {
            Page::Overview => "overview",
            Page::MyBose => "mybose",
        }
    }

    pub fn from_key(key: &str) -> Option<Page> {
        match key {
            "mybose" => Some(Page::MyBose),
            "overview" => Some(Page::Overview),
            _ => Some(Page::Overview),
        }
    }

    /// Name of the page in logs (not translated).
    pub fn title(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::MyBose => "My Bose devices",
        }
    }

    /// The header bar subtitle, in the desktop language.
    pub fn subtitle(self) -> String {
        match self {
            Page::Overview => crate::t!("page-overview-subtitle"),
            Page::MyBose => crate::t!("page-mybose-subtitle"),
        }
    }
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
    /// A party-mode command is waiting for the speaker's answer: the
    /// party controls stay disabled until it arrives.
    pub party_busy: bool,
    /// Why the last party-mode command failed, until the next one.
    pub party_error: Option<String>,
    /// Activity log; newest first.
    pub log: Vec<LogEntry>,
    /// Whether the user has enabled Quiet Mode for the running session.
    pub quiet_mode: bool,
    /// Battery history we keep around for the sparkline.
    pub history: Vec<u8>,
    /// The Bose devices paired with this computer (BlueZ); `None`
    /// until the first read, or when BlueZ is not reachable.
    pub host_devices: Option<Vec<PairedBose>>,
    /// Currently selected page in the sidebar.
    pub current_page: Page,
    /// True when the app runs against the simulated device
    /// (developers only); nothing about it is persisted.
    pub simulator: bool,
}

impl AppModel {
    pub fn new(service: Arc<dyn DeviceService>) -> Self {
        Self {
            service,
            persisted: PersistedState::load(),
            snapshot: None,
            connection: ConnectionState::NotStarted,
            in_flight: Vec::new(),
            party_busy: false,
            party_error: None,
            log: vec![LogEntry {
                timestamp: Utc::now(),
                level: LogLevel::Info,
                message: "Welcome to Bose Connect for Linux".into(),
            }],
            quiet_mode: false,
            history: Vec::new(),
            host_devices: None,
            current_page: Page::Overview,
            simulator: false,
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
        let message = s.into();
        tracing::warn!(target: "app", "{message}");
        self.log.push(LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Error,
            message,
        });
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    /// The address the GUI is connected or connecting to.
    pub fn active_address(&self) -> Option<String> {
        match &self.connection {
            ConnectionState::Connecting(address) => Some(address.clone()),
            ConnectionState::Connected => self
                .snapshot
                .as_ref()
                .map(|s| crate::i18n::format_address(s.address)),
            _ => None,
        }
    }

    /// The display name of a paired device, else its address.
    pub fn device_name(&self, address: &str) -> String {
        self.host_devices
            .iter()
            .flatten()
            .find(|d| d.address.eq_ignore_ascii_case(address))
            .map_or_else(|| address.to_owned(), |d| d.name.clone())
    }

    /// True when BlueZ answered and lists no paired Bose device.
    pub fn no_paired_devices(&self) -> bool {
        self.host_devices.as_ref().is_some_and(Vec::is_empty)
    }

    /// Capabilities snapshot.
    pub fn capabilities(&self) -> Capability {
        match &self.snapshot {
            Some(s) => s.capabilities,
            None => Capability {
                noise_cancelling: true,
                self_voice: true,
                pairing_toggle: true,
                party_mode: false,
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

    /// Close the connection to the current device.
    Disconnect,

    /// Re-read the Bose devices paired with this computer.
    ReloadDevices,

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
    SetVolume(u8),
    SendMediaKey(bose_connect::MediaKey),

    /// Party mode (SoundLink Color II): join the speaker `peer`, with
    /// `source` as the music source (`None`: this computer).
    JoinParty {
        peer: bose_connect::BdAddr,
        source: Option<bose_connect::BdAddr>,
    },
    /// Separate the joined speaker.
    LeaveParty(bose_connect::BdAddr),
    /// Switch two joined speakers between party and stereo mode.
    SetPartyMode(bose_connect::PartyMode),
    /// Make PipeWire open Bluetooth audio links at 44.1 kHz.
    Use44100,

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

    /// Dismiss an error or warning.
    Acknowledge,

    /// Navigate to a different sidebar page. Persisted so the
    /// next launch reopens on the same view.
    NavigateTo(Page),
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

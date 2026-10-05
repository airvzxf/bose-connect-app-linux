//! Tiny "internationalization" helper.
//!
//! We deliberately avoid pulling `gettext`/`fluent` here — none of
//! the keys we need span more than a few words and storing them in
//! one constant table keeps the binary smaller than a full
//! `.mo` loader.

use bose_connect::{AutoOff as AutoOffWire, BdAddr, NoiseCancelling, PromptLanguage, SelfVoice};

/// Display label for a [`NoiseCancelling`] level.
pub fn noise_cancelling_label(level: NoiseCancelling) -> &'static str {
    match level {
        NoiseCancelling::High => "High",
        NoiseCancelling::Low => "Low",
        NoiseCancelling::Off => "Off",
        NoiseCancelling::Dne => "Not supported",
    }
}

/// Display label for a [`SelfVoice`] level.
pub fn self_voice_label(level: SelfVoice) -> &'static str {
    match level {
        SelfVoice::Off => "Off",
        SelfVoice::High => "High",
        SelfVoice::Medium => "Medium",
        SelfVoice::Low => "Low",
    }
}

/// Display label for an [`AutoOffWire`] choice.
pub fn auto_off_label(min: AutoOffWire) -> &'static str {
    match min {
        AutoOffWire::Never => "Never",
        AutoOffWire::Min5 => "5 minutes",
        AutoOffWire::Min20 => "20 minutes",
        AutoOffWire::Min40 => "40 minutes",
        AutoOffWire::Min60 => "1 hour",
        AutoOffWire::Min180 => "3 hours",
    }
}

/// Display label for a [`PromptLanguage`].
pub fn language_label(lang: PromptLanguage) -> &'static str {
    match lang {
        PromptLanguage::En => "English",
        PromptLanguage::Fr => "French",
        PromptLanguage::It => "Italian",
        PromptLanguage::De => "German",
        PromptLanguage::Es => "Spanish",
        PromptLanguage::Pt => "Portuguese",
        PromptLanguage::Zh => "Chinese",
        PromptLanguage::Ko => "Korean",
        PromptLanguage::Ru => "Russian",
        PromptLanguage::Pl => "Polish",
        PromptLanguage::Nl => "Dutch",
        PromptLanguage::Ja => "Japanese",
        PromptLanguage::Sv => "Swedish",
    }
}

/// Display string for a Bluetooth address in canonical form.
pub fn format_address(addr: BdAddr) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        addr.b[0], addr.b[1], addr.b[2], addr.b[3], addr.b[4], addr.b[5]
    )
}

/// Hard-coded copy table consulted by the GUI. Keeping this in a
/// static table means we can later swap in a fluent-based lookup
/// without touching the rest of the codebase.
pub mod strings {
    pub const APP_TITLE: &str = "Bose Connect";
    pub const HERO_BATTERY_PLACEHOLDER: &str = "—";
    pub const SECTION_PAIRED: &str = "Paired Devices";
    pub const SECTION_QUICK: &str = "Quick Settings";
    pub const SECTION_DEVICE: &str = "Device";
    pub const STATUS_CONNECTED: &str = "Connected";
    pub const STATUS_DISCONNECTED: &str = "Disconnected";
    pub const STATUS_CONNECTING: &str = "Connecting…";
    pub const STATUS_REFRESHING: &str = "Refreshing…";
    pub const ACTION_REFRESH: &str = "Refresh";
    pub const ACTION_SCAN: &str = "Scan for devices";
    pub const ACTION_CONNECT: &str = "Connect";
    pub const ACTION_DISCONNECT: &str = "Disconnect";
    pub const ACTION_FORGET: &str = "Forget";
    pub const ACTION_APPLY: &str = "Apply";
    pub const ACTION_QUIT_MODE: &str = "Quiet mode";
    pub const NOTIF_BATTERY_LOW: &str = "Battery low — plug in soon";
    pub const NOTIF_BATTERY_CRITICAL: &str = "Battery critical — last 5 %";
    pub const TOOLTIP_DISCOVER: &str = "Discover Bose devices";
    pub const TOOLTIP_AUTO_OFF: &str = "Auto-off timer";
    pub const TOOLTIP_VOICE: &str = "Voice prompts";
    pub const TOOLTIP_NC: &str = "Noise cancelling";
    pub const TOOLTIP_LANGUAGE: &str = "Voice prompt language";
    pub const WINDOW_DEFAULT_WIDTH: i32 = 1180;
    pub const WINDOW_DEFAULT_HEIGHT: i32 = 760;
}

/// Lookup table consulted by `crate::transport::prompt_language_label`.
pub(crate) const LANG_NAMES: &[(u8, &str)] = &[
    (0x21, "English"),
    (0x22, "Français"),
    (0x23, "Italiano"),
    (0x24, "Deutsch"),
    (0x26, "Español"),
    (0x27, "Português"),
    (0x28, "中文"),
    (0x29, "한국어"),
    (0x2a, "Русский"),
    (0x2b, "Polski"),
    (0x2e, "Nederlands"),
    (0x2f, "日本語"),
    (0x32, "Svenska"),
];

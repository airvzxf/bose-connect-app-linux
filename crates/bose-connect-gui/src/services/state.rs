//! Persistent user state.
//!
//! We serialise a small JSON file at
//! `~/.config/bose-connect/state.json` so the GUI remembers the
//! last-connected address and the active profile.
//!
//! Profiles bundle a noise-cancelling level, a voice-prompts on/off,
//! an auto-off value and a language. Applying a profile dispatches
//! every atomic change to the device service in sequence.

use std::path::PathBuf;

use bose_connect::{AutoOff as AutoOffWire, NoiseCancelling, PromptLanguage};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    #[serde(default)]
    pub last_address: Option<String>,
    #[serde(default)]
    pub active_profile: Option<ProfileName>,
    /// Last page the user was on, so the window reopens on the
    /// same view. Stored as the stable string key of the enum.
    #[serde(default)]
    pub last_page: Option<String>,
}

impl PersistedState {
    pub fn load() -> Self {
        match Self::path() {
            Some(p) if p.exists() => std::fs::read_to_string(&p)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            _ => Self::default(),
        }
    }

    pub fn save(&self) {
        if let Some(path) = Self::path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(&path, s);
            }
        }
    }

    pub fn path() -> Option<PathBuf> {
        ProjectDirs::from("com", "airvzxf", "bose-connect").map(|d| {
            let mut p = d.config_dir().to_path_buf();
            p.push("state.json");
            p
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileName {
    Focus,
    Travel,
    Home,
    Quiet,
}

impl ProfileName {
    pub fn label(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::Travel => "Travel",
            Self::Home => "Home",
            Self::Quiet => "Quiet",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProfileSettings {
    pub noise_cancelling: Option<NoiseCancelling>,
    pub voice_prompts: Option<bool>,
    pub auto_off: Option<AutoOffWire>,
    pub language: Option<PromptLanguage>,
}

pub struct Profiles;

impl Profiles {
    pub fn resolve(name: ProfileName) -> ProfileSettings {
        match name {
            ProfileName::Focus => ProfileSettings {
                noise_cancelling: Some(NoiseCancelling::High),
                voice_prompts: Some(false),
                auto_off: Some(AutoOffWire::Min20),
                language: Some(PromptLanguage::En),
            },
            ProfileName::Travel => ProfileSettings {
                noise_cancelling: Some(NoiseCancelling::High),
                voice_prompts: Some(true),
                auto_off: Some(AutoOffWire::Min60),
                language: Some(PromptLanguage::En),
            },
            ProfileName::Home => ProfileSettings {
                noise_cancelling: Some(NoiseCancelling::Low),
                voice_prompts: Some(true),
                auto_off: Some(AutoOffWire::Never),
                language: Some(PromptLanguage::En),
            },
            ProfileName::Quiet => ProfileSettings {
                noise_cancelling: Some(NoiseCancelling::High),
                voice_prompts: Some(false),
                auto_off: Some(AutoOffWire::Never),
                language: Some(PromptLanguage::En),
            },
        }
    }

    pub fn all() -> &'static [ProfileName] {
        &[
            ProfileName::Focus,
            ProfileName::Travel,
            ProfileName::Home,
            ProfileName::Quiet,
        ]
    }
}

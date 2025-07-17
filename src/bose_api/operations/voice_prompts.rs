use serde::{Deserialize, Serialize};

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Eq, Copy, Clone, PartialOrd, Ord, clap::ValueEnum,
)]
pub enum VoicePrompts {
    #[clap(name = "off")]
    Off,
    #[clap(name = "on")]
    On,
}

impl From<VoicePrompts> for u8 {
    fn from(value: VoicePrompts) -> Self {
        match value {
            VoicePrompts::Off => 0x80,
            VoicePrompts::On => 0xff,
        }
    }
}

impl From<u8> for VoicePrompts {
    fn from(value: u8) -> Self {
        match value {
            0x80 => VoicePrompts::Off,
            0xff => VoicePrompts::On,
            _ => VoicePrompts::Off, // Default to off
        }
    }
}

use serde::{Deserialize, Serialize};

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Eq, Copy, Clone, PartialOrd, Ord, clap::ValueEnum,
)]
pub enum SelfVoice {
    #[clap(name = "off")]
    Off,
    #[clap(name = "high")]
    High,
    #[clap(name = "medium")]
    Medium,
    #[clap(name = "low")]
    Low,
    #[clap(skip)]
    Unknown,
}

impl From<u8> for SelfVoice {
    fn from(value: u8) -> Self {
        match value {
            0x00 => SelfVoice::Off,
            0x01 => SelfVoice::High,
            0x02 => SelfVoice::Medium,
            0x03 => SelfVoice::Low,
            _ => SelfVoice::Unknown,
        }
    }
}

impl From<SelfVoice> for u8 {
    fn from(value: SelfVoice) -> Self {
        match value {
            SelfVoice::Off => 0x00,
            SelfVoice::High => 0x01,
            SelfVoice::Medium => 0x02,
            SelfVoice::Low => 0x03,
            SelfVoice::Unknown => 0x00, // Default to off
        }
    }
}

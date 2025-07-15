use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum PromptLanguage {
    Dutch,
    English,
    French,
    German,
    Italian,
    Japanese,
    Korean,
    Mandarin,
    Polish,
    Portuguese,
    Russian,
    SpanishMx,
    Swedish,
    Unknown,
}

impl From<u8> for PromptLanguage {
    fn from(value: u8) -> Self {
        match value {
            0xA1 => PromptLanguage::English,
            0xA2 => PromptLanguage::French,
            0xA3 => PromptLanguage::Italian,
            0xA4 => PromptLanguage::German,
            0xA6 => PromptLanguage::SpanishMx,
            0xA7 => PromptLanguage::Portuguese,
            0xA8 => PromptLanguage::Mandarin,
            0xA9 => PromptLanguage::Korean,
            0xAA => PromptLanguage::Russian,
            0xAB => PromptLanguage::Polish,
            0xAE => PromptLanguage::Dutch,
            0xAF => PromptLanguage::Japanese,
            0xB2 => PromptLanguage::Swedish,
            _ => PromptLanguage::Unknown,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Copy, Clone)]
pub enum AutoOff {
    Never,
    Minutes5,
    Minutes20,
    Minutes40,
    Minutes60,
    Minutes180,
    Unknown,
}

impl From<u8> for AutoOff {
    fn from(value: u8) -> Self {
        match value {
            0x00 => AutoOff::Never,
            0x05 => AutoOff::Minutes5,
            0x14 => AutoOff::Minutes20,
            0x28 => AutoOff::Minutes40,
            0x3C => AutoOff::Minutes60,
            0xB4 => AutoOff::Minutes180,
            _ => AutoOff::Unknown,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum NoiseCancelling {
    Unknown,
    High,
    Low,
    Off,
}

impl From<u8> for NoiseCancelling {
    fn from(value: u8) -> Self {
        match value {
            0x00 => NoiseCancelling::Off,
            0x01 => NoiseCancelling::High,
            0x03 => NoiseCancelling::Low,
            _ => NoiseCancelling::Unknown,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct DeviceStatus {
    pub name: String,
    pub language: PromptLanguage,
    pub auto_off: AutoOff,
    pub noise_cancelling: NoiseCancelling,
}

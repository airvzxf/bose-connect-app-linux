use crate::bose_api::operations::self_voice::SelfVoice;
use crate::bose_api::operations::voice_prompts::VoicePrompts;
use serde::{Deserialize, Serialize};

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Eq, Copy, Clone, PartialOrd, Ord, clap::ValueEnum,
)]
pub enum PromptLanguage {
    #[clap(name = "dutch")]
    Dutch,
    #[clap(name = "en")]
    English,
    #[clap(name = "fr")]
    French,
    #[clap(name = "de")]
    German,
    #[clap(name = "it")]
    Italian,
    #[clap(name = "jp")]
    Japanese,
    #[clap(name = "kr")]
    Korean,
    #[clap(name = "cn")]
    Mandarin,
    #[clap(name = "pl")]
    Polish,
    #[clap(name = "pt")]
    Portuguese,
    #[clap(name = "ru")]
    Russian,
    #[clap(name = "es-mx")]
    SpanishMx,
    #[clap(name = "se")]
    Swedish,
    #[clap(name = "en-off")]
    EnglishDisabled,
    #[clap(name = "fr-off")]
    FrenchDisabled,
    #[clap(name = "it-off")]
    ItalianDisabled,
    #[clap(name = "de-off")]
    GermanDisabled,
    #[clap(name = "es-mx-off")]
    SpanishMxDisabled,
    #[clap(name = "pt-off")]
    PortugueseDisabled,
    #[clap(name = "cn-off")]
    MandarinDisabled,
    #[clap(name = "kr-off")]
    KoreanDisabled,
    #[clap(name = "fr-off")]
    RussianDisabled,
    #[clap(name = "pl-off")]
    PolishDisabled,
    #[clap(name = "dutch-off")]
    DutchDisabled,
    #[clap(name = "jp-off")]
    JapaneseDisabled,
    #[clap(name = "se-off")]
    SwedishDisabled,
    #[clap(skip)]
    Unknown,
}

impl From<u8> for PromptLanguage {
    fn from(value: u8) -> Self {
        match value {
            0x81 => PromptLanguage::EnglishDisabled,
            0x82 => PromptLanguage::FrenchDisabled,
            0x83 => PromptLanguage::ItalianDisabled,
            0x85 => PromptLanguage::GermanDisabled,
            0x86 => PromptLanguage::SpanishMxDisabled,
            0x87 => PromptLanguage::PortugueseDisabled,
            0x88 => PromptLanguage::MandarinDisabled,
            0x89 => PromptLanguage::KoreanDisabled,
            0x8A => PromptLanguage::RussianDisabled,
            0x8B => PromptLanguage::PolishDisabled,
            0x8E => PromptLanguage::DutchDisabled,
            0x8F => PromptLanguage::JapaneseDisabled,
            0x92 => PromptLanguage::SwedishDisabled,
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

impl From<PromptLanguage> for u8 {
    fn from(value: PromptLanguage) -> Self {
        match value {
            PromptLanguage::EnglishDisabled => 0x81,
            PromptLanguage::FrenchDisabled => 0x82,
            PromptLanguage::ItalianDisabled => 0x83,
            PromptLanguage::GermanDisabled => 0x84,
            PromptLanguage::SpanishMxDisabled => 0x86,
            PromptLanguage::PortugueseDisabled => 0x87,
            PromptLanguage::MandarinDisabled => 0x88,
            PromptLanguage::KoreanDisabled => 0x89,
            PromptLanguage::RussianDisabled => 0x8A,
            PromptLanguage::PolishDisabled => 0x8B,
            PromptLanguage::DutchDisabled => 0x8E,
            PromptLanguage::JapaneseDisabled => 0x8F,
            PromptLanguage::SwedishDisabled => 0x92,
            PromptLanguage::English => 0xA1,
            PromptLanguage::French => 0xA2,
            PromptLanguage::Italian => 0xA3,
            PromptLanguage::German => 0xA4,
            PromptLanguage::SpanishMx => 0xA6,
            PromptLanguage::Portuguese => 0xA7,
            PromptLanguage::Mandarin => 0xA8,
            PromptLanguage::Korean => 0xA9,
            PromptLanguage::Russian => 0xAA,
            PromptLanguage::Polish => 0xAB,
            PromptLanguage::Dutch => 0xAE,
            PromptLanguage::Japanese => 0xAF,
            PromptLanguage::Swedish => 0xB2,
            PromptLanguage::Unknown => 0x00, // Default to off
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

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Eq, Copy, Clone, PartialOrd, Ord, clap::ValueEnum,
)]
pub enum NoiseCancelling {
    #[clap(name = "off")]
    Off,
    #[clap(name = "low")]
    Low,
    #[clap(name = "high")]
    High,
    #[clap(skip)]
    Unknown,
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

impl From<NoiseCancelling> for u8 {
    fn from(value: NoiseCancelling) -> Self {
        match value {
            NoiseCancelling::Off => 0x00,
            NoiseCancelling::High => 0x01,
            NoiseCancelling::Low => 0x03,
            NoiseCancelling::Unknown => 0x00, // Default to off
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Copy, Clone)]
pub enum FunctionButton {
    Other,
    NoiseCancelling,
    Unknown,
}

impl From<u8> for FunctionButton {
    fn from(value: u8) -> Self {
        match value {
            0x01 => FunctionButton::Other,
            0x02 => FunctionButton::NoiseCancelling,
            _ => FunctionButton::Unknown,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct DeviceStatus {
    pub name: String,
    pub language: PromptLanguage,
    pub auto_off: AutoOff,
    pub noise_cancelling: NoiseCancelling,
    pub voice_prompts: VoicePrompts,
    pub function_button: FunctionButton,
    pub self_voice: SelfVoice,
}

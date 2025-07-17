use crate::bose_api::operations::device_status::{NoiseCancelling, PromptLanguage};
use crate::bose_api::operations::pairing::Pairing;
use crate::bose_api::operations::self_voice::SelfVoice;
use crate::bose_api::operations::voice_prompts::VoicePrompts;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Bluetooth address of the Bose device
    #[arg(short, long)]
    pub address: String,

    /// Manually specify the device model (e.g., "QC35II", "SoundLinkColorII")
    #[arg(long)]
    pub model: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Init connection
    InitConnection,
    /// Gets the battery level of the connected device
    Battery,
    /// Gets the device status (name, language, auto-off, noise-cancelling)
    DeviceStatus,
    /// Gets the device name
    Name,
    /// Gets the prompt language
    Language,
    /// Gets the auto-off setting
    AutoOff,
    /// Gets the noise-cancelling setting
    NoiseCancelling,
    /// Gets the voice prompts setting
    VoicePrompts,
    /// Gets the device ID
    DeviceId,
    /// Gets the firmware version
    FirmwareVersion,
    /// Gets the serial number
    SerialNumber,
    /// Gets the paired devices
    PairedDevices,
    /// Gets information about a specific device
    DeviceInformation { address: String },
    /// Sets the auto-off setting
    SetAutoOff {
        /// The auto-off value
        #[arg(value_enum)]
        value: AutoOffValue,
    },
    /// Sets the noise-cancelling setting
    SetNoiseCancelling {
        /// The noise-cancelling value
        #[arg(value_enum)]
        value: NoiseCancelling,
    },
    /// Sets the prompt language
    SetPromptLanguage {
        /// The prompt language value
        #[arg(value_enum)]
        value: PromptLanguage,
    },
    /// Sets the self-voice setting
    SetSelfVoice {
        /// The self-voice value
        #[arg(value_enum)]
        value: SelfVoice,
    },
    /// Sets the device name
    SetName {
        /// The new name for the device
        name: String,
    },
    /// Sets the pairing setting
    SetPairing {
        /// The pairing value
        #[arg(value_enum)]
        value: Pairing,
    },
    /// Sets the voice prompts setting
    SetVoicePrompts {
        /// The voice prompts value
        #[arg(value_enum)]
        value: VoicePrompts,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum AutoOffValue {
    #[clap(name = "never")]
    Never,
    #[clap(name = "5")]
    Minutes5,
    #[clap(name = "20")]
    Minutes20,
    #[clap(name = "40")]
    Minutes40,
    #[clap(name = "60")]
    Minutes60,
    #[clap(name = "180")]
    Minutes180,
}

impl AutoOffValue {
    pub fn to_minutes(self) -> u16 {
        match self {
            AutoOffValue::Never => 0,
            AutoOffValue::Minutes5 => 5,
            AutoOffValue::Minutes20 => 20,
            AutoOffValue::Minutes40 => 40,
            AutoOffValue::Minutes60 => 60,
            AutoOffValue::Minutes180 => 180,
        }
    }
}

impl From<AutoOffValue> for u8 {
    fn from(value: AutoOffValue) -> Self {
        match value {
            AutoOffValue::Never => 0x00,
            AutoOffValue::Minutes5 => 0x05,
            AutoOffValue::Minutes20 => 0x14,
            AutoOffValue::Minutes40 => 0x28,
            AutoOffValue::Minutes60 => 0x3C,
            AutoOffValue::Minutes180 => 0xB4,
        }
    }
}

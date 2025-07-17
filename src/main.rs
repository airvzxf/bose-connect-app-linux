mod bose_api;
mod cli;

use crate::bose_api::device::{BoseDevice, Model};
use crate::bose_api::operations::battery::BatteryInfo;
use crate::bose_api::operations::device_id::DeviceIdInfo;
use crate::bose_api::operations::device_information::DeviceInformationInfo;
use crate::bose_api::operations::device_status::DeviceStatus;
use crate::bose_api::operations::firmware_version::FirmwareVersionInfo;
use crate::bose_api::operations::paired_devices::PairedDeviceInfo;
use crate::bose_api::operations::serial_number::SerialNumberInfo;
use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use serde_json::json;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    let cli: Cli = Cli::parse();

    let model_override: Option<Model> = match cli.model {
        Some(model_str) => Some(Model::from_str(&model_str)?),
        None => None,
    };

    let mut bose_device: BoseDevice = BoseDevice::connect(&cli.address, model_override).await?;

    match cli.command {
        Commands::Battery => {
            let battery_info: BatteryInfo = bose_device.get_battery_level().await?;
            let json_output: String = serde_json::to_string_pretty(&battery_info)?;
            println!("{json_output}");
        }
        Commands::DeviceStatus
        | Commands::Name
        | Commands::Language
        | Commands::AutoOff
        | Commands::NoiseCancelling => {
            let device_status: DeviceStatus = bose_device.get_device_status().await?;
            let json_output: String = match cli.command {
                Commands::DeviceStatus => serde_json::to_string_pretty(&device_status)?,
                Commands::Name => {
                    serde_json::to_string_pretty(&json!({ "name": device_status.name }))?
                }
                Commands::Language => {
                    serde_json::to_string_pretty(&json!({ "language": device_status.language }))?
                }
                Commands::AutoOff => {
                    serde_json::to_string_pretty(&json!({ "auto_off": device_status.auto_off }))?
                }
                Commands::NoiseCancelling => serde_json::to_string_pretty(
                    &json!({ "noise_cancelling": device_status.noise_cancelling }),
                )?,
                _ => unreachable!(),
            };
            println!("{json_output}");
        }
        Commands::DeviceInformation { address } => {
            let device_information_info: DeviceInformationInfo =
                bose_device.get_device_information(&address).await?;
            let json_output: String = serde_json::to_string_pretty(&device_information_info)?;
            println!("{json_output}");
        }
        Commands::PairedDevices => {
            let paired_devices_info: PairedDeviceInfo = bose_device.get_paired_devices().await?;
            let json_output: String = serde_json::to_string_pretty(&paired_devices_info)?;
            println!("{json_output}");
        }
        Commands::SerialNumber => {
            let serial_number_info: SerialNumberInfo = bose_device.get_serial_number().await?;
            let json_output: String = serde_json::to_string_pretty(&serial_number_info)?;
            println!("{json_output}");
        }
        Commands::FirmwareVersion => {
            let firmware_version_info: FirmwareVersionInfo =
                bose_device.get_firmware_version().await?;
            let json_output: String = serde_json::to_string_pretty(&firmware_version_info)?;
            println!("{json_output}");
        }
        Commands::DeviceId => {
            let device_id_info: DeviceIdInfo = bose_device.get_device_id().await?;
            let json_output: String = serde_json::to_string_pretty(&device_id_info)?;
            println!("{json_output}");
        }
        Commands::SetAutoOff { value } => {
            bose_device.set_auto_off(value).await?;
            let json_output: String =
                serde_json::to_string_pretty(&json!({ "auto_off_set_to": value.to_minutes() }))?;
            println!("{json_output}");
        }
        Commands::SetNoiseCancelling { value } => {
            bose_device.set_noise_cancelling(value).await?;
            let json_output: String =
                serde_json::to_string_pretty(&json!({ "noise_cancelling_set_to": value }))?;
            println!("{json_output}");
        }
        Commands::SetPromptLanguage { value } => {
            bose_device.set_prompt_language(value).await?;
            let json_output: String =
                serde_json::to_string_pretty(&json!({ "prompt_language_set_to": value }))?;
            println!("{json_output}");
        }
        Commands::SetSelfVoice { value } => {
            bose_device.set_self_voice(value).await?;
            let json_output: String =
                serde_json::to_string_pretty(&json!({ "self_voice_set_to": value }))?;
            println!("{json_output}");
        }
        Commands::SetName { name } => {
            bose_device.set_name(&name).await?;
            let json_output: String =
                serde_json::to_string_pretty(&json!({ "name_set_to": name }))?;
            println!("{json_output}");
        }
        Commands::SetPairing { value } => {
            bose_device.set_pairing(value).await?;
            let json_output: String =
                serde_json::to_string_pretty(&json!({ "pairing_set_to": value }))?;
            println!("{json_output}");
        }
    }

    Ok(())
}

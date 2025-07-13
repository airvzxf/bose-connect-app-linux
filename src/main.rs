mod bose_api;
mod cli;

use crate::bose_api::device::{BoseDevice, Model};
use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use serde_json::json;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let model_override = match cli.model {
        Some(model_str) => Some(Model::from_str(&model_str)?),
        None => None,
    };

    let mut bose_device = BoseDevice::connect(&cli.address, model_override).await?;

    match cli.command {
        Commands::Battery => {
            let battery_info = bose_device.get_battery_level().await?;
            let json_output = serde_json::to_string_pretty(&battery_info)?;
            println!("{json_output}");
        }
        Commands::DeviceStatus
        | Commands::Name
        | Commands::Language
        | Commands::AutoOff
        | Commands::NoiseCancelling => {
            let device_status = bose_device.get_device_status().await?;
            let json_output = match cli.command {
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
            let device_information_info = bose_device.get_device_information(&address).await?;
            let json_output = serde_json::to_string_pretty(&device_information_info)?;
            println!("{json_output}");
        }
        Commands::PairedDevices => {
            let paired_devices_info = bose_device.get_paired_devices().await?;
            let json_output = serde_json::to_string_pretty(&paired_devices_info)?;
            println!("{json_output}");
        }
        Commands::SerialNumber => {
            let serial_number_info = bose_device.get_serial_number().await?;
            let json_output = serde_json::to_string_pretty(&serial_number_info)?;
            println!("{json_output}");
        }
        Commands::FirmwareVersion => {
            let firmware_version_info = bose_device.get_firmware_version().await?;
            let json_output = serde_json::to_string_pretty(&firmware_version_info)?;
            println!("{json_output}");
        }
        Commands::DeviceId => {
            let device_id_info = bose_device.get_device_id().await?;
            let json_output = serde_json::to_string_pretty(&device_id_info)?;
            println!("{json_output}");
        }
    }

    Ok(())
}

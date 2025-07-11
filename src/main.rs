mod bose_api;
mod cli;

use crate::bose_api::device::{BoseDevice, Model};
use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
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
        Commands::GetBattery => {
            let battery_info = bose_device.get_battery_level().await?;
            let json_output = serde_json::to_string_pretty(&battery_info)?;
            println!("{json_output}");
        }
    }

    Ok(())
}

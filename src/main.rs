mod bose_api;
mod cli;

use crate::bose_api::device::BoseDevice;
use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("Connecting to device: {}", cli.address);

    let mut bose_device = BoseDevice::connect(&cli.address).await?;

    match cli.command {
        Commands::GetBattery => {
            let battery_info =
                crate::bose_api::operations::battery::get_battery_level(&mut bose_device).await?;
            println!("Battery Level: {}%", battery_info.level);
        }
    }

    Ok(())
}

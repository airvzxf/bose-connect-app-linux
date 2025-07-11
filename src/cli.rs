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
    /// Gets the battery level of the connected device
    GetBattery,
}

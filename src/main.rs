mod bluetooth;
mod cli;

use anyhow::{Result, anyhow};
use bluetooth::connect_to_device;
use clap::Parser;
use cli::{Cli, Commands};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("Connecting to device: {}", cli.address);

    let mut stream = connect_to_device(&cli.address).await?;

    // Perform initial connection handshake
    let init_send_bytes: [u8; 4] = [0x00, 0x01, 0x01, 0x00];
    let init_ack_bytes: [u8; 4] = [0x00, 0x01, 0x03, 0x05];
    let mut init_ack_buffer = vec![0; init_ack_bytes.len()];
    let mut garbage_buffer = vec![0; 5];

    stream.write_all(&init_send_bytes).await?;
    println!("Sent Init: {init_send_bytes:?}");

    stream.read_exact(&mut init_ack_buffer).await?;
    println!("Received Init ACK: {init_ack_buffer:?}");

    if init_ack_buffer != init_ack_bytes {
        return Err(anyhow!(
            "Init ACK mismatch: Expected {:?}, got {:?}",
            init_ack_bytes,
            init_ack_buffer
        ));
    }

    stream.read_exact(&mut garbage_buffer).await?;
    println!("Received Init Garbage: {garbage_buffer:?}");

    match cli.command {
        Commands::GetBattery => {
            println!("Getting battery level...");

            let send_bytes: [u8; 4] = [0x02, 0x02, 0x01, 0x00];
            let ack_bytes: [u8; 4] = [0x02, 0x02, 0x03, 0x01];

            stream.write_all(&send_bytes).await?;
            println!("Sent: {send_bytes:?}");

            let mut ack_buffer = vec![0; ack_bytes.len()];
            stream.read_exact(&mut ack_buffer).await?;
            println!("Received ACK: {ack_buffer:?}");

            if ack_buffer != ack_bytes {
                return Err(anyhow!(
                    "ACK mismatch: Expected {:?}, got {:?}",
                    ack_bytes,
                    ack_buffer
                ));
            }

            let mut level_byte = [0; 1];
            stream.read_exact(&mut level_byte).await?;
            let battery_level = level_byte[0];

            println!("Battery Level: {battery_level}%");
        }
    }

    Ok(())
}

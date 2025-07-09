use anyhow::{Result, anyhow};
use bluer::rfcomm::{Socket, SocketAddr, Stream};
use bluer::{Address, Session};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::bose_api::firmware::{Firmware, detect_firmware};

pub struct BoseDevice {
    pub stream: Stream,
    pub firmware: Box<dyn Firmware>,
}

impl BoseDevice {
    pub async fn connect(address: &str) -> Result<Self> {
        let session = Session::new().await?;
        let _adapter = session.default_adapter().await?;

        println!("Connecting to Bluetooth device with address: {address}");

        let device_address: Address = address.parse()?;

        // The Bose RFCOMM channel found in based.h
        let rfcomm_channel = 8;

        let socket_addr = SocketAddr::new(device_address, rfcomm_channel);
        let socket = Socket::new()?; // Create a new socket for each connection attempt
        let mut stream = socket.connect(socket_addr).await?;

        println!("Successfully connected to device: {address}");

        // Initial Handshake (moved from main.rs)
        let init_send_bytes: [u8; 4] = [0x00, 0x01, 0x01, 0x00];
        let init_ack_bytes: [u8; 4] = [0x00, 0x01, 0x03, 0x05];
        let mut init_ack_buffer = vec![0; init_ack_bytes.len()];

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

        let mut firmware_version_bytes = vec![0; 5];
        stream.read_exact(&mut firmware_version_bytes).await?;
        println!("Received Firmware Version: {firmware_version_bytes:?}");

        let firmware = detect_firmware(&firmware_version_bytes)?;

        Ok(BoseDevice { stream, firmware })
    }

    pub async fn execute_command(
        &mut self,
        send_bytes: &[u8],
        expected_ack: &[u8],
        response_len: usize,
    ) -> Result<Vec<u8>> {
        self.stream.write_all(send_bytes).await?;

        let mut ack_buffer = vec![0; expected_ack.len()];
        let bytes_read_ack = self.stream.read(&mut ack_buffer).await?;

        if bytes_read_ack != expected_ack.len() || ack_buffer != expected_ack {
            return Err(anyhow!(
                "Command ACK mismatch or incomplete: Expected {:?}, Received {} bytes: {:?}",
                expected_ack,
                bytes_read_ack,
                ack_buffer
            ));
        }

        let mut response_buffer = vec![0; response_len];
        let bytes_read_response = self.stream.read(&mut response_buffer).await?;

        if bytes_read_response != response_len {
            return Err(anyhow!(
                "Command response incomplete: Expected {} bytes, Received {} bytes: {:?}",
                response_len,
                bytes_read_response,
                response_buffer
            ));
        }

        Ok(response_buffer)
    }
}

use anyhow::Result;
use bluer::rfcomm::{Socket, SocketAddr, Stream};
use bluer::{Address, Session};

pub async fn connect_to_device(address: &str) -> Result<Stream> {
    let session = Session::new().await?;
    let _adapter = session.default_adapter().await?;

    println!("Connecting to Bluetooth device with address: {address}");

    let device_address: Address = address.parse()?;

    // The Bose RFCOMM channel found in based.h
    let rfcomm_channel = 8;

    let socket_addr = SocketAddr::new(device_address, rfcomm_channel);
    let socket = Socket::new()?; // Create a new socket for each connection attempt
    let stream = socket.connect(socket_addr).await?;

    println!("Successfully connected to device: {address}");
    Ok(stream)
}

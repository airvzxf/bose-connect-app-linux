use crate::bose_api::BoseError;
use crate::bose_api::firmware::{Firmware, detect_firmware};
use crate::bose_api::operations::battery::{BatteryInfo, parse_battery_level};
use bluer::rfcomm::{Socket, SocketAddr, Stream};
use bluer::{Adapter, Address, Session};
use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Model {
    Qc35ii,
    SoundLinkColorII,
    Unknown,
}

impl FromStr for Model {
    type Err = BoseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "qc35ii" => Ok(Model::Qc35ii),
            "soundlinkcolorii" => Ok(Model::SoundLinkColorII),
            _ => Err(BoseError::InvalidInput(format!("Unknown model: {s}"))),
        }
    }
}

pub struct BoseDevice {
    stream: Stream,
    firmware: Box<dyn Firmware>,
}

async fn detect_model(adapter: &Adapter, address: Address) -> Result<Model, BoseError> {
    let device = adapter.device(address)?;
    let device_name = device.name().await?.unwrap_or_default();
    let device_mod_alias = device.modalias().await?.unwrap();
    let device_vendor = device_mod_alias.vendor;
    let device_product = device_mod_alias.product;
    let device_device = device_mod_alias.device;

    // TODO: The `vendor` 158 (0x009E) is referred to as "Bose Corporation" in the Linux kernel.
    //       The `product` 16416 (0x4020) is the "QC35 II" and 16397 (0x400D) is the "SoundLink Color II".
    //       We can use these values to identify the device model more reliably.
    //       However, for now, we will use the device name as a fallback because the meaning is unknown.
    //       Create the logic to handle the device name and vendor/product IDs to detect the model.
    println!("Device address: {address}");
    println!("Device name: {device_name}");
    println!("Device vendor: {device_vendor} (0x{device_vendor:X})");
    println!("Device device: {device_device} (0x{device_device:X})");
    println!("Device product: {device_product} (0x{device_product:X})");

    // 2. Fallback to device name
    if device_name
        .to_lowercase()
        .contains("QuietComfort 35 Series 2")
    {
        return Ok(Model::Qc35ii);
    }
    if device_name.to_lowercase().contains("SoundLink Color II") {
        return Ok(Model::SoundLinkColorII);
    }

    Ok(Model::Unknown)
}

impl BoseDevice {
    pub async fn connect(address: &str, model_override: Option<Model>) -> Result<Self, BoseError> {
        let session = Session::new().await?;
        let adapter = session.default_adapter().await?;
        let device_address: Address = address.parse().unwrap();

        let model = match model_override {
            Some(model) => model,
            None => detect_model(&adapter, device_address).await?,
        };

        if model == Model::Unknown {
            return Err(BoseError::InvalidInput(
                "Could not determine device model. Please specify it with the --model flag."
                    .to_string(),
            ));
        }

        let rfcomm_channel = 8;
        let socket_addr = SocketAddr::new(device_address, rfcomm_channel);
        let socket = Socket::new()?;
        let mut stream = socket.connect(socket_addr).await?;

        // Initial Handshake
        let init_send_bytes: [u8; 4] = [0x00, 0x01, 0x01, 0x00];
        let init_ack_bytes: [u8; 4] = [0x00, 0x01, 0x03, 0x05];
        let mut init_ack_buffer = vec![0; init_ack_bytes.len()];

        stream.write_all(&init_send_bytes).await?;
        stream.read_exact(&mut init_ack_buffer).await?;

        if init_ack_buffer != init_ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: init_ack_bytes.to_vec(),
                got: init_ack_buffer,
            });
        }

        let mut protocol_version_bytes = vec![0; 5];
        stream.read_exact(&mut protocol_version_bytes).await?;

        let firmware = detect_firmware(model, &protocol_version_bytes)?;

        Ok(BoseDevice { stream, firmware })
    }

    async fn execute_command(
        &mut self,
        send_bytes: &[u8],
        expected_ack: &[u8],
        response_len: usize,
    ) -> Result<Vec<u8>, BoseError> {
        self.stream.write_all(send_bytes).await?;

        let mut ack_buffer = vec![0; expected_ack.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != expected_ack {
            return Err(BoseError::AckMismatch {
                expected: expected_ack.to_vec(),
                got: ack_buffer,
            });
        }

        let mut response_buffer = vec![0; response_len];
        self.stream.read_exact(&mut response_buffer).await?;

        Ok(response_buffer)
    }

    pub async fn get_battery_level(&mut self) -> Result<BatteryInfo, BoseError> {
        let (send_bytes, ack_bytes) = self.firmware.get_battery_level_command();
        let response = self.execute_command(&send_bytes, &ack_bytes, 1).await?;
        parse_battery_level(&response)
    }
}

use crate::bose_api::BoseError;
use crate::bose_api::firmware::{Firmware, detect_firmware};
use crate::bose_api::operations::battery::{BatteryInfo, parse_battery_level};
use crate::bose_api::operations::device_id::{DeviceIdInfo, parse_device_id};
use crate::bose_api::operations::device_status::{
    AutoOff, DeviceStatus, NoiseCancelling, PromptLanguage,
};
use crate::bose_api::operations::firmware_version::{FirmwareVersionInfo, parse_firmware_version};
use crate::bose_api::operations::paired_devices::{PairedDeviceInfo, parse_paired_devices};
use crate::bose_api::operations::serial_number::{SerialNumberInfo, parse_serial_number};
use bluer::rfcomm::{Socket, SocketAddr, Stream};
use bluer::{Adapter, Address, Session};
use std::str::FromStr;
use std::time::Duration;
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

    // 1. Attempt to detect model using Vendor and Product IDs.
    if let Some(modalias) = device.modalias().await? {
        // The `vendor` 158 (0x009E) is "Bose Corporation".
        if modalias.vendor == 158 {
            match modalias.product {
                // Product ID 16416 (0x4020) is "QC35 II".
                16416 => return Ok(Model::Qc35ii),
                // Product ID 16397 (0x400D) is "SoundLink Color II".
                16397 => return Ok(Model::SoundLinkColorII),
                _ => {
                    // Known Bose vendor, but unknown product. Fallback to device name.
                }
            }
        }
    }

    // 2. Fallback to device name if Device ID detection fails.
    let device_name = device.name().await?.unwrap_or_default();
    if device_name.to_lowercase().contains("quietcomfort 35 2") {
        return Ok(Model::Qc35ii);
    }
    if device_name.to_lowercase().contains("soundlink color") {
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

    pub async fn get_battery_level(&mut self) -> Result<BatteryInfo, BoseError> {
        let (send_bytes, ack_bytes) = self.firmware.get_battery_level_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut response_buffer = vec![0; 1];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_battery_level(&response_buffer)
    }

    async fn read_value(&mut self, ack_len: usize) -> Result<Vec<u8>, BoseError> {
        let mut len_buffer = vec![0; ack_len];
        self.stream.read_exact(&mut len_buffer).await?;
        let len = len_buffer[ack_len - 1] as usize;

        let mut value_buffer = vec![0; len];
        self.stream.read_exact(&mut value_buffer).await?;
        Ok(value_buffer)
    }

    pub async fn get_paired_devices(&mut self) -> Result<PairedDeviceInfo, BoseError> {
        let (send_bytes, ack_bytes) = self.firmware.get_paired_devices_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut num_devices_buffer = [0; 1];
        self.stream.read_exact(&mut num_devices_buffer).await?;
        let num_devices = (num_devices_buffer[0] / 6) as usize;

        let mut connected_buffer = [0; 1];
        self.stream.read_exact(&mut connected_buffer).await?;
        let connected = connected_buffer[0];

        let mut addresses_buffer = vec![0; num_devices * 6];
        self.stream.read_exact(&mut addresses_buffer).await?;

        let mut response_buffer = Vec::new();
        response_buffer.push(num_devices_buffer[0]);
        response_buffer.push(connected);
        response_buffer.extend_from_slice(&addresses_buffer);

        parse_paired_devices(&response_buffer)
    }

    pub async fn get_serial_number(&mut self) -> Result<SerialNumberInfo, BoseError> {
        let (send_bytes, ack_bytes) = self.firmware.get_serial_number_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut len_buffer = [0; 1];
        self.stream.read_exact(&mut len_buffer).await?;
        let len = len_buffer[0] as usize;

        let mut response_buffer = vec![0; len];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_serial_number(&response_buffer)
    }

    pub async fn get_firmware_version(&mut self) -> Result<FirmwareVersionInfo, BoseError> {
        let (send_bytes, ack_bytes) = self.firmware.get_firmware_version_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut response_buffer = vec![0; 5];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_firmware_version(&response_buffer)
    }

    pub async fn get_device_id(&mut self) -> Result<DeviceIdInfo, BoseError> {
        let (send_bytes, ack_bytes) = self.firmware.get_device_id_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut response_buffer = vec![0; 3];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_device_id(&response_buffer)
    }

    pub async fn get_device_status(&mut self) -> Result<DeviceStatus, BoseError> {
        let (send_bytes, ack_bytes) = self.firmware.get_device_status_command();
        let response_timeout = Duration::from_secs(5);

        tokio::time::timeout(response_timeout, async {
            self.stream.write_all(&send_bytes).await?;

            let mut ack_buffer = vec![0; ack_bytes.len()];
            self.stream.read_exact(&mut ack_buffer).await?;

            if ack_buffer != ack_bytes {
                return Err(BoseError::AckMismatch {
                    expected: ack_bytes.to_vec(),
                    got: ack_buffer,
                });
            }

            let ack_len = ack_bytes.len();

            let name_bytes = self.read_value(ack_len).await?;
            let name = String::from_utf8_lossy(&name_bytes)
                .trim_start_matches('\u{0}')
                .to_string();

            let language_bytes = self.read_value(ack_len).await?;
            let language = language_bytes
                .first()
                .map_or(PromptLanguage::Unknown, |&v| PromptLanguage::from(v));

            let auto_off_bytes = self.read_value(ack_len).await?;
            let auto_off = auto_off_bytes
                .first()
                .map_or(AutoOff::Unknown, |&v| AutoOff::from(v));

            let nc_bytes = self.read_value(ack_len).await?;
            let noise_cancelling = nc_bytes
                .first()
                .map_or(NoiseCancelling::Unknown, |&v| NoiseCancelling::from(v));

            Ok(DeviceStatus {
                name,
                language,
                auto_off,
                noise_cancelling,
            })
        })
        .await?
    }
}

use crate::bose_api::BoseError;
use crate::bose_api::firmware::{Firmware, detect_firmware};
use crate::bose_api::operations::battery::{BatteryInfo, parse_battery_level};
use crate::bose_api::operations::connect_device::ConnectDeviceInfo;
use crate::bose_api::operations::device_id::{DeviceIdInfo, parse_device_id};
use crate::bose_api::operations::device_information::{
    DeviceInformationInfo, parse_device_information,
};
use crate::bose_api::operations::device_status::{
    AutoOff, DeviceStatus, NoiseCancelling, PromptLanguage,
};
use crate::bose_api::operations::disconnect_device::DisconnectDeviceInfo;
use crate::bose_api::operations::firmware_version::{FirmwareVersionInfo, parse_firmware_version};
use crate::bose_api::operations::init_connection::{InitConnectionInfo, parse_init_connection};
use crate::bose_api::operations::paired_devices::{PairedDeviceInfo, parse_paired_devices};
use crate::bose_api::operations::pairing::Pairing;
use crate::bose_api::operations::self_voice::SelfVoice;
use crate::bose_api::operations::serial_number::{SerialNumberInfo, parse_serial_number};
use crate::bose_api::operations::voice_prompts::VoicePrompts;
use crate::cli::AutoOffValue;
use bluer::rfcomm::{Socket, SocketAddr, Stream};
use bluer::{Adapter, Address, Device, Session};
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
    let device: Device = adapter.device(address)?;

    // 1. Attempt to detect model using Vendor and Product IDs.
    if let Some(mod_alias) = device.modalias().await? {
        // The `vendor` 158 (0x009E) is "Bose Corporation".
        if mod_alias.vendor == 158 {
            match mod_alias.product {
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
    let device_name: String = device.name().await?.unwrap_or_default();
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
        let session: Session = Session::new().await?;
        let adapter: Adapter = session.default_adapter().await?;
        let device_address: Address = address.parse().unwrap();

        let model: Model = match model_override {
            Some(model) => model,
            None => detect_model(&adapter, device_address).await?,
        };

        if model == Model::Unknown {
            return Err(BoseError::InvalidInput(
                "Could not determine device model. Please specify it with the --model flag."
                    .to_string(),
            ));
        }

        let rfcomm_channel: u8 = 8;
        let socket_addr: SocketAddr = SocketAddr::new(device_address, rfcomm_channel);
        let socket: Socket = Socket::new()?;
        let mut stream: Stream = socket.connect(socket_addr).await?;

        // Initial Handshake
        let init_send_bytes: [u8; 4] = [0x00, 0x01, 0x01, 0x00];
        let init_ack_bytes: [u8; 4] = [0x00, 0x01, 0x03, 0x05];
        let mut init_ack_buffer: Vec<u8> = vec![0; init_ack_bytes.len()];

        stream.write_all(&init_send_bytes).await?;
        stream.read_exact(&mut init_ack_buffer).await?;

        if init_ack_buffer != init_ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: init_ack_bytes.to_vec(),
                got: init_ack_buffer,
            });
        }

        let mut protocol_version_bytes: Vec<u8> = vec![0; 5];
        stream.read_exact(&mut protocol_version_bytes).await?;

        let firmware: Box<dyn Firmware> = detect_firmware(model, &protocol_version_bytes)?;

        Ok(BoseDevice { stream, firmware })
    }

    pub async fn init_connection(&mut self) -> Result<InitConnectionInfo, BoseError> {
        let (send_bytes, ack_bytes): ([u8; 4], [u8; 4]) = self.firmware.init_connection_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut response_buffer: Vec<u8> = vec![0; 5];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_init_connection(&response_buffer)
    }

    pub async fn get_battery_level(&mut self) -> Result<BatteryInfo, BoseError> {
        let (send_bytes, ack_bytes): ([u8; 4], [u8; 4]) = self.firmware.get_battery_level_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut response_buffer: Vec<u8> = vec![0; 1];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_battery_level(&response_buffer)
    }

    async fn read_value(&mut self, ack_len: usize) -> Result<Vec<u8>, BoseError> {
        let mut len_buffer: Vec<u8> = vec![0; ack_len];
        self.stream.read_exact(&mut len_buffer).await?;
        let len: usize = len_buffer[ack_len - 1] as usize;

        let mut value_buffer: Vec<u8> = vec![0; len];
        self.stream.read_exact(&mut value_buffer).await?;
        Ok(value_buffer)
    }

    pub async fn get_device_information(
        &mut self,
        address: &str,
    ) -> Result<DeviceInformationInfo, BoseError> {
        let (send_prefix, ack_bytes): ([u8; 4], [u8; 3]) =
            self.firmware.get_device_information_command();
        let mut address_bytes: [u8; 6] = [0u8; 6];
        hex::decode_to_slice(address.replace(":", ""), &mut address_bytes)?;

        let mut send_packet: [u8; 10] = [0u8; 10];
        send_packet[0..4].copy_from_slice(&send_prefix);
        send_packet[4..10].copy_from_slice(&address_bytes);

        self.stream.write_all(&send_packet).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut len_buffer: [u8; 1] = [0; 1];
        self.stream.read_exact(&mut len_buffer).await?;
        let len: usize = len_buffer[0] as usize;

        let mut response_buffer: Vec<u8> = vec![0; len];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_device_information(&response_buffer)
    }

    pub async fn get_paired_devices(&mut self) -> Result<PairedDeviceInfo, BoseError> {
        let (send_bytes, ack_bytes): ([u8; 4], [u8; 3]) =
            self.firmware.get_paired_devices_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut num_devices_buffer: [u8; 1] = [0; 1];
        self.stream.read_exact(&mut num_devices_buffer).await?;
        let num_devices: usize = (num_devices_buffer[0] / 6) as usize;

        let mut connected_buffer: [u8; 1] = [0; 1];
        self.stream.read_exact(&mut connected_buffer).await?;
        let connected: u8 = connected_buffer[0];

        let mut addresses_buffer: Vec<u8> = vec![0; num_devices * 6];
        self.stream.read_exact(&mut addresses_buffer).await?;

        let mut response_buffer: Vec<u8> = Vec::new();
        response_buffer.push(num_devices_buffer[0]);
        response_buffer.push(connected);
        response_buffer.extend_from_slice(&addresses_buffer);

        parse_paired_devices(&response_buffer)
    }

    pub async fn get_serial_number(&mut self) -> Result<SerialNumberInfo, BoseError> {
        let (send_bytes, ack_bytes): ([u8; 4], [u8; 3]) = self.firmware.get_serial_number_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut len_buffer: [u8; 1] = [0; 1];
        self.stream.read_exact(&mut len_buffer).await?;
        let len: usize = len_buffer[0] as usize;

        let mut response_buffer: Vec<u8> = vec![0; len];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_serial_number(&response_buffer)
    }

    pub async fn get_firmware_version(&mut self) -> Result<FirmwareVersionInfo, BoseError> {
        let (send_bytes, ack_bytes): ([u8; 4], [u8; 4]) =
            self.firmware.get_firmware_version_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut response_buffer: Vec<u8> = vec![0; 5];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_firmware_version(&response_buffer)
    }

    pub async fn get_device_id(&mut self) -> Result<DeviceIdInfo, BoseError> {
        let (send_bytes, ack_bytes): ([u8; 4], [u8; 4]) = self.firmware.get_device_id_command();
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut response_buffer: Vec<u8> = vec![0; 3];
        self.stream.read_exact(&mut response_buffer).await?;

        parse_device_id(&response_buffer)
    }

    pub async fn get_device_status(&mut self) -> Result<DeviceStatus, BoseError> {
        let (send_bytes, ack_bytes): ([u8; 4], [u8; 4]) = self.firmware.get_device_status_command();
        let response_timeout: Duration = Duration::from_secs(5);

        tokio::time::timeout(response_timeout, async {
            self.stream.write_all(&send_bytes).await?;

            let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
            self.stream.read_exact(&mut ack_buffer).await?;

            if ack_buffer != ack_bytes {
                return Err(BoseError::AckMismatch {
                    expected: ack_bytes.to_vec(),
                    got: ack_buffer,
                });
            }

            let ack_len: usize = ack_bytes.len();

            let name_bytes: Vec<u8> = self.read_value(ack_len).await?;
            let name: String = String::from_utf8_lossy(&name_bytes)
                .trim_start_matches('\u{0}')
                .to_string();

            let language_bytes: Vec<u8> = self.read_value(ack_len).await?;
            let language: PromptLanguage = language_bytes
                .first()
                .map_or(PromptLanguage::Unknown, |&v| PromptLanguage::from(v));

            let voice_prompts: VoicePrompts = if language == PromptLanguage::Disable {
                VoicePrompts::Off
            } else {
                VoicePrompts::On
            };

            let auto_off_bytes: Vec<u8> = self.read_value(ack_len).await?;
            let auto_off: AutoOff = auto_off_bytes
                .first()
                .map_or(AutoOff::Unknown, |&v| AutoOff::from(v));

            let nc_bytes: Vec<u8> = self.read_value(ack_len).await?;
            let noise_cancelling: NoiseCancelling = nc_bytes
                .first()
                .map_or(NoiseCancelling::Unknown, |&v| NoiseCancelling::from(v));

            Ok(DeviceStatus {
                name,
                language,
                auto_off,
                noise_cancelling,
                voice_prompts,
            })
        })
        .await?
    }

    pub async fn set_auto_off(&mut self, value: AutoOffValue) -> Result<(), BoseError> {
        let (send_bytes, ack_bytes): ([u8; 5], [u8; 5]) =
            self.firmware.set_auto_off_command(value.into());
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        Ok(())
    }

    pub async fn set_noise_cancelling(&mut self, value: NoiseCancelling) -> Result<(), BoseError> {
        let (send_bytes, ack_bytes): ([u8; 5], [u8; 6]) =
            self.firmware.set_noise_cancelling_command(value.into());
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        Ok(())
    }

    pub async fn set_prompt_language(&mut self, value: PromptLanguage) -> Result<(), BoseError> {
        let (send_bytes, ack_bytes): ([u8; 5], [u8; 9]) =
            self.firmware.set_prompt_language_command(value.into());
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        Ok(())
    }

    pub async fn set_self_voice(&mut self, value: SelfVoice) -> Result<(), BoseError> {
        let (send_bytes, ack_bytes): ([u8; 7], [u8; 7]) =
            self.firmware.set_self_voice_command(value.into());
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        Ok(())
    }

    pub async fn set_name(&mut self, name: &str) -> Result<(), BoseError> {
        let name_bytes: Vec<u8> = name.as_bytes().to_vec();
        let (send_bytes, ack_bytes): (Vec<u8>, [u8; 5]) =
            self.firmware.set_name_command(&name_bytes);
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        Ok(())
    }

    pub async fn set_voice_prompts(&mut self, value: VoicePrompts) -> Result<(), BoseError> {
        let (send_bytes, ack_bytes): ([u8; 5], [u8; 9]) =
            self.firmware.set_prompt_language_command(value.into());
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer[0..4] != ack_bytes[0..4] && ack_buffer[5..] != ack_bytes[5..] {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        Ok(())
    }

    pub async fn set_pairing(&mut self, value: Pairing) -> Result<(), BoseError> {
        let (send_bytes, ack_bytes): ([u8; 5], [u8; 5]) =
            self.firmware.set_pairing_command(value.into());
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        Ok(())
    }

    pub async fn connect_device(&mut self, address: &str) -> Result<ConnectDeviceInfo, BoseError> {
        let (send_bytes, ack_bytes) = self.firmware.connect_device_command(address)?;
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        let mut response_buffer: Vec<u8> = vec![0; 6];
        self.stream.read_exact(&mut response_buffer).await?;

        Ok(ConnectDeviceInfo {
            address: address.to_string(),
            status: "connected".to_string(),
        })
    }

    pub async fn disconnect_device(
        &mut self,
        address: &str,
    ) -> Result<DisconnectDeviceInfo, BoseError> {
        let (send_bytes, ack_bytes) = self.firmware.disconnect_device_command(address)?;
        self.stream.write_all(&send_bytes).await?;

        let mut ack_buffer: Vec<u8> = vec![0; ack_bytes.len()];
        self.stream.read_exact(&mut ack_buffer).await?;

        if ack_buffer != ack_bytes {
            return Err(BoseError::AckMismatch {
                expected: ack_bytes.to_vec(),
                got: ack_buffer,
            });
        }

        Ok(DisconnectDeviceInfo {
            address: address.to_string(),
            status: "disconnected".to_string(),
        })
    }
}

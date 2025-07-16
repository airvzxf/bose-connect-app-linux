use crate::bose_api::BoseError;
use crate::bose_api::device::Model;

pub mod baywolf;
pub mod isaac;

use baywolf::BayWolfFirmware;
use isaac::IsaacFirmware;

pub trait Firmware {
    fn get_battery_level_command(&self) -> ([u8; 4], [u8; 4]);
    fn get_device_status_command(&self) -> ([u8; 4], [u8; 4]);
    fn get_device_id_command(&self) -> ([u8; 4], [u8; 4]);
    fn get_firmware_version_command(&self) -> ([u8; 4], [u8; 4]);
    fn get_serial_number_command(&self) -> ([u8; 4], [u8; 3]);
    fn get_paired_devices_command(&self) -> ([u8; 4], [u8; 3]);
    fn get_device_information_command(&self) -> ([u8; 4], [u8; 3]);
    fn set_auto_off_command(&self, value: u8) -> ([u8; 5], [u8; 5]);
    fn set_noise_cancelling_command(&self, value: u8) -> ([u8; 5], [u8; 6]);
    fn set_prompt_language_command(&self, value: u8) -> ([u8; 5], [u8; 9]);
    fn set_self_voice_command(&self, value: u8) -> ([u8; 7], [u8; 7]);
    fn set_name_command(&self, name_bytes: &[u8]) -> (Vec<u8>, [u8; 5]);
}

pub fn detect_firmware(
    model: Model,
    _protocol_version_bytes: &[u8],
) -> Result<Box<dyn Firmware>, BoseError> {
    match model {
        Model::Qc35ii => Ok(Box::new(BayWolfFirmware)),
        Model::SoundLinkColorII => Ok(Box::new(IsaacFirmware)),
        Model::Unknown => Err(BoseError::UnknownFirmware("Unknown model".to_string())),
    }
}

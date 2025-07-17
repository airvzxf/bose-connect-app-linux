use crate::bose_api::BoseError;
use crate::bose_api::firmware::Firmware;

pub struct IsaacFirmware;

impl Firmware for IsaacFirmware {
    fn init_connection_command(&self) -> ([u8; 4], [u8; 4]) {
        // Isaac specific byte codes for init connection (assuming same as BayWolf for now)
        ([0x00, 0x01, 0x01, 0x00], [0x00, 0x01, 0x03, 0x05])
    }

    fn get_battery_level_command(&self) -> ([u8; 4], [u8; 4]) {
        // Isaac specific byte codes for getting battery level (assuming same as BayWolf for now)
        ([0x02, 0x02, 0x01, 0x00], [0x02, 0x02, 0x03, 0x01])
    }

    fn get_device_status_command(&self) -> ([u8; 4], [u8; 4]) {
        // Isaac specific byte codes for getting device status (assuming same as BayWolf for now)
        ([0x01, 0x01, 0x05, 0x00], [0x01, 0x01, 0x07, 0x00])
    }

    fn get_device_id_command(&self) -> ([u8; 4], [u8; 4]) {
        // Isaac specific byte codes for getting device ID (assuming same as BayWolf for now)
        ([0x00, 0x03, 0x01, 0x00], [0x00, 0x03, 0x03, 0x03])
    }

    fn get_firmware_version_command(&self) -> ([u8; 4], [u8; 4]) {
        // Isaac specific byte codes for getting firmware version (assuming same as BayWolf for now)
        ([0x00, 0x05, 0x01, 0x00], [0x00, 0x05, 0x03, 0x05])
    }

    fn get_serial_number_command(&self) -> ([u8; 4], [u8; 3]) {
        // Isaac specific byte codes for getting serial number (assuming same as BayWolf for now)
        ([0x00, 0x07, 0x01, 0x00], [0x00, 0x07, 0x03])
    }

    fn get_paired_devices_command(&self) -> ([u8; 4], [u8; 3]) {
        // Isaac specific byte codes for getting paired devices (assuming same as BayWolf for now)
        ([0x04, 0x04, 0x01, 0x00], [0x04, 0x04, 0x03])
    }

    fn get_device_information_command(&self) -> ([u8; 4], [u8; 3]) {
        // Isaac specific byte codes for getting device information (assuming same as BayWolf for now)
        ([0x04, 0x05, 0x01, 0x06], [0x04, 0x05, 0x03])
    }

    fn set_auto_off_command(&self, value: u8) -> ([u8; 5], [u8; 5]) {
        // Isaac specific byte codes for setting auto off (assuming same as BayWolf for now)
        (
            [0x01, 0x04, 0x02, 0x01, value],
            [0x01, 0x04, 0x03, 0x01, value],
        )
    }

    fn set_noise_cancelling_command(&self, value: u8) -> ([u8; 5], [u8; 6]) {
        // Isaac specific byte codes for setting noise-cancelling (assuming same as BayWolf for now)
        (
            [0x01, 0x06, 0x02, 0x01, value],
            [0x01, 0x06, 0x03, 0x02, value, 0x0b],
        )
    }

    fn set_prompt_language_command(&self, value: u8) -> ([u8; 5], [u8; 9]) {
        // Isaac specific byte codes for setting prompt language (assuming same as BayWolf for now)
        (
            [0x01, 0x03, 0x02, 0x01, value],
            [0x01, 0x03, 0x03, 0x05, value, 0x00, 0x04, 0xcf, 0xde],
        )
    }

    fn set_self_voice_command(&self, value: u8) -> ([u8; 7], [u8; 7]) {
        // Isaac specific byte codes for setting self voice (assuming same as BayWolf for now)
        (
            [0x01, 0x0b, 0x02, 0x02, 0x01, value, 0x38],
            [0x01, 0x0b, 0x03, 0x03, 0x01, value, 0x0f],
        )
    }

    fn set_name_command(&self, name_bytes: &[u8]) -> (Vec<u8>, [u8; 5]) {
        // Isaac specific byte codes for setting name (assuming same as BayWolf for now)
        let name_size: u8 = name_bytes.len() as u8;
        let mut send_bytes: Vec<u8> = vec![0x01, 0x02, 0x02, name_size];
        send_bytes.extend_from_slice(name_bytes);
        (send_bytes, [0x01, 0x02, 0x03, name_size + 1, 0x00])
    }

    fn set_pairing_command(&self, value: u8) -> ([u8; 5], [u8; 5]) {
        // Isaac specific byte codes for setting pairing (assuming same as BayWolf for now)
        (
            [0x04, 0x08, 0x05, 0x01, value],
            [0x04, 0x08, 0x06, 0x01, value],
        )
    }

    fn connect_device_command(&self, address: &str) -> Result<(Vec<u8>, [u8; 4]), BoseError> {
        // Isaac specific byte codes for connecting a device (assuming same as BayWolf for now)
        let mut address_bytes: [u8; 6] = [0u8; 6];
        hex::decode_to_slice(address.replace(":", ""), &mut address_bytes)?;
        let mut send_bytes: Vec<u8> = vec![0x04, 0x01, 0x05, 0x07, 0x00];
        send_bytes.extend_from_slice(&address_bytes);
        Ok((send_bytes, [0x04, 0x01, 0x07, 0x06]))
    }

    fn disconnect_device_command(&self, address: &str) -> Result<(Vec<u8>, [u8; 4]), BoseError> {
        // Isaac specific byte codes for disconnecting a device (assuming same as BayWolf for now)
        let mut address_bytes: [u8; 6] = [0u8; 6];
        hex::decode_to_slice(address.replace(":", ""), &mut address_bytes)?;
        let mut send_bytes: Vec<u8> = vec![0x04, 0x02, 0x05, 0x06];
        send_bytes.extend_from_slice(&address_bytes);
        Ok((send_bytes, [0x04, 0x02, 0x07, 0x07]))
    }
}

use crate::bose_api::firmware::Firmware;

pub struct BayWolfFirmware;

impl Firmware for BayWolfFirmware {
    fn init_connection_command(&self) -> ([u8; 4], [u8; 4]) {
        // BayWolf specific byte codes for init connection
        ([0x00, 0x01, 0x01, 0x00], [0x00, 0x01, 0x03, 0x05])
    }

    fn get_battery_level_command(&self) -> ([u8; 4], [u8; 4]) {
        // BayWolf specific byte codes for getting battery level
        ([0x02, 0x02, 0x01, 0x00], [0x02, 0x02, 0x03, 0x01])
    }

    fn get_device_status_command(&self) -> ([u8; 4], [u8; 4]) {
        // BayWolf specific byte codes for getting device status
        ([0x01, 0x01, 0x05, 0x00], [0x01, 0x01, 0x07, 0x00])
    }

    fn get_device_id_command(&self) -> ([u8; 4], [u8; 4]) {
        // BayWolf specific byte codes for getting device ID
        ([0x00, 0x03, 0x01, 0x00], [0x00, 0x03, 0x03, 0x03])
    }

    fn get_firmware_version_command(&self) -> ([u8; 4], [u8; 4]) {
        // BayWolf specific byte codes for getting firmware version
        ([0x00, 0x05, 0x01, 0x00], [0x00, 0x05, 0x03, 0x05])
    }

    fn get_serial_number_command(&self) -> ([u8; 4], [u8; 3]) {
        // BayWolf specific byte codes for getting serial number
        ([0x00, 0x07, 0x01, 0x00], [0x00, 0x07, 0x03])
    }

    fn get_paired_devices_command(&self) -> ([u8; 4], [u8; 3]) {
        // BayWolf specific byte codes for getting paired devices
        ([0x04, 0x04, 0x01, 0x00], [0x04, 0x04, 0x03])
    }

    fn get_device_information_command(&self) -> ([u8; 4], [u8; 3]) {
        // BayWolf specific byte codes for getting device information
        ([0x04, 0x05, 0x01, 0x06], [0x04, 0x05, 0x03])
    }

    fn set_auto_off_command(&self, value: u8) -> ([u8; 5], [u8; 5]) {
        // BayWolf specific byte codes for setting auto off
        (
            [0x01, 0x04, 0x02, 0x01, value],
            [0x01, 0x04, 0x03, 0x01, value],
        )
    }

    fn set_noise_cancelling_command(&self, value: u8) -> ([u8; 5], [u8; 6]) {
        // BayWolf specific byte codes for setting noise-cancelling
        (
            [0x01, 0x06, 0x02, 0x01, value],
            [0x01, 0x06, 0x03, 0x02, value, 0x0b],
        )
    }

    fn set_prompt_language_command(&self, value: u8) -> ([u8; 5], [u8; 9]) {
        // BayWolf specific byte codes for setting prompt language
        (
            [0x01, 0x03, 0x02, 0x01, value],
            [0x01, 0x03, 0x03, 0x05, value, 0x00, 0x04, 0xcf, 0xde],
        )
    }

    fn set_self_voice_command(&self, value: u8) -> ([u8; 7], [u8; 7]) {
        // BayWolf specific byte codes for setting self voice
        (
            [0x01, 0x0b, 0x02, 0x02, 0x01, value, 0x38],
            [0x01, 0x0b, 0x03, 0x03, 0x01, value, 0x0f],
        )
    }

    fn set_name_command(&self, name_bytes: &[u8]) -> (Vec<u8>, [u8; 5]) {
        // BayWolf specific byte codes for setting name
        let name_size: u8 = name_bytes.len() as u8;
        let mut send_bytes: Vec<u8> = vec![0x01, 0x02, 0x02, name_size];
        send_bytes.extend_from_slice(name_bytes);
        (send_bytes, [0x01, 0x02, 0x03, name_size + 1, 0x00])
    }

    fn set_pairing_command(&self, value: u8) -> ([u8; 5], [u8; 5]) {
        // BayWolf specific byte codes for setting pairing
        (
            [0x04, 0x08, 0x05, 0x01, value],
            [0x04, 0x08, 0x06, 0x01, value],
        )
    }
}

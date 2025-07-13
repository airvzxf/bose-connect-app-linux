use crate::bose_api::firmware::Firmware;

pub struct IsaacFirmware;

impl Firmware for IsaacFirmware {
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
}

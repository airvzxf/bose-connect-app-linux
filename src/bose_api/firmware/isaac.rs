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
}

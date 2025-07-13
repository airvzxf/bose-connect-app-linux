use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DeviceStatus {
    Current,
    Connected,
    Disconnected,
    Unknown,
}

impl From<u8> for DeviceStatus {
    fn from(val: u8) -> Self {
        match val {
            0x03 => DeviceStatus::Current,
            0x01 => DeviceStatus::Connected,
            0x00 => DeviceStatus::Disconnected,
            _ => DeviceStatus::Unknown,
        }
    }
}

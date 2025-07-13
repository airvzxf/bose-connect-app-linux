use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DevicesConnected {
    One,
    Two,
    Unknown,
}

impl DevicesConnected {
    pub fn to_u8(self) -> u8 {
        match self {
            DevicesConnected::One => 1,
            DevicesConnected::Two => 2,
            DevicesConnected::Unknown => 0,
        }
    }
}

impl From<u8> for DevicesConnected {
    fn from(val: u8) -> Self {
        match val {
            0x01 => DevicesConnected::One,
            0x03 => DevicesConnected::Two,
            _ => DevicesConnected::Unknown,
        }
    }
}

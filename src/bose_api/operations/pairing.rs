use serde::{Deserialize, Serialize};

#[derive(
    Serialize, Deserialize, Debug, PartialEq, Eq, Copy, Clone, PartialOrd, Ord, clap::ValueEnum,
)]
pub enum Pairing {
    #[clap(name = "off")]
    Off,
    #[clap(name = "on")]
    On,
}

impl From<u8> for Pairing {
    fn from(value: u8) -> Self {
        match value {
            0x00 => Pairing::Off,
            0x01 => Pairing::On,
            _ => Pairing::Off, // Default to off
        }
    }
}

impl From<Pairing> for u8 {
    fn from(value: Pairing) -> Self {
        match value {
            Pairing::Off => 0x00,
            Pairing::On => 0x01,
        }
    }
}

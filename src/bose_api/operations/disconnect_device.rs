use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct DisconnectDeviceInfo {
    pub address: String,
    pub status: String,
}

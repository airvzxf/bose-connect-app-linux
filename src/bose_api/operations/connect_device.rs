use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ConnectDeviceInfo {
    pub address: String,
    pub status: String,
}

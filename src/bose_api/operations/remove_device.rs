use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct RemoveDeviceInfo {
    pub address: String,
    pub status: String,
}

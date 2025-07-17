use crate::bose_api::BoseError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ConnectDeviceInfo {
    pub address: String,
}

pub fn parse_connect_device_info(response: &[u8]) -> Result<ConnectDeviceInfo, BoseError> {
    if response.len() < 6 {
        return Err(BoseError::InvalidResponse);
    }

    let address: String = response
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<String>>()
        .join(":");

    Ok(ConnectDeviceInfo { address })
}

#[cfg(test)]
mod tests {
    use super::{ConnectDeviceInfo, parse_connect_device_info};
    use crate::bose_api::BoseError;

    #[test]
    fn test_connect_device_info_parsing() {
        let sample_response: Vec<u8> = vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let connect_device_info: ConnectDeviceInfo =
            parse_connect_device_info(&sample_response).unwrap();
        assert_eq!(
            connect_device_info,
            ConnectDeviceInfo {
                address: "11:22:33:44:55:66".to_string(),
            }
        );
    }

    #[test]
    fn test_connect_device_info_parsing_invalid() {
        let sample_response: Vec<u8> = vec![0x12];
        let result: Result<ConnectDeviceInfo, BoseError> =
            parse_connect_device_info(&sample_response);
        assert!(result.is_err());
    }
}

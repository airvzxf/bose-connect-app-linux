use crate::bose_api::BoseError;
use crate::bose_api::operations::device_status_info::DeviceStatus;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct DeviceInformationInfo {
    pub address: String,
    pub status: DeviceStatus,
    pub name: String,
}

pub fn parse_device_information(response: &[u8]) -> Result<DeviceInformationInfo, BoseError> {
    if response.len() < 8 {
        return Err(BoseError::InvalidResponse);
    }

    let address_bytes: &[u8] = &response[0..6];
    let address: String = address_bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<String>>()
        .join(":");

    let status: DeviceStatus = DeviceStatus::from(response[6]);
    let name: String = String::from_utf8_lossy(&response[9..]).to_string();

    Ok(DeviceInformationInfo {
        address,
        status,
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::{DeviceInformationInfo, parse_device_information};
    use crate::bose_api::BoseError;
    use crate::bose_api::operations::device_status_info::DeviceStatus;

    #[test]
    fn test_device_information_parsing() {
        let sample_response: Vec<u8> = vec![
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x01, 0x00, 0x00, 0x42, 0x6f, 0x73, 0x65, 0x20,
            0x51, 0x43, 0x33, 0x35,
        ];
        let device_information_info: DeviceInformationInfo =
            parse_device_information(&sample_response).unwrap();
        assert_eq!(
            device_information_info,
            DeviceInformationInfo {
                address: "11:22:33:44:55:66".to_string(),
                status: DeviceStatus::Connected,
                name: "Bose QC35".to_string(),
            }
        );
    }

    #[test]
    fn test_device_information_parsing_invalid() {
        let sample_response: Vec<u8> = vec![0x12];
        let result: Result<DeviceInformationInfo, BoseError> =
            parse_device_information(&sample_response);
        assert!(result.is_err());
    }
}

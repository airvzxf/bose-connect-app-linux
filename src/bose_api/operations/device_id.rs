use crate::bose_api::BoseError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct DeviceIdInfo {
    pub id: u16,
    pub index: u8,
}

pub fn parse_device_id(response: &[u8]) -> Result<DeviceIdInfo, BoseError> {
    if response.len() < 3 {
        return Err(BoseError::InvalidResponse);
    }
    let id = u16::from_be_bytes([response[0], response[1]]);
    let index = response[2];
    Ok(DeviceIdInfo { id, index })
}

#[cfg(test)]
mod tests {
    use super::{DeviceIdInfo, parse_device_id};

    #[test]
    fn test_device_id_parsing() {
        let sample_response = vec![0x12, 0x34, 0x56];
        let device_id_info = parse_device_id(&sample_response).unwrap();
        assert_eq!(
            device_id_info,
            DeviceIdInfo {
                id: 0x1234,
                index: 0x56
            }
        );
    }

    #[test]
    fn test_device_id_parsing_invalid() {
        let sample_response = vec![0x12, 0x34];
        let result = parse_device_id(&sample_response);
        assert!(result.is_err());
    }
}

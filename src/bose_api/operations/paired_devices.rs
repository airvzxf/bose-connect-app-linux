use crate::bose_api::BoseError;
use crate::bose_api::operations::devices_connected::DevicesConnected;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct PairedDeviceInfo {
    pub addresses: Vec<String>,
    pub num_devices: usize,
    pub connected: u8,
}

pub fn parse_paired_devices(response: &[u8]) -> Result<PairedDeviceInfo, BoseError> {
    if response.len() < 2 {
        return Err(BoseError::InvalidResponse);
    }

    let num_devices_byte: u8 = response[0];
    let num_devices: usize = (num_devices_byte / 6) as usize;
    let connected: u8 = DevicesConnected::from(response[1]).to_u8();

    let mut addresses: Vec<String> = Vec::new();
    for i in 0..num_devices {
        let start: usize = 2 + i * 6;
        let end: usize = start + 6;
        if response.len() < end {
            return Err(BoseError::InvalidResponse);
        }
        let address_bytes: &[u8] = &response[start..end];
        let address: String = address_bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<String>>()
            .join(":");
        addresses.push(address);
    }

    Ok(PairedDeviceInfo {
        addresses,
        num_devices,
        connected,
    })
}

#[cfg(test)]
mod tests {
    use super::{PairedDeviceInfo, parse_paired_devices};
    use crate::bose_api::BoseError;

    #[test]
    fn test_paired_devices_parsing() {
        let sample_response: Vec<u8> = vec![
            12, 3, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC,
        ];
        let paired_device_info: PairedDeviceInfo = parse_paired_devices(&sample_response).unwrap();
        assert_eq!(
            paired_device_info,
            PairedDeviceInfo {
                addresses: vec![
                    "11:22:33:44:55:66".to_string(),
                    "77:88:99:AA:BB:CC".to_string(),
                ],
                num_devices: 2,
                connected: 2,
            }
        );
    }

    #[test]
    fn test_paired_devices_parsing_invalid() {
        let sample_response: Vec<u8> = vec![0x12];
        let result: Result<PairedDeviceInfo, BoseError> = parse_paired_devices(&sample_response);
        assert!(result.is_err());
    }
}

use crate::bose_api::BoseError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct FirmwareVersionInfo {
    pub version: String,
}

pub fn parse_firmware_version(response: &[u8]) -> Result<FirmwareVersionInfo, BoseError> {
    if response.is_empty() {
        return Err(BoseError::InvalidResponse);
    }
    let version = String::from_utf8_lossy(response).to_string();
    Ok(FirmwareVersionInfo { version })
}

#[cfg(test)]
mod tests {
    use super::{FirmwareVersionInfo, parse_firmware_version};

    #[test]
    fn test_firmware_version_parsing() {
        let sample_response = vec![0x31, 0x2e, 0x32, 0x2e, 0x33];
        let firmware_version_info = parse_firmware_version(&sample_response).unwrap();
        assert_eq!(
            firmware_version_info,
            FirmwareVersionInfo {
                version: "1.2.3".to_string()
            }
        );
    }

    #[test]
    fn test_firmware_version_parsing_empty() {
        let sample_response = vec![];
        let result = parse_firmware_version(&sample_response);
        assert!(result.is_err());
    }
}

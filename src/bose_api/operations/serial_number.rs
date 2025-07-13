use crate::bose_api::BoseError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct SerialNumberInfo {
    pub serial_number: String,
}

pub fn parse_serial_number(response: &[u8]) -> Result<SerialNumberInfo, BoseError> {
    if response.is_empty() {
        return Err(BoseError::InvalidResponse);
    }
    let serial_number = String::from_utf8_lossy(response).to_string();
    Ok(SerialNumberInfo { serial_number })
}

#[cfg(test)]
mod tests {
    use super::{SerialNumberInfo, parse_serial_number};

    #[test]
    fn test_serial_number_parsing() {
        let sample_response = vec![0x31, 0x32, 0x33, 0x34, 0x35];
        let serial_number_info = parse_serial_number(&sample_response).unwrap();
        assert_eq!(
            serial_number_info,
            SerialNumberInfo {
                serial_number: "12345".to_string()
            }
        );
    }

    #[test]
    fn test_serial_number_parsing_empty() {
        let sample_response = vec![];
        let result = parse_serial_number(&sample_response);
        assert!(result.is_err());
    }
}

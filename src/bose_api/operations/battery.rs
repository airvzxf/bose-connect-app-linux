use crate::bose_api::BoseError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct BatteryInfo {
    pub level: u8,
}

pub fn parse_battery_level(response: &[u8]) -> Result<BatteryInfo, BoseError> {
    if response.is_empty() {
        return Err(BoseError::InvalidResponse);
    }
    Ok(BatteryInfo { level: response[0] })
}

#[cfg(test)]
mod tests {
    use super::{BatteryInfo, parse_battery_level};

    #[test]
    fn test_battery_level_parsing() {
        let sample_response = vec![90];
        let battery_info = parse_battery_level(&sample_response).unwrap();
        assert_eq!(battery_info, BatteryInfo { level: 90 });
    }

    #[test]
    fn test_battery_level_parsing_empty() {
        let sample_response = vec![];
        let result = parse_battery_level(&sample_response);
        assert!(result.is_err());
    }

    #[test]
    fn test_battery_level_parsing_max_value() {
        let sample_response = vec![255];
        let battery_info = parse_battery_level(&sample_response).unwrap();
        assert_eq!(battery_info, BatteryInfo { level: 255 });
    }
}

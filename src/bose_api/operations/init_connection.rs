use crate::bose_api::BoseError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct InitConnectionInfo {
    pub protocol_version: String,
}

pub fn parse_init_connection(response: &[u8]) -> Result<InitConnectionInfo, BoseError> {
    if response.is_empty() {
        return Err(BoseError::InvalidResponse);
    }
    let protocol_version: String = String::from_utf8_lossy(response).to_string();
    Ok(InitConnectionInfo { protocol_version })
}

#[cfg(test)]
mod tests {
    use super::{InitConnectionInfo, parse_init_connection};
    use crate::bose_api::BoseError;

    #[test]
    fn test_init_connection_parsing() {
        let sample_response: Vec<u8> = vec![0x31, 0x2e, 0x30, 0x2e, 0x34];
        let init_connection_info: InitConnectionInfo =
            parse_init_connection(&sample_response).unwrap();
        assert_eq!(
            init_connection_info,
            InitConnectionInfo {
                protocol_version: "1.0.4".to_string()
            }
        );
    }

    #[test]
    fn test_init_connection_parsing_empty() {
        let sample_response: Vec<u8> = vec![];
        let result: Result<InitConnectionInfo, BoseError> = parse_init_connection(&sample_response);
        assert!(result.is_err());
    }
}

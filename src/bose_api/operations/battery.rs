use crate::bose_api::device::BoseDevice;
use anyhow::Result;

pub struct BatteryInfo {
    pub level: u8,
}

pub async fn get_battery_level(device: &mut BoseDevice) -> Result<BatteryInfo> {
    let (send_bytes, ack_bytes) = device.firmware.get_battery_level_command();

    let response = device.execute_command(&send_bytes, &ack_bytes, 1).await?;

    Ok(BatteryInfo { level: response[0] })
}

#[cfg(test)]
mod tests {
    use super::BatteryInfo;

    #[test]
    fn test_battery_info_parsing() {
        let sample_response = vec![90];
        let battery_info = BatteryInfo {
            level: sample_response[0],
        };
        assert_eq!(battery_info.level, 90);
    }
}

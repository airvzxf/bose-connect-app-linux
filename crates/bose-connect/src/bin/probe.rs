//! Sanity-probe binary. Connects to the device, runs `init`, and
//! dumps the result. Used by maintainers when validating protocol
//! changes against a real headset; not part of the user-facing
//! CLI.

use std::env;
use std::process::ExitCode;

use bose_connect::{BoseDevice, BoseError};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: bose-connect-probe <bt-address>");
        return ExitCode::from(2);
    }

    match run(&args[1]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("probe failed: {e}");
            ExitCode::from(1)
        }
    }
}

fn run(address: &str) -> Result<(), BoseError> {
    let mut device = BoseDevice::open(address)?;
    let (device_id, index) = device.device_id()?;
    println!("device_id=0x{:04x} index={}", device_id, index);
    println!("firmware={}", device.firmware_version()?);
    println!("serial={}", device.serial_number()?);
    println!("battery={}%", device.battery_level()?);
    Ok(())
}

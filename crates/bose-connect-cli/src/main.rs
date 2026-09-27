//! Command-line interface for the Bose Connect protocol.
//!
//! This binary is a faithful Rust translation of the original
//! `src/main.c` CLI. Every flag below corresponds to one `case` in
//! the C `getopt_long` loop; the runtime semantics are preserved
//! (one RFCOMM connection per invocation, `init_connection` runs
//! before every command, the connection closes when the command
//! returns).
//!
//! Run `bose-connect-app-linux --help` for the full usage string.

use std::process::ExitCode;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bose_connect::protocol::language_from_arg;
use bose_connect::{
    BdAddr, BoseDevice, DeviceStatus, DevicesConnected, NoiseCancelling, PromptLanguage, SelfVoice,
    VP_ENABLE_BIT, VP_MASK,
};
use clap::{Parser, ValueHint};

/// Bose Connect for Linux — control Bose headphones over RFCOMM.
///
/// The CLI is a per-invocation driver: one connection is opened,
/// the protocol handshake runs, the requested command runs, and the
/// connection closes. Use the `--info` flag for a multi-query dump.
#[derive(Debug, Parser)]
#[command(
    name = "bose-connect-app-linux",
    version,
    about,
    long_about = None,
    disable_help_flag = false,
    arg_required_else_help = false,
)]
struct Cli {
    /// Bluetooth address of the Bose device ("AA:BB:CC:DD:EE:FF").
    #[arg(value_hint = ValueHint::Other, required = true)]
    address: String,

    /// Print all device information.
    #[arg(short = 'i', long = "info", conflicts_with_all = &[
        "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    info: bool,

    /// Print the device status (name, language, voice-prompts, auto-off, NC).
    #[arg(short = 'd', long = "device-status", conflicts_with_all = &[
        "info", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    device_status: bool,

    /// Print the firmware version.
    #[arg(short = 'f', long = "firmware-version", conflicts_with_all = &[
        "info", "device_status", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    firmware_version: bool,

    /// Print the serial number.
    #[arg(short = 's', long = "serial-number", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    serial_number: bool,

    /// Print the battery level as a percent.
    #[arg(short = 'b', long = "battery-level", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    battery_level: bool,

    /// Print the list of paired devices.
    #[arg(short = 'a', long = "paired-devices", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    paired_devices: bool,

    /// Print the device id and index revision.
    #[arg(long = "device-id", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    device_id: bool,

    /// Change the device name.
    #[arg(short = 'n', long = "name", value_name = "NAME", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    name: Option<String>,

    /// Change the auto-off time. minutes: never, 5, 20, 40, 60, 180.
    #[arg(short = 'o', long = "auto-off", value_name = "MINUTES", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    auto_off: Option<String>,

    /// Change the noise-cancelling level. level: high, low, off.
    #[arg(short = 'c', long = "noise-cancelling", value_name = "LEVEL", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    noise_cancelling: Option<String>,

    /// Change the prompt language.
    #[arg(short = 'l', long = "prompt-language", value_name = "LANG", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    prompt_language: Option<String>,

    /// Toggle voice prompts. switch: on, off.
    #[arg(short = 'v', long = "voice-prompts", value_name = "SWITCH", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    voice_prompts: Option<String>,

    /// Toggle pairing discoverability. status: on, off.
    #[arg(short = 'p', long = "pairing", value_name = "STATUS", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    pairing: Option<String>,

    /// Change the self-voice level. level: high, medium, low, off.
    #[arg(short = 'e', long = "self-voice", value_name = "LEVEL", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
    ])]
    self_voice: Option<String>,

    /// Connect to the device at the given Bluetooth address.
    #[arg(long = "connect-device", value_name = "ADDRESS", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "disconnect_device", "remove_device", "send_packet",
    ])]
    connect_device: Option<String>,

    /// Disconnect the device at the given Bluetooth address.
    #[arg(long = "disconnect-device", value_name = "ADDRESS", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "remove_device", "send_packet",
    ])]
    disconnect_device: Option<String>,

    /// Remove the device at the given Bluetooth address from the pairing list.
    #[arg(long = "remove-device", value_name = "ADDRESS", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "send_packet",
    ])]
    remove_device: Option<String>,

    /// Send a raw hex packet (e.g. `0a1b2c3d`) and print the response.
    #[arg(long = "send-packet", value_name = "HEX", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device",
    ])]
    send_packet: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {:#}", e);
            ExitCode::from(1)
        }
    }
}

fn dispatch(cli: Cli) -> Result<()> {
    let mut device = BoseDevice::open(&cli.address)
        .with_context(|| format!("failed to open Bose device at {}", cli.address))?;

    // Mirror the C `do_get_information` retry/back-off pattern: each
    // sub-query gets up to 3 attempts, with a 1-second sleep between
    // failures. This survives the occasional dropped packet Bose
    // headphones emit during sustained polling.
    if cli.info {
        return run_info(&mut device);
    }

    if cli.device_status {
        do_device_status(&mut device)?;
    } else if cli.firmware_version {
        do_firmware_version(&mut device)?;
    } else if cli.serial_number {
        do_serial_number(&mut device)?;
    } else if cli.battery_level {
        do_battery_level(&mut device)?;
    } else if cli.paired_devices {
        do_paired_devices(&mut device)?;
    } else if cli.device_id {
        do_device_id(&mut device)?;
    } else if let Some(name) = cli.name.as_deref() {
        device
            .set_name(name)
            .with_context(|| format!("setting name to {name:?}"))?;
    } else if let Some(minutes) = cli.auto_off.as_deref() {
        let parsed = parse_auto_off(minutes)?;
        device.set_auto_off(parsed)?;
    } else if let Some(level) = cli.noise_cancelling.as_deref() {
        let parsed = parse_noise_cancelling(level)?;
        // Re-fetch the device id so we can refuse early on devices
        // that have no NC hardware. The C code's behaviour.
        let (device_id, _) = device.device_id()?;
        if !bose_connect::has_noise_cancelling(device_id) {
            bail!("this device does not have noise cancelling");
        }
        device.set_noise_cancelling(parsed)?;
    } else if let Some(lang) = cli.prompt_language.as_deref() {
        let parsed = language_from_arg(lang)
            .ok_or_else(|| anyhow::anyhow!("invalid prompt language argument: {lang}"))?;
        device.set_language_keep_voice_prompts(parsed)?;
    } else if let Some(switch) = cli.voice_prompts.as_deref() {
        let on = parse_voice_prompts(switch)?;
        device.set_voice_prompts(on)?;
    } else if let Some(status) = cli.pairing.as_deref() {
        let on = parse_pairing(status)?;
        device.set_pairing(on)?;
    } else if let Some(level) = cli.self_voice.as_deref() {
        let parsed = parse_self_voice(level)?;
        device.set_self_voice(parsed)?;
    } else if let Some(addr) = cli.connect_device.as_deref() {
        let addr = parse_address(addr)?;
        device.connect_device(addr)?;
    } else if let Some(addr) = cli.disconnect_device.as_deref() {
        let addr = parse_address(addr)?;
        device.disconnect_device(addr)?;
    } else if let Some(addr) = cli.remove_device.as_deref() {
        let addr = parse_address(addr)?;
        device.remove_device(addr)?;
    } else if let Some(hex) = cli.send_packet.as_deref() {
        do_send_packet(&mut device, hex)?;
    } else {
        // No command flag. The C code prints the usage on bare
        // invocation; mirror that.
        use clap::CommandFactory;
        let mut cmd = Cli::command();
        cmd.print_help().ok();
        println!();
        return Ok(());
    }

    Ok(())
}

fn run_info(device: &mut BoseDevice) -> Result<()> {
    // C equivalent: `do_get_information` with up to 3 retries and a
    // 1-second sleep between attempts. We replicate the structure
    // here so the operator-facing log output matches the C version.
    const MAX_RETRIES: usize = 3;
    const SLEEP: Duration = Duration::from_secs(1);

    // Each sub-query prints its own banner (mirrors the C
    // `do_get_*` helpers); the retries only print on failure, so
    // successful runs are quiet between banners.
    let (id, index) =
        retry(MAX_RETRIES, SLEEP, || device.device_id()).context("getting device id")?;
    println!("Device ID: 0x{:04x} | Index: {}", id, index);
    std::thread::sleep(SLEEP);

    let serial =
        retry(MAX_RETRIES, SLEEP, || device.serial_number()).context("getting serial number")?;
    println!("Serial number: {}", serial);
    std::thread::sleep(SLEEP);

    let fw = retry(MAX_RETRIES, SLEEP, || device.firmware_version())
        .context("getting firmware version")?;
    println!("Firmware version: {}", fw);
    std::thread::sleep(SLEEP);

    let batt =
        retry(MAX_RETRIES, SLEEP, || device.battery_level()).context("getting battery level")?;
    println!("Battery level: {}", batt);
    std::thread::sleep(SLEEP);

    // device_status and paired_devices have multi-line printers in
    // the C `do_*` helpers. Reuse them so the operator-facing
    // format matches the C version byte-for-byte.
    retry(MAX_RETRIES, SLEEP, || do_device_status(device)).context("getting device status")?;
    std::thread::sleep(SLEEP);

    retry(MAX_RETRIES, SLEEP, || do_paired_devices(device)).context("getting paired devices")?;

    Ok(())
}

fn retry<T, E, F>(max: usize, sleep: Duration, mut f: F) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: std::fmt::Display,
{
    for attempt in 1..=max {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if attempt < max => {
                eprintln!("attempt {attempt}/{max} failed: {e}; sleeping {sleep:?}");
                std::thread::sleep(sleep);
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("loop always returns")
}

fn do_device_status(device: &mut BoseDevice) -> Result<()> {
    let status = device.device_status()?;
    println!("Status:");
    println!("\tName: {}", status.name);

    // The Bose SLC II echoes the prompt-language byte with bit 7
    // set as a status indicator, and encodes the actual language
    // in the lower 5 bits; without AND+OR the lookup would fall
    // through to 'Unknown' for every successful read. Same
    // workaround the C code applies (commit 56917d5).
    let cleaned = (status.language & VP_MASK) | VP_ENABLE_BIT;
    if let Some(pl) = PromptLanguage::from_u8(cleaned) {
        println!("\tLanguage: {}", pl.as_str());
    } else {
        println!("\tLanguage: Unknown [0x{:02x}]", status.language);
    }
    println!(
        "\tVoice Prompts: {}",
        if status.language & VP_ENABLE_BIT != 0 {
            "on"
        } else {
            "off"
        }
    );
    if status.minutes == 0 {
        println!("\tAuto-Off: never");
    } else {
        println!("\tAuto-Off: {}", status.minutes);
    }
    if status.level != NoiseCancelling::Dne {
        let s = match status.level {
            NoiseCancelling::High => "high",
            NoiseCancelling::Low => "low",
            NoiseCancelling::Off => "off",
            NoiseCancelling::Dne => unreachable!(),
        };
        println!("\tNoise Cancelling: {}", s);
    }
    Ok(())
}

fn do_firmware_version(device: &mut BoseDevice) -> Result<()> {
    let v = device.firmware_version()?;
    println!("Firmware version: {}", v);
    Ok(())
}

fn do_serial_number(device: &mut BoseDevice) -> Result<()> {
    let s = device.serial_number()?;
    println!("Serial number: {}", s);
    Ok(())
}

fn do_battery_level(device: &mut BoseDevice) -> Result<()> {
    let level = device.battery_level()?;
    println!("Battery level: {}", level);
    Ok(())
}

fn do_paired_devices(device: &mut BoseDevice) -> Result<()> {
    let pd = device.paired_devices()?;
    println!("Paired devices: {}", pd.num_devices);
    let count = pd.connected.count().ok_or_else(|| {
        anyhow::anyhow!(
            "unknown device-connected count: 0x{:02x}",
            pd.connected as u8
        )
    })?;
    println!("\tConnected: {}", count);

    for i in 0..pd.num_devices {
        let info = device.device_info(pd.addresses[i])?;
        let canonical = bose_connect::address::reverse_ba2str(&info.address);
        let s = std::str::from_utf8(&canonical[..17]).unwrap_or("??:??:??:??:??:??");
        let glyph = match info.status {
            DeviceStatus::This => '!',
            DeviceStatus::Connected => '*',
            DeviceStatus::Disconnected => ' ',
        };
        let name = std::str::from_utf8(&info.name[..info.name_len]).unwrap_or("");
        println!("\tDevice: {} | {} | {}", glyph, s, name);
    }

    println!("\t[!] Indicates the current device.");
    println!("\t[*] Indicates other connected devices.");

    // Suppress unused warning for the DevicesConnected enum import
    // (kept here so future log lines can reference it without a new
    // import).
    let _: Option<DevicesConnected> = None;
    Ok(())
}

fn do_device_id(device: &mut BoseDevice) -> Result<()> {
    let (id, index) = device.device_id()?;
    println!("Device ID: 0x{:04x} | Index: {}", id, index);
    Ok(())
}

fn do_send_packet(device: &mut BoseDevice, hex: &str) -> Result<()> {
    let bytes = parse_hex_packet(hex)?;
    let received = bose_connect::protocol::send_packet(device.connection(), &bytes)?;
    print!("Received package:\n\t");
    for b in &received {
        print!("{:02x} ", b);
    }
    println!();
    Ok(())
}

// ---------------------------------------------------------------------------
// Parsing helpers. Mirror the C `do_set_*` argument parsers so the
// operator-facing error messages match the original CLI.
// ---------------------------------------------------------------------------

fn parse_auto_off(s: &str) -> Result<bose_connect::AutoOff> {
    bose_connect::AutoOff::from_arg(s)
        .ok_or_else(|| anyhow::anyhow!("invalid auto-off argument: {s}"))
}

fn parse_noise_cancelling(s: &str) -> Result<NoiseCancelling> {
    Ok(match s {
        "high" => NoiseCancelling::High,
        "low" => NoiseCancelling::Low,
        "off" => NoiseCancelling::Off,
        _ => bail!("invalid noise cancelling argument: {s}"),
    })
}

fn parse_voice_prompts(s: &str) -> Result<bool> {
    Ok(match s {
        "on" => true,
        "off" => false,
        _ => bail!("invalid voice prompt argument: {s}"),
    })
}

fn parse_pairing(s: &str) -> Result<bool> {
    Ok(match s {
        "on" => true,
        "off" => false,
        _ => bail!("invalid pairing argument: {s}"),
    })
}

fn parse_self_voice(s: &str) -> Result<SelfVoice> {
    bose_connect::SelfVoice::from_arg(s)
        .ok_or_else(|| anyhow::anyhow!("invalid self voice argument: {s}"))
}

fn parse_address(s: &str) -> Result<BdAddr> {
    BdAddr::from_canonical(s).ok_or_else(|| anyhow::anyhow!("invalid bluetooth address: {s}"))
}

fn parse_hex_packet(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        bail!("hex packet length {} is odd", s.len());
    }
    let bytes_str = s.as_bytes();
    let mut out = Vec::with_capacity(bytes_str.len() / 2);
    let mut i = 0;
    while i < bytes_str.len() {
        let pair = [bytes_str[i], bytes_str[i + 1]];
        let b = bose_connect::util::str_to_byte(&pair)
            .ok_or_else(|| anyhow::anyhow!("hex packet contains non-hex characters near {i}"))?;
        out.push(b);
        i += 2;
    }
    Ok(out)
}

#[allow(dead_code)]
fn _silence_unused_for_fuzz_targets(s: &str) {
    // Reserved hook so we can later add `cargo-fuzz` harnesses
    // without touching the imports above.
    let _ = parse_hex_packet(s);
}

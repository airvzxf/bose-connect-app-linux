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
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    info: bool,

    /// Print the device status (name, language, voice-prompts, auto-off, NC).
    #[arg(short = 'd', long = "device-status", conflicts_with_all = &[
        "info", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    device_status: bool,

    /// Print the firmware version.
    #[arg(short = 'f', long = "firmware-version", conflicts_with_all = &[
        "info", "device_status", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    firmware_version: bool,

    /// Print the serial number.
    #[arg(short = 's', long = "serial-number", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    serial_number: bool,

    /// Print the battery level as a percent.
    #[arg(short = 'b', long = "battery-level", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    battery_level: bool,

    /// Print the list of paired devices.
    #[arg(short = 'a', long = "paired-devices", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    paired_devices: bool,

    /// Print the device id and index revision.
    #[arg(long = "device-id", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    device_id: bool,

    /// Change the device name.
    #[arg(short = 'n', long = "name", value_name = "NAME", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    name: Option<String>,

    /// Change the auto-off time. minutes: never, 5, 20, 40, 60, 180.
    #[arg(short = 'o', long = "auto-off", value_name = "MINUTES", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    auto_off: Option<String>,

    /// Change the noise-cancelling level. level: high, low, off. On
    /// the QC Ultra, use --audio-mode instead.
    #[arg(short = 'c', long = "noise-cancelling", value_name = "LEVEL", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    noise_cancelling: Option<String>,

    /// Change the prompt language.
    #[arg(short = 'l', long = "prompt-language", value_name = "LANG", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    prompt_language: Option<String>,

    /// Toggle voice prompts. switch: on, off.
    #[arg(short = 'v', long = "voice-prompts", value_name = "SWITCH", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    voice_prompts: Option<String>,

    /// Toggle pairing discoverability. status: on, off.
    #[arg(short = 'p', long = "pairing", value_name = "STATUS", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    pairing: Option<String>,

    /// Change the self-voice level. level: high, medium, low, off.
    #[arg(short = 'e', long = "self-voice", value_name = "LEVEL", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing",
        "connect_device", "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    self_voice: Option<String>,

    /// Connect to the device at the given Bluetooth address.
    #[arg(long = "connect-device", value_name = "ADDRESS", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "disconnect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    connect_device: Option<String>,

    /// Disconnect the device at the given Bluetooth address.
    #[arg(long = "disconnect-device", value_name = "ADDRESS", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "remove_device", "send_packet", "audio_mode",
    ])]
    disconnect_device: Option<String>,

    /// Remove the device at the given Bluetooth address from the pairing list.
    #[arg(long = "remove-device", value_name = "ADDRESS", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "send_packet", "audio_mode",
    ])]
    remove_device: Option<String>,

    /// Switch the audio mode (QC Ultra). mode: its name (quiet, aware,
    /// immersion, …) or its slot index.
    #[arg(short = 'm', long = "audio-mode", value_name = "MODE", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
        "set_volume", "send_media_key", "active_device", "device_bd_addr",
    ])]
    audio_mode: Option<String>,

    /// RFCOMM channel to use instead of the automatic choice
    /// (8, then 2).
    #[arg(long = "channel", value_name = "CHANNEL")]
    channel: Option<u8>,

    /// Send a raw hex packet (e.g. `0a1b2c3d`) and print the response.
    #[arg(long = "send-packet", value_name = "HEX", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "audio_mode",
    ])]
    send_packet: Option<String>,

    /// Set the volume. The range is device-specific (e.g. 0..=25 on a QC35 II)
    #[arg(long = "set-volume", value_name = "LEVEL", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
        "send_media_key", "active_device", "device_bd_addr", "audio_mode",
    ])]
    set_volume: Option<u8>,

    /// Send a media key. key: pause (play/pause), next, prev
    #[arg(long = "send-media-key", value_name = "KEY", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
        "set_volume", "active_device", "device_bd_addr", "audio_mode",
    ])]
    send_media_key: Option<String>,

    /// Print the active audio source's BT address.
    #[arg(long = "active-device", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
        "set_volume", "send_media_key", "device_bd_addr", "audio_mode",
    ])]
    active_device: bool,

    /// Print the device's BT address.
    #[arg(long = "device-bd-addr", conflicts_with_all = &[
        "info", "device_status", "firmware_version", "serial_number", "battery_level",
        "paired_devices", "device_id", "name", "auto_off", "noise_cancelling",
        "prompt_language", "voice_prompts", "pairing", "self_voice",
        "connect_device", "disconnect_device", "remove_device", "send_packet",
        "set_volume", "send_media_key", "active_device", "audio_mode",
    ])]
    device_bd_addr: bool,
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
    let device = match cli.channel {
        Some(channel) => BoseDevice::open_channel(&cli.address, channel),
        None => BoseDevice::open(&cli.address),
    };
    let mut device =
        device.with_context(|| format!("failed to open Bose device at {}", cli.address))?;

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
        require_legacy_settings(&mut device, "auto-off")?;
        device.set_auto_off(parsed)?;
    } else if let Some(level) = cli.noise_cancelling.as_deref() {
        let parsed = parse_noise_cancelling(level)?;
        // Re-fetch the device id so we can refuse early on devices
        // that have no NC hardware. The C code's behaviour.
        let (device_id, _) = device.device_id()?;
        if bose_connect::has_audio_modes(device_id) {
            bail!(
                "this device uses audio modes instead of noise-cancelling levels; use --audio-mode"
            );
        }
        if !bose_connect::has_noise_cancelling(device_id) {
            bail!("this device does not have noise cancelling");
        }
        device.set_noise_cancelling(parsed)?;
    } else if let Some(lang) = cli.prompt_language.as_deref() {
        let parsed = language_from_arg(lang)
            .ok_or_else(|| anyhow::anyhow!("invalid prompt language argument: {lang}"))?;
        require_legacy_settings(&mut device, "prompt-language")?;
        device.set_language_keep_voice_prompts(parsed)?;
    } else if let Some(switch) = cli.voice_prompts.as_deref() {
        let on = parse_voice_prompts(switch)?;
        require_legacy_settings(&mut device, "voice-prompts")?;
        device.set_voice_prompts(on)?;
    } else if let Some(status) = cli.pairing.as_deref() {
        let on = parse_pairing(status)?;
        // Pre-flight: refuse early on devices that don't
        // implement the pairing-toggle command. Without this
        // check the protocol would waste ~3 s (1 s per retry)
        // waiting for a response that never comes before
        // surfacing AckMismatch. The C version does not
        // short-circuit here; the SoundLink II returns
        // AckMismatch on `SET_PAIRING` after a full timeout
        // budget.
        let (device_id, _) = device.device_id()?;
        if !bose_connect::has_pairing_toggle(device_id) {
            bail!("this device does not support pairing-toggle");
        }
        device.set_pairing(on)?;
    } else if let Some(level) = cli.self_voice.as_deref() {
        let parsed = parse_self_voice(level)?;
        // Pre-flight: refuse early on devices that don't
        // implement the self-voice command. Without this the
        // protocol would waste ~3 s (1 s per retry) waiting for
        // a response that never comes. Same rationale as the
        // pairing-toggle check above.
        let (device_id, _) = device.device_id()?;
        if !bose_connect::has_self_voice(device_id) {
            bail!("this device does not support self-voice");
        }
        device.set_self_voice(parsed)?;
    } else if let Some(addr) = cli.connect_device.as_deref() {
        let addr = parse_address(addr)?;
        do_paired_op_verify(&mut device, "connect", addr, |d, a| d.connect_device(a))?;
    } else if let Some(addr) = cli.disconnect_device.as_deref() {
        let addr = parse_address(addr)?;
        do_paired_op_verify(&mut device, "disconnect", addr, |d, a| {
            d.disconnect_device(a)
        })?;
    } else if let Some(addr) = cli.remove_device.as_deref() {
        let addr = parse_address(addr)?;
        do_paired_op_verify(&mut device, "remove", addr, |d, a| d.remove_device(a))?;
    } else if let Some(mode) = cli.audio_mode.as_deref() {
        do_set_audio_mode(&mut device, mode)?;
    } else if let Some(hex) = cli.send_packet.as_deref() {
        do_send_packet(&mut device, hex)?;
    } else if let Some(level) = cli.set_volume {
        let max = device.set_volume(level)?;
        println!("Volume: {} (device scale max {})", level, max);
    } else if let Some(key) = cli.send_media_key.as_deref() {
        let parsed = parse_media_key(key)?;
        device.send_media_key(parsed)?;
        println!("Sent media key: {:?}", parsed);
    } else if cli.active_device {
        let addr = device.active_device()?;
        println!("Active device: {}", format_address(&addr));
    } else if cli.device_bd_addr {
        let addr = device.device_bd_addr()?;
        println!("Device BD addr: {}", format_address(&addr));
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
    //
    // The QC Ultra also sets bit 6, so only the low 5 bits are kept
    // for the language code.
    let cleaned = (status.language & VP_MASK & LANGUAGE_CODE_MASK) | VP_ENABLE_BIT;
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
    match status.minutes {
        Some(0) => println!("\tAuto-Off: never"),
        Some(minutes) => println!("\tAuto-Off: {}", minutes),
        None => println!("\tAuto-Off: unknown"),
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
    if bose_connect::has_audio_modes(status.device_id) {
        let modes = device.audio_modes()?;
        let current = device.audio_mode()?;
        let name = modes
            .iter()
            .find(|(index, _)| *index == current)
            .map_or("unknown", |(_, name)| name.as_str());
        println!("\tAudio Mode: {}", name);
        let names: Vec<&str> = modes.iter().map(|(_, name)| name.as_str()).collect();
        println!("\tAudio Modes Available: {}", names.join(", "));
    }
    Ok(())
}

/// Low 5 bits of the prompt-language byte: the language code.
const LANGUAGE_CODE_MASK: u8 = 0x1f;

/// Refuse `setting` on devices whose payload layout for it is not
/// the 1-byte QC35 one, rather than writing a guessed value.
fn require_legacy_settings(device: &mut BoseDevice, setting: &str) -> Result<()> {
    let (device_id, _) = device.device_id()?;
    if !bose_connect::has_legacy_settings(device_id) {
        bail!("{setting} is not supported on this device yet");
    }
    Ok(())
}

fn do_set_audio_mode(device: &mut BoseDevice, mode: &str) -> Result<()> {
    let (device_id, _) = device.device_id()?;
    if !bose_connect::has_audio_modes(device_id) {
        bail!("this device does not have audio modes");
    }
    let modes = device.audio_modes()?;
    let (index, name) = modes
        .iter()
        .find(|(index, name)| name.eq_ignore_ascii_case(mode) || index.to_string() == mode)
        .ok_or_else(|| {
            let names: Vec<&str> = modes.iter().map(|(_, name)| name.as_str()).collect();
            anyhow::anyhow!(
                "unknown audio mode {mode:?}; available: {}",
                names.join(", ")
            )
        })?;
    device.set_audio_mode(*index)?;
    println!("Audio mode: {}", name);
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
    // Retry budget for each per-device query. The Bose SLC II
    // occasionally takes more than the 1 s `SO_RCVTIMEO` to
    // reply to one of the `GET_DEVICE_INFO` packets (especially
    // for the currently-connected device, which is busy serving
    // A2DP audio at the same time). Three retries per device
    // with a 500 ms back-off is enough in practice; the original
    // C code has no per-device retry and surfaces the EAGAIN as
    // a hard failure on `--paired-devices`.
    const PER_DEVICE_RETRIES: usize = 3;
    const PER_DEVICE_BACKOFF: Duration = Duration::from_millis(500);

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
        // Retry the per-device query up to PER_DEVICE_RETRIES
        // times. We log to stderr so the operator can see why
        // the second/third attempt fired.
        let info = {
            let mut last_err: Option<bose_connect::BoseError> = None;
            let mut result: Option<bose_connect::DeviceInfo> = None;
            for attempt in 1..=PER_DEVICE_RETRIES {
                match device.device_info(pd.addresses[i]) {
                    Ok(info) => {
                        result = Some(info);
                        break;
                    }
                    Err(e) => {
                        if attempt < PER_DEVICE_RETRIES {
                            eprintln!(
                                "[paired-devices] device {} attempt {}/{} failed: {e}; retrying in {:?}",
                                i, attempt, PER_DEVICE_RETRIES, PER_DEVICE_BACKOFF
                            );
                            std::thread::sleep(PER_DEVICE_BACKOFF);
                        }
                        last_err = Some(e);
                    }
                }
            }
            result.ok_or_else(|| {
                anyhow::anyhow!(
                    "device_info for paired device {} failed after {} attempts: {}",
                    i,
                    PER_DEVICE_RETRIES,
                    last_err.expect("loop either sets result or last_err")
                )
            })?
        };

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

/// Run one of the paired-device operations (connect / disconnect /
/// remove) and verify the effect took place by re-querying
/// `get_paired_devices` afterwards. This is the A4 layer: the
/// permissive ACK matcher at the protocol level (A2) tells us the
/// device didn't return an error, but the device may have
/// silently rejected the command (e.g. it doesn't support
/// `REMOVE_DEVICE` while in party-mode). The verification step
/// closes that gap by checking the Bose's own view of the world.
fn do_paired_op_verify<F>(
    device: &mut BoseDevice,
    op: &str,
    address: BdAddr,
    mut action: F,
) -> Result<()>
where
    F: FnMut(&mut BoseDevice, BdAddr) -> Result<(), bose_connect::BoseError>,
{
    const MAX_ATTEMPTS: usize = 3;
    const RETRY_BACKOFF: Duration = Duration::from_secs(1);

    // Snapshot paired devices BEFORE the operation, so we know
    // whether the address was already in the list (in which
    // case connect is a no-op verify, and remove is the only
    // operation with a clear before/after diff).
    let before = snapshot_paired_addresses(device)?;

    let canonical = format_address(&address);
    eprintln!("[paired-op:{op}] target = {canonical}");

    for attempt in 1..=MAX_ATTEMPTS {
        match action(device, address) {
            Ok(()) => {
                eprintln!("[paired-op:{op}] attempt {attempt}/{MAX_ATTEMPTS}: protocol OK");
            }
            Err(e) => {
                if attempt < MAX_ATTEMPTS {
                    eprintln!(
                        "[paired-op:{op}] attempt {attempt}/{MAX_ATTEMPTS}: protocol error: {e}; retrying in {RETRY_BACKOFF:?}"
                    );
                    std::thread::sleep(RETRY_BACKOFF);
                    continue;
                }
                return Err(anyhow::anyhow!(
                    "{op} {canonical} failed after {MAX_ATTEMPTS} attempts: {e}"
                ));
            }
        }

        // Protocol said OK — verify the device's internal state
        // actually changed.
        let after = snapshot_paired_addresses(device)?;
        let op_result = verify_paired_change(op, &before, &after, &address);
        match op_result {
            Ok(()) => {
                eprintln!(
                    "[paired-op:{op}] verified state change after {attempt}/{MAX_ATTEMPTS} attempts"
                );
                return Ok(());
            }
            Err(verify_err) => {
                if attempt < MAX_ATTEMPTS {
                    eprintln!(
                        "[paired-op:{op}] attempt {attempt}/{MAX_ATTEMPTS}: protocol OK but state unchanged ({verify_err}); retrying"
                    );
                    std::thread::sleep(RETRY_BACKOFF);
                    continue;
                }
                return Err(anyhow::anyhow!(
                    "{op} {canonical}: protocol returned OK but the device state never changed ({verify_err}). \
                     The command was likely rejected silently. Check the device's pairing list manually."
                ));
            }
        }
    }
    unreachable!("loop returns or continues")
}

fn snapshot_paired_addresses(
    device: &mut BoseDevice,
) -> Result<std::collections::BTreeSet<[u8; 6]>> {
    let pd = device.paired_devices()?;
    Ok(pd.addresses[..pd.num_devices]
        .iter()
        .map(|bd| bd.b)
        .collect())
}

fn verify_paired_change(
    op: &str,
    _before: &std::collections::BTreeSet<[u8; 6]>,
    after: &std::collections::BTreeSet<[u8; 6]>,
    address: &BdAddr,
) -> Result<(), &'static str> {
    match op {
        "connect" => {
            if after.contains(&address.b) {
                Ok(())
            } else {
                Err("connect accepted by protocol but address is not in the post-snapshot")
            }
        }
        "disconnect" => Ok(()),
        "remove" => {
            if !after.contains(&address.b) {
                Ok(())
            } else {
                Err("remove accepted by protocol but address is still in the post-snapshot")
            }
        }
        _ => Err("unknown paired-op"),
    }
}

fn format_address(address: &BdAddr) -> String {
    let canonical = bose_connect::address::reverse_ba2str(address);
    std::str::from_utf8(&canonical[..17])
        .unwrap_or("??:??:??:??:??:??")
        .to_string()
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

fn parse_media_key(s: &str) -> Result<bose_connect::MediaKey> {
    bose_connect::MediaKey::from_arg(s)
        .ok_or_else(|| anyhow::anyhow!("invalid media key argument: {s}"))
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

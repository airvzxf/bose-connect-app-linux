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

mod bluez;

use std::process::ExitCode;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bose_connect::protocol::language_from_arg;
use bose_connect::{
    BdAddr, BoseDevice, BoseError, DeviceStatus, DevicesConnected, EqBand, ImmersiveAudio,
    MediaKey, NoiseCancelling, PartyMode, PromptLanguage, SelfVoice, ShortcutAction, VP_ENABLE_BIT,
};
use clap::builder::styling::{AnsiColor, Styles};
use clap::{ArgGroup, Parser, ValueHint};

/// Colours of the help and error output, the same as Cargo's. Clap
/// drops them when the output is not a terminal or `NO_COLOR` is set.
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().bold())
    .valid(AnsiColor::Green.on_default().bold())
    .invalid(AnsiColor::Yellow.on_default().bold());

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
    styles = STYLES,
)]
#[command(group(
    // At most one command per run; `--channel` combines with any of them.
    ArgGroup::new("command").multiple(false)
))]
struct Cli {
    /// Bluetooth address of the Bose device ("AA:BB:CC:DD:EE:FF").
    #[arg(value_hint = ValueHint::Other, required = true)]
    address: String,

    /// Print all device information.
    #[arg(
        help_heading = "Device information",
        short = 'i',
        long = "info",
        group = "command"
    )]
    info: bool,

    /// Print the device status (name, language, voice-prompts, auto-off, NC).
    #[arg(
        help_heading = "Device information",
        short = 'd',
        long = "device-status",
        group = "command"
    )]
    device_status: bool,

    /// Print the firmware version.
    #[arg(
        help_heading = "Device information",
        short = 'f',
        long = "firmware-version",
        group = "command"
    )]
    firmware_version: bool,

    /// Print the serial number.
    #[arg(
        help_heading = "Device information",
        short = 's',
        long = "serial-number",
        group = "command"
    )]
    serial_number: bool,

    /// Print the battery level as a percent.
    #[arg(
        help_heading = "Device information",
        short = 'b',
        long = "battery-level",
        group = "command"
    )]
    battery_level: bool,

    /// Print the device id and index revision.
    #[arg(
        help_heading = "Device information",
        long = "device-id",
        group = "command"
    )]
    device_id: bool,

    /// Print the device's BT address.
    #[arg(
        help_heading = "Device information",
        long = "device-bd-addr",
        group = "command"
    )]
    device_bd_addr: bool,

    /// Switch the audio mode (QC Ultra, QC45). mode: its name (quiet,
    /// aware, immersion, …) or its slot index.
    #[arg(
        help_heading = "Sound",
        short = 'm',
        long = "audio-mode",
        value_name = "MODE",
        group = "command"
    )]
    audio_mode: Option<String>,

    /// Change the noise-cancelling level. level: high, low, off. On
    /// the QC Ultra and QC45, use --audio-mode instead. On the NC 700,
    /// level: off, high or 0..=10 (0 is off, as in the Bose Music app).
    #[arg(
        help_heading = "Sound",
        short = 'c',
        long = "noise-cancelling",
        value_name = "LEVEL",
        group = "command"
    )]
    noise_cancelling: Option<String>,

    /// Set equalizer bands (QC Ultra, QC45, NC 700), e.g. `bass=4,mid=0,treble=-3`. Each in -10..=10.
    #[arg(
        help_heading = "Sound",
        long = "equalizer",
        value_name = "BANDS",
        group = "command"
    )]
    equalizer: Option<String>,

    /// Immersive audio (QC Ultra). setting: off, still, motion.
    #[arg(
        help_heading = "Sound",
        long = "immersive-audio",
        value_name = "SETTING",
        group = "command"
    )]
    immersive_audio: Option<String>,

    /// Change the self-voice level. level: high, medium, low, off.
    #[arg(
        help_heading = "Sound",
        short = 'e',
        long = "self-voice",
        value_name = "LEVEL",
        group = "command"
    )]
    self_voice: Option<String>,

    /// Set the volume. The range is device-specific (e.g. 0..=25 on a QC35 II)
    #[arg(
        help_heading = "Playback",
        long = "set-volume",
        value_name = "LEVEL",
        group = "command"
    )]
    set_volume: Option<u8>,

    /// Send a media key. key: pause (play/pause), next, prev
    #[arg(
        help_heading = "Playback",
        long = "send-media-key",
        value_name = "KEY",
        group = "command"
    )]
    send_media_key: Option<String>,

    /// Print the active audio source's BT address.
    #[arg(help_heading = "Playback", long = "active-device", group = "command")]
    active_device: bool,

    /// Change the device name.
    #[arg(
        help_heading = "Settings",
        short = 'n',
        long = "name",
        value_name = "NAME",
        group = "command"
    )]
    name: Option<String>,

    /// Change the auto-off time. minutes: never, 5, 20, 40, 60, 180, or 1440
    /// (24 hours, QC Ultra only).
    #[arg(
        help_heading = "Settings",
        short = 'o',
        long = "auto-off",
        value_name = "MINUTES",
        group = "command"
    )]
    auto_off: Option<String>,

    /// Change the prompt language.
    #[arg(
        help_heading = "Settings",
        short = 'l',
        long = "prompt-language",
        value_name = "LANG",
        group = "command"
    )]
    prompt_language: Option<String>,

    /// Toggle voice prompts. switch: on, off.
    #[arg(
        help_heading = "Settings",
        short = 'v',
        long = "voice-prompts",
        value_name = "SWITCH",
        group = "command"
    )]
    voice_prompts: Option<String>,

    /// Print how BlueZ sees the device's audio link to this computer,
    /// without opening a Bose connection.
    #[arg(
        help_heading = "Device information",
        long = "audio-state",
        group = "command"
    )]
    audio_state: bool,

    /// Announce the battery level at power-on (QC Ultra). switch: on, off.
    #[arg(
        help_heading = "Settings",
        long = "battery-prompt",
        value_name = "SWITCH",
        group = "command"
    )]
    battery_prompt: Option<String>,

    /// Pause when the headphones come off and resume when they go on (QC Ultra). switch: on, off.
    #[arg(
        help_heading = "Settings",
        long = "auto-play-pause",
        value_name = "SWITCH",
        group = "command"
    )]
    auto_play_pause: Option<String>,

    /// Answer calls by putting the headphones on (QC Ultra). switch: on, off.
    #[arg(
        help_heading = "Settings",
        long = "auto-answer",
        value_name = "SWITCH",
        group = "command"
    )]
    auto_answer: Option<String>,

    /// Start in the last audio mode used (QC Ultra, QC45) or at the last
    /// noise-cancelling step (NC 700). switch: on, off.
    #[arg(
        help_heading = "Settings",
        long = "remember-mode",
        value_name = "SWITCH",
        group = "command"
    )]
    remember_mode: Option<String>,

    /// Switch off after an hour without motion (NC 700). switch: on, off.
    #[arg(
        help_heading = "Settings",
        long = "motion-auto-off",
        value_name = "SWITCH",
        group = "command"
    )]
    motion_auto_off: Option<String>,

    /// Set the three noise-cancelling favourites (NC 700), e.g. `10,5,0`
    /// (0 is off, 10 is the strongest).
    #[arg(
        help_heading = "Sound",
        long = "nc-favorites",
        value_name = "LEVELS",
        group = "command"
    )]
    nc_favorites: Option<String>,

    /// Print what the device sends on its own (button presses, battery,
    /// setting changes) for this many seconds.
    #[arg(
        help_heading = "Advanced",
        long = "watch",
        value_name = "SECONDS",
        group = "command"
    )]
    watch: Option<u64>,

    /// Touch-and-hold shortcut of the right earcup (QC Ultra, QC45, NC 700). action: battery,
    /// immersive, assistant, spotify, wake-word (NC 700), off.
    #[arg(
        help_heading = "Settings",
        long = "shortcut",
        value_name = "ACTION",
        group = "command"
    )]
    shortcut: Option<String>,

    /// Print the list of paired devices.
    #[arg(
        help_heading = "Paired devices",
        short = 'a',
        long = "paired-devices",
        group = "command"
    )]
    paired_devices: bool,

    /// Connect to the device at the given Bluetooth address.
    #[arg(
        help_heading = "Paired devices",
        long = "connect-device",
        value_name = "ADDRESS",
        group = "command"
    )]
    connect_device: Option<String>,

    /// Disconnect the device at the given Bluetooth address.
    #[arg(
        help_heading = "Paired devices",
        long = "disconnect-device",
        value_name = "ADDRESS",
        group = "command"
    )]
    disconnect_device: Option<String>,

    /// Remove the device at the given Bluetooth address from the pairing list.
    #[arg(
        help_heading = "Paired devices",
        long = "remove-device",
        value_name = "ADDRESS",
        group = "command"
    )]
    remove_device: Option<String>,

    /// Join the SoundLink Color II at the given address to this one
    /// (party mode). The other speaker must be on and paired with this
    /// one. Stop the music for a minute before joining when this
    /// computer is the source; see the README.
    #[arg(
        help_heading = "Party mode (SoundLink Color II)",
        long = "party-join",
        value_name = "SPEAKER",
        group = "command"
    )]
    party_join: Option<String>,

    /// Music source for `--party-join` (default: this computer's
    /// Bluetooth adapter). Give the phone's address when the phone
    /// plays the music.
    #[arg(
        help_heading = "Party mode (SoundLink Color II)",
        long = "party-source",
        value_name = "ADDRESS",
        requires = "party_join"
    )]
    party_source: Option<String>,

    /// Separate the joined speaker at the given address.
    #[arg(
        help_heading = "Party mode (SoundLink Color II)",
        long = "party-leave",
        value_name = "SPEAKER",
        group = "command"
    )]
    party_leave: Option<String>,

    /// Print or switch the party mode. mode: status, party, stereo.
    #[arg(
        help_heading = "Party mode (SoundLink Color II)",
        long = "party-mode",
        value_name = "MODE",
        group = "command"
    )]
    party_mode: Option<String>,

    /// Toggle pairing discoverability. status: on, off.
    #[arg(
        help_heading = "Paired devices",
        short = 'p',
        long = "pairing",
        value_name = "STATUS",
        group = "command"
    )]
    pairing: Option<String>,

    /// RFCOMM channel to use instead of the automatic choice
    /// (8, then 2, then 1).
    #[arg(help_heading = "Advanced", long = "channel", value_name = "CHANNEL")]
    channel: Option<u8>,

    /// Send a raw hex packet (e.g. `0a1b2c3d`) and print the response.
    #[arg(
        help_heading = "Advanced",
        long = "send-packet",
        value_name = "HEX",
        group = "command"
    )]
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
    // Read BlueZ before opening the Bose connection: that connection
    // brings the Bluetooth link up by itself, and on a SoundLink Color
    // II it counts towards the speaker's connection limit.
    if cli.audio_state {
        let state = bluez::audio_state(&cli.address)?;
        println!("Audio: {}", bluez::describe(state));
        if state.needs_reconnect_hint() {
            println!("{}", bluez::RECONNECT_HINT);
        }
        return Ok(());
    }
    let audio = cli.info.then(|| bluez::audio_state(&cli.address));

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
        run_info(&mut device)?;
        match audio {
            Some(Ok(state)) => {
                println!("Audio: {}", bluez::describe(state));
                if state.needs_reconnect_hint() {
                    println!("{}", bluez::RECONNECT_HINT);
                }
            }
            Some(Err(e)) => println!("Audio: unavailable ({e:#})"),
            None => {}
        }
        return Ok(());
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
        // Re-fetch the device id so we can refuse early on devices
        // that have no NC hardware. The C code's behaviour.
        let (device_id, _) = device.device_id()?;
        if bose_connect::has_noise_cancelling_steps(device_id) {
            device.set_noise_cancelling_level(parse_noise_cancelling_level(level)?)?;
        } else {
            let parsed = parse_noise_cancelling(level)?;
            if bose_connect::has_audio_modes(device_id) {
                bail!(
                    "this device uses audio modes instead of noise-cancelling levels; use --audio-mode"
                );
            }
            if !bose_connect::has_noise_cancelling(device_id) {
                bail!("this device does not have noise cancelling");
            }
            device.set_noise_cancelling(parsed)?;
        }
    } else if let Some(lang) = cli.prompt_language.as_deref() {
        let parsed = language_from_arg(lang)
            .ok_or_else(|| anyhow::anyhow!("invalid prompt language argument: {lang}"))?;
        require_legacy_settings(&mut device, "prompt-language")?;
        device.set_language_keep_voice_prompts(parsed)?;
    } else if let Some(switch) = cli.voice_prompts.as_deref() {
        let on = parse_voice_prompts(switch)?;
        device.set_voice_prompts(on)?;
    } else if let Some(status) = cli.pairing.as_deref() {
        let on = parse_pairing(status)?;
        // Pre-flight: refuse early on devices that don't
        // implement the pairing-toggle command. Without this
        // check the protocol would waste ~3 s (1 s per retry)
        // waiting for a response that never comes before
        // surfacing AckMismatch. The C version does not
        // short-circuit here.
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
    } else if let Some(peer) = cli.party_join.as_deref() {
        let peer = parse_address(peer)?;
        let source = match cli.party_source.as_deref() {
            Some(addr) => parse_address(addr)?,
            None => parse_address(&bluez::adapter_address()?)?,
        };
        require_party_mode(&mut device)?;
        device.join_party(peer, source)?;
        println!("Joined {}; party mode", format_address(&peer));
    } else if let Some(peer) = cli.party_leave.as_deref() {
        let peer = parse_address(peer)?;
        require_party_mode(&mut device)?;
        device.leave_party(peer)?;
        println!("Separated {}", format_address(&peer));
    } else if let Some(mode) = cli.party_mode.as_deref() {
        require_party_mode(&mut device)?;
        match mode {
            "status" => {}
            "party" => device.set_party_mode(PartyMode::Party)?,
            "stereo" => device.set_party_mode(PartyMode::Stereo)?,
            other => bail!("invalid party mode: {other} (status, party, stereo)"),
        }
        println!("Party mode: {}", party_mode_name(device.party_mode()?));
    } else if let Some(mode) = cli.audio_mode.as_deref() {
        do_set_audio_mode(&mut device, mode)?;
    } else if let Some(hex) = cli.send_packet.as_deref() {
        do_send_packet(&mut device, hex)?;
    } else if let Some(level) = cli.set_volume {
        let max = device.set_volume(level)?;
        println!("Volume: {} (device scale max {})", level, max);
    } else if let Some(key) = cli.send_media_key.as_deref() {
        let parsed = parse_media_key(key)?;
        if parsed == MediaKey::Pause {
            let (device_id, _) = device.device_id()?;
            if bose_connect::has_audio_modes(device_id) {
                bail!(
                    "pause is refused on this device: it never completes on a QC Ultra or a \
                     QC45; on a QC Ultra it locks media control, the phone's included, until \
                     the phone reconnects, and on a QC45 it reaches some sources and not others"
                );
            }
        }
        device.send_media_key(parsed)?;
        println!("Sent media key: {:?}", parsed);
    } else if cli.active_device {
        let addr = device.active_device()?;
        println!("Active device: {}", format_address(&addr));
    } else if cli.device_bd_addr {
        let addr = device.device_bd_addr()?;
        println!("Device BD addr: {}", format_address(&addr));
    } else if let Some(bands) = cli.equalizer.as_deref() {
        for (band, value) in parse_equalizer(bands)? {
            device
                .set_equalizer_band(band, value)
                .with_context(|| format!("setting {band:?} to {value}"))?;
        }
    } else if let Some(setting) = cli.immersive_audio.as_deref() {
        device.set_immersive_audio(parse_immersive_audio(setting)?)?;
    } else if let Some(switch) = cli.battery_prompt.as_deref() {
        device.set_battery_prompt(parse_switch("battery-prompt", switch)?)?;
    } else if let Some(switch) = cli.auto_play_pause.as_deref() {
        device.set_auto_play_pause(parse_switch("auto-play-pause", switch)?)?;
    } else if let Some(switch) = cli.auto_answer.as_deref() {
        device.set_auto_answer_calls(parse_switch("auto-answer", switch)?)?;
    } else if let Some(switch) = cli.remember_mode.as_deref() {
        let on = parse_switch("remember-mode", switch)?;
        let (device_id, _) = device.device_id()?;
        if bose_connect::has_nc_settings(device_id) {
            device.set_remember_noise_cancelling(on)?;
        } else {
            device.set_remember_audio_mode(on)?;
        }
    } else if let Some(switch) = cli.motion_auto_off.as_deref() {
        require_nc_settings(&mut device, "motion-auto-off")?;
        device.set_motion_auto_off(parse_switch("motion-auto-off", switch)?)?;
    } else if let Some(seconds) = cli.watch {
        do_watch(&mut device, seconds)?;
    } else if let Some(levels) = cli.nc_favorites.as_deref() {
        require_nc_settings(&mut device, "nc-favorites")?;
        device.set_noise_cancelling_favorites(&parse_nc_favorites(levels)?)?;
    } else if let Some(action) = cli.shortcut.as_deref() {
        let action = parse_shortcut(action)?;
        let (device_id, _) = device.device_id()?;
        device.set_shortcut_event(bose_connect::shortcut_event(device_id), action)?;
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

/// The function blocks the Bose Music app subscribes to.
const WATCH_BLOCKS: [u8; 8] = [0, 1, 2, 4, 5, 9, 16, 18];

/// Subscribe to notifications and print every packet the device sends
/// for `seconds`, with the time since the start.
fn do_watch(device: &mut BoseDevice, seconds: u64) -> Result<()> {
    device.enable_notifications(&WATCH_BLOCKS)?;
    println!("Watching for {seconds} s; press buttons or change settings.");
    let start = std::time::Instant::now();
    let end = start + std::time::Duration::from_secs(seconds);
    while std::time::Instant::now() < end {
        if let Some(n) = device.read_notification()? {
            let hex: Vec<String> = n.payload.iter().map(|b| format!("{b:02x}")).collect();
            println!(
                "+{:>5.1}s  {:02x} {:02x}  op {}  [{}]  {}",
                start.elapsed().as_secs_f32(),
                n.block,
                n.function,
                n.operator,
                hex.join(" "),
                notification_label(n.block, n.function)
            );
        }
    }
    Ok(())
}

/// A name for the notifications seen so far; empty for the others.
fn notification_label(block: u8, function: u8) -> &'static str {
    match (block, function) {
        (0x02, 0x02) => "battery",
        (0x02, 0x0c) => "button",
        (0x01, 0x05) => "noise cancelling",
        (0x01, 0x0f) => "noise cancelling favourites",
        (0x01, 0x09) => "shortcut",
        (0x01, 0x07) => "equalizer",
        (0x05, 0x05) => "volume",
        (0x04, 0x04) => "paired devices",
        _ => "",
    }
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
    // set as a status indicator and the QC Ultra also sets bit 6;
    // only the low 5 bits are the language. Same workaround the C
    // code applies (commit 56917d5).
    if let Some(pl) = PromptLanguage::from_status_byte(status.language) {
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
        let name = modes.iter().find(|(index, _)| *index == current).map_or(
            // `ff`: the current mode was changed away from its slot,
            // e.g. its immersive-audio setting.
            if current == 0xff {
                "modified"
            } else {
                "unknown"
            },
            |(_, name)| name.as_str(),
        );
        println!("\tAudio Mode: {}", name);
        let names: Vec<&str> = modes.iter().map(|(_, name)| name.as_str()).collect();
        println!("\tAudio Modes Available: {}", names.join(", "));
        println!(
            "\tRemember Mode: {}",
            if device.remember_audio_mode()? {
                "on"
            } else {
                "off"
            }
        );
    }
    print_settings(device, status.device_id)
}

/// The settings decoded on the QC Ultra (firmware 1.6.7), each printed
/// only on the devices known to have it.
fn print_settings(device: &mut BoseDevice, device_id: u16) -> Result<()> {
    let on_off = |on: bool| if on { "on" } else { "off" };
    if bose_connect::has_equalizer(device_id) {
        let bands: Vec<String> = device
            .equalizer()?
            .iter()
            .map(|b| format!("{}={}", eq_band_name(b.band), b.value))
            .collect();
        println!("\tEqualizer: {}", bands.join(", "));
    }
    if bose_connect::has_immersive_audio(device_id) {
        let immersive = match device.immersive_audio()? {
            ImmersiveAudio::Off => "off",
            ImmersiveAudio::Still => "still",
            ImmersiveAudio::Motion => "motion",
        };
        println!("\tImmersive Audio: {immersive}");
    }
    if bose_connect::has_battery_prompt(device_id) {
        println!("\tBattery Prompt: {}", on_off(device.battery_prompt()?));
    }
    if bose_connect::has_auto_play_pause(device_id) {
        println!("\tAuto Play/Pause: {}", on_off(device.auto_play_pause()?));
    }
    if bose_connect::has_auto_answer(device_id) {
        println!(
            "\tAuto Answer Calls: {}",
            on_off(device.auto_answer_calls()?)
        );
    }
    if bose_connect::has_shortcut(device_id) {
        let shortcut = match device.shortcut()? {
            ShortcutAction::BatteryLevel => "battery",
            ShortcutAction::ImmersiveAudio => "immersive",
            ShortcutAction::VoiceAssistant => "assistant",
            ShortcutAction::Spotify => "spotify",
            ShortcutAction::ToggleWakeWord => "wake-word",
            ShortcutAction::Off => "off",
        };
        println!("\tShortcut: {shortcut}");
    }
    if bose_connect::has_noise_cancelling_steps(device_id) {
        let nc = device.noise_cancelling_steps()?;
        println!(
            "\tNoise Cancelling: {}/{}",
            nc.level(),
            nc.steps.saturating_sub(1)
        );
        println!("\tNoise Cancelling Favorites: {}", {
            let favorites = device.noise_cancelling_favorites()?;
            favorites
                .levels(nc.steps)
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        });
    }
    if bose_connect::has_nc_settings(device_id) {
        let on_off = |on: bool| if on { "on" } else { "off" };
        println!(
            "\tRemember Last Noise Cancelling: {}",
            on_off(device.remember_noise_cancelling()?)
        );
        println!("\tMotion Auto-Off: {}", on_off(device.motion_auto_off()?));
    }
    Ok(())
}

fn eq_band_name(band: EqBand) -> &'static str {
    match band {
        EqBand::Bass => "bass",
        EqBand::Mid => "mid",
        EqBand::Treble => "treble",
    }
}

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
/// header-based reply check at the protocol level (A2) tells us the
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
            // The NC 700 sends no final result for a remove: the state
            // check below is the test, and waiting again would only
            // repeat the 15 s.
            Err(BoseError::Io(ref io)) if io.kind() == std::io::ErrorKind::TimedOut => {
                eprintln!(
                    "[paired-op:{op}] attempt {attempt}/{MAX_ATTEMPTS}: no final result; checking the device"
                );
            }
            Err(e) => {
                // An ERROR packet is the device's answer; asking again
                // gets the same one.
                if attempt < MAX_ATTEMPTS && !matches!(e, BoseError::DeviceError { .. }) {
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
        if op == "connect" && !wait_until_connected(device, address) {
            if attempt < MAX_ATTEMPTS {
                eprintln!(
                    "[paired-op:{op}] attempt {attempt}/{MAX_ATTEMPTS}: protocol OK but the device is not connected; retrying"
                );
                std::thread::sleep(RETRY_BACKOFF);
                continue;
            }
            return Err(anyhow::anyhow!(
                "connect {canonical}: the device acknowledged the request but is not connected"
            ));
        }
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

/// Poll the headphones' own view of `address` for a few seconds until it
/// reads as connected (the NC 700 never sends a final result for a connect).
fn wait_until_connected(device: &mut BoseDevice, address: BdAddr) -> bool {
    for _ in 0..5 {
        if let Ok(info) = device.device_info(address) {
            if info.status != DeviceStatus::Disconnected {
                return true;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    false
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

/// The NC 700 level as the Bose Music app shows it: `0` (off) to `10`
/// (strongest). `off` and `high` are the two ends.
fn parse_noise_cancelling_level(s: &str) -> Result<u8> {
    match s {
        "off" => Ok(0),
        "high" => Ok(10),
        _ => match s.parse::<u8>() {
            Ok(level) if level <= 10 => Ok(level),
            _ => {
                bail!("invalid noise cancelling argument: {s} (off, high or 0..=10 on this device)")
            }
        },
    }
}

/// `10,5,0`: the NC 700 favourites as the Bose Music app shows them.
fn parse_nc_favorites(s: &str) -> Result<Vec<u8>> {
    s.split(',')
        .map(|part| {
            part.trim()
                .parse::<u8>()
                .map_err(|_| anyhow::anyhow!("invalid noise cancelling favourite: {part:?}"))
        })
        .collect()
}

/// Refuse `setting` on devices that are not the NC 700 rather than
/// writing a function they were never asked about.
fn require_nc_settings(device: &mut BoseDevice, setting: &str) -> Result<()> {
    let (device_id, _) = device.device_id()?;
    if !bose_connect::has_nc_settings(device_id) {
        bail!("--{setting} is only known to work on the NC 700 Headphones");
    }
    Ok(())
}

fn parse_voice_prompts(s: &str) -> Result<bool> {
    Ok(match s {
        "on" => true,
        "off" => false,
        _ => bail!("invalid voice prompt argument: {s}"),
    })
}

fn parse_switch(flag: &str, s: &str) -> Result<bool> {
    Ok(match s {
        "on" => true,
        "off" => false,
        _ => bail!("invalid {flag} argument: {s} (expected on or off)"),
    })
}

/// `bass=4,mid=0,treble=-3` (any subset, any order).
fn parse_equalizer(s: &str) -> Result<Vec<(EqBand, i8)>> {
    s.split(',')
        .map(|pair| {
            let (band, value) = pair.split_once('=').ok_or_else(|| {
                anyhow::anyhow!("invalid equalizer band {pair:?}: use band=value")
            })?;
            let band = match band.trim() {
                "bass" => EqBand::Bass,
                "mid" => EqBand::Mid,
                "treble" => EqBand::Treble,
                other => bail!("invalid equalizer band {other:?}: use bass, mid or treble"),
            };
            let value: i8 = value
                .trim()
                .parse()
                .with_context(|| format!("invalid equalizer value {value:?}"))?;
            Ok((band, value))
        })
        .collect()
}

fn parse_immersive_audio(s: &str) -> Result<ImmersiveAudio> {
    Ok(match s {
        "off" => ImmersiveAudio::Off,
        "still" => ImmersiveAudio::Still,
        "motion" => ImmersiveAudio::Motion,
        _ => bail!("invalid immersive-audio argument: {s} (expected off, still or motion)"),
    })
}

fn parse_shortcut(s: &str) -> Result<ShortcutAction> {
    Ok(match s {
        "battery" => ShortcutAction::BatteryLevel,
        "immersive" => ShortcutAction::ImmersiveAudio,
        "assistant" => ShortcutAction::VoiceAssistant,
        "spotify" => ShortcutAction::Spotify,
        "wake-word" => ShortcutAction::ToggleWakeWord,
        "off" => ShortcutAction::Off,
        _ => bail!(
            "invalid shortcut argument: {s} (expected battery, immersive, assistant, spotify, wake-word or off)"
        ),
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

/// Refuse party-mode commands on devices without them.
fn require_party_mode(device: &mut BoseDevice) -> Result<()> {
    let (device_id, _) = device.device_id()?;
    if !bose_connect::has_party_mode(device_id) {
        bail!("this device does not support party mode (SoundLink Color II only)");
    }
    Ok(())
}

fn party_mode_name(mode: PartyMode) -> &'static str {
    match mode {
        PartyMode::Off => "off",
        PartyMode::Party => "party",
        PartyMode::Stereo => "stereo (this speaker left, the other right)",
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    const ADDRESS: &str = "AA:BB:CC:DD:EE:FF";

    /// One example invocation per command flag.
    fn commands() -> Vec<Vec<&'static str>> {
        let mut cmd = Cli::command();
        // Resolves `#[arg(group = ...)]` into the group's members.
        cmd.build();
        let group = cmd
            .get_groups()
            .find(|g| g.get_id() == "command")
            .expect("command group");
        group
            .get_args()
            .map(|id| {
                let arg = cmd.get_arguments().find(|a| a.get_id() == id).unwrap();
                let flag: &'static str = format!("--{}", arg.get_long().unwrap()).leak();
                if arg.get_action().takes_values() {
                    vec![flag, "1"]
                } else {
                    vec![flag]
                }
            })
            .collect()
    }

    #[test]
    fn every_command_flag_is_in_the_command_group() {
        let mut cmd = Cli::command();
        cmd.build();
        let in_group = commands().len();
        // Every argument except the address, the modifiers --channel and
        // --party-source, --help and --version.
        let all = cmd
            .get_arguments()
            .filter(|a| {
                !matches!(
                    a.get_id().as_str(),
                    "address" | "channel" | "party_source" | "help" | "version"
                )
            })
            .count();
        assert_eq!(in_group, all);
    }

    #[test]
    fn two_commands_are_refused() {
        let commands = commands();
        for (i, a) in commands.iter().enumerate() {
            for b in &commands[i + 1..] {
                let args = [&["prog", ADDRESS][..], a, b].concat();
                assert!(Cli::try_parse_from(&args).is_err(), "accepted {args:?}");
            }
        }
    }

    #[test]
    fn channel_combines_with_any_command() {
        for command in commands() {
            let args = [&["prog", ADDRESS, "--channel", "2"][..], &command].concat();
            assert!(Cli::try_parse_from(&args).is_ok(), "refused {args:?}");
        }
    }
}

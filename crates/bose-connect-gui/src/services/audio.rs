//! This computer's audio side of party mode (SoundLink Color II):
//! the PipeWire sink of a Bluetooth device, the restart of its
//! audio stream after a mode change, pausing the media players, and
//! the sample rate of the audio link.
//!
//! With this computer as the music source, two SoundLink Color II
//! worked in party mode only when the audio link used SBC at
//! 44.1 kHz, the computer sent no audio for a minute before the
//! join, and the computer restarted its audio stream after every
//! mode change (see "Party and stereo mode" in `docs/protocol.md`).
//!
//! Every function here blocks (child processes, D-Bus calls), so
//! callers run them off the GTK main thread.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};

/// How long the computer must send no audio before a join.
pub const QUIET_BEFORE_JOIN: Duration = Duration::from_secs(60);

/// Pause between suspending and resuming the sink.
const RESTART_PAUSE: Duration = Duration::from_millis(1_000);

/// Sample rate the SoundLink Color II needs in party mode.
pub const PARTY_SAMPLE_RATE: u32 = 44_100;

/// A2DP codec id of SBC in `org.bluez.MediaTransport1.Codec`.
const A2DP_CODEC_SBC: u8 = 0x00;

/// The sample rate of an SBC configuration (A2DP specification,
/// "Codec Specific Information Elements" of SBC): the high nibble of
/// the first byte has one bit per rate. `None` for other codecs or a
/// malformed configuration.
pub fn sbc_sample_rate(codec: u8, configuration: &[u8]) -> Option<u32> {
    if codec != A2DP_CODEC_SBC {
        return None;
    }
    match configuration.first()? & 0xf0 {
        0x80 => Some(16_000),
        0x40 => Some(32_000),
        0x20 => Some(44_100),
        0x10 => Some(48_000),
        _ => None,
    }
}

/// The name and state (`RUNNING`, `IDLE`, `SUSPENDED`) of the
/// PipeWire sink of the Bluetooth device at `address` in the output
/// of `pactl list sinks short` (tab-separated `index, name, driver,
/// format, state`), for example `bluez_output.AA_BB_CC_DD_EE_FF.1`.
pub fn bluez_sink(address: &str, pactl_sinks: &str) -> Option<(String, String)> {
    let prefix = format!("bluez_output.{}.", address.replace(':', "_")).to_ascii_uppercase();
    pactl_sinks.lines().find_map(|line| {
        let mut fields = line.split('\t');
        let name = fields.nth(1)?.trim();
        let state = fields.nth(2).unwrap_or_default().trim();
        name.to_ascii_uppercase()
            .starts_with(&prefix)
            .then(|| (name.to_owned(), state.to_owned()))
    })
}

/// Restart the audio stream to the device at `address` when it is
/// playing: suspend its sink and resume it, which makes BlueZ send an
/// AVDTP Suspend and a Start. A SoundLink Color II that was playing
/// stays silent after a party-mode change until this happens.
///
/// A sink that is not playing is left alone: the next song starts a
/// new stream anyway, and resuming an idle sink right after a join
/// was followed by a lockup of the other speaker (SoundLink Color II,
/// firmware 4.0.1). Returns whether the stream was restarted.
pub fn restart_stream(address: &str) -> Result<bool> {
    let sinks = pactl(&["list", "sinks", "short"])?;
    let Some((sink, state)) = bluez_sink(address, &sinks) else {
        return Ok(false);
    };
    if state != "RUNNING" {
        tracing::info!(target: "audio", "{sink} is {state}: no restart needed");
        return Ok(false);
    }
    pactl(&["suspend-sink", &sink, "1"])?;
    std::thread::sleep(RESTART_PAUSE);
    pactl(&["suspend-sink", &sink, "0"])?;
    tracing::info!(target: "audio", "restarted the audio stream of {sink}");
    Ok(true)
}

fn pactl(args: &[&str]) -> Result<String> {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .context("cannot run pactl (install pipewire-pulse or pulseaudio-utils)")?;
    if !output.status.success() {
        bail!(
            "pactl {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Ask every media player on the session bus (MPRIS) to pause.
/// Returns how many players accepted. A player without MPRIS support
/// keeps playing; the countdown before a join waits for it.
pub fn pause_media_players() -> usize {
    use gtk::gio;
    let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else {
        return 0;
    };
    let names = conn
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
            None,
            None,
            gio::DBusCallFlags::NONE,
            1_500,
            None::<&gio::Cancellable>,
        )
        .ok()
        .and_then(|reply| reply.try_child_value(0))
        .and_then(|names| names.get::<Vec<String>>())
        .unwrap_or_default();
    names
        .iter()
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
        .filter(|name| {
            conn.call_sync(
                Some(name),
                "/org/mpris/MediaPlayer2",
                "org.mpris.MediaPlayer2.Player",
                "Pause",
                None,
                None,
                gio::DBusCallFlags::NONE,
                1_500,
                None::<&gio::Cancellable>,
            )
            .is_ok()
        })
        .count()
}

/// The WirePlumber drop-in that makes PipeWire pick 44.1 kHz for the
/// Bluetooth audio links this computer opens.
pub fn wireplumber_rate_file() -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    Some(
        dirs.config_dir()
            .join("wireplumber/wireplumber.conf.d/60-bose-connect-44100.conf"),
    )
}

/// The content of [`wireplumber_rate_file`].
pub const WIREPLUMBER_RATE_CONF: &str = "\
# Written by Bose Connect for Linux: the SoundLink Color II needs
# 44.1 kHz in party mode. Delete this file to go back to 48 kHz.
monitor.bluez.properties = {
  bluez5.default.rate = 44100
}
";

/// Write [`WIREPLUMBER_RATE_CONF`] and restart WirePlumber so it
/// reads it. Audio links that are already open keep their rate until
/// the device connects again.
pub fn use_44100_for_bluetooth() -> Result<PathBuf> {
    let path = wireplumber_rate_file().context("cannot find the configuration directory")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    std::fs::write(&path, WIREPLUMBER_RATE_CONF)
        .with_context(|| format!("cannot write {}", path.display()))?;
    let status = Command::new("systemctl")
        .args(["--user", "restart", "wireplumber"])
        .status()
        .context("cannot run systemctl to restart WirePlumber")?;
    if !status.success() {
        bail!("systemctl --user restart wireplumber failed; restart it or log in again");
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sbc_rates() {
        // Configurations BlueZ reported for a SoundLink Color II.
        assert_eq!(sbc_sample_rate(0, &[0x21, 0x15, 2, 53]), Some(44_100));
        assert_eq!(sbc_sample_rate(0, &[0x11, 0x15, 2, 53]), Some(48_000));
        assert_eq!(sbc_sample_rate(0, &[0x41]), Some(32_000));
        assert_eq!(sbc_sample_rate(0, &[0x81]), Some(16_000));
        assert_eq!(sbc_sample_rate(0, &[]), None);
        assert_eq!(
            sbc_sample_rate(0, &[0x31]),
            None,
            "two rates: not a configuration"
        );
        assert_eq!(sbc_sample_rate(2, &[0x21]), None, "AAC is not decoded");
    }

    #[test]
    fn finds_the_sink_of_a_device() {
        let sinks = "49\talsa_output.pci-0000_00_1f.3.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n\
                     87\tbluez_output.02_00_00_00_00_05.1\tPipeWire\ts16le 2ch 44100Hz\tRUNNING\n";
        assert_eq!(
            bluez_sink("02:00:00:00:00:05", sinks),
            Some((
                "bluez_output.02_00_00_00_00_05.1".to_owned(),
                "RUNNING".to_owned()
            ))
        );
        assert_eq!(
            bluez_sink(
                "02:00:00:00:00:0a",
                "1\tbluez_output.02_00_00_00_00_0A.a2dp-sink\tPipeWire\tx\tSUSPENDED"
            ),
            Some((
                "bluez_output.02_00_00_00_00_0A.a2dp-sink".to_owned(),
                "SUSPENDED".to_owned()
            ))
        );
        assert_eq!(bluez_sink("02:00:00:00:00:06", sinks), None);
        assert_eq!(bluez_sink("02:00:00:00:00:05", ""), None);
    }

    #[test]
    fn the_wireplumber_drop_in_sets_the_rate() {
        assert!(WIREPLUMBER_RATE_CONF.contains("bluez5.default.rate = 44100"));
        assert!(WIREPLUMBER_RATE_CONF.contains("monitor.bluez.properties"));
    }
}

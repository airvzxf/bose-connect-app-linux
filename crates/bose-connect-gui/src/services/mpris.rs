//! This computer's media players, over MPRIS on the session bus.
//!
//! When this computer is the device's music source, the device knows
//! the track only if an AVRCP media player is registered with BlueZ
//! (`mpris-proxy`), and its media keys reach the player only through
//! the desktop's key handling. The window reads and controls the
//! player directly instead.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use gtk::gio;
use gtk::glib::{self, prelude::*, Variant, VariantDict};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const TIMEOUT_MS: i32 = 1_000;

/// What a player reports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerStatus {
    /// Its bus name, to send commands back to the same player.
    pub bus_name: String,
    /// `PlaybackStatus` is "Playing".
    pub playing: bool,
    /// `PlaybackStatus` is "Paused".
    pub paused: bool,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Seconds into the track.
    pub position: Option<u32>,
    /// Track length in seconds.
    pub length: Option<u32>,
}

/// The player to show: the first one playing, else the first paused
/// one. `None` when no player runs.
pub fn current_player() -> Result<Option<PlayerStatus>> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .context("cannot reach the session bus")?;
    let names = call(
        &bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListNames",
        None,
    )?;
    let names: Vec<String> = names
        .child_value(0)
        .get()
        .ok_or_else(|| anyhow!("unexpected ListNames reply"))?;
    let mut paused = None;
    for name in names.into_iter().filter(|n| n.starts_with(PREFIX)) {
        let Ok(status) = read_player(&bus, &name) else {
            continue;
        };
        if status.playing {
            return Ok(Some(status));
        }
        if status.paused && paused.is_none() {
            paused = Some(status);
        }
    }
    Ok(paused)
}

/// Send `PlayPause`, `Next` or `Previous` to the player `bus_name`.
pub fn command(bus_name: &str, method: &str) -> Result<()> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .context("cannot reach the session bus")?;
    call(&bus, bus_name, PATH, PLAYER, method, None)?;
    Ok(())
}

fn read_player(bus: &gio::DBusConnection, name: &str) -> Result<PlayerStatus> {
    let all = call(
        bus,
        name,
        PATH,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        Some(&(PLAYER,).to_variant()),
    )?;
    let props = VariantDict::new(Some(&all.child_value(0)));
    let status: String = lookup(&props, "PlaybackStatus").unwrap_or_default();
    let metadata = props
        .lookup_value("Metadata", None)
        .map(|v| VariantDict::new(Some(&v)));
    let text = |key: &str| -> Option<String> {
        let value = metadata.as_ref()?.lookup_value(key, None)?;
        value
            .get::<String>()
            .or_else(|| value.get::<Vec<String>>().map(|v| v.join(", ")))
            .filter(|s| !s.is_empty())
    };
    let micros = |value: Option<Variant>| -> Option<u32> {
        let value = value?;
        let us = value
            .get::<i64>()
            .or_else(|| value.get::<u64>().and_then(|v| i64::try_from(v).ok()))?;
        u32::try_from(Duration::from_micros(u64::try_from(us).ok()?).as_secs()).ok()
    };
    Ok(PlayerStatus {
        bus_name: name.to_owned(),
        playing: status == "Playing",
        paused: status == "Paused",
        title: text("xesam:title"),
        artist: text("xesam:artist"),
        album: text("xesam:album"),
        position: micros(props.lookup_value("Position", None)),
        length: micros(
            metadata
                .as_ref()
                .and_then(|m| m.lookup_value("mpris:length", None)),
        ),
    })
}

fn lookup<T: glib::variant::FromVariant>(dict: &VariantDict, key: &str) -> Option<T> {
    dict.lookup_value(key, None)?.get()
}

fn call(
    bus: &gio::DBusConnection,
    name: &str,
    path: &str,
    interface: &str,
    method: &str,
    args: Option<&Variant>,
) -> Result<Variant> {
    bus.call_sync(
        Some(name),
        path,
        interface,
        method,
        args,
        None,
        gio::DBusCallFlags::NONE,
        TIMEOUT_MS,
        gio::Cancellable::NONE,
    )
    .with_context(|| format!("{name}: {method}"))
}

#[cfg(test)]
mod tests {
    /// Needs a desktop session; run by hand with `--ignored`.
    #[test]
    #[ignore]
    fn reads_the_running_player() {
        println!("{:#?}", super::current_player());
    }
}

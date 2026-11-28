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
    let mut paused = None;
    for name in player_names(&bus)? {
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

/// Every player that reports "Playing".
pub fn playing_players() -> Vec<PlayerStatus> {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return Vec::new();
    };
    player_names(&bus)
        .unwrap_or_default()
        .iter()
        .filter_map(|name| read_player(&bus, name).ok())
        .filter(|status| status.playing)
        .collect()
}

impl PlayerStatus {
    /// "Title — Artist", or the player's short name when it reports
    /// no title.
    pub fn describe(&self) -> String {
        match (&self.title, &self.artist) {
            (Some(title), Some(artist)) => format!("{title} — {artist}"),
            (Some(title), None) => title.clone(),
            _ => self
                .bus_name
                .trim_start_matches(PREFIX)
                .split('.')
                .next()
                .unwrap_or_default()
                .to_owned(),
        }
    }
}

/// Call `on_change` on the main context each time a player changes
/// its playback status or track (`PropertiesChanged` of the player
/// interface) or jumps in the track (`Seeked`). The watch lasts as
/// long as the returned subscriptions.
pub fn watch_players(on_change: impl Fn() + 'static) -> Result<[gio::SignalSubscription; 2]> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .context("cannot reach the session bus")?;
    let on_change = std::rc::Rc::new(on_change);
    let changed = {
        let on_change = on_change.clone();
        bus.subscribe_to_signal(
            None,
            Some("org.freedesktop.DBus.Properties"),
            Some("PropertiesChanged"),
            Some(PATH),
            Some(PLAYER),
            gio::DBusSignalFlags::NONE,
            move |signal| {
                if affects_now_playing(signal.parameters) {
                    on_change();
                }
            },
        )
    };
    let seeked = bus.subscribe_to_signal(
        None,
        Some(PLAYER),
        Some("Seeked"),
        Some(PATH),
        None,
        gio::DBusSignalFlags::NONE,
        move |_| on_change(),
    );
    Ok([changed, seeked])
}

/// Whether a `PropertiesChanged(interface, changed, invalidated)`
/// touches what the window shows. Volume or rate changes do not.
fn affects_now_playing(parameters: &Variant) -> bool {
    const SHOWN: [&str; 2] = ["PlaybackStatus", "Metadata"];
    // `VariantDict::new` panics on anything but a{sv}.
    let changed = parameters
        .try_child_value(1)
        .filter(|c| c.is_type(glib::VariantTy::VARDICT))
        .map(|c| VariantDict::new(Some(&c)));
    let invalidated: Vec<String> = parameters
        .try_child_value(2)
        .and_then(|v| v.get())
        .unwrap_or_default();
    SHOWN.iter().any(|key| {
        changed.as_ref().is_some_and(|c| c.contains(key)) || invalidated.iter().any(|i| i == key)
    })
}

/// Send `PlayPause`, `Next` or `Previous` to the player `bus_name`.
pub fn command(bus_name: &str, method: &str) -> Result<()> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .context("cannot reach the session bus")?;
    call(&bus, bus_name, PATH, PLAYER, method, None)?;
    Ok(())
}

/// The bus names of the running players.
fn player_names(bus: &gio::DBusConnection) -> Result<Vec<String>> {
    let names = call(
        bus,
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
    Ok(names
        .into_iter()
        .filter(|n| n.starts_with(PREFIX))
        .collect())
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
    use super::*;

    fn properties_changed(changed: &[&str], invalidated: &[&str]) -> Variant {
        let dict = VariantDict::new(None);
        for key in changed {
            dict.insert(key, "x");
        }
        Variant::tuple_from_iter([
            PLAYER.to_variant(),
            dict.end(),
            invalidated.to_vec().to_variant(),
        ])
    }

    #[test]
    fn only_status_and_track_changes_count() {
        assert!(affects_now_playing(&properties_changed(
            &["PlaybackStatus"],
            &[]
        )));
        assert!(affects_now_playing(&properties_changed(
            &["Volume", "Metadata"],
            &[]
        )));
        assert!(affects_now_playing(&properties_changed(&[], &["Metadata"])));
        assert!(!affects_now_playing(&properties_changed(
            &["Volume", "Rate"],
            &[]
        )));
        assert!(!affects_now_playing(&("x", 1u32).to_variant()), "malformed");
    }

    #[test]
    fn describes_a_player() {
        let mut p = PlayerStatus {
            bus_name: "org.mpris.MediaPlayer2.spotify.instance42".into(),
            ..PlayerStatus::default()
        };
        assert_eq!(p.describe(), "spotify");
        p.title = Some("Song".into());
        assert_eq!(p.describe(), "Song");
        p.artist = Some("Band".into());
        assert_eq!(p.describe(), "Song — Band");
    }

    /// Needs a desktop session; run by hand with `--ignored`.
    #[test]
    #[ignore]
    fn reads_the_running_player() {
        println!("{:#?}", super::current_player());
    }
}

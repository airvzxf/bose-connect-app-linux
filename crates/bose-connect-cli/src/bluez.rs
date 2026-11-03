//! Read a device's audio state from BlueZ over the D-Bus system bus.
//!
//! One `GetManagedObjects` call on `org.bluez`: the `org.bluez.Device1`
//! object whose `Address` matches, and the `org.bluez.MediaTransport1`
//! objects whose `Device` is that object. See
//! [`bose_connect::AudioState`].

use std::collections::HashMap;

use anyhow::{Context, Result};
use bose_connect::AudioState;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

type Properties = HashMap<String, OwnedValue>;
type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

/// Ask BlueZ for the audio state of the device at `address`
/// (`AA:BB:CC:DD:EE:FF`, any case). Does not touch the device.
pub fn audio_state(address: &str) -> Result<AudioState> {
    let conn = zbus::blocking::Connection::system().context("cannot reach the D-Bus system bus")?;
    let reply = conn
        .call_method(
            Some("org.bluez"),
            "/",
            Some("org.freedesktop.DBus.ObjectManager"),
            "GetManagedObjects",
            &(),
        )
        .context("cannot query BlueZ (is bluetoothd running?)")?;
    let objects: ManagedObjects = reply
        .body()
        .deserialize()
        .context("unexpected GetManagedObjects reply")?;
    Ok(classify(&objects, address))
}

fn classify(objects: &ManagedObjects, address: &str) -> AudioState {
    let device = objects.iter().find_map(|(path, ifaces)| {
        let props = ifaces.get("org.bluez.Device1")?;
        let addr = string(props.get("Address")?)?;
        addr.eq_ignore_ascii_case(address).then(|| {
            (
                path,
                props.get("Connected").and_then(boolean).unwrap_or(false),
            )
        })
    });
    let Some((device_path, connected)) = device else {
        return AudioState::classify::<&str>(None, &[]);
    };
    let states: Vec<String> = objects
        .values()
        .filter_map(|ifaces| ifaces.get("org.bluez.MediaTransport1"))
        .filter(|props| {
            props
                .get("Device")
                .and_then(|v| OwnedObjectPath::try_from(v.try_clone().ok()?).ok())
                .is_some_and(|p| &p == device_path)
        })
        .map(|props| {
            props
                .get("State")
                .and_then(string)
                .unwrap_or_else(|| "idle".to_string())
        })
        .collect();
    AudioState::classify(Some(connected), &states)
}

fn string(value: &OwnedValue) -> Option<String> {
    String::try_from(value.try_clone().ok()?).ok()
}

fn boolean(value: &OwnedValue) -> Option<bool> {
    bool::try_from(value).ok()
}

/// One-line description for the CLI.
pub fn describe(state: AudioState) -> &'static str {
    match state {
        AudioState::Unknown => "unknown to BlueZ (not paired with this computer)",
        AudioState::Disconnected => "not connected to this computer",
        AudioState::ConnectedWithoutAudio => "connected, but without audio to this computer",
        AudioState::Idle => "connected, audio ready (not playing)",
        AudioState::Playing => "connected, playing audio from this computer",
    }
}

/// Advice printed after [`describe`] when the state calls for it.
pub const RECONNECT_HINT: &str = "If you expect sound from this computer, disconnect the \
device and connect it again from this computer (for example `bluetoothctl disconnect ADDR` \
and then `bluetoothctl connect ADDR`). When the device starts the connection itself, BlueZ \
can fail to open the audio stream.";

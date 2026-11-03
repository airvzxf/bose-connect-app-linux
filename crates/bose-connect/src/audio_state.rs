//! How BlueZ sees a device's audio link on this computer.
//!
//! The Bose protocol runs on its own RFCOMM channel and says nothing
//! about whether this computer has an audio stream to the device. That
//! comes from BlueZ over D-Bus: `org.bluez.Device1.Connected`, and the
//! `org.bluez.MediaTransport1` objects whose `Device` property is that
//! device. This module only classifies those facts; the CLI and the
//! GUI read them from D-Bus.
//!
//! Why it matters: on some Linux systems, when the headphones start the
//! connection, BlueZ starts its own audio (A2DP) connection at the same
//! time, the two collide (`a2dp-sink profile connect failed: Device or
//! resource busy`), and the device ends up connected with no working
//! audio stream. Disconnecting and reconnecting from this computer
//! fixes it (QC Ultra on CachyOS, 2026-10-29: 0 of 6 headphone-started
//! connections played, 5 of 5 computer-started ones did).
//!
//! In that failure BlueZ still creates a media transport, and even
//! reports it `active` while a player tries to use it, but PipeWire's
//! `Acquire` gets no reply. The transport is then a direct child of
//! the device (`/org/bluez/hci0/dev_XX/fd0`) instead of a child of
//! one of the device's stream endpoints (`/org/bluez/hci0/dev_XX/sep6/fd0`)
//! as after a working connection. Seen 2 of 2 times on a QC Ultra,
//! BlueZ on CachyOS (2026-11-03); the working connections had `sepN`.

/// Audio link between this computer and a device, as BlueZ reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioState {
    /// BlueZ does not know the device (not paired with this computer).
    Unknown,
    /// Known to BlueZ but not connected.
    Disconnected,
    /// Connected, but without a usable media transport (none, or only
    /// one outside a stream endpoint): this computer cannot send audio
    /// to the device.
    ConnectedWithoutAudio,
    /// Connected with a media transport that is not streaming.
    Idle,
    /// Connected and streaming (or about to: BlueZ state `pending`).
    Playing,
}

impl AudioState {
    /// Classify the BlueZ facts. `device` is `None` when BlueZ has no
    /// `Device1` object for the address, else its `Connected`
    /// property. `transports` are the object path and `State`
    /// (`idle`, `pending`, `active`, `broadcasting`) of the device's
    /// `MediaTransport1` objects. A transport whose path has no
    /// `sepN` component is not usable (see the module docs).
    pub fn classify<P: AsRef<str>, S: AsRef<str>>(
        device: Option<bool>,
        transports: &[(P, S)],
    ) -> Self {
        let usable: Vec<&str> = transports
            .iter()
            .filter(|(path, _)| has_stream_endpoint(path.as_ref()))
            .map(|(_, state)| state.as_ref())
            .collect();
        match device {
            None => Self::Unknown,
            Some(false) => Self::Disconnected,
            Some(true) if usable.is_empty() => Self::ConnectedWithoutAudio,
            Some(true) => {
                let playing = usable
                    .iter()
                    .any(|s| matches!(*s, "active" | "pending" | "broadcasting"));
                if playing {
                    Self::Playing
                } else {
                    Self::Idle
                }
            }
        }
    }

    /// True when the device is connected but has no audio stream to
    /// this computer, the state worth a hint to the user.
    pub fn needs_reconnect_hint(self) -> bool {
        self == Self::ConnectedWithoutAudio
    }
}

/// True when a transport path has a `sepN` component, i.e. BlueZ made
/// it for one of the device's stream endpoints.
fn has_stream_endpoint(path: &str) -> bool {
    path.split('/').any(|part| {
        part.strip_prefix("sep")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

#[cfg(test)]
mod tests {
    use super::AudioState;

    const NONE: [(&str, &str); 0] = [];
    const DEV: &str = "/org/bluez/hci0/dev_BC_87_FA_45_4C_C2";

    #[test]
    fn unknown_and_disconnected() {
        assert_eq!(AudioState::classify(None, &NONE), AudioState::Unknown);
        assert_eq!(
            AudioState::classify(Some(false), &[(format!("{DEV}/sep6/fd0"), "active")]),
            AudioState::Disconnected
        );
    }

    #[test]
    fn connected_without_transport_needs_a_hint() {
        let state = AudioState::classify(Some(true), &NONE);
        assert_eq!(state, AudioState::ConnectedWithoutAudio);
        assert!(state.needs_reconnect_hint());
    }

    #[test]
    fn transport_states() {
        assert_eq!(
            AudioState::classify(Some(true), &[(format!("{DEV}/sep6/fd0"), "idle")]),
            AudioState::Idle
        );
        assert_eq!(
            AudioState::classify(
                Some(true),
                &[
                    (format!("{DEV}/sep5/fd1"), "idle"),
                    (format!("{DEV}/sep6/fd0"), "active")
                ]
            ),
            AudioState::Playing
        );
        assert_eq!(
            AudioState::classify(Some(true), &[(format!("{DEV}/sep6/fd0"), "pending")]),
            AudioState::Playing
        );
        assert!(!AudioState::Idle.needs_reconnect_hint());
    }

    /// QC Ultra, 2026-11-03: after a headphone-started connection the
    /// transport was `dev_…/fd0`, `active`, and nothing played.
    #[test]
    fn transport_outside_a_stream_endpoint_is_not_usable() {
        let state = AudioState::classify(Some(true), &[(format!("{DEV}/fd0"), "active")]);
        assert_eq!(state, AudioState::ConnectedWithoutAudio);
        let mixed = [
            (format!("{DEV}/fd0"), "active"),
            (format!("{DEV}/sep6/fd1"), "idle"),
        ];
        assert_eq!(AudioState::classify(Some(true), &mixed), AudioState::Idle);
    }
}

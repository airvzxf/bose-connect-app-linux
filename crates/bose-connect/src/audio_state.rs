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
//! time, the two collide, and the device ends up connected with no
//! audio stream. Disconnecting and reconnecting from this computer
//! fixes it (QC Ultra on CachyOS, 2026-10-29: 0 of 6 headphone-started
//! connections played, 5 of 5 computer-started ones did).

/// Audio link between this computer and a device, as BlueZ reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioState {
    /// BlueZ does not know the device (not paired with this computer).
    Unknown,
    /// Known to BlueZ but not connected.
    Disconnected,
    /// Connected, but BlueZ has no media transport for it: this
    /// computer cannot send audio to the device.
    ConnectedWithoutAudio,
    /// Connected with a media transport that is not streaming.
    Idle,
    /// Connected and streaming (or about to: BlueZ state `pending`).
    Playing,
}

impl AudioState {
    /// Classify the BlueZ facts. `device` is `None` when BlueZ has no
    /// `Device1` object for the address, else its `Connected`
    /// property. `transport_states` are the `State` properties
    /// (`idle`, `pending`, `active`, `broadcasting`) of the device's
    /// `MediaTransport1` objects.
    pub fn classify<S: AsRef<str>>(device: Option<bool>, transport_states: &[S]) -> Self {
        match device {
            None => Self::Unknown,
            Some(false) => Self::Disconnected,
            Some(true) if transport_states.is_empty() => Self::ConnectedWithoutAudio,
            Some(true) => {
                let playing = transport_states
                    .iter()
                    .any(|s| matches!(s.as_ref(), "active" | "pending" | "broadcasting"));
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

#[cfg(test)]
mod tests {
    use super::AudioState;

    const NONE: [&str; 0] = [];

    #[test]
    fn unknown_and_disconnected() {
        assert_eq!(AudioState::classify(None, &NONE), AudioState::Unknown);
        assert_eq!(
            AudioState::classify(Some(false), &["active"]),
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
            AudioState::classify(Some(true), &["idle"]),
            AudioState::Idle
        );
        assert_eq!(
            AudioState::classify(Some(true), &["idle", "active"]),
            AudioState::Playing
        );
        assert_eq!(
            AudioState::classify(Some(true), &["pending"]),
            AudioState::Playing
        );
        assert!(!AudioState::Idle.needs_reconnect_hint());
    }
}

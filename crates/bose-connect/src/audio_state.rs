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
//! one of the device's stream endpoints (`/org/bluez/hci0/dev_XX/sep6/fd0`).
//!
//! Such a path only means that the device started the audio stream
//! itself; it is not always broken. On a QC35 II (firmware 4.8.1) a
//! `dev_XX/fd0` transport played fine (2026-11-03). On a QC Ultra
//! (firmware 1.6.7, BlueZ on CachyOS) it never played: 3 of 3 times on
//! 2026-11-03, and 6 of 6 headphone-started connections on 2026-10-29.
//! So it counts as broken only for the products in
//! [`DEVICE_STARTED_STREAM_FAILS`], read from BlueZ's `Modalias`.

/// Bose product ids (as in `Modalias` `bluetooth:v009EpXXXX…`) whose
/// device-started audio stream did not play on this computer: the QC
/// Ultra Headphones.
pub const DEVICE_STARTED_STREAM_FAILS: [u16; 1] = [0x4066];

/// Audio link between this computer and a device, as BlueZ reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioState {
    /// BlueZ does not know the device (not paired with this computer).
    Unknown,
    /// Known to BlueZ but not connected.
    Disconnected,
    /// Connected, but without a usable media transport (none, or only
    /// a device-started one on a product in
    /// [`DEVICE_STARTED_STREAM_FAILS`]): this computer cannot send audio
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
    /// `MediaTransport1` objects. `product_id` is the Bose product id
    /// from the device's `Modalias` (see [`product_id_from_modalias`]).
    /// A transport whose path has no `sepN` component is not usable on
    /// the products in [`DEVICE_STARTED_STREAM_FAILS`] (see the module
    /// docs).
    pub fn classify<P: AsRef<str>, S: AsRef<str>>(
        device: Option<bool>,
        product_id: Option<u16>,
        transports: &[(P, S)],
    ) -> Self {
        let device_started_fails =
            product_id.is_some_and(|id| DEVICE_STARTED_STREAM_FAILS.contains(&id));
        let usable: Vec<&str> = transports
            .iter()
            .filter(|(path, _)| !device_started_fails || has_stream_endpoint(path.as_ref()))
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

/// The Bose product id in a BlueZ `Modalias`
/// (`bluetooth:v009Ep4066d0167` → `0x4066`), or `None` when the vendor
/// is not Bose (`009E`) or the string has another shape.
pub fn product_id_from_modalias(modalias: &str) -> Option<u16> {
    let rest = modalias.strip_prefix("bluetooth:v")?;
    let (vendor, rest) = rest.split_at_checked(4)?;
    if !vendor.eq_ignore_ascii_case("009E") {
        return None;
    }
    let product = rest.strip_prefix('p').or_else(|| rest.strip_prefix('P'))?;
    u16::from_str_radix(product.get(..4)?, 16).ok()
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
    use super::{product_id_from_modalias, AudioState};

    const NONE: [(&str, &str); 0] = [];
    const DEV: &str = "/org/bluez/hci0/dev_BC_87_FA_45_4C_C2";
    const ULTRA: Option<u16> = Some(0x4066);

    #[test]
    fn unknown_and_disconnected() {
        assert_eq!(AudioState::classify(None, None, &NONE), AudioState::Unknown);
        assert_eq!(
            AudioState::classify(Some(false), ULTRA, &[(format!("{DEV}/sep6/fd0"), "active")]),
            AudioState::Disconnected
        );
    }

    #[test]
    fn connected_without_transport_needs_a_hint() {
        let state = AudioState::classify(Some(true), ULTRA, &NONE);
        assert_eq!(state, AudioState::ConnectedWithoutAudio);
        assert!(state.needs_reconnect_hint());
    }

    #[test]
    fn transport_states() {
        assert_eq!(
            AudioState::classify(Some(true), ULTRA, &[(format!("{DEV}/sep6/fd0"), "idle")]),
            AudioState::Idle
        );
        assert_eq!(
            AudioState::classify(
                Some(true),
                ULTRA,
                &[
                    (format!("{DEV}/sep5/fd1"), "idle"),
                    (format!("{DEV}/sep6/fd0"), "active")
                ]
            ),
            AudioState::Playing
        );
        assert_eq!(
            AudioState::classify(Some(true), ULTRA, &[(format!("{DEV}/sep6/fd0"), "pending")]),
            AudioState::Playing
        );
        assert!(!AudioState::Idle.needs_reconnect_hint());
    }

    /// QC Ultra, 2026-11-03: after a headphone-started connection the
    /// transport was `dev_…/fd0`, `active`, and nothing played.
    #[test]
    fn transport_outside_a_stream_endpoint_is_not_usable() {
        let state = AudioState::classify(Some(true), ULTRA, &[(format!("{DEV}/fd0"), "active")]);
        assert_eq!(state, AudioState::ConnectedWithoutAudio);
        let mixed = [
            (format!("{DEV}/fd0"), "active"),
            (format!("{DEV}/sep6/fd1"), "idle"),
        ];
        assert_eq!(
            AudioState::classify(Some(true), ULTRA, &mixed),
            AudioState::Idle
        );
    }

    /// QC35 II, 2026-11-03: a `dev_…/fd0` transport after a
    /// headphone-started connection played fine.
    #[test]
    fn device_started_stream_is_fine_on_other_products() {
        let path = "/org/bluez/hci0/dev_2C_41_A1_02_6D_6F/fd0";
        let qc35 = Some(0x4020);
        assert_eq!(
            AudioState::classify(Some(true), qc35, &[(path, "active")]),
            AudioState::Playing
        );
        assert_eq!(
            AudioState::classify(Some(true), None, &[(path, "idle")]),
            AudioState::Idle
        );
    }

    #[test]
    fn modalias() {
        assert_eq!(
            product_id_from_modalias("bluetooth:v009Ep4066d0167"),
            Some(0x4066)
        );
        assert_eq!(
            product_id_from_modalias("bluetooth:v009EP400Dd0401"),
            Some(0x400d)
        );
        assert_eq!(product_id_from_modalias("usb:v05ACp0257d0001"), None);
        assert_eq!(product_id_from_modalias("bluetooth:v004Cp0001d0001"), None);
        assert_eq!(product_id_from_modalias(""), None);
    }
}

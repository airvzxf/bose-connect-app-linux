//! MPRIS MediaPlayer2 service stub.
//!
//! `org.mpris.MediaPlayer2.bose-connect-gui` would expose the
//! headphone state to KDE Connect, Plasma's audio widget, and any
//! other MPRIS-aware tool. Implementing the full D-Bus interface
//! is out-of-scope for the first iteration (we wire it via
//! `busctl` introspection later); for now we keep the in-process
//! representation so the GUI can show its current PlaybackStatus
//! badge even before the D-Bus object is published.

use parking_lot::Mutex;

#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    pub playback_status: String,        // "Playing" | "Paused" | "Stopped"
    pub noise_cancelling_level: String, // "off" | "low" | "high"
}

pub struct MediaPlayerService {
    state: Mutex<PlayerState>,
}

impl MediaPlayerService {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(PlayerState {
                playback_status: "Stopped".into(),
                noise_cancelling_level: "high".into(),
            }),
        }
    }

    pub fn set_status(&self, status: &str) {
        self.state.lock().playback_status = status.to_string();
    }

    pub fn set_noise_cancelling(&self, level: &str) {
        self.state.lock().noise_cancelling_level = level.to_string();
    }

    pub fn state(&self) -> PlayerState {
        self.state.lock().clone()
    }
}

impl Default for MediaPlayerService {
    fn default() -> Self {
        Self::new()
    }
}

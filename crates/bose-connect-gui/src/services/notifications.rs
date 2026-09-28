//! D-Bus / libnotify native notifications via `notify-rust`.
//!
//! On KDE Plasma 6 (the host that ships this code) `notify-rust`
//! publishes to `org.freedesktop.Notifications`, which is the same
//! service the Plasma notification popups use. The
//! `cinnamon`/`gnome-flashback`/`kde5-notifications` fallbacks all
//! read from the same bus name, so this single pathway covers
//! every standard Linux desktop.
//!
//! ## KDE Plasma 6 routing hints
//!
//! Plain `appname(...)` is not enough for KDE to integrate the
//! notification with the bell (history) and to apply the
//! auto-dismiss timeout configured for the application. KDE
//! routes notifications by `desktop-entry` — the basename of
//! the `.desktop` file under `$XDG_DATA_DIRS/applications/` —
//! and without that hint it treats the notification as an
//! unknown-app transient overlay. We set three hints to
//! follow the freedesktop convention that every other desktop
//! app uses:
//!
//! - `Hint::DesktopEntry(...)` — basename of our `.desktop`
//!   (matches the `StartupWMClass=com.airvzxf.bose-connect-gui`
//!   in `packaging/linux/bose-connect-gui.desktop`).
//! - `Hint::ImagePath(...)` — the icon name KDE should resolve
//!   from `hicolor`. With our `.desktop` installed system-wide
//!   this points at the SVG shipped in the GResource bundle and
//!   copied to `/usr/share/icons/hicolor/` by the package.
//!   Until then KDE falls back to the generic battery / app
//!   glyph; that's still accepted, but we ship the hint so
//!   production installs upgrade cleanly.
//! - `Hint::Category(...)` — KDE uses this to bucket the
//!   notification into the system-services section.

use std::time::Duration;

use anyhow::Result;
use notify_rust::{Hint, Notification, Urgency};

/// Filename (without extension) of the `.desktop` file we ship.
/// KDE Plasma 6, GNOME Shell, and every other spec-compliant
/// notification daemon key on this string to route the
/// notification to the right bell and apply the per-app
/// preferences (sound, urgency, dismiss timeout).
pub const DESKTOP_ENTRY: &str = "bose-connect-gui";

#[derive(Debug, Clone)]
pub enum NotificationLevel {
    Info,
    Warning,
    Critical,
}

pub struct Notifications;

impl Notifications {
    /// Fire a desktop notification. KDE Plasma 6 routes the
    /// notification through the bell (history) and applies the
    /// configured auto-dismiss timeout because we set the
    /// `DesktopEntry` hint to match the basenames of our
    /// `.desktop` and `gio.Application::application_id()`.
    pub fn notify(title: &str, body: &str, level: NotificationLevel) -> Result<()> {
        let mut n = Notification::new();
        let urgency = match level {
            NotificationLevel::Info => Urgency::Low,
            NotificationLevel::Warning => Urgency::Normal,
            NotificationLevel::Critical => Urgency::Critical,
        };
        n.summary(title)
            .body(body)
            // Match KDE's Plasma 6 default `Critical` timeout
            // (15 s on the linear Urgency → seconds mapping)
            // so the notification actually sticks around long
            // enough for the user to see it.
            .timeout(Duration::from_secs(match urgency {
                Urgency::Critical => 0, // 0 ⇒ server-default = 15 s in Plasma
                Urgency::Normal => 6,
                Urgency::Low => 4,
            }))
            .appname("Bose Connect for Linux")
            .icon("bose-connect-gui")
            // The hints below are what makes KDE Plasma 6
            // drop the notification into the bell history
            // instead of treating it as an unknown-app
            // transient overlay.
            .hint(Hint::DesktopEntry(DESKTOP_ENTRY.to_string()))
            .hint(Hint::ImagePath(DESKTOP_ENTRY.to_string()))
            .hint(Hint::Category("device.removed".to_string()))
            .hint(Hint::Urgency(urgency));
        n.show()?;
        Ok(())
    }
}

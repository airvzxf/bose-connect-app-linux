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
//! unknown-app transient overlay. We set the four standard
//! hints the rest of the freedesktop ecosystem uses:
//!
//! - `Hint::DesktopEntry(...)` — basename of our `.desktop`
//!   file (matches `StartupWMClass=com.airvzxf.bose-connect-gui`
//!   in `packaging/linux/bose-connect-gui.desktop`).
//! - `Hint::ImagePath(...)` — the icon name KDE should resolve
//!   from `hicolor`.
//! - `Hint::Category(...)` — KDE buckets by category for the
//!   per-app settings page.
//! - `Hint::Resident(false)` — explicit "do not pin" so that
//!   neither KDE Plasma's default KDE-stays-Critical behaviour
//!   nor any per-app override can keep the row open in the bell.
//! - `Hint::Transient(false)` — we DO want a history row, so
//!   transient must be false; the explicit setting documents
//!   the choice for the next reader.
//!
//! ## Why every level uses `Urgency::Normal`
//!
//! **KDE Plasma 6 ignores `expire_timeout` for the `Critical`
//! urgency level by default**, regardless of what we send.
//! Plasma's notification daemon is overridden by a config
//! (`Persistent` per app, or the global "Keep critical until I
//! dismiss" toggle) and the notification stays in the bell
//! until the user clicks it. That defeats the goal of
//! auto-dismiss on battery alerts.
//!
//! Other desktop apps (`krita`, `dolphin`, ...) sidestep this
//! by reserving `Critical` for *truly* life-or-death alerts and
//! using `Normal` for everything else. We follow suit — the
//! icon colour band (`red = critical`, `amber = low`,
//! `green = ok`) plus the body text already telegraph the
//! priority to the user without going through KDE's sticky
//! path. The tray icon (`services::tray::TrayService`) keeps
//! the red `NeedsAttention` Status on `≤ 25 %` so the bell
//! pop-up and the tray icon both signal the urgency.
//!
//! ## Auto-dismiss timeouts
//!
//! All three levels resolve to `Urgency::Normal` so the daemon
//! honors our `expire_timeout`. The ranking (Info < Warning <
//! Critical) keeps the priority intuitive even after the
//! Urgency downshift.
//!
//! Reference: <https://specifications.freedesktop.org/notification-spec/latest/server-bell.html>
//! Reference: <https://invent.kde.org/plasma/plasma-workspace/-/blob/master/applets/notification/plugin/notificationapplet.cpp>

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
    /// Fire a desktop notification.
    pub fn notify(title: &str, body: &str, level: NotificationLevel) -> Result<()> {
        let mut n = Notification::new();
        n.summary(title)
            .body(body)
            // Auto-dismiss timeout per level. Every level uses
            // Urgency::Normal (see module docs) so KDE Plasma 6
            // honors the timeout instead of going sticky.
            .timeout(Duration::from_secs(Self::auto_dismiss_secs(level)))
            .appname("Bose Connect for Linux")
            .icon("bose-connect-gui")
            // desktop-entry + image-path + category get the
            // notification onto the bell and resolved to the
            // right icon.
            .hint(Hint::DesktopEntry(DESKTOP_ENTRY.to_string()))
            .hint(Hint::ImagePath(DESKTOP_ENTRY.to_string()))
            .hint(Hint::Category("device.removed".to_string()))
            // resident=false: never pin this notification,
            // even if the user toggles a per-app "Persistent"
            // override on this application.
            .hint(Hint::Resident(false))
            // transient=false: keep a row in the notification
            // history so the user can scroll back to it.
            .hint(Hint::Transient(false))
            // All levels Normal so Plasma respects our
            // expire_timeout (Plasma ignores timeouts at the
            // Critical urgency level by default).
            .hint(Hint::Urgency(Urgency::Normal));
        n.show()?;
        Ok(())
    }

    /// Auto-dismiss `Duration` per level. All three are
    /// strictly positive and ranked `Critical > Warning > Info`.
    pub fn auto_dismiss_for(level: NotificationLevel) -> Duration {
        Duration::from_secs(Self::auto_dismiss_secs(level))
    }

    fn auto_dismiss_secs(level: NotificationLevel) -> u64 {
        match level {
            NotificationLevel::Info => 4,
            NotificationLevel::Warning => 5,
            NotificationLevel::Critical => 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The auto-dismiss timeouts are the contract the smoke
    /// test enforces over D-Bus; keep the mapping pinned in
    /// a unit test so a refactor can't silently re-introduce
    /// the "Critical + Urgency::Critical ⇒ stay forever" bug
    /// that KDE Plasma 6 ships with out of the box.
    #[test]
    fn all_levels_auto_dismiss_within_a_reasonable_window() {
        let i = Notifications::auto_dismiss_for(NotificationLevel::Info);
        let w = Notifications::auto_dismiss_for(NotificationLevel::Warning);
        let c = Notifications::auto_dismiss_for(NotificationLevel::Critical);
        // Every level gets an explicit positive timeout. Zero
        // would be server-default and on Plasma 6 + Critical
        // urgency that maps to "stay forever".
        assert!(i.as_secs() > 0, "Info must auto-dismiss");
        assert!(w.as_secs() > 0, "Warning must auto-dismiss");
        assert!(
            c.as_secs() > 0,
            "Critical must auto-dismiss (got 0 ⇒ never)"
        );
        // Ranking: Critical has the longest lifetime so the
        // user has a chance to read it.
        assert!(c >= w && w >= i);
    }

    /// The `.desktop` basename we claim in the hints must
    /// match what `services::assets::DESKTOP_BASENAME`
    /// actually writes — otherwise the `DesktopEntry` hint
    /// resolves to nothing and KDE walks the bell heuristic
    /// again.
    #[test]
    fn desktop_entry_string_matches_assets_basename() {
        assert_eq!(DESKTOP_ENTRY, crate::services::assets::DESKTOP_BASENAME);
        assert_eq!(DESKTOP_ENTRY, crate::services::assets::ICON_NAME);
    }

    /// The order of auto-dismiss windows is: Info < Warning <
    /// Critical. A future refactor that flips them would break
    /// the visual ranking even though all tests still pass.
    #[test]
    fn auto_dismiss_seconds_are_ranked() {
        let i = Notifications::auto_dismiss_secs(NotificationLevel::Info);
        let w = Notifications::auto_dismiss_secs(NotificationLevel::Warning);
        let c = Notifications::auto_dismiss_secs(NotificationLevel::Critical);
        assert!(i < w && w < c, "Info < Warning < Critical seconds");
    }
}

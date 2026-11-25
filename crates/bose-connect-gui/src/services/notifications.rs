//! D-Bus / libnotify native notifications via `notify-rust`.
//!
//! On KDE Plasma 6 (the host that ships this code) `notify-rust`
//! publishes to `org.freedesktop.Notifications`, which is the same
//! service the Plasma notification popups use. The
//! `cinnamon`/`gnome-flashback`/`kde5-notifications` fallbacks all
//! read from the same bus name, so this single pathway covers
//! every standard Linux desktop.
//!
//! ## Icon and routing hints
//!
//! The notification carries the application icon itself as
//! `image-data`, rendered from the binary's GResource bundle, so
//! it shows the icon whether or not a package installed it into
//! the icon theme. The GUI never installs desktop files or icons
//! into the user's home directory (see `services::assets`).
//!
//! - `app_icon` and `Hint::DesktopEntry(...)` name the
//!   application id, which is what the packages install
//!   (`com.airvzxf.bose-connect-gui.desktop` and the icon of the
//!   same name). With a package installed, KDE Plasma 6 files the
//!   notification under the application in the bell and applies
//!   its per-application settings.
//! - `Hint::ImageData(...)` — the icon pixels.
//! - `Hint::Category(...)` — KDE buckets by category for the
//!   per-app settings page.
//! - `Hint::Resident(false)` — explicit "do not pin".
//! - `Hint::Transient(false)` — we DO want a history row.
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
use notify_rust::{Hint, Image, Notification, Urgency};

use crate::services::icon::{self, APP_ID};

/// Filename (without extension) of the `.desktop` file the
/// packages install. KDE Plasma 6, GNOME Shell, and every other
/// spec-compliant notification daemon key on this string to route
/// the notification to the right bell and apply the per-app
/// preferences (sound, urgency, dismiss timeout).
pub const DESKTOP_ENTRY: &str = APP_ID;

/// Side in pixels of the icon sent with each notification.
const IMAGE_PX: i32 = 64;

/// The application icon as notification `image-data`.
///
/// notify-rust picks the `image-data` hint name from the server's
/// specification version, which it asks the server for the first
/// time it converts an image. Doing that inside `show()` nests a
/// blocking zbus call in another one and panics, so the version is
/// read here, before `show()`.
fn app_image() -> Result<Image> {
    let _ = &*notify_rust::SPEC_VERSION;
    let (w, h, data) = icon::render_rgba(IMAGE_PX)?;
    Ok(Image::from_rgba(w, h, data)?)
}

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
            .icon(APP_ID)
            .hint(Hint::DesktopEntry(DESKTOP_ENTRY.to_string()))
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
        match app_image() {
            Ok(image) => {
                n.hint(Hint::ImageData(image));
            }
            Err(err) => tracing::warn!(
                target: "notifications",
                "could not render the notification icon: {err}",
            ),
        }
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

    /// The `DesktopEntry` hint and the icon name must be the
    /// names the packages install, or KDE cannot match the
    /// notification to the application.
    #[test]
    fn desktop_entry_matches_the_packaged_names() {
        let desktop = include_str!("../../../../packaging/linux/desktop/bose-connect-gui.desktop");
        assert!(desktop.lines().any(|l| l == format!("Icon={APP_ID}")));
        let deb = include_str!("../../../../packaging/debian/cargo-deb.toml");
        let aur = include_str!("../../../../packaging/aur/PKGBUILD");
        for recipe in [deb, aur] {
            assert!(recipe.contains(&format!("applications/{DESKTOP_ENTRY}.desktop")));
            assert!(recipe.contains(&format!("scalable/apps/{APP_ID}.svg")));
        }
    }

    #[test]
    fn the_icon_renders_as_notification_image_data() {
        app_image().expect("the notification icon must render");
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

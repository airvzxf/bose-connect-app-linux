//! D-Bus / libnotify native notifications via `notify-rust`.
//!
//! On KDE Plasma 6 (the host that ships this code) `notify-rust`
//! publishes to `org.freedesktop.Notifications`, which is the same
//! service the Plasma notification popups use. The
//! `cinnamon`/`gnome-flashback`/`kde5-notifications` fallbacks all
//! read from the same bus name, so this single pathway covers
//! every standard Linux desktop.

use std::time::Duration;

use anyhow::Result;
use notify_rust::{Hint, Notification, Urgency};

#[derive(Debug, Clone)]
pub enum NotificationLevel {
    Info,
    Warning,
    Critical,
}

pub struct Notifications;

impl Notifications {
    pub fn notify(title: &str, body: &str, level: NotificationLevel) -> Result<()> {
        let mut n = Notification::new();
        n.summary(title).body(body).timeout(Duration::from_secs(6));
        let urgency = match level {
            NotificationLevel::Info => Urgency::Low,
            NotificationLevel::Warning => Urgency::Normal,
            NotificationLevel::Critical => Urgency::Critical,
        };
        n.appname("Bose Connect for Linux")
            .hint(Hint::Urgency(urgency));
        n.show()?;
        Ok(())
    }
}

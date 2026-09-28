//! # bose-connect-gui
//!
//! GTK4 desktop application that wraps the [`bose_connect`]
//! library and adds:
//!
//! * A modern libadwaita UI with a hero battery readout, three
//!   setting tiles, paired-devices panel, profile bar, and an
//!   activity log.
//!
//! * A KDE Plasma / freedesktop StatusNotifierItem tray icon,
//!   wired directly through `gio::DBus` so we don't have to
//!   depend on the `ksni` crate's glib-0.20 pin. The icon is
//!   the SVG shipped in `resources/icons/`, rendered at 22×22
//!   and 44×44 for HiDPI.
//!
//! * BlueZ device discovery, also over `gio::DBus`, that lets
//!   the user pick an address without leaving the app.
//!
//! * Persistent state (last-connected address, active profile) under
//!   `~/.config/bose-connect/state.json`.
//!
//! ## Architecture
//!
//! ```text
//!   ┌────────────────────────────────────────────────────────────┐
//!   │                    GTK4 / libadwaita UI                    │
//!   │                                                            │
//!   │          AppModel  ←→  view::build_root  ←→  Widgets     │
//!   │                       (flume::Sender<AppMsg>)              │
//!   └────────────────────────────┬───────────────────────────────┘
//!                                │ async commands
//!                                ▼
//!   ┌────────────────────────────────────────────────────────────┐
//!   │                  DeviceService (trait)                     │
//!   │                                                            │
//!   │   MockService  ←──────────→   RealService (RFCOMM)        │
//!   │   (in-process)              (via bose_connect)             │
//!   └────────────────────────────────────────────────────────────┘
//! ```
//!
//! The mock service is the default; the real service wires to a
//! `BoseDevice` over RFCOMM and is gated behind the
//! `real-bluetooth` cargo feature so the binary builds on systems
//! that don't have `libbluetooth-dev`. The tray icon and the
//! Bluetooth discovery both live in `services::tray` and
//! `services::bluetooth`, hand-rolled on top of `gio::DBus`.
//!
//! ## Screen tour
//!
//! The mockup is intentionally simple. A single toolbar at the top
//! (menu / refresh / quiet-mode pill), then a stacked layout:
//!
//!   * Connection banner
//!   * Hero battery card (left = identity, right = battery %)
//!   * Quick setting tiles (NC / voice / language / auto-off)
//!   * Profiles + Paired devices, side by side
//!   * Activity log
//!
//! Battery and connection state are also reflected in the tray
//! menu, so the user can hit the OS shortcut on the headphones
//! and see the update without taking focus from their DAW.

#![deny(rustdoc::broken_intra_doc_links, rustdoc::invalid_html_tags)]

/// Register the GResource bundle compiled by `build.rs` with
/// the running gio process. Idempotent: gio de-duplicates and
/// the second call is a cheap refcount bump. Every entry point
/// that needs to look up icons, CSS, or GMenu XML by resource
/// path must call this *before* the first lookup.
pub fn register_resources() {
    // `gio::resources_register_include!` is just sugar over this
    // pattern, but it expands to `include_bytes!(...)` which only
    // accepts string literals — `concat!(env!("OUT_DIR"), "...")`
    // doesn't work inside it. We hit the C API directly with a
    // `Bytes` that owns the same bytes the build script wrote.
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/gresource.gresource"));
    let bytes = gtk::glib::Bytes::from_static(bytes);
    if let Ok(resource) = gtk::gio::Resource::from_data(&bytes) {
        gtk::gio::resources_register(&resource);
    } else {
        tracing::warn!(
            target: "gresource",
            "compiled gresource bundle could not be parsed; \
             icons, CSS, and GMenu will fall back to disk lookups"
        );
    }
}

pub mod app;
pub mod i18n;
pub mod services;
pub mod transport;

pub use services::device::DeviceService;

#[cfg(test)]
mod tests_for_lib_rs {
    use super::*;
    #[test]
    fn register_resources_is_idempotent() {
        // Two calls in a row must succeed — gio internally
        // bumps a refcount and our wrapper just logs on error.
        register_resources();
        register_resources();
    }
}

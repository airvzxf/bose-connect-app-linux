//! # bose-connect-gui
#![deny(rustdoc::broken_intra_doc_links, rustdoc::invalid_html_tags)]
//!
//! GTK4 + Relm4 desktop application that wraps the
//! [`bose_connect`] library and adds:
//!
//! * A modern libadwaita UI with a hero battery readout, three
//!   setting tiles, paired-devices panel, profile bar, and an
//!   activity log.
//!
//! * A KDE Plasma / freedesktop StatusNotifierItem tray icon.
//!
//! * A D-Bus MediaPlayer2 service so KDE Connect, the Plasma
//!   volume widget, and any other MPRIS-aware tool can react to
//!   the headphone state.
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
//!   │          AppModel  ←→  AppComponent  ←→  Widgets          │
//!   │                       (Relm4)                              │
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
//! that don't have `libbluetooth-dev`.
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

pub mod app;
pub mod i18n;
pub mod services;
pub mod transport;

pub use services::device::DeviceService;

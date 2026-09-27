//! App module — the top-level Relm4 component.
//!
//! The pieces live in:
//!   * `model`        — `AppModel`, `AppMsg`
//!   * `component`    — `AppComponent`'s `SimpleComponent` impl
//!   * `view`         — `view` function + `widgets` subtrees
//!   * `widgets`      — atomic widgets; mostly pure functions
//!
//! Re-exported here for callers that want to import once.

pub mod component;
pub mod model;
pub mod view;
pub mod widgets;

pub use component::{AppComponent, AppWidgets};
pub use model::{AppModel, AppMsg};

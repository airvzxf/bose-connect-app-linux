//! App module — the top-level window + widget tree.
//!
//! The pieces live in:
//!   * `model`    — `AppModel`, `AppMsg`
//!   * `view`     — `view` function + `widgets` subtrees
//!   * `widgets`  — atomic widgets; mostly pure functions
//!
//! Re-exported here for callers that want to import once.

pub mod model;
pub mod view;
pub mod widgets;

pub use model::{AppModel, AppMsg};

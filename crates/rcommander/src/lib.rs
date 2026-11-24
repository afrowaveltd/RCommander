//! RCommander — a cross-platform (Linux / Windows / Android) dual-pane file
//! manager.
//!
//! The crate is split into a headless, fully tested **model** (navigation, file
//! operations, dialogs, command dispatch) and a thin **egui** view that renders
//! the model. This keeps every behaviour testable without a window system.

pub mod agent_host;
pub mod archive_bridge;
pub mod i18n;
pub mod keys;
pub mod model;
pub mod ops_runner;
pub mod paths;
pub mod settings_writer;
pub mod theme;
pub mod ui;

pub use model::{Model, Panel, Side};

/// Version reported in the About dialog and to MCP clients.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

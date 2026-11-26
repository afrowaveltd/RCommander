//! `rc-core` — platform-independent model and engine for RCommander.
//!
//! This crate contains everything that is not UI and not platform-specific:
//! directory listing model, sorting, masks/wildcards, selection, file
//! operations, configuration, histories, hotlists, tabs, the internal command
//! catalogue and the default keyboard map.
//!
//! Every module is unit-tested; the crate is deliberately free of any GUI,
//! async runtime or network dependency so that it can be reused by the desktop
//! app, the Android front-end and the MCP server.

pub mod audit;
pub mod branch;
pub mod checksum;
pub mod command;
pub mod comment;
pub mod compare;
pub mod config;
pub mod desktop;
pub mod diff;
pub mod drive;
pub mod dupes;
pub mod encode;
pub mod entry;
pub mod error;
pub mod font;
pub mod format;
pub mod history;
pub mod hotlist;
pub mod keymap;
pub mod knownhosts;
pub mod listing;
pub mod location;
pub mod mask;
pub mod ops;
pub mod policy;
pub mod prompt;
pub mod quicksearch;
pub mod secrets;
pub mod selection;
pub mod sessions;
pub mod sorting;
pub mod split;
pub mod syntax;
pub mod tabs;
pub mod theme;
pub mod transaction;
pub mod workspace;

pub use entry::{Attrs, Entry, EntryKind};
pub use error::{CoreError, CoreResult};
pub use location::Location;
pub use mask::{MaskSet, Pattern};
pub use selection::Selection;
pub use sorting::{SortField, SortSpec};

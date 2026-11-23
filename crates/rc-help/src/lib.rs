//! `rc-help` — the in-application help system of RCommander.
//!
//! Help content is *data*, not code: a directory tree of markdown files on
//! disk, one folder per topic and one `<lang>.md` file per language. This crate
//! discovers that tree, extracts titles, falls back between languages, resolves
//! cross-topic links, parses the (small) markdown subset a rich egui renderer
//! needs, and searches it.
//!
//! Nothing is compiled in, so users add topics and translations by dropping
//! files into the tree.
//!
//! # Layout
//!
//! ```text
//! Docs/
//!   Previewer/
//!     en.md
//!     cs.md
//!   FTP connections/
//!     en.md
//!   Keyboard shortcuts/
//!     en.md
//!   Advanced/
//!     Packing/
//!       en.md
//! ```
//!
//! * A folder that directly contains at least one `<lang>.md` file is a topic.
//! * The folder name is the topic id; an optional `NN - ` prefix sets the sort
//!   order and is removed from the id.
//! * The first `# Heading` of a file is that language's title; a file without a
//!   heading uses the topic id (the folder name) as its title.
//! * Nesting is supported to any depth; the folder name is the id, and
//!   [`HelpTopic::relative_path`] carries the full path below the root.
//! * A missing translation falls back to the fallback language (`"en"` by
//!   convention, passed by the caller).
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//!
//! let index = rc_help::HelpIndex::load(Path::new("Docs"))?;
//! for topic in index.topics() {
//!     println!("{} — {}", topic.order, topic.id);
//! }
//! if let Some(document) = index.body("Previewer", "cs", "en") {
//!     println!("{} ({})", document.title, document.language);
//!     println!("{}", rc_help::to_plain_text(&document.blocks));
//! }
//! # Ok::<(), rc_help::HelpError>(())
//! ```
//!
//! # Guarantees
//!
//! * A missing root directory yields an empty index, not an error.
//! * Malformed markdown, unterminated fences, CRLF, a BOM and non-UTF-8 bytes
//!   are tolerated; parsing never fails and never panics.
//! * Symlink cycles cannot hang the scan.
//! * Unresolvable links yield `None`.

#![deny(missing_docs)]

mod error;
mod markdown;
mod plain;
mod search;
mod topic;

pub use error::{HelpError, Result};
pub use markdown::{Align, Block, Span, parse_markdown};
pub use plain::to_plain_text;
pub use search::{SearchHit, search};
pub use topic::{HelpDocument, HelpIndex, HelpTopic, MAX_WALK_DEPTH, PATH_SEPARATOR};

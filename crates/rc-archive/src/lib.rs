//! `rc-archive` — the archive engine of RCommander.
//!
//! RCommander treats an archive as if it were a directory: the file panel lists
//! it, the user *enters* it, copies members out of it and packs new archives
//! from the packer dialog. This crate is the model behind those views; it has no
//! GUI and no async runtime, so it can be reused by the desktop app, the Android
//! front-end and the MCP server.
//!
//! # Overview
//!
//! * [`detect`] / [`detect_bytes`] classify a file as a container, using magic
//!   bytes first and the file name second ([`ArchiveKind`]).
//! * [`open`] yields a [`ArchiveRead`] — a uniform reader for every supported
//!   container.
//! * [`list_dir`] and [`is_dir`] build an *inside the archive* directory listing
//!   out of rc-core [`rc_core::entry::Entry`] values, so the panel renders
//!   archive members exactly like real files.
//! * [`extract_all`], [`extract_members`] and [`extract_member_to`] write
//!   members back to a real file system and refuse to escape the destination
//!   directory.
//! * [`create`], [`add_to_zip`] and [`remove_from_zip`] build and modify
//!   archives atomically (temporary file + rename).
//! * [`test()`] and [`describe`] implement the "Test archive" command and the
//!   status line summary.
//!
//! # Supported formats
//!
//! Reading: zip, tar, tar.gz/tgz, tar.bz2/tbz2, tar.xz/txz, tar.zst/tzst,
//! single-file gz/bz2/xz/zst and 7z. Writing: zip (stored or deflated) and
//! tar.gz. RAR and CAB are recognised but not decodable (see
//! [`supported_for_reading`]).
//!
//! # Memory limits
//!
//! All extraction paths stream through a fixed 64 KiB buffer, so extracting a
//! member never needs memory proportional to the member size. The two exceptions
//! are documented on
//! [`ArchiveRead::read`] (which returns the whole member by design) and the 7z
//! backend, which holds the decoder's dictionary/block buffers while a solid
//! block is being decoded.
//!
//! # Malformed input
//!
//! Every parser entry point returns [`ArchiveError`]; malformed, truncated or
//! hostile archives produce an error instead of a panic. Member names that
//! would escape the extraction root are rejected with
//! [`ArchiveError::UnsafePath`].

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]

mod backend;
mod create;
mod describe;
mod error;
mod extract;
mod kind;
mod listing;
mod member;
mod mutate;
mod open;
mod read;
mod stream;
mod testing;
mod timeutil;

pub use create::{PackOptions, PackOutcome, create};
pub use describe::describe;
pub use error::{ArchiveError, Result};
pub use extract::{ExtractOutcome, extract_all, extract_member_to, extract_members};
pub use kind::{ArchiveKind, detect, detect_bytes, extensions, is_archive_path};
pub use listing::{ArchiveDir, is_dir, list_dir};
pub use mutate::{add_to_zip, remove_from_zip};
pub use open::{open, supported_for_reading, supported_for_writing};
pub use read::{ArchiveEntry, ArchiveRead};
pub use testing::{TestReport, test};

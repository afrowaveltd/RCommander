//! `rc-viewer` — the smart F3 viewer engine of **RCommander**.
//!
//! The viewer answers two questions for the GUI and nothing more:
//!
//! 1. *What is this file?* — [`detect`] classifies content from **magic bytes**
//!    plus the file name and never trusts the extension over real content.
//! 2. *What should be rendered?* — [`load`] / [`open_bytes`] produce a
//!    render-ready model ([`ViewerContent`]) that the GUI can paint directly.
//!
//! # Design rules
//!
//! * **Magic wins over the name.** `fake.png` containing text is [`ContentKind::Text`],
//!   a JPEG named `photo.png` is a JPEG. The extension is only consulted to give
//!   plain text a richer `format_hint` (`html`, `markdown`, `json`, `csv`, `xml`, …).
//! * **Unknown binary content is a hexdump.** [`ContentKind::Hex`] is the fallback
//!   for opaque bytes; recognised binary containers (PDF, ELF, PE, archives,
//!   SQLite, fonts, …) keep their descriptive metadata instead.
//! * **Never panic on malformed input.** A corrupt or truncated image yields
//!   [`ViewerContent::Unsupported`] with a message, an undecodable byte stream
//!   yields replacement characters, and every whole-file read is bounded by
//!   [`ViewerOptions::max_bytes_read`] so a multi-gigabyte file cannot exhaust memory.
//! * **Bounded memory.** Text/hex previews are truncated to `max_bytes_read`
//!   (`truncated` flags are set), images are budgeted by `max_image_pixels` with a
//!   hard cap on the decoded buffer size. [`load_text_window`] pages through a
//!   file that is too large for one preview.
//!
//! # Example
//!
//! ```no_run
//! use rc_viewer::{describe, load, ViewerOptions};
//!
//! let opts = ViewerOptions::default();
//! let loaded = load(std::path::Path::new("report.pdf"), &opts).unwrap();
//! println!("{}", describe(&loaded));
//! ```

#![deny(missing_docs)]

mod classify;
mod encoding;
mod hex;
mod img;
mod load;
mod media;
mod options;
mod text;

pub use classify::{ContentKind, FileFormat, detect, probe};
pub use encoding::decode_text;
pub use hex::{HexDocument, HexLine, hex_document, load_hex, reformat};
pub use img::{
    ImageDocument, MAX_DECODED_BYTES, MAX_IMAGE_FILE_BYTES, decode_image_bytes, load_image,
};
pub use load::{LoadedViewer, ViewerContent, describe, load, open_bytes};
pub use media::{BinaryDocument, MediaDocument};
pub use options::{EncodingChoice, ViewerOptions};
pub use text::{TextDocument, load_text, load_text_window};

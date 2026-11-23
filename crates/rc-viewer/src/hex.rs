//! Hexdump model: offsets, hex bytes and an ASCII gutter, pre-formatted so the
//! GUI only has to lay out monospace rows.

use std::path::Path;

use crate::classify::read_head;
use crate::options::ViewerOptions;

/// One formatted hexdump row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexLine {
    /// Absolute offset of the first byte of this row inside the file.
    pub offset: u64,
    /// Raw bytes of the row (`1..=64` bytes, fewer on the last row).
    pub bytes: Vec<u8>,
    /// The same bytes as lower-case hex, space separated and exactly two
    /// characters per byte: `"00 1f 2a"`. No padding on a short last row.
    pub hex: String,
    /// Printable ASCII of the row; every non-printable byte (`< 0x20` or
    /// `> 0x7e`) appears as `.`.
    pub ascii: String,
}

/// A hexdump of a whole file or of an arbitrary byte slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexDocument {
    /// Pre-formatted rows.
    pub lines: Vec<HexLine>,
    /// Size of the source in bytes. For [`load_hex`] this is the real file size
    /// (so `total_bytes > lines * bytes_per_line` means truncated); for
    /// [`hex_document`] it is the length of the slice that was passed in.
    pub total_bytes: u64,
    /// Whether only a prefix of the source is represented by [`Self::lines`].
    pub truncated: bool,
    /// Bytes per row actually used (the requested value clamped to `1..=64`).
    pub bytes_per_line: usize,
}

impl HexDocument {
    /// Number of rows in the dump.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Number of bytes represented by [`Self::lines`].
    pub fn bytes_shown(&self) -> u64 {
        self.lines.iter().map(|line| line.bytes.len() as u64).sum()
    }

    /// Whether the dump contains no rows at all.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Format `bytes` as a hexdump starting at `base_offset`, `bytes_per_line` bytes
/// per row.
///
/// `bytes_per_line` is clamped to `1..=64`. An empty slice yields a document
/// with no rows. This is the "render an arbitrary byte slice" entry point — the
/// GUI uses it for previews of embedded data (archive members, clipboard blobs).
///
/// ```
/// let doc = rc_viewer::hex_document(b"\x00\x1f\x2a", 0, 16);
/// assert_eq!(doc.lines[0].hex, "00 1f 2a");
/// assert_eq!(doc.lines[0].ascii, "..*"); // 0x2a is a printable '*'
/// assert_eq!(rc_viewer::hex_document(b"ab", 0, 0).line_count(), 2); // 0 clamps to 1
/// ```
pub fn hex_document(bytes: &[u8], base_offset: u64, bytes_per_line: usize) -> HexDocument {
    let per_line = bytes_per_line.clamp(1, 64);
    let rows = bytes.len().div_ceil(per_line);
    let mut lines = Vec::with_capacity(rows);
    for (index, chunk) in bytes.chunks(per_line).enumerate() {
        let offset = base_offset.wrapping_add((index * per_line) as u64);
        lines.push(format_line(offset, chunk));
    }
    HexDocument {
        lines,
        total_bytes: bytes.len() as u64,
        truncated: false,
        bytes_per_line: per_line,
    }
}

/// Read at most [`ViewerOptions::max_bytes_read`] bytes of `path` and format them
/// as a hexdump.
///
/// `total_bytes` reports the real file size, so the GUI can show "showing
/// 8 MiB of 20 GiB".
pub fn load_hex(path: &Path, opts: &ViewerOptions) -> std::io::Result<HexDocument> {
    let per_line = opts.bytes_per_line();
    let (bytes, total_bytes) = read_head(path, opts.max_bytes_read)?;
    let mut doc = hex_document(&bytes, 0, per_line);
    doc.total_bytes = total_bytes;
    doc.truncated = total_bytes > bytes.len() as u64;
    Ok(doc)
}

/// Format one row.
fn format_line(offset: u64, chunk: &[u8]) -> HexLine {
    let mut hex = String::with_capacity(chunk.len() * 3);
    let mut ascii = String::with_capacity(chunk.len());
    for (index, &byte) in chunk.iter().enumerate() {
        if index > 0 {
            hex.push(' ');
        }
        hex.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        hex.push(char::from_digit((byte & 0x0F) as u32, 16).unwrap_or('0'));
        ascii.push(if (0x20..=0x7E).contains(&byte) {
            byte as char
        } else {
            '.'
        });
    }
    HexLine {
        offset,
        bytes: chunk.to_vec(),
        hex,
        ascii,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_row_is_sixteen_bytes() {
        let bytes: Vec<u8> = (0u8..32).collect();
        let doc = hex_document(&bytes, 0, 16);
        assert_eq!(doc.bytes_per_line, 16);
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.bytes_shown(), 32);

        assert_eq!(doc.lines[0].offset, 0);
        assert_eq!(doc.lines[0].bytes.len(), 16);
        assert_eq!(
            doc.lines[0].hex,
            "00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f"
        );
        assert_eq!(doc.lines[0].ascii, "................");
    }

    #[test]
    fn offsets_advance_by_the_row_size() {
        let bytes: Vec<u8> = (0u8..48).collect();
        let doc = hex_document(&bytes, 0, 16);
        assert_eq!(doc.lines[0].offset, 0);
        assert_eq!(doc.lines[1].offset, 16);
        assert_eq!(doc.lines[2].offset, 32);
        assert_eq!(doc.lines[2].bytes[0], 32);
    }

    #[test]
    fn hex_string_format_is_exact() {
        let doc = hex_document(&[0x00, 0x1F, 0x2A], 0, 16);
        assert_eq!(doc.lines[0].hex, "00 1f 2a");
        // No trailing space and no padding on a short row.
        assert_eq!(doc.lines[0].hex.len(), 8);
        assert!(!doc.lines[0].hex.ends_with(' '));
        assert!(!doc.lines[0].hex.contains("  "));
    }

    #[test]
    fn ascii_gutter_replaces_non_printable_bytes() {
        let doc = hex_document(b"A\x00\x7f \x7e\xff\t", 0, 16);
        assert_eq!(doc.lines[0].ascii, "A.. ~..");
        // Space is printable and therefore kept.
        assert_eq!(doc.lines[0].ascii.chars().nth(3), Some(' '));
        assert_eq!(doc.lines[0].ascii.chars().nth(4), Some('~'));
    }

    #[test]
    fn eight_bytes_per_line_groups_correctly() {
        let bytes: Vec<u8> = (0u8..20).collect();
        let doc = hex_document(&bytes, 0, 8);
        assert_eq!(doc.bytes_per_line, 8);
        assert_eq!(doc.line_count(), 3);
        assert_eq!(doc.lines[0].hex, "00 01 02 03 04 05 06 07");
        assert_eq!(doc.lines[1].offset, 8);
        assert_eq!(doc.lines[2].offset, 16);
        // Odd remainder row.
        assert_eq!(doc.lines[2].bytes, vec![16, 17, 18, 19]);
        assert_eq!(doc.lines[2].hex, "10 11 12 13");
        assert_eq!(doc.lines[2].ascii.len(), 4);
    }

    #[test]
    fn one_byte_per_line_is_supported() {
        let doc = hex_document(b"ab", 0, 1);
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.lines[0].hex, "61");
        assert_eq!(doc.lines[0].ascii, "a");
        assert_eq!(doc.lines[1].offset, 1);
        assert_eq!(doc.lines[1].hex, "62");
    }

    #[test]
    fn row_size_is_clamped_to_the_supported_range() {
        let doc = hex_document(&[0u8; 4], 0, 0);
        assert_eq!(doc.bytes_per_line, 1);
        assert_eq!(doc.line_count(), 4);

        let doc = hex_document(&[0u8; 200], 0, 1000);
        assert_eq!(doc.bytes_per_line, 64);
        assert_eq!(doc.line_count(), 4);
        assert_eq!(doc.lines[0].bytes.len(), 64);
        // Last row is the remainder (200 - 3*64 = 8).
        assert_eq!(doc.lines[3].bytes.len(), 8);
    }

    #[test]
    fn empty_input_has_no_lines() {
        let doc = hex_document(&[], 0, 16);
        assert_eq!(doc.line_count(), 0);
        assert!(doc.is_empty());
        assert_eq!(doc.total_bytes, 0);
        assert_eq!(doc.bytes_shown(), 0);
        assert!(!doc.truncated);
        assert_eq!(doc.bytes_per_line, 16);
    }

    #[test]
    fn base_offset_is_respected() {
        let bytes: Vec<u8> = (0u8..32).collect();
        let doc = hex_document(&bytes, 0x1000, 16);
        assert_eq!(doc.lines[0].offset, 0x1000);
        assert_eq!(doc.lines[1].offset, 0x1010);
        assert_eq!(doc.lines[1].bytes[0], 16);
        // The dump covers the slice only, the base offset is presentation data.
        assert_eq!(doc.total_bytes, 32);
    }

    #[test]
    fn base_offset_near_the_end_of_the_address_space_does_not_overflow() {
        let doc = hex_document(&[1, 2, 3], u64::MAX - 1, 1);
        assert_eq!(doc.lines[0].offset, u64::MAX - 1);
        assert_eq!(doc.lines[1].offset, u64::MAX);
        // Wraps instead of panicking in debug builds.
        assert_eq!(doc.lines[2].offset, 0);
    }

    #[test]
    fn load_hex_reports_size_and_truncation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("data.bin");
        let bytes: Vec<u8> = (0u8..100).collect();
        std::fs::write(&path, &bytes).expect("write");

        let mut opts = ViewerOptions {
            max_bytes_read: 32,
            ..ViewerOptions::default()
        };
        let doc = load_hex(&path, &opts).expect("load_hex");
        assert!(doc.truncated);
        assert_eq!(doc.total_bytes, 100);
        assert_eq!(doc.bytes_shown(), 32);
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.lines[1].offset, 16);

        opts.max_bytes_read = 4096;
        let doc = load_hex(&path, &opts).expect("load_hex");
        assert!(!doc.truncated);
        assert_eq!(doc.total_bytes, 100);
        assert_eq!(doc.bytes_shown(), 100);
        assert_eq!(doc.line_count(), 7); // 6 full rows plus 4 bytes
        assert_eq!(doc.lines[6].bytes.len(), 4);
    }

    #[test]
    fn load_hex_accepts_an_empty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.bin");
        std::fs::write(&path, b"").expect("write");
        let doc = load_hex(&path, &ViewerOptions::default()).expect("load_hex");
        assert!(doc.is_empty());
        assert_eq!(doc.total_bytes, 0);
        assert!(!doc.truncated);
    }

    #[test]
    fn load_hex_honours_bytes_per_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("data.bin");
        std::fs::write(&path, (0u8..16).collect::<Vec<u8>>()).expect("write");
        let opts = ViewerOptions {
            hex_bytes_per_line: 8,
            ..ViewerOptions::default()
        };
        let doc = load_hex(&path, &opts).expect("load_hex");
        assert_eq!(doc.bytes_per_line, 8);
        assert_eq!(doc.line_count(), 2);
    }

    #[test]
    fn load_hex_propagates_missing_file_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope.bin");
        assert!(load_hex(&missing, &ViewerOptions::default()).is_err());
    }
}

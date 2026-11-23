//! Text documents: decoding, line indexing, RTL awareness and search.

use std::path::Path;

use crate::classify::read_head;
use crate::encoding;
use crate::options::ViewerOptions;

/// How many bytes of the file head are sniffed to fix the encoding of a window.
const ENCODING_PROBE_BYTES: usize = 8192;

/// A decoded text file, ready to be rendered.
///
/// The document holds at most [`ViewerOptions::max_bytes_read`] bytes of decoded
/// text; [`Self::truncated`] tells the GUI that the file continues and
/// [`Self::total_bytes`] how large it really is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDocument {
    /// Decoded text. Always valid UTF-8, malformed input became `U+FFFD`.
    pub text: String,
    /// Canonical name of the resolved encoding, e.g. `"utf-8"`, `"utf-16le"`,
    /// `"windows-1250"`, `"koi8-r"`.
    pub encoding: &'static str,
    /// Whether the source started with a byte-order mark (which was stripped).
    pub had_bom: bool,
    /// Whether strong right-to-left characters dominate the text, so the GUI can
    /// pick a right-to-left layout and alignment.
    pub is_rtl: bool,
    /// Only the first [`ViewerOptions::max_bytes_read`] bytes were decoded.
    pub truncated: bool,
    /// Size of the file in bytes (not the size of [`Self::text`]).
    pub total_bytes: u64,
    /// Byte offsets of line starts. Empty for empty text, and a final trailing
    /// newline does not start an extra line.
    pub line_starts: Vec<usize>,
}

impl TextDocument {
    /// Number of lines in the document.
    ///
    /// A trailing newline does not add an empty line: `"a\n"` has one line,
    /// `""` has none.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The text of line `index`, without its line terminator (`LF` or `CRLF`).
    ///
    /// Returns `None` when `index` is out of range.
    pub fn line(&self, index: usize) -> Option<&str> {
        let start = *self.line_starts.get(index)?;
        let end = match self.line_starts.get(index + 1) {
            // The next line starts one byte after the '\n' that ended this one.
            Some(&next) => next.saturating_sub(1),
            // The last line: drop its own terminator when the text ends with one.
            None if self.text.ends_with('\n') => self.text.len() - 1,
            None => self.text.len(),
        };
        // `get` on a `&str` returns `None` instead of panicking when the range is
        // not on character boundaries (e.g. if a caller mutated `line_starts`).
        let raw = self.text.get(start..end)?;
        Some(raw.strip_suffix('\r').unwrap_or(raw))
    }

    /// Iterate over every line without its terminator.
    pub fn lines(&self) -> impl Iterator<Item = &str> + '_ {
        (0..self.line_count()).filter_map(|index| self.line(index))
    }

    /// Find `needle` at or after `from_line`.
    ///
    /// Returns `(line, column)` where `column` is a **character** offset inside
    /// the returned line (not a byte offset), so a GUI can position a cursor
    /// without re-scanning. An empty needle never matches.
    ///
    /// ```
    /// let doc = rc_viewer::open_bytes("a.txt", b"one\ntwo\nthree", &Default::default());
    /// let rc_viewer::ViewerContent::Text(doc) = doc.content else { panic!() };
    /// assert_eq!(doc.find("two", 0, true), Some((1, 0)));
    /// assert_eq!(doc.find("nope", 0, true), None);
    /// ```
    pub fn find(
        &self,
        needle: &str,
        from_line: usize,
        case_sensitive: bool,
    ) -> Option<(usize, usize)> {
        if needle.is_empty() {
            return None;
        }
        for index in from_line..self.line_count() {
            let Some(line) = self.line(index) else {
                continue;
            };
            let column = if case_sensitive {
                line.find(needle)
                    .map(|byte| line.get(..byte).map_or(0, |s| s.chars().count()))
            } else {
                find_case_insensitive(line, needle)
            };
            if let Some(column) = column {
                return Some((index, column));
            }
        }
        None
    }
}

/// Load `path` as a text document.
///
/// Reads at most [`ViewerOptions::max_bytes_read`] bytes. I/O errors are
/// returned; *decoding* never fails (see [`crate::decode_text`]).
pub fn load_text(path: &Path, opts: &ViewerOptions) -> std::io::Result<TextDocument> {
    let (bytes, total_bytes) = read_head(path, opts.max_bytes_read)?;
    Ok(build(&bytes, opts, total_bytes, false))
}

/// Load one window of a very large text file.
///
/// This is the incremental-loading hook: the GUI can scroll past
/// [`ViewerOptions::max_bytes_read`] by asking for successive windows, and the
/// engine never holds more than `max_bytes` of decoded text. The window starts at
/// byte `offset`; advance it by whatever you consumed (whole lines are the
/// natural granularity).
///
/// [`TextDocument::line_starts`] are relative to the window, while
/// [`TextDocument::total_bytes`] still reports the whole file and
/// [`TextDocument::truncated`] tells whether anything follows the window.
pub fn load_text_window(
    path: &Path,
    offset: u64,
    max_bytes: usize,
    opts: &ViewerOptions,
) -> std::io::Result<TextDocument> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)?;
    let total_bytes = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    file.seek(SeekFrom::Start(offset))?;
    let mut buffer = Vec::new();
    file.take(max_bytes as u64).read_to_end(&mut buffer)?;

    let scent = if offset == 0 {
        encoding::resolve(&buffer, &opts.encoding)
    } else {
        // The encoding belongs to the file, not to the window: sniffing a slice
        // taken from the middle would guess nonsense, so sniff the head and
        // decode the window with that answer (the BOM, if any, was already
        // consumed with byte 0 and is not part of this window).
        let mut probe = Vec::new();
        std::fs::File::open(path)?
            .take(ENCODING_PROBE_BYTES as u64)
            .read_to_end(&mut probe)?;
        let snouted = encoding::resolve(&probe, &opts.encoding);
        encoding::Scent {
            skip: 0,
            had_bom: false,
            ..snouted
        }
    };
    let mut document = build_with_scent(&buffer, &scent, total_bytes, offset > 0);
    document.truncated = offset.saturating_add(buffer.len() as u64) < total_bytes;
    Ok(document)
}

/// Build a document from an in-memory byte slice (used by
/// [`crate::open_bytes`]).
pub(crate) fn document(bytes: &[u8], opts: &ViewerOptions) -> TextDocument {
    let limit = opts.max_bytes_read.min(bytes.len());
    let prefix = bytes.get(..limit).unwrap_or(bytes);
    build(prefix, opts, bytes.len() as u64, false)
}

/// Decode and index `bytes`, which must already be capped to the preview limit.
///
/// `starts_mid_stream` marks a window that does not begin at byte 0 of the file,
/// whose first decoded character may be the tail of a cut multi-byte sequence.
fn build(
    bytes: &[u8],
    opts: &ViewerOptions,
    total_bytes: u64,
    starts_mid_stream: bool,
) -> TextDocument {
    let scent = encoding::resolve(bytes, &opts.encoding);
    build_with_scent(bytes, &scent, total_bytes, starts_mid_stream)
}

/// Decode and index `bytes` with an already resolved encoding.
fn build_with_scent(
    bytes: &[u8],
    scent: &encoding::Scent,
    total_bytes: u64,
    starts_mid_stream: bool,
) -> TextDocument {
    let truncated = total_bytes > bytes.len() as u64;
    let (mut text, encoding_name, had_bom) = encoding::decode_with(bytes, scent);
    if truncated {
        // A preview that stops inside a multi-byte sequence ends with U+FFFD;
        // that is an artefact of the cut, not corruption in the file.
        while text.ends_with('\u{FFFD}') {
            text.pop();
        }
    }
    if starts_mid_stream {
        // Same for a window that starts inside a multi-byte sequence.
        while text.starts_with('\u{FFFD}') {
            text.remove(0);
        }
    }
    let is_rtl = is_rtl_text(&text);
    let line_starts = compute_line_starts(&text);
    TextDocument {
        text,
        encoding: encoding_name,
        had_bom,
        is_rtl,
        truncated,
        total_bytes,
        line_starts,
    }
}

/// Byte offsets of every line start.
///
/// A trailing newline does not start an extra line, so `"a\n"` yields `[0]` and
/// `""` yields `[]`.
fn compute_line_starts(text: &str) -> Vec<usize> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut starts = Vec::with_capacity(text.len() / 40 + 1);
    starts.push(0);
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' && index + 1 < text.len() {
            starts.push(index + 1);
        }
    }
    starts
}

/// Whether strong right-to-left characters outnumber strong left-to-right ones.
///
/// Weak characters (digits, punctuation, whitespace) do not vote, so a mostly
/// English file with a Hebrew word stays left-to-right.
fn is_rtl_text(text: &str) -> bool {
    let mut rtl = 0usize;
    let mut ltr = 0usize;
    for ch in text.chars() {
        let code = ch as u32;
        if is_strong_rtl(code) {
            rtl += 1;
        } else if is_strong_ltr(code) {
            ltr += 1;
        }
    }
    rtl > 0 && rtl > ltr
}

/// Hebrew, Arabic, Syriac, Thaana, N'Ko, Samaritan, Mandaic, presentation forms.
fn is_strong_rtl(code: u32) -> bool {
    matches!(
        code,
        0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF
    )
}

/// Latin, Greek, Cyrillic and the Indic/CJK blocks used as LTR counterweight.
fn is_strong_ltr(code: u32) -> bool {
    matches!(code, 0x41..=0x5A | 0x61..=0x7A | 0x00C0..=0x024F | 0x0370..=0x058F | 0x0900..=0x1FFF)
}

/// Case-insensitive search that reports a character column of the original line.
fn find_case_insensitive(line: &str, needle: &str) -> Option<usize> {
    let lowered: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if lowered.is_empty() {
        return None;
    }
    // One entry per lowered character, carrying the column of its source char, so
    // multi-char case foldings (e.g. 'İ') cannot desynchronise the positions.
    let mut flat: Vec<(char, usize)> = Vec::with_capacity(line.len());
    for (column, ch) in line.chars().enumerate() {
        for lowered_ch in ch.to_lowercase() {
            flat.push((lowered_ch, column));
        }
    }
    if flat.len() < lowered.len() {
        return None;
    }
    for start in 0..=flat.len() - lowered.len() {
        let matched = flat[start..start + lowered.len()]
            .iter()
            .zip(lowered.iter())
            .all(|((hay, _), needle_ch)| hay == needle_ch);
        if matched {
            return Some(flat[start].1);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::EncodingChoice;

    fn doc_of(text: &str) -> TextDocument {
        build(
            text.as_bytes(),
            &ViewerOptions::default(),
            text.len() as u64,
            false,
        )
    }

    #[test]
    fn line_indexing_covers_the_usual_shapes() {
        let doc = doc_of("alpha\nbeta\ngamma");
        assert_eq!(doc.line_count(), 3);
        assert_eq!(doc.line(0), Some("alpha"));
        assert_eq!(doc.line(1), Some("beta"));
        assert_eq!(doc.line(2), Some("gamma"));
        assert_eq!(doc.line(3), None);
        assert_eq!(doc.line_starts, vec![0, 6, 11]);
    }

    #[test]
    fn trailing_newline_does_not_add_an_empty_line() {
        let doc = doc_of("a\nb\n");
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.line(1), Some("b"));
        assert_eq!(doc.line(2), None);
    }

    #[test]
    fn blank_lines_are_preserved() {
        let doc = doc_of("a\n\nb\n\n");
        assert_eq!(doc.line_count(), 4);
        assert_eq!(doc.line(1), Some(""));
        assert_eq!(doc.line(3), Some(""));
    }

    #[test]
    fn single_newline_is_one_empty_line() {
        let doc = doc_of("\n");
        assert_eq!(doc.line_count(), 1);
        assert_eq!(doc.line(0), Some(""));
    }

    #[test]
    fn empty_text_has_no_lines() {
        let doc = doc_of("");
        assert_eq!(doc.line_count(), 0);
        assert_eq!(doc.line(0), None);
        assert!(doc.line_starts.is_empty());
        assert!(!doc.truncated);
        assert!(!doc.is_rtl);
    }

    #[test]
    fn crlf_lines_exclude_the_carriage_return() {
        let doc = doc_of("one\r\ntwo\r\nthree");
        assert_eq!(doc.line_count(), 3);
        assert_eq!(doc.line(0), Some("one"));
        assert_eq!(doc.line(1), Some("two"));
        assert_eq!(doc.line(2), Some("three"));
        assert!(!doc.line(0).unwrap().contains('\r'));
    }

    #[test]
    fn lone_cr_does_not_split_lines() {
        let doc = doc_of("a\rb\nc");
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.line(0), Some("a\rb"));
    }

    #[test]
    fn find_reports_line_and_character_column() {
        let doc = doc_of("alpha\nbeta\ngamma");
        assert_eq!(doc.find("alpha", 0, true), Some((0, 0)));
        assert_eq!(doc.find("beta", 0, true), Some((1, 0)));
        assert_eq!(doc.find("mma", 0, true), Some((2, 2)));
        assert_eq!(doc.find("nope", 0, true), None);
    }

    #[test]
    fn find_honours_from_line() {
        let doc = doc_of("x\nx\nx");
        assert_eq!(doc.find("x", 0, true), Some((0, 0)));
        assert_eq!(doc.find("x", 1, true), Some((1, 0)));
        assert_eq!(doc.find("x", 3, true), None);
        assert_eq!(doc.find("x", 99, true), None);
    }

    #[test]
    fn find_is_case_insensitive_when_asked() {
        let doc = doc_of("Hello World\nÚSCHOVNA");
        assert_eq!(doc.find("hello", 0, true), None);
        assert_eq!(doc.find("hello", 0, false), Some((0, 0)));
        assert_eq!(doc.find("WORLD", 0, false), Some((0, 6)));
        // Non-ASCII case folding works too.
        assert_eq!(doc.find("úschovna", 0, false), Some((1, 0)));
        assert_eq!(doc.find("úschovna", 0, true), None);
    }

    #[test]
    fn find_column_counts_characters_not_bytes() {
        let doc = doc_of("příliš hello");
        let (line, column) = doc.find("hello", 0, true).expect("found");
        assert_eq!(line, 0);
        assert_eq!(column, 7);
        // Sanity: the byte offset really is larger than the character column.
        assert_eq!(doc.line(0).unwrap().find("hello"), Some(10));
    }

    #[test]
    fn find_ignores_an_empty_needle() {
        let doc = doc_of("abc");
        assert_eq!(doc.find("", 0, true), None);
        assert_eq!(doc.find("", 0, false), None);
    }

    #[test]
    fn find_works_on_a_file_without_trailing_newline() {
        let doc = doc_of("first\nlast");
        assert_eq!(doc.find("last", 0, true), Some((1, 0)));
        assert_eq!(doc.find("st", 1, true), Some((1, 2)));
    }

    #[test]
    fn lines_iterator_matches_random_access() {
        let doc = doc_of("a\nbb\nccc\n");
        let collected: Vec<&str> = doc.lines().collect();
        assert_eq!(collected, vec!["a", "bb", "ccc"]);
    }

    #[test]
    fn arabic_text_is_rtl() {
        let doc = doc_of("مرحبا بالعالم");
        assert!(doc.is_rtl);
    }

    #[test]
    fn hebrew_text_is_rtl() {
        let doc = doc_of("שלום עולם");
        assert!(doc.is_rtl);
    }

    #[test]
    fn czech_text_is_not_rtl() {
        let doc = doc_of("Příliš žluťoučký kůň úpěl ďábelské ódy.");
        assert!(!doc.is_rtl);
    }

    #[test]
    fn mostly_latin_text_with_one_rtl_word_stays_ltr() {
        let doc = doc_of("The Hebrew word שלום appears once in this long English sentence.");
        assert!(!doc.is_rtl);
    }

    #[test]
    fn mostly_rtl_text_with_one_latin_word_is_rtl() {
        let doc = doc_of("שלום עולם, hello, מה שלומך היום?");
        assert!(doc.is_rtl);
    }

    #[test]
    fn digits_and_punctuation_alone_are_not_rtl() {
        assert!(!doc_of("12345 !@#$% ...").is_rtl);
    }

    #[test]
    fn truncation_sets_flag_and_keeps_total_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("big.txt");
        let mut content = String::new();
        for index in 0..100 {
            content.push_str(&format!("line {index}\n"));
        }
        std::fs::write(&path, &content).expect("write");

        let opts = ViewerOptions {
            max_bytes_read: 50,
            ..ViewerOptions::default()
        };
        let doc = load_text(&path, &opts).expect("load_text");
        assert!(doc.truncated);
        assert_eq!(doc.total_bytes, content.len() as u64);
        assert_eq!(doc.text.len(), 50);
        assert!(content.starts_with(&doc.text));
    }

    #[test]
    fn truncation_does_not_leave_a_replacement_char_at_the_cut() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("accented.txt");
        // 100 two-byte characters: cutting at 51 bytes lands inside a character.
        std::fs::write(&path, "é".repeat(100)).expect("write");

        let opts = ViewerOptions {
            max_bytes_read: 51,
            ..ViewerOptions::default()
        };
        let doc = load_text(&path, &opts).expect("load_text");
        assert!(doc.truncated);
        assert_eq!(doc.total_bytes, 200);
        assert!(!doc.text.ends_with('\u{FFFD}'), "text tail: {:?}", doc.text);
        assert_eq!(doc.text.chars().count(), 25);
    }

    #[test]
    fn untruncated_file_reads_completely() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("small.txt");
        std::fs::write(&path, "héllo\nwörld\n").expect("write");
        let doc = load_text(&path, &ViewerOptions::default()).expect("load_text");
        assert!(!doc.truncated);
        assert_eq!(doc.total_bytes, 14);
        assert_eq!(doc.encoding, "utf-8");
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.line(1), Some("wörld"));
    }

    #[test]
    fn load_text_handles_utf16_with_bom() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("utf16le.txt");
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "ahoj\nsvěte".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        std::fs::write(&path, &bytes).expect("write");
        let doc = load_text(&path, &ViewerOptions::default()).expect("load_text");
        assert_eq!(doc.encoding, "utf-16le");
        assert!(doc.had_bom);
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.line(1), Some("světe"));
        assert_eq!(doc.total_bytes, bytes.len() as u64);
    }

    #[test]
    fn load_text_honours_an_explicit_encoding() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cp1250.txt");
        // "čau" in windows-1250: E8 61 75.
        std::fs::write(&path, [0xE8, b'a', b'u']).expect("write");

        let opts = ViewerOptions {
            encoding: EncodingChoice::Explicit("windows-1250"),
            ..ViewerOptions::default()
        };
        let doc = load_text(&path, &opts).expect("load_text");
        assert_eq!(doc.encoding, "windows-1250");
        assert_eq!(doc.text, "čau");
    }

    #[test]
    fn load_text_accepts_an_empty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.txt");
        std::fs::write(&path, b"").expect("write");
        let doc = load_text(&path, &ViewerOptions::default()).expect("load_text");
        assert!(doc.text.is_empty());
        assert_eq!(doc.line_count(), 0);
        assert_eq!(doc.total_bytes, 0);
        assert!(!doc.truncated);
        assert_eq!(doc.encoding, "utf-8");
    }

    #[test]
    fn load_text_propagates_missing_file_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(load_text(&dir.path().join("nope.txt"), &ViewerOptions::default()).is_err());
    }

    #[test]
    fn zero_byte_read_limit_reads_nothing_but_reports_the_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f.txt");
        std::fs::write(&path, b"hello").expect("write");
        let opts = ViewerOptions {
            max_bytes_read: 0,
            ..ViewerOptions::default()
        };
        let doc = load_text(&path, &opts).expect("load_text");
        assert!(doc.text.is_empty());
        assert_eq!(doc.total_bytes, 5);
        assert!(doc.truncated);
    }

    #[test]
    fn malformed_utf8_input_yields_replacements_without_panicking() {
        let bytes = b"ok \xFF\xFE\xFD tail".to_vec();
        let doc = build(&bytes, &ViewerOptions::default(), bytes.len() as u64, false);
        assert!(doc.text.starts_with("ok "));
        assert!(std::str::from_utf8(doc.text.as_bytes()).is_ok());
    }

    #[test]
    fn load_text_window_pages_through_a_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("long.txt");
        let mut content = String::new();
        for index in 0..200 {
            content.push_str(&format!("line {index:03}\n"));
        }
        std::fs::write(&path, &content).expect("write");

        let opts = ViewerOptions::default();
        let first = load_text_window(&path, 0, 90, &opts).expect("window 0");
        assert!(first.truncated, "more content follows the first window");
        assert_eq!(first.total_bytes, content.len() as u64);
        assert_eq!(first.line_count(), 10);
        assert_eq!(first.line(0), Some("line 000"));

        // The second window starts exactly where the first one ended.
        let second = load_text_window(&path, 90, 90, &opts).expect("window 1");
        assert_eq!(second.line(0), Some("line 010")); // 90 bytes = 10 rows of 9
        assert_eq!(second.line_starts.first(), Some(&0));

        // The last window reports that nothing follows it.
        let tail_offset = content.len() as u64 - 20;
        let tail = load_text_window(&path, tail_offset, 4096, &opts).expect("window tail");
        assert!(!tail.truncated);
        assert_eq!(tail.total_bytes, content.len() as u64);
    }

    #[test]
    fn load_text_window_drops_a_character_cut_at_the_start() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("accented.txt");
        std::fs::write(&path, "xééé").expect("write"); // 1 + 3*2 bytes

        // Offset 2 lands inside the first two-byte character; the window must
        // still decode as UTF-8 (the encoding comes from the file head).
        let window = load_text_window(&path, 2, 8, &ViewerOptions::default()).expect("window");
        assert_eq!(window.encoding, "utf-8");
        assert!(
            !window.text.starts_with('\u{FFFD}'),
            "text: {:?}",
            window.text
        );
        assert_eq!(window.text, "éé");
        assert_eq!(window.total_bytes, 7);
        assert!(!window.truncated);
    }

    #[test]
    fn load_text_window_keeps_the_file_encoding() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cp1250.txt");
        // "čaučau" in windows-1250: E8 61 75 E8 61 75.
        std::fs::write(&path, [0xE8, b'a', b'u', 0xE8, b'a', b'u']).expect("write");
        let opts = ViewerOptions {
            encoding: EncodingChoice::Explicit("windows-1250"),
            ..ViewerOptions::default()
        };
        let window = load_text_window(&path, 3, 3, &opts).expect("window");
        assert_eq!(window.encoding, "windows-1250");
        assert_eq!(window.text, "čau");
    }

    #[test]
    fn load_text_window_propagates_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            load_text_window(
                &dir.path().join("nope.txt"),
                0,
                16,
                &ViewerOptions::default()
            )
            .is_err()
        );
    }

    #[test]
    fn document_from_bytes_respects_the_limit() {
        let opts = ViewerOptions {
            max_bytes_read: 4,
            ..ViewerOptions::default()
        };
        let doc = document(b"0123456789", &opts);
        assert_eq!(doc.text, "0123");
        assert_eq!(doc.total_bytes, 10);
        assert!(doc.truncated);

        let doc = document(b"01", &opts);
        assert_eq!(doc.text, "01");
        assert!(!doc.truncated);
    }
}

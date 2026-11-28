//! Content classification from magic bytes and the file name.
//!
//! # Classification rules
//!
//! * An empty file is [`ContentKind::Empty`].
//! * A recognised **container signature** decides the kind: images (PNG, JPEG,
//!   GIF, BMP, WebP, TIFF, ICO, HEIC/AVIF, JP2, PSD), audio/video containers
//!   ([`ContentKind::Media`]) and descriptive binary formats (PDF, ELF, PE,
//!   Mach-O, Java class, WebAssembly, ZIP/gzip/bzip2/xz/7z/RAR/tar/CAB/ar,
//!   SQLite, OLE2, fonts, Windows registry) which are [`ContentKind::Binary`].
//! * Otherwise the bytes are inspected: BOMs, UTF-8 validity, a BOM-less UTF-16
//!   heuristic and control-byte statistics decide between [`ContentKind::Text`]
//!   and [`ContentKind::Hex`] (opaque binary content, the F3 fallback).
//! * **The extension is never used to override real content.** `fake.png`
//!   containing text is text; it is only used to give text a richer
//!   `format_hint` (`html`, `markdown`, `json`, `csv`, `xml`, `yaml`, `toml`,
//!   `ini`).
//! * [`detect`] never returns [`ContentKind::Unsupported`]: the kind is known even
//!   when the *rendering* fails, and [`crate::load`] turns that failure into
//!   [`crate::ViewerContent::Unsupported`].

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::encoding;

/// A coarse, viewer-oriented classification of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentKind {
    /// Decodable text; see [`crate::TextDocument`].
    Text,
    /// A decodable image (`png`, `jpeg`, `gif`, `bmp`, `webp`, `tiff`, `ico`);
    /// see [`crate::ImageDocument`]. Signatures the bundled decoder cannot read
    /// (`heic`, `avif`, `jp2`, `psd`) keep this kind so the GUI can report why.
    Image,
    /// Opaque bytes with no better model: render as a hexdump. This is the
    /// fallback for anything that is neither text nor a recognised container.
    Hex,
    /// Audio or video; see [`crate::MediaDocument`].
    Media,
    /// A recognised non-renderable container (document, executable, archive,
    /// database, font, …); see [`crate::BinaryDocument`].
    Binary,
    /// The file has zero bytes.
    Empty,
    /// Content the viewer cannot render (e.g. a corrupt or unsupported image).
    ///
    /// Produced by [`crate::load`] / [`crate::open_bytes`], never by [`detect`]:
    /// the format is still known, only the rendering is impossible.
    Unsupported,
}

/// What [`detect`] decided about a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFormat {
    /// Coarse classification driving the renderer choice.
    pub kind: ContentKind,
    /// Human readable format name: `"png"`, `"utf-8 text"`, `"elf"`, `"zip"`,
    /// `"html"`, `"windows-1250 text"`, …
    pub format_hint: String,
    /// Best-effort MIME type, `""` when unknown.
    pub mime: String,
}

/// Classify `head` (the first bytes of the file) using `name` for hints.
///
/// `head` may be shorter than the file — magic bytes live at the start. Passing
/// an empty slice always yields [`ContentKind::Empty`], which is what a 0-byte
/// file looks like.
///
/// ```no_run
/// use rc_viewer::{detect, ContentKind};
///
/// let f = detect("notes.txt", b"hello");
/// assert_eq!(f.kind, ContentKind::Text);
/// assert_eq!(f.format_hint, "utf-8 text");
///
/// // The name does not override the content.
/// let f = detect("fake.png", b"hello");
/// assert_eq!(f.kind, ContentKind::Text);
/// ```
pub fn detect(name: &str, head: &[u8]) -> FileFormat {
    if head.is_empty() {
        return FileFormat {
            kind: ContentKind::Empty,
            format_hint: "empty".to_string(),
            mime: String::new(),
        };
    }
    if let Some((kind, hint, mime)) = container(head) {
        return FileFormat {
            kind,
            format_hint: hint.to_string(),
            mime: mime.to_string(),
        };
    }
    if looks_like_text(head) {
        let (hint, mime) = match structured_hint(name, head) {
            Some((hint, mime)) => (hint.to_string(), mime),
            // Plain text: name the detected encoding so the GUI can show it.
            None => (format!("{} text", encoding::sniff_name(head)), "text/plain"),
        };
        return FileFormat {
            kind: ContentKind::Text,
            format_hint: hint,
            mime: mime.to_string(),
        };
    }
    FileFormat {
        kind: ContentKind::Hex,
        format_hint: "data".to_string(),
        mime: "application/octet-stream".to_string(),
    }
}

/// Read at most `max` bytes from the beginning of `path`.
///
/// This is the cheap probe `detect` needs: one `open`, one bounded read. It never
/// reads more than `max` bytes, so it is safe on multi-gigabyte files, block
/// devices and pipes.
pub fn probe(path: &Path, max: usize) -> std::io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut buf = Vec::new();
    file.take(max as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Read a bounded preview plus the real file size.
///
/// Returns `(bytes_read, total_bytes)`; `bytes_read.len() < total_bytes` means
/// the caller had to truncate. Non-regular files (pipes, devices) report the
/// number of bytes actually read.
pub(crate) fn read_head(path: &Path, max: usize) -> std::io::Result<(Vec<u8>, u64)> {
    let file = File::open(path)?;
    let declared = match file.metadata() {
        Ok(meta) if meta.is_file() => meta.len(),
        _ => 0,
    };
    let mut buf = Vec::new();
    file.take(max as u64).read_to_end(&mut buf)?;
    let total = declared.max(buf.len() as u64);
    Ok((buf, total))
}

/// Slice helper that never panics.
fn at(head: &[u8], start: usize, len: usize) -> Option<&[u8]> {
    head.get(start..start.checked_add(len)?)
}

/// Wrap a magic hit.
fn hit(
    kind: ContentKind,
    hint: &'static str,
    mime: &'static str,
) -> Option<(ContentKind, &'static str, &'static str)> {
    Some((kind, hint, mime))
}

/// Match known container signatures.
fn container(head: &[u8]) -> Option<(ContentKind, &'static str, &'static str)> {
    use ContentKind::{Binary, Image, Media};

    // A Unicode byte-order mark is text, full stop. It must be checked first
    // because the UTF-16 LE mark (FF FE) also satisfies an MPEG audio frame
    // sync pattern.
    if head.starts_with(&[0xFF, 0xFE])
        || head.starts_with(&[0xFE, 0xFF])
        || head.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
        || head.starts_with(&encoding::BOM_UTF8)
    {
        return None;
    }

    // ---------------- images ----------------
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        return hit(Image, "png", "image/png");
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return hit(Image, "jpeg", "image/jpeg");
    }
    if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        return hit(Image, "gif", "image/gif");
    }
    // A BMP header is 14 bytes; the two byte magic alone is too weak.
    if head.starts_with(b"BM") && head.len() >= 14 {
        return hit(Image, "bmp", "image/bmp");
    }
    if at(head, 0, 4) == Some(b"RIFF") && at(head, 8, 4) == Some(b"WEBP") {
        return hit(Image, "webp", "image/webp");
    }
    if head.starts_with(b"II*\x00") || head.starts_with(b"MM\x00*") {
        return hit(Image, "tiff", "image/tiff");
    }
    if head.starts_with(&[0x00, 0x00, 0x01, 0x00]) && head.len() >= 6 {
        return hit(Image, "ico", "image/x-icon");
    }
    if head.starts_with(&[0x00, 0x00, 0x00, 0x0C, 0x6A, 0x50, 0x20, 0x20]) {
        return hit(Image, "jp2", "image/jp2");
    }
    if head.starts_with(b"8BPS") {
        return hit(Image, "psd", "image/vnd.adobe.photoshop");
    }

    // Netpbm's short signature needs a plausible header, not just "P1".
    if looks_like_pnm(head) {
        return hit(Image, "pnm", "image/x-portable-anymap");
    }
    if head.starts_with(b"farbfeld") && head.len() >= 16 {
        return hit(Image, "farbfeld", "image/x-farbfeld");
    }

    // ---------------- ISO base media (ftyp) ----------------
    //
    // The same box carries images (HEIC/AVIF) and audio/video (MP4/MOV/M4A/3GP),
    // so the major brand decides.
    if at(head, 4, 4) == Some(b"ftyp") {
        let brand = at(head, 8, 4).unwrap_or(b"    ");
        return Some(match brand {
            b"heic" | b"heix" | b"heim" | b"heis" | b"mif1" | b"msf1" => {
                (Image, "heic", "image/heic")
            }
            b"avif" | b"avis" => (Image, "avif", "image/avif"),
            b"M4A " | b"M4B " | b"M4P " => (Media, "m4a", "audio/mp4"),
            b"qt  " => (Media, "mov", "video/quicktime"),
            b"3gp4" | b"3gp5" | b"3g2a" => (Media, "3gp", "video/3gpp"),
            _ => (Media, "mp4", "video/mp4"),
        });
    }

    // ---------------- audio / video ----------------
    if at(head, 0, 4) == Some(b"RIFF") {
        match at(head, 8, 4) {
            Some(b"WAVE") => return hit(Media, "wav", "audio/wav"),
            Some(b"AVI ") => return hit(Media, "avi", "video/x-msvideo"),
            _ => {}
        }
    }
    if head.starts_with(b"ID3") {
        return hit(Media, "mp3", "audio/mpeg");
    }
    // MPEG audio frame sync: 11 set bits, version != reserved, layer != reserved.
    // Checked after JPEG (which also starts with 0xFF).
    if head.len() >= 4
        && head[0] == 0xFF
        && head[1] & 0xE0 == 0xE0
        && head[1] & 0x18 != 0x08
        && head[1] & 0x06 != 0x00
    {
        return hit(Media, "mp3", "audio/mpeg");
    }
    if head.starts_with(b"OggS") {
        return hit(Media, "ogg", "audio/ogg");
    }
    if head.starts_with(b"fLaC") {
        return hit(Media, "flac", "audio/flac");
    }
    if head.starts_with(b"MThd") {
        return hit(Media, "midi", "audio/midi");
    }
    if at(head, 0, 4) == Some(b"FORM") && matches!(at(head, 8, 4), Some(b"AIFF") | Some(b"AIFC")) {
        return hit(Media, "aiff", "audio/aiff");
    }
    if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        // Matroska and WebM share the EBML header.
        return hit(Media, "matroska", "video/x-matroska");
    }
    if head.starts_with(b"FLV\x01") {
        return hit(Media, "flv", "video/x-flv");
    }
    if head.starts_with(&[0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11]) {
        return hit(Media, "asf", "video/x-ms-asf");
    }
    if head.starts_with(&[0x00, 0x00, 0x01, 0xBA]) || head.starts_with(&[0x00, 0x00, 0x01, 0xB3]) {
        return hit(Media, "mpeg", "video/mpeg");
    }
    if head.starts_with(b"#!AMR") {
        return hit(Media, "amr", "audio/amr");
    }
    if head.starts_with(b"wvpk") {
        return hit(Media, "wavpack", "audio/x-wavpack");
    }

    // ---------------- documents & executables ----------------
    if head.starts_with(b"%PDF-") {
        return hit(Binary, "pdf", "application/pdf");
    }
    if head.starts_with(b"\x7FELF") {
        return hit(Binary, "elf", "application/x-elf");
    }
    if looks_like_pe(head) {
        return hit(
            Binary,
            "pe",
            "application/vnd.microsoft.portable-executable",
        );
    }
    if head.starts_with(&[0xFE, 0xED, 0xFA, 0xCE])
        || head.starts_with(&[0xFE, 0xED, 0xFA, 0xCF])
        || head.starts_with(&[0xCE, 0xFA, 0xED, 0xFE])
        || head.starts_with(&[0xCF, 0xFA, 0xED, 0xFE])
    {
        return hit(Binary, "macho", "application/x-mach-binary");
    }
    if head.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE]) {
        // 0xCAFEBABE is both the Java class magic and the Mach-O universal
        // binary magic; the Java class file version right after it disambiguates.
        let version = at(head, 6, 2)
            .map(|v| u16::from_be_bytes([v[0], v[1]]))
            .unwrap_or(0);
        if (45..=100).contains(&version) {
            return hit(Binary, "java-class", "application/java-vm");
        }
        return hit(Binary, "macho", "application/x-mach-binary");
    }
    if head.starts_with(b"\x00asm") {
        return hit(Binary, "wasm", "application/wasm");
    }
    if head.starts_with(b"SQLite format 3\x00") {
        return hit(Binary, "sqlite", "application/vnd.sqlite3");
    }
    if head.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        // Legacy Office documents, MSI installers and Outlook messages.
        return hit(Binary, "ole2", "application/x-ole-storage");
    }

    // ---------------- archives ----------------
    if head.starts_with(b"PK\x03\x04")
        || head.starts_with(b"PK\x05\x06")
        || head.starts_with(b"PK\x07\x08")
    {
        return hit(Binary, "zip", "application/zip");
    }
    if head.starts_with(&[0x1F, 0x8B]) {
        return hit(Binary, "gzip", "application/gzip");
    }
    if head.starts_with(b"BZh") && head.get(3).is_some_and(|b| b.is_ascii_digit()) {
        return hit(Binary, "bzip2", "application/x-bzip2");
    }
    if head.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        return hit(Binary, "xz", "application/x-xz");
    }
    if head.starts_with(b"7z\xBC\xAF\x27\x1C") {
        return hit(Binary, "7z", "application/x-7z-compressed");
    }
    if head.starts_with(b"Rar!\x1A\x07") {
        return hit(Binary, "rar", "application/vnd.rar");
    }
    if at(head, 257, 5) == Some(b"ustar") {
        return hit(Binary, "tar", "application/x-tar");
    }
    if head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        return hit(Binary, "zstd", "application/zstd");
    }
    if head.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        return hit(Binary, "lz4", "application/x-lz4");
    }
    if head.starts_with(b"MSCF") {
        return hit(Binary, "cab", "application/vnd.ms-cab-compressed");
    }
    if head.starts_with(b"!<arch>\n") {
        return hit(Binary, "ar", "application/x-archive");
    }

    // ---------------- fonts & windows registry ----------------
    if head.starts_with(b"OTTO")
        || head.starts_with(b"ttcf")
        || head.starts_with(b"wOFF")
        || head.starts_with(b"wOF2")
        || (head.starts_with(&[0x00, 0x01, 0x00, 0x00]) && head.len() >= 12)
    {
        return hit(Binary, "font", "font/ttf");
    }
    if head.starts_with(b"regf") {
        return hit(Binary, "windows-registry", "application/x-windows-registry");
    }

    None
}

/// Require a Netpbm magic number followed by whitespace and plausible
/// dimensions (or a PAM header field). Comments may precede dimensions.
fn looks_like_pnm(head: &[u8]) -> bool {
    let [b'P', kind @ b'1'..=b'7', separator, rest @ ..] = head else {
        return false;
    };
    if !separator.is_ascii_whitespace() {
        return false;
    }
    let mut tokens = rest
        .split(|&byte| byte == b'\n' || byte == b'\r')
        .flat_map(|line| {
            let content = line.split(|&byte| byte == b'#').next().unwrap_or_default();
            content
                .split(|byte| byte.is_ascii_whitespace())
                .filter(|token| !token.is_empty())
        });
    if *kind == b'7' {
        return tokens.next().is_some_and(|token| {
            matches!(
                token,
                b"WIDTH" | b"HEIGHT" | b"DEPTH" | b"MAXVAL" | b"TUPLTYPE"
            )
        });
    }
    (0..2).all(|_| {
        tokens
            .next()
            .is_some_and(|token| !token.is_empty() && token.iter().all(u8::is_ascii_digit))
    })
}

/// True for a DOS/PE executable: an `MZ` header whose `e_lfanew` points at a
/// `PE\0\0` signature (or beyond the probe, when the probe is short).
fn looks_like_pe(head: &[u8]) -> bool {
    if head.len() < 0x40 || !head.starts_with(b"MZ") {
        return false;
    }
    let offset = head
        .get(0x3C..0x40)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .unwrap_or(0);
    if !(0x40..=0x1000).contains(&offset) {
        return false;
    }
    match head.get(offset..offset + 4) {
        Some(sig) => sig == b"PE\x00\x00",
        // The probe is too short to reach the PE header; trust the DOS header.
        None => true,
    }
}

/// Decide whether `head` is decodable text rather than opaque bytes.
fn looks_like_text(head: &[u8]) -> bool {
    if head.is_empty() {
        return false;
    }
    // A byte-order mark is decisive.
    if head.starts_with(&encoding::BOM_UTF8)
        || head.starts_with(&[0xFF, 0xFE])
        || head.starts_with(&[0xFE, 0xFF])
        || head.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
    {
        return true;
    }
    let has_nul = head.contains(&0);
    if has_nul {
        // BOM-less UTF-16 is the only realistic text with NUL bytes.
        return encoding::looks_like_utf16(head);
    }
    if encoding::looks_like_utf8(head) {
        return control_ratio_ok(head);
    }
    // Not UTF-8: legacy 8-bit text has a sprinkling of high bytes, while opaque
    // data is full of them.
    if !control_ratio_ok(head) {
        return false;
    }
    if head.len() < 16 {
        // Too few bytes for statistics: absence of NUL/control bytes is all the
        // evidence there is, and short text is the far more likely reading.
        return true;
    }
    if sparse_high_bytes(head) {
        return true;
    }
    // Dense high bytes are typical for text in a legacy CJK encoding (or for
    // opaque data); accept it only when the statistically detected encoding
    // decodes the probe almost cleanly.
    dense_high_bytes_decode_cleanly(head)
}

/// No more than ~1% stray control bytes (tabs, line breaks, ESC are fine).
fn control_ratio_ok(head: &[u8]) -> bool {
    let mut control = 0usize;
    for &b in head {
        let stray = match b {
            0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x1B => false,
            b if b < 0x20 => true,
            0x7F => true,
            _ => false,
        };
        if stray {
            control += 1;
        }
    }
    control.saturating_mul(100) <= head.len()
}

/// Mostly-ASCII content: the profile of Latin/Cyrillic/Greek legacy text.
fn sparse_high_bytes(head: &[u8]) -> bool {
    let high = head.iter().filter(|&&b| b >= 0x80).count();
    high.saturating_mul(2) <= head.len()
}

/// Accept high-byte-dense text only when a CJK legacy encoding explains it.
///
/// Random binary data decodes to a stream full of replacement characters, real
/// CJK text does not.
fn dense_high_bytes_decode_cleanly(head: &[u8]) -> bool {
    // Real multi-byte text still contains ASCII: line breaks, spaces, digits.
    let has_ascii = head
        .iter()
        .any(|&b| b == b' ' || b == b'\n' || b == b'\t' || b.is_ascii_graphic());
    if !has_ascii {
        return false;
    }
    let Some(enc) = encoding::cjk_guess(head) else {
        return false;
    };
    let (decoded, _had_errors) = enc.decode_without_bom_handling(head);
    let chars = decoded.chars().count();
    let broken = decoded.chars().filter(|c| *c == '\u{FFFD}').count();
    chars > 0 && broken.saturating_mul(50) <= chars
}

/// A richer `(format_hint, mime)` for text, from the content and then the name.
fn structured_hint(name: &str, head: &[u8]) -> Option<(&'static str, &'static str)> {
    // Content first: markup announces itself.
    let text = head.get(..head.len().min(512)).unwrap_or(head);
    let trimmed = trim_ascii_start(text);
    if starts_with_ignore_case(trimmed, b"<?xml") {
        return Some(("xml", "application/xml"));
    }
    if starts_with_ignore_case(trimmed, b"<!doctype html")
        || starts_with_ignore_case(trimmed, b"<html")
    {
        return Some(("html", "text/html"));
    }
    // WEBVTT declares itself even without a filename extension.
    let vtt = trimmed.strip_prefix(b"\xef\xbb\xbf").unwrap_or(trimmed);
    if let Some(rest) = vtt.strip_prefix(b"WEBVTT")
        && rest.first().is_none_or(|byte| byte.is_ascii_whitespace())
    {
        return Some(("vtt", "text/vtt"));
    }
    let ext = extension(name)?;
    Some(match ext.as_str() {
        "html" | "htm" | "shtml" => ("html", "text/html"),
        "md" | "markdown" | "mdown" | "mkd" | "mkdn" => ("markdown", "text/markdown"),
        "json" => ("json", "application/json"),
        "jsonl" | "ndjson" => ("jsonl", "application/x-ndjson"),
        "srt" => ("srt", "application/x-subrip"),
        "vtt" => ("vtt", "text/vtt"),
        "csv" => ("csv", "text/csv"),
        "tsv" | "tab" => ("csv", "text/tab-separated-values"),
        "xml" | "xsl" | "xslt" | "xsd" | "svg" | "plist" | "xhtml" => ("xml", "application/xml"),
        "yaml" | "yml" => ("yaml", "application/yaml"),
        "toml" => ("toml", "application/toml"),
        "ini" | "cfg" | "conf" => ("ini", "text/plain"),
        _ => return None,
    })
}

/// The lower-cased extension of a file name, if any.
fn extension(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let (stem, ext) = base.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() || ext.len() > 12 {
        return None;
    }
    if !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Skip leading ASCII whitespace.
fn trim_ascii_start(bytes: &[u8]) -> &[u8] {
    let skip = bytes.iter().take_while(|b| b.is_ascii_whitespace()).count();
    bytes.get(skip..).unwrap_or(&[])
}

/// ASCII case-insensitive prefix test.
fn starts_with_ignore_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn kind_of(name: &str, head: &[u8]) -> (ContentKind, String, String) {
        let f = detect(name, head);
        (f.kind, f.format_hint, f.mime)
    }

    #[test]
    fn png_magic_is_an_image() {
        let (kind, hint, mime) = kind_of("x.png", b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0DIHDR");
        assert_eq!(kind, ContentKind::Image);
        assert_eq!(hint, "png");
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn netpbm_and_farbfeld_are_detected_from_content() {
        for bytes in [
            &b"P1\n2 1\n0 1\n"[..],
            &b"P6\n# comment\n2 1\n255\n"[..],
            &b"P7\nWIDTH 1\nHEIGHT 1\n"[..],
        ] {
            let (kind, hint, _) = kind_of("wrong.txt", bytes);
            assert_eq!(kind, ContentKind::Image);
            assert_eq!(hint, "pnm");
        }
        assert_eq!(
            kind_of("notes.txt", b"P1 is the first priority").0,
            ContentKind::Text
        );
        assert_eq!(
            kind_of("wrong.txt", b"farbfeld\x00\x00\x00\x01\x00\x00\x00\x01").1,
            "farbfeld"
        );
    }

    #[test]
    fn jpeg_magic_is_an_image() {
        let (kind, hint, mime) = kind_of("x.jpg", &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]);
        assert_eq!(kind, ContentKind::Image);
        assert_eq!(hint, "jpeg");
        assert_eq!(mime, "image/jpeg");
    }

    #[test]
    fn gif_magic_is_an_image() {
        for magic in [&b"GIF87a"[..], &b"GIF89a"[..]] {
            let (kind, hint, _) = kind_of("x.gif", magic);
            assert_eq!(kind, ContentKind::Image);
            assert_eq!(hint, "gif");
        }
    }

    #[test]
    fn bmp_magic_needs_a_full_header() {
        let (kind, hint, _) = kind_of(
            "x.bmp",
            b"BM\x36\x00\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00",
        );
        assert_eq!(kind, ContentKind::Image);
        assert_eq!(hint, "bmp");
        // Two bytes alone are too weak to call it a BMP.
        let (kind, _, _) = kind_of("x.bmp", b"BM");
        assert_ne!(kind, ContentKind::Image);
    }

    #[test]
    fn webp_magic_is_an_image() {
        let (kind, hint, mime) = kind_of("x.webp", b"RIFF\x24\x00\x00\x00WEBPVP8 ");
        assert_eq!(kind, ContentKind::Image);
        assert_eq!(hint, "webp");
        assert_eq!(mime, "image/webp");
    }

    #[test]
    fn tiff_and_ico_are_images() {
        for (head, want) in [
            (&b"II*\x00\x08\x00\x00\x00"[..], "tiff"),
            (&b"MM\x00*\x00\x00\x00\x08"[..], "tiff"),
            (&[0x00, 0x00, 0x01, 0x00, 0x01, 0x00][..], "ico"),
        ] {
            let (kind, hint, _) = kind_of("x.img", head);
            assert_eq!(kind, ContentKind::Image, "hint {want}");
            assert_eq!(hint, want);
        }
    }

    #[test]
    fn unsupported_image_codecs_are_still_classified_as_images() {
        let mut heic = vec![0x00, 0x00, 0x00, 0x18];
        heic.extend_from_slice(b"ftypheic");
        let (kind, hint, mime) = kind_of("x.heic", &heic);
        assert_eq!(kind, ContentKind::Image);
        assert_eq!(hint, "heic");
        assert_eq!(mime, "image/heic");

        let mut avif = vec![0x00, 0x00, 0x00, 0x1C];
        avif.extend_from_slice(b"ftypavif");
        assert_eq!(kind_of("x.avif", &avif).1, "avif");

        assert_eq!(kind_of("x.psd", b"8BPS\x00\x01").1, "psd");
        assert_eq!(
            kind_of("x.jp2", &[0x00, 0x00, 0x00, 0x0C, 0x6A, 0x50, 0x20, 0x20]).1,
            "jp2"
        );
    }

    #[test]
    fn empty_slice_is_empty() {
        let f = detect("anything.txt", &[]);
        assert_eq!(f.kind, ContentKind::Empty);
        assert_eq!(f.format_hint, "empty");
        assert_eq!(f.mime, "");
    }

    #[test]
    fn text_detection_covers_boms_ascii_and_utf8() {
        let (kind, hint, mime) = kind_of("a.txt", b"plain ascii text\n");
        assert_eq!(kind, ContentKind::Text);
        assert_eq!(hint, "utf-8 text");
        assert_eq!(mime, "text/plain");

        let mut bom = crate::encoding::BOM_UTF8.to_vec();
        bom.extend_from_slice(b"hello");
        assert_eq!(kind_of("a.dat", &bom).0, ContentKind::Text);

        let utf16 = [0xFF, 0xFE, b'h', 0x00, b'i', 0x00];
        let (kind, hint, _) = kind_of("a.dat", &utf16);
        assert_eq!(kind, ContentKind::Text);
        assert_eq!(hint, "utf-16le text");

        let utf32 = [0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00];
        assert_eq!(kind_of("a.dat", &utf32).0, ContentKind::Text);
    }

    #[test]
    fn nul_heavy_bytes_are_binary_not_text() {
        let (kind, hint, mime) = kind_of("x.bin", b"\x00\x01\x02\x00\x03\x00\xff\x00");
        assert_eq!(kind, ContentKind::Hex);
        assert_eq!(hint, "data");
        assert_eq!(mime, "application/octet-stream");
    }

    #[test]
    fn bomless_utf16_is_text() {
        let le: Vec<u8> = "hello world"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let (kind, hint, _) = kind_of("a.dat", &le);
        assert_eq!(kind, ContentKind::Text);
        assert_eq!(hint, "utf-16le text");

        let be: Vec<u8> = "hello world"
            .encode_utf16()
            .flat_map(|u| u.to_be_bytes())
            .collect();
        assert_eq!(kind_of("a.dat", &be).1, "utf-16be text");
    }

    #[test]
    fn latin1_style_text_is_text() {
        // 0xE8 0x9E (č, ž in windows-1250) among ASCII letters.
        let bytes = b"Zlu\x9Eou\xE8k\xFD k\xF9\xF2".to_vec();
        assert_eq!(kind_of("czech.txt", &bytes).0, ContentKind::Text);
    }

    #[test]
    fn high_byte_noise_is_binary() {
        let bytes: Vec<u8> = (0u8..=255).filter(|b| *b >= 0x80).collect();
        assert_eq!(kind_of("noise.bin", &bytes).0, ContentKind::Hex);
    }

    #[test]
    fn magic_wins_over_a_lying_extension() {
        // A text file called fake.png stays text.
        let (kind, hint, mime) = kind_of("fake.png", b"this is definitely not a png\n");
        assert_eq!(kind, ContentKind::Text);
        assert_eq!(hint, "utf-8 text");
        assert_eq!(mime, "text/plain");

        // A real PNG called photo.txt is still an image.
        let (kind, hint, _) = kind_of("photo.txt", b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0D");
        assert_eq!(kind, ContentKind::Image);
        assert_eq!(hint, "png");

        // A JPEG called photo.png is a JPEG, not a PNG.
        let (_, hint, _) = kind_of("photo.png", &[0xFF, 0xD8, 0xFF, 0xE1]);
        assert_eq!(hint, "jpeg");
    }

    #[test]
    fn pdf_elf_and_pe_are_binary_documents() {
        let (kind, hint, mime) = kind_of("doc.pdf", b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
        assert_eq!(
            (kind, hint.as_str(), mime.as_str()),
            (ContentKind::Binary, "pdf", "application/pdf")
        );

        let (kind, hint, _) = kind_of("a.out", b"\x7FELF\x02\x01\x01\x00");
        assert_eq!((kind, hint.as_str()), (ContentKind::Binary, "elf"));

        // A minimal PE: MZ, e_lfanew = 0x80 with a PE signature there.
        let mut pe = vec![0u8; 0x100];
        pe[0..2].copy_from_slice(b"MZ");
        pe[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\x00\x00");
        let (kind, hint, _) = kind_of("app.exe", &pe);
        assert_eq!((kind, hint.as_str()), (ContentKind::Binary, "pe"));
    }

    #[test]
    fn mz_without_a_pe_header_is_not_an_executable() {
        let mut fake = vec![b'x'; 0x100];
        fake[0..2].copy_from_slice(b"MZ");
        // e_lfanew stays zero, so there is no PE signature where one must be.
        assert_ne!(kind_of("app.exe", &fake).0, ContentKind::Binary);
    }

    #[test]
    fn java_class_and_macho_share_their_magic() {
        let java = [0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x41];
        assert_eq!(kind_of("A.class", &java).1, "java-class");
        let fat = [0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x02];
        assert_eq!(kind_of("binary", &fat).1, "macho");
        let macho = [0xCF, 0xFA, 0xED, 0xFE, 0x07, 0x00, 0x00, 0x01];
        assert_eq!(kind_of("binary", &macho).1, "macho");
    }

    #[test]
    fn archives_are_recognised() {
        let cases: [(&[u8], &str); 11] = [
            (b"PK\x03\x04\x14\x00", "zip"),
            (b"PK\x05\x06\x00\x00", "zip"),
            (b"\x1F\x8B\x08\x00", "gzip"),
            (b"BZh9\x17\x72", "bzip2"),
            (b"\xFD7zXZ\x00", "xz"),
            (b"7z\xBC\xAF\x27\x1C", "7z"),
            (b"Rar!\x1A\x07\x00", "rar"),
            (b"\x28\xB5\x2F\xFD\x00", "zstd"),
            (b"\x04\x22\x4D\x18\x00", "lz4"),
            (b"MSCF\x00\x00", "cab"),
            (b"!<arch>\n", "ar"),
        ];
        for (head, want) in cases {
            let (kind, hint, _) = kind_of("archive.bin", head);
            assert_eq!(kind, ContentKind::Binary, "hint {want}");
            assert_eq!(hint, want);
        }
    }

    #[test]
    fn tar_is_recognised_by_its_ustar_marker() {
        let mut head = vec![0u8; 512];
        head[257..262].copy_from_slice(b"ustar");
        let (kind, hint, mime) = kind_of("a.tar", &head);
        assert_eq!(kind, ContentKind::Binary);
        assert_eq!(hint, "tar");
        assert_eq!(mime, "application/x-tar");
    }

    #[test]
    fn sqlite_ole_font_and_registry_are_binary() {
        assert_eq!(kind_of("db", b"SQLite format 3\x00").1, "sqlite");
        assert_eq!(
            kind_of("doc", &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]).1,
            "ole2"
        );
        assert_eq!(kind_of("f", b"OTTO\x00\x00").1, "font");
        assert_eq!(kind_of("f", b"wOF2\x00\x00").1, "font");
        // A TrueType header needs at least 12 bytes to be distinguishable.
        assert_eq!(
            kind_of(
                "f",
                &[
                    0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x80, 0x00, 0x03, 0x00, 0x10
                ]
            )
            .1,
            "font"
        );
        assert_eq!(kind_of("ntuser.dat", b"regf\x00\x00").1, "windows-registry");
    }

    #[test]
    fn audio_and_video_are_media() {
        let cases: [(&[u8], &str, &str, &str); 12] = [
            (b"RIFF\x24\x00\x00\x00WAVEfmt ", "wav", "audio/wav", "audio"),
            (
                b"RIFF\x24\x00\x00\x00AVI LIST",
                "avi",
                "video/x-msvideo",
                "video",
            ),
            (
                b"ID3\x04\x00\x00\x00\x00\x00\x00",
                "mp3",
                "audio/mpeg",
                "audio",
            ),
            (b"OggS\x00\x02\x00\x00", "ogg", "audio/ogg", "audio"),
            (b"fLaC\x00\x00\x00\x22", "flac", "audio/flac", "audio"),
            (b"MThd\x00\x00\x00\x06", "midi", "audio/midi", "audio"),
            (b"FORM\x00\x00\x00\x00AIFF", "aiff", "audio/aiff", "audio"),
            (
                b"\x1A\x45\xDF\xA3\x9F\x42\x86\x81",
                "matroska",
                "video/x-matroska",
                "video",
            ),
            (b"FLV\x01\x05\x00\x00\x00", "flv", "video/x-flv", "video"),
            (b"\x00\x00\x01\xBA\x44\x00", "mpeg", "video/mpeg", "video"),
            (b"#!AMR\n\x00\x00", "amr", "audio/amr", "audio"),
            (
                b"wvpk\x00\x00\x00\x00",
                "wavpack",
                "audio/x-wavpack",
                "audio",
            ),
        ];
        for (head, hint, mime, _class) in cases {
            let (kind, got_hint, got_mime) = kind_of("media", head);
            assert_eq!(kind, ContentKind::Media, "hint {hint}");
            assert_eq!(got_hint, hint);
            assert_eq!(got_mime, mime);
        }
    }

    #[test]
    fn mp4_family_uses_the_ftyp_brand() {
        let mut mp4 = vec![0x00, 0x00, 0x00, 0x18];
        mp4.extend_from_slice(b"ftypisom");
        assert_eq!(kind_of("m.mp4", &mp4).1, "mp4");

        let mut m4a = vec![0x00, 0x00, 0x00, 0x18];
        m4a.extend_from_slice(b"ftypM4A ");
        let (kind, hint, mime) = kind_of("m.m4a", &m4a);
        assert_eq!(
            (kind, hint.as_str(), mime.as_str()),
            (ContentKind::Media, "m4a", "audio/mp4")
        );

        let mut mov = vec![0x00, 0x00, 0x00, 0x14];
        mov.extend_from_slice(b"ftypqt  ");
        assert_eq!(kind_of("m.mov", &mov).1, "mov");
    }

    #[test]
    fn mpeg_audio_frame_sync_is_not_confused_with_jpeg() {
        // 0xFF 0xFB is an MPEG-1 Layer III frame header.
        assert_eq!(kind_of("s.mp3", &[0xFF, 0xFB, 0x90, 0x64]).1, "mp3");
        // 0xFF 0xD8 0xFF is a JPEG regardless of the frame-sync bits.
        assert_eq!(kind_of("s.bin", &[0xFF, 0xD8, 0xFF, 0xE0]).1, "jpeg");
    }

    #[test]
    fn structured_text_gets_a_format_hint() {
        assert_eq!(
            kind_of("page.html", b"<!DOCTYPE html><html></html>").1,
            "html"
        );
        assert_eq!(
            kind_of("page.htm", b"<html><body>x</body></html>").1,
            "html"
        );
        assert_eq!(kind_of("page.dat", b"  <!doctype HTML>\n<p>x").1, "html");
        assert_eq!(
            kind_of("feed.xml", b"<?xml version=\"1.0\"?><rss/>").1,
            "xml"
        );
        assert_eq!(kind_of("feed.dat", b"<?xml?>").1, "xml");
        assert_eq!(kind_of("notes.md", b"# Title\n\nbody\n").1, "markdown");
        assert_eq!(kind_of("data.json", b"{\"a\":1}").1, "json");
        assert_eq!(kind_of("events.JSONL", b"{}\n{}\n").1, "jsonl");
        assert_eq!(kind_of("events.ndjson", b"{}\n").2, "application/x-ndjson");
        assert_eq!(
            kind_of("lesson.SRT", b"1\n00:00:01,000 --> 00:00:02,000\nHi").1,
            "srt"
        );
        assert_eq!(kind_of("lesson.vtt", b"WEBVTT\n\n").2, "text/vtt");
        assert_eq!(
            kind_of("unnamed.txt", b"WEBVTT\n\n00:01.000 --> 00:02.000\nHi").1,
            "vtt"
        );
        assert_ne!(kind_of("notes.txt", b"WEBVTTish").1, "vtt");
        assert_eq!(
            kind_of("fake.jsonl", b"\x89PNG\r\n\x1a\n").0,
            ContentKind::Image
        );
        assert_eq!(kind_of("table.csv", b"a,b\n1,2\n").1, "csv");
        assert_eq!(kind_of("table.tsv", b"a\tb\n1\t2\n").1, "csv");
        assert_eq!(kind_of("conf.yml", b"a: 1\n").1, "yaml");
        assert_eq!(kind_of("conf.toml", b"a = 1\n").1, "toml");
        assert_eq!(kind_of("conf.ini", b"[x]\n").1, "ini");
        assert_eq!(kind_of("drawing.svg", b"<svg/>\n").1, "xml");
    }

    #[test]
    fn structured_text_keeps_the_right_mime_types() {
        assert_eq!(kind_of("a.html", b"<html>").2, "text/html");
        assert_eq!(kind_of("a.md", b"# x").2, "text/markdown");
        assert_eq!(kind_of("a.json", b"{}").2, "application/json");
        assert_eq!(kind_of("a.csv", b"a,b").2, "text/csv");
        assert_eq!(kind_of("a.xml", b"<a/>").2, "application/xml");
    }

    #[test]
    fn unknown_extensions_fall_back_to_encoding_named_text() {
        assert_eq!(kind_of("README", b"hello\n").1, "utf-8 text");
        assert_eq!(kind_of("x.weird", b"hello\n").1, "utf-8 text");
        assert_eq!(kind_of(".bashrc", b"export A=1\n").1, "utf-8 text");
    }

    #[test]
    fn extension_parsing_is_sane() {
        assert_eq!(extension("a/b/c.PNG").as_deref(), Some("png"));
        assert_eq!(extension("C:\\dir\\file.CSV").as_deref(), Some("csv"));
        assert_eq!(extension("noext"), None);
        assert_eq!(extension(".hidden"), None);
        assert_eq!(extension("trailing."), None);
        assert_eq!(extension("very.longextthatisnotanextension"), None);
    }

    #[test]
    fn probe_reads_only_the_requested_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sample.bin");
        {
            let mut f = std::fs::File::create(&path).expect("create");
            f.write_all(&(0u8..=255).collect::<Vec<u8>>())
                .expect("write");
        }
        let head = probe(&path, 8).expect("probe");
        assert_eq!(head, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let all = probe(&path, 4096).expect("probe");
        assert_eq!(all.len(), 256);
        let none = probe(&path, 0).expect("probe");
        assert!(none.is_empty());
    }

    #[test]
    fn probe_handles_empty_and_missing_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let empty = dir.path().join("empty.bin");
        std::fs::File::create(&empty).expect("create");
        assert!(probe(&empty, 16).expect("probe").is_empty());
        assert_eq!(
            detect("empty.bin", &probe(&empty, 16).expect("probe")).kind,
            ContentKind::Empty
        );

        let missing = dir.path().join("nope.bin");
        assert!(probe(&missing, 16).is_err());
    }

    #[test]
    fn read_head_reports_the_real_size_when_truncated() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("big.txt");
        std::fs::write(&path, b"0123456789").expect("write");
        let (bytes, total) = read_head(&path, 4).expect("read_head");
        assert_eq!(bytes, b"0123");
        assert_eq!(total, 10);
        let (bytes, total) = read_head(&path, 4096).expect("read_head");
        assert_eq!(bytes.len(), 10);
        assert_eq!(total, 10);
    }

    #[test]
    fn every_kind_is_reachable_from_detect_except_unsupported() {
        // Documented: `detect` never returns Unsupported; the loader produces it
        // when a recognised container cannot be rendered.
        let samples: [&[u8]; 6] = [
            b"hello",
            b"\x89PNG\r\n\x1a\n",
            b"\x00\x01\x00\x02",
            b"OggS\x00\x00",
            b"%PDF-1.4",
            b"",
        ];
        let kinds: Vec<ContentKind> = samples.iter().map(|s| detect("f", s).kind).collect();
        assert!(kinds.contains(&ContentKind::Text));
        assert!(kinds.contains(&ContentKind::Image));
        assert!(kinds.contains(&ContentKind::Hex));
        assert!(kinds.contains(&ContentKind::Media));
        assert!(kinds.contains(&ContentKind::Binary));
        assert!(kinds.contains(&ContentKind::Empty));
        assert!(!kinds.contains(&ContentKind::Unsupported));
    }
}

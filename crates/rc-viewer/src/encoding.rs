//! Encoding sniffing and decoding.
//!
//! Detection order (first match wins):
//!
//! 1. a byte-order mark — UTF-32 LE/BE, UTF-8, UTF-16 LE/BE;
//! 2. an explicitly requested encoding label (unless a BOM contradicted it);
//! 3. valid UTF-8 (which also covers pure ASCII);
//! 4. a BOM-less UTF-16 heuristic (NUL bytes on the odd or the even offsets);
//! 5. statistical detection with `chardetng`, clamped to `windows-1252` when the
//!    detector answers UTF-8 for a stream that is not valid UTF-8.
//!
//! Decoding never fails: malformed sequences become `U+FFFD`, exactly like a
//! browser would render them.

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{
    BIG5, EUC_JP, EUC_KR, Encoding, GBK, SHIFT_JIS, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252,
};

use crate::options::EncodingChoice;

/// UTF-8 byte-order mark.
pub(crate) const BOM_UTF8: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Which decoder a [`Scent`] resolves to. `encoding_rs` cannot express UTF-32,
/// so those two variants are decoded by this crate.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Kind {
    /// A WHATWG single-byte or multi-byte encoding handled by `encoding_rs`.
    Rs(&'static Encoding),
    /// UTF-32, little endian, decoded manually.
    Utf32Le,
    /// UTF-32, big endian, decoded manually.
    Utf32Be,
}

/// The outcome of sniffing: which decoder, its canonical name, whether a BOM was
/// consumed and how many leading bytes belong to that BOM.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scent {
    /// Canonical, lower-case encoding name (e.g. `"utf-16le"`, `"windows-1250"`).
    pub name: &'static str,
    /// Decoder to use.
    pub kind: Kind,
    /// Whether the stream started with a byte-order mark.
    pub had_bom: bool,
    /// Number of leading bytes to skip before decoding.
    pub skip: usize,
}

/// Decode a byte stream to a `String`.
///
/// `expected_bom` is an encoding label requested by the caller. An empty string,
/// `"auto"`, `"none"`, `"binary"` or `"detect"` all mean "sniff it", and an
/// unknown label also falls back to sniffing rather than failing. A byte-order
/// mark in the stream always overrides the request.
///
/// Returns `(text, canonical encoding name, had_bom)`. Malformed input yields
/// `U+FFFD` replacement characters — this function never panics and never
/// returns an error.
///
/// ```
/// let (text, enc, bom) = rc_viewer::decode_text(b"hi", "");
/// assert_eq!((text.as_str(), enc, bom), ("hi", "utf-8", false));
///
/// let (text, enc, bom) = rc_viewer::decode_text(b"\xFF\xFEa\x00", "");
/// assert_eq!((text.as_str(), enc, bom), ("a", "utf-16le", true));
/// ```
pub fn decode_text(bytes: &[u8], expected_bom: &str) -> (String, &'static str, bool) {
    let scent = resolve_label(bytes, expected_bom);
    decode_with(bytes, &scent)
}

/// Decode `bytes` using an already resolved [`Scent`].
pub(crate) fn decode_with(bytes: &[u8], scent: &Scent) -> (String, &'static str, bool) {
    let body = bytes.get(scent.skip..).unwrap_or(&[]);
    let text = match scent.kind {
        Kind::Rs(enc) => enc.decode_without_bom_handling(body).0.into_owned(),
        Kind::Utf32Le => decode_utf32(body, true),
        Kind::Utf32Be => decode_utf32(body, false),
    };
    (text, scent.name, scent.had_bom)
}

/// Resolve the decoder for `bytes` according to the GUI's encoding choice.
pub(crate) fn resolve(bytes: &[u8], choice: &EncodingChoice) -> Scent {
    match choice {
        EncodingChoice::Auto => resolve_label(bytes, ""),
        EncodingChoice::Explicit(label) => resolve_label(bytes, label),
    }
}

/// Resolve the decoder for `bytes` given a caller supplied label.
///
/// A BOM wins over the label; an unresolvable label means "sniff".
pub(crate) fn resolve_label(bytes: &[u8], label: &str) -> Scent {
    if let Some(scent) = sniff_bom(bytes) {
        return scent;
    }
    if let Some(scent) = from_label(label) {
        return scent;
    }
    sniff_auto(bytes)
}

/// Resolve an encoding label without looking at any bytes.
///
/// Returns `None` for auto-style labels and for labels the platform does not
/// know.
pub(crate) fn from_label(label: &str) -> Option<Scent> {
    let label = label.trim();
    if label.is_empty()
        || label.eq_ignore_ascii_case("auto")
        || label.eq_ignore_ascii_case("none")
        || label.eq_ignore_ascii_case("binary")
        || label.eq_ignore_ascii_case("detect")
    {
        return None;
    }
    let lower = label.to_ascii_lowercase();
    if matches!(lower.as_str(), "utf-32" | "utf32" | "utf-32le" | "utf32le") {
        return Some(Scent {
            name: "utf-32le",
            kind: Kind::Utf32Le,
            had_bom: false,
            skip: 0,
        });
    }
    if matches!(lower.as_str(), "utf-32be" | "utf32be") {
        return Some(Scent {
            name: "utf-32be",
            kind: Kind::Utf32Be,
            had_bom: false,
            skip: 0,
        });
    }
    let enc = Encoding::for_label(label.as_bytes())?;
    Some(Scent {
        name: canonical_name(enc),
        kind: Kind::Rs(enc),
        had_bom: false,
        skip: 0,
    })
}

/// The canonical lower-case name of an `encoding_rs` encoding.
///
/// Falls back to `Encoding::name()` (also `&'static str`) for encodings this
/// crate does not name explicitly.
pub(crate) fn canonical_name(enc: &'static Encoding) -> &'static str {
    match enc.name() {
        "UTF-8" => "utf-8",
        "UTF-16LE" => "utf-16le",
        "UTF-16BE" => "utf-16be",
        "windows-1250" => "windows-1250",
        "windows-1251" => "windows-1251",
        "windows-1252" => "windows-1252",
        "windows-1253" => "windows-1253",
        "windows-1254" => "windows-1254",
        "windows-1255" => "windows-1255",
        "windows-1256" => "windows-1256",
        "windows-1257" => "windows-1257",
        "windows-1258" => "windows-1258",
        "ISO-8859-2" => "iso-8859-2",
        "ISO-8859-3" => "iso-8859-3",
        "ISO-8859-4" => "iso-8859-4",
        "ISO-8859-5" => "iso-8859-5",
        "ISO-8859-6" => "iso-8859-6",
        "ISO-8859-7" => "iso-8859-7",
        "ISO-8859-8" => "iso-8859-8",
        "ISO-8859-10" => "iso-8859-10",
        "ISO-8859-13" => "iso-8859-13",
        "ISO-8859-14" => "iso-8859-14",
        "ISO-8859-15" => "iso-8859-15",
        "ISO-8859-16" => "iso-8859-16",
        "KOI8-R" => "koi8-r",
        "KOI8-U" => "koi8-u",
        "IBM866" => "ibm866",
        "macintosh" => "macintosh",
        "x-mac-cyrillic" => "x-mac-cyrillic",
        "Shift_JIS" => "shift_jis",
        "EUC-JP" => "euc-jp",
        "GBK" => "gbk",
        "Big5" => "big5",
        "EUC-KR" => "euc-kr",
        "ISO-2022-JP" => "iso-2022-jp",
        other => other,
    }
}

/// The canonical name an auto-detector would pick for `head`, without decoding.
pub(crate) fn sniff_name(head: &[u8]) -> &'static str {
    resolve_label(head, "").name
}

/// Whether `bytes` look like BOM-less UTF-16 (used by the text/binary heuristic).
pub(crate) fn looks_like_utf16(bytes: &[u8]) -> bool {
    sniff_utf16_without_bom(bytes).is_some()
}

/// Whether `bytes` are (almost) valid UTF-8.
///
/// An *incomplete sequence at the very end* is tolerated: a preview is cut at an
/// arbitrary byte offset, and a truncated tail must not make a UTF-8 file look
/// like a legacy single-byte file.
pub(crate) fn looks_like_utf8(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(error) => error.error_len().is_none() && bytes.len() - error.valid_up_to() <= 3,
    }
}

/// The statistically most likely legacy multi-byte (CJK) encoding for `bytes`.
///
/// Returns `None` when the detector prefers a single byte encoding, because
/// those are handled by the high-byte ratio heuristic instead.
pub(crate) fn cjk_guess(bytes: &[u8]) -> Option<&'static Encoding> {
    if bytes.is_empty() {
        return None;
    }
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let enc = detector.guess(None, Utf8Detection::Deny);
    let is_cjk = enc == SHIFT_JIS || enc == EUC_JP || enc == GBK || enc == BIG5 || enc == EUC_KR;
    is_cjk.then_some(enc)
}

/// Detect a leading byte-order mark.
fn sniff_bom(bytes: &[u8]) -> Option<Scent> {
    // UTF-32 BOMs must be tested before UTF-16: FF FE 00 00 also starts with the
    // UTF-16 LE mark.
    if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        return Some(Scent {
            name: "utf-32le",
            kind: Kind::Utf32Le,
            had_bom: true,
            skip: 4,
        });
    }
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return Some(Scent {
            name: "utf-32be",
            kind: Kind::Utf32Be,
            had_bom: true,
            skip: 4,
        });
    }
    if bytes.starts_with(&BOM_UTF8) {
        return Some(Scent {
            name: "utf-8",
            kind: Kind::Rs(UTF_8),
            had_bom: true,
            skip: 3,
        });
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Some(Scent {
            name: "utf-16le",
            kind: Kind::Rs(UTF_16LE),
            had_bom: true,
            skip: 2,
        });
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Some(Scent {
            name: "utf-16be",
            kind: Kind::Rs(UTF_16BE),
            had_bom: true,
            skip: 2,
        });
    }
    None
}

/// Statistical / structural detection for streams without a BOM.
fn sniff_auto(bytes: &[u8]) -> Scent {
    if bytes.is_empty() {
        return Scent {
            name: "utf-8",
            kind: Kind::Rs(UTF_8),
            had_bom: false,
            skip: 0,
        };
    }
    // ASCII ranged UTF-16 is *technically* valid UTF-8 (the NUL halves are legal
    // characters), so the UTF-16 pattern has to be tested first.
    if let Some(scent) = sniff_utf16_without_bom(bytes) {
        return scent;
    }
    if looks_like_utf8(bytes) {
        return Scent {
            name: "utf-8",
            kind: Kind::Rs(UTF_8),
            had_bom: false,
            skip: 0,
        };
    }
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    // `feed` panics when called again after `last == true`; we call it exactly once.
    detector.feed(bytes, true);
    // The stream is not valid UTF-8, so letting the detector answer UTF-8 would
    // be wrong; the standard fallback for that case is windows-1252.
    let guessed = detector.guess(None, Utf8Detection::Deny);
    let enc = if guessed == UTF_8 {
        WINDOWS_1252
    } else {
        guessed
    };
    Scent {
        name: canonical_name(enc),
        kind: Kind::Rs(enc),
        had_bom: false,
        skip: 0,
    }
}

/// Guess BOM-less UTF-16 by looking for NUL bytes on alternating offsets.
///
/// The NUL pattern alone is not enough (structured binary data can produce it),
/// so the winning candidate must also decode to plausible characters.
fn sniff_utf16_without_bom(bytes: &[u8]) -> Option<Scent> {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units = bytes.len() / 2;
    let odd_nul = bytes.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
    let even_nul = bytes.iter().step_by(2).filter(|&&b| b == 0).count();
    // Latin text in UTF-16 is mostly ASCII, i.e. one NUL per code unit; random
    // binary data has only a few NULs out of 256, far below this threshold.
    let threshold = units / 3;
    if odd_nul > threshold && odd_nul > even_nul && decodes_as_plausible_text(bytes, UTF_16LE) {
        return Some(Scent {
            name: "utf-16le",
            kind: Kind::Rs(UTF_16LE),
            had_bom: false,
            skip: 0,
        });
    }
    if even_nul > threshold && even_nul > odd_nul && decodes_as_plausible_text(bytes, UTF_16BE) {
        return Some(Scent {
            name: "utf-16be",
            kind: Kind::Rs(UTF_16BE),
            had_bom: false,
            skip: 0,
        });
    }
    None
}

/// Whether `bytes` decode cleanly in `enc` (no malformed input, no control
/// characters beyond the usual whitespace).
fn decodes_as_plausible_text(bytes: &[u8], enc: &'static Encoding) -> bool {
    let (text, had_errors) = enc.decode_without_bom_handling(bytes);
    if had_errors {
        return false;
    }
    let mut chars = 0usize;
    for ch in text.chars() {
        chars += 1;
        let code = ch as u32;
        let allowed_control = matches!(code, 0x09..=0x0D);
        if (code < 0x20 && !allowed_control) || code == 0x7F || (0x80..=0x9F).contains(&code) {
            return false;
        }
    }
    chars > 0
}

/// Decode UTF-32, replacing out-of-range values and lone surrogates with `U+FFFD`.
fn decode_utf32(bytes: &[u8], little_endian: bool) -> String {
    let mut out = String::with_capacity(bytes.len() / 2);
    let mut chunks = bytes.chunks_exact(4);
    for chunk in &mut chunks {
        let raw = if little_endian {
            u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        } else {
            u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        };
        out.push(char::from_u32(raw).unwrap_or('\u{FFFD}'));
    }
    if !chunks.remainder().is_empty() {
        out.push('\u{FFFD}');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding_rs::{
        IBM866, ISO_2022_JP, ISO_8859_2, ISO_8859_3, ISO_8859_4, ISO_8859_5, ISO_8859_6,
        ISO_8859_7, ISO_8859_8, ISO_8859_10, ISO_8859_13, ISO_8859_14, ISO_8859_15, ISO_8859_16,
        KOI8_R, KOI8_U, MACINTOSH, WINDOWS_1250, WINDOWS_1251, WINDOWS_1253, WINDOWS_1254,
        WINDOWS_1255, WINDOWS_1256, WINDOWS_1257, WINDOWS_1258, X_MAC_CYRILLIC,
    };

    fn enc_of(bytes: &[u8]) -> String {
        let (_, enc, _) = decode_text(bytes, "");
        enc.to_string()
    }

    #[test]
    fn plain_ascii_is_utf8() {
        let (text, enc, bom) = decode_text(b"Hello, world!\n", "");
        assert_eq!(text, "Hello, world!\n");
        assert_eq!(enc, "utf-8");
        assert!(!bom);
    }

    #[test]
    fn empty_input_is_empty_utf8() {
        let (text, enc, bom) = decode_text(b"", "");
        assert!(text.is_empty());
        assert_eq!(enc, "utf-8");
        assert!(!bom);
    }

    #[test]
    fn valid_utf8_multibyte_is_kept() {
        let (text, enc, _) = decode_text("Příliš žluťoučký kůň".as_bytes(), "");
        assert_eq!(text, "Příliš žluťoučký kůň");
        assert_eq!(enc, "utf-8");
    }

    #[test]
    fn utf8_bom_is_stripped_and_reported() {
        let mut bytes = BOM_UTF8.to_vec();
        bytes.extend_from_slice(b"abc");
        let (text, enc, bom) = decode_text(&bytes, "");
        assert_eq!(text, "abc");
        assert_eq!(enc, "utf-8");
        assert!(bom);
    }

    #[test]
    fn utf16le_bom_is_decoded() {
        let bytes = [0xFF, 0xFE, b'H', 0x00, b'i', 0x00];
        let (text, enc, bom) = decode_text(&bytes, "");
        assert_eq!(text, "Hi");
        assert_eq!(enc, "utf-16le");
        assert!(bom);
    }

    #[test]
    fn utf16be_bom_is_decoded() {
        let bytes = [0xFE, 0xFF, 0x00, b'H', 0x00, b'i'];
        let (text, enc, bom) = decode_text(&bytes, "");
        assert_eq!(text, "Hi");
        assert_eq!(enc, "utf-16be");
        assert!(bom);
    }

    #[test]
    fn utf16_with_non_ascii_payload() {
        // "č" is U+010D -> 0D 01 in LE, 01 0D in BE.
        let (le, _, _) = decode_text(&[0xFF, 0xFE, 0x0D, 0x01], "");
        assert_eq!(le, "č");
        let (be, _, _) = decode_text(&[0xFE, 0xFF, 0x01, 0x0D], "");
        assert_eq!(be, "č");
    }

    #[test]
    fn utf32le_bom_is_decoded() {
        let bytes = [0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00];
        let (text, enc, bom) = decode_text(&bytes, "");
        assert_eq!(text, "A");
        assert_eq!(enc, "utf-32le");
        assert!(bom);
    }

    #[test]
    fn utf32be_bom_is_decoded() {
        let bytes = [0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x41];
        let (text, enc, bom) = decode_text(&bytes, "");
        assert_eq!(text, "A");
        assert_eq!(enc, "utf-32be");
        assert!(bom);
    }

    #[test]
    fn utf32_surrogates_and_out_of_range_become_replacements() {
        // 0x0000D800 is a lone surrogate, 0xFFFFFFFF is out of range.
        let bytes = [
            0xFF, 0xFE, 0x00, 0x00, 0x00, 0xD8, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
        ];
        let (text, _, _) = decode_text(&bytes, "");
        assert_eq!(text, "\u{FFFD}\u{FFFD}");
    }

    #[test]
    fn utf32_truncated_tail_does_not_panic() {
        let bytes = [0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00, 0x42]; // half a code unit
        let (text, _, _) = decode_text(&bytes, "");
        assert_eq!(text, "A\u{FFFD}");
    }

    #[test]
    fn explicit_windows_1250_maps_e8_to_czech_c() {
        // 0xE8 is 'č' in windows-1250 and 'è' in windows-1252.
        let (text, enc, bom) = decode_text(&[0xE8, b'a', b'u'], "windows-1250");
        assert_eq!(text, "čau");
        assert_eq!(enc, "windows-1250");
        assert!(!bom);
    }

    #[test]
    fn explicit_windows_1252_differs_from_1250_for_same_byte() {
        let (text, enc, _) = decode_text(&[0xE8], "windows-1252");
        assert_eq!(text, "è");
        assert_eq!(enc, "windows-1252");
    }

    #[test]
    fn explicit_iso_8859_2_is_honoured() {
        // 0xB1 is 'ą' in ISO-8859-2.
        let (text, enc, _) = decode_text(&[0xB1, b'g'], "iso-8859-2");
        assert_eq!(text, "ąg");
        assert_eq!(enc, "iso-8859-2");
    }

    #[test]
    fn explicit_koi8_r_is_honoured() {
        // KOI8-R "Привет".
        let bytes = [0xF0, 0xD2, 0xC9, 0xD7, 0xC5, 0xD4];
        let (text, enc, _) = decode_text(&bytes, "koi8-r");
        assert_eq!(text, "Привет");
        assert_eq!(enc, "koi8-r");
    }

    #[test]
    fn explicit_label_loses_to_a_bom() {
        let bytes = [0xFF, 0xFE, b'a', 0x00];
        let (text, enc, bom) = decode_text(&bytes, "windows-1250");
        assert_eq!(text, "a");
        assert_eq!(enc, "utf-16le");
        assert!(bom);
    }

    #[test]
    fn unknown_or_auto_labels_fall_back_to_sniffing() {
        for label in ["", "auto", "AUTO", "none", "binary", "detect", "klingon-42"] {
            let (text, enc, _) = decode_text(b"abc", label);
            assert_eq!(text, "abc", "label {label:?}");
            assert_eq!(enc, "utf-8", "label {label:?}");
        }
    }

    #[test]
    fn truncated_utf8_sequence_does_not_panic() {
        // "é" is C3 A9; the continuation byte is missing.
        let bytes = [b'a', 0xC3];
        let (text, _, _) = decode_text(&bytes, "");
        assert!(text.starts_with('a'), "text: {text:?}");
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
    }

    #[test]
    fn truncated_utf8_keeps_replacement_characters() {
        let mut bytes = b"abc".to_vec();
        bytes.extend_from_slice(&[0xE2, 0x82]); // two of three bytes of U+20AC
        let (text, _, _) = decode_text(&bytes, "utf-8");
        assert!(text.starts_with("abc"));
        assert!(text.contains('\u{FFFD}'), "text: {text:?}");
    }

    #[test]
    fn truncated_utf16_odd_byte_count_does_not_panic() {
        let bytes = [0xFF, 0xFE, b'a', 0x00, b'b'];
        let (text, enc, _) = decode_text(&bytes, "");
        assert_eq!(enc, "utf-16le");
        assert!(text.starts_with('a'));
        assert!(text.contains('\u{FFFD}'), "text: {text:?}");
    }

    #[test]
    fn bomless_utf16le_is_recognised() {
        let bytes: Vec<u8> = "Hello UTF-16"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert_eq!(enc_of(&bytes), "utf-16le");
        let (text, _, _) = decode_text(&bytes, "");
        assert_eq!(text, "Hello UTF-16");
    }

    #[test]
    fn bomless_utf16be_is_recognised() {
        let bytes: Vec<u8> = "Hello UTF-16"
            .encode_utf16()
            .flat_map(|u| u.to_be_bytes())
            .collect();
        assert_eq!(enc_of(&bytes), "utf-16be");
        let (text, _, _) = decode_text(&bytes, "");
        assert_eq!(text, "Hello UTF-16");
    }

    #[test]
    fn bomless_utf16le_with_diacritics_is_recognised() {
        let bytes: Vec<u8> = "Příliš žluťoučký kůň"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert_eq!(enc_of(&bytes), "utf-16le");
        let (text, _, _) = decode_text(&bytes, "");
        assert_eq!(text, "Příliš žluťoučký kůň");
    }

    #[test]
    fn legacy_czech_text_is_detected_as_an_8bit_encoding() {
        let sample = "Příliš žluťoučký kůň úpěl ďábelské ódy";
        let bytes = WINDOWS_1250.encode(sample).0;
        let (text, enc, _) = decode_text(&bytes, "");
        assert!(
            matches!(
                enc,
                "windows-1250" | "iso-8859-2" | "windows-1252" | "macintosh"
            ),
            "unexpected detection {enc}"
        );
        assert!(!text.is_empty());
        assert!(!text.chars().any(|c| c == '\u{FFFD}'));
    }

    #[test]
    fn random_binary_is_decoded_without_panicking() {
        let mut bytes = Vec::new();
        let mut state = 0x1234_5678u32;
        for _ in 0..512 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            bytes.push((state & 0xFF) as u8);
        }
        let (text, _, _) = decode_text(&bytes, "");
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
    }

    #[test]
    fn sniff_name_matches_decode_result() {
        assert_eq!(sniff_name(b"plain ascii"), "utf-8");
        assert_eq!(sniff_name(&[0xFF, 0xFE, b'a', 0x00]), "utf-16le");
        assert_eq!(sniff_name(&[]), "utf-8");
    }

    #[test]
    fn canonical_names_are_lower_case() {
        assert_eq!(canonical_name(UTF_8), "utf-8");
        assert_eq!(canonical_name(WINDOWS_1250), "windows-1250");
        assert_eq!(canonical_name(ISO_8859_2), "iso-8859-2");
        assert_eq!(canonical_name(KOI8_R), "koi8-r");
        assert_eq!(canonical_name(SHIFT_JIS), "shift_jis");
        for enc in [
            BIG5,
            EUC_JP,
            EUC_KR,
            GBK,
            IBM866,
            ISO_2022_JP,
            ISO_8859_3,
            ISO_8859_4,
            ISO_8859_5,
            ISO_8859_6,
            ISO_8859_7,
            ISO_8859_8,
            ISO_8859_10,
            ISO_8859_13,
            ISO_8859_14,
            ISO_8859_15,
            ISO_8859_16,
            KOI8_U,
            MACINTOSH,
            WINDOWS_1251,
            WINDOWS_1252,
            WINDOWS_1253,
            WINDOWS_1254,
            WINDOWS_1255,
            WINDOWS_1256,
            WINDOWS_1257,
            WINDOWS_1258,
            X_MAC_CYRILLIC,
        ] {
            let name = canonical_name(enc);
            assert!(!name.is_empty());
            assert!(
                name.chars().all(|c| !c.is_uppercase()),
                "name {name} is not lower case"
            );
        }
    }

    #[test]
    fn label_resolution_covers_utf32_spellings() {
        for label in ["utf-32", "utf32", "utf-32le", "UTF32LE"] {
            let scent = from_label(label).expect(label);
            assert_eq!(scent.name, "utf-32le");
            assert!(matches!(scent.kind, Kind::Utf32Le));
        }
        let scent = from_label("utf-32be").expect("utf-32be");
        assert_eq!(scent.name, "utf-32be");
        assert!(matches!(scent.kind, Kind::Utf32Be));
        assert!(from_label("").is_none());
        assert!(from_label("auto").is_none());
        assert!(from_label("no-such-encoding").is_none());
    }

    #[test]
    fn resolve_honours_encoding_choice() {
        let bytes = [0xE8, b'a'];
        let auto = resolve(&bytes, &EncodingChoice::Auto);
        let explicit = resolve(&bytes, &EncodingChoice::Explicit("windows-1250"));
        assert_eq!(explicit.name, "windows-1250");
        assert_ne!(auto.name, "windows-1250");
    }
}

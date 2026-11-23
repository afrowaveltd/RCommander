//! Remote-name encoding.
//!
//! FTP has no negotiation for the character set of paths and listings: the
//! control channel is nominally ASCII and every byte above `0x7F` is whatever
//! the server felt like (historically `latin-1`, on modern servers UTF-8, on
//! old Windows servers a code page). [`ConnectionSpec::encoding`] therefore
//! carries a label and this module maps it onto `encoding_rs`.
//!
//! Rules:
//!
//! * The label is resolved with the WHATWG label lookup (`encoding_rs`
//!   semantics), so `"latin-1"`, `"iso-8859-1"` and `"l1"` all resolve to
//!   `windows-1252`, `"cp1250"`/`"windows-1250"` to the Central European code
//!   page and `"utf8"`/`"utf-8"` to UTF-8.
//! * An empty label, `"auto"` or an unknown label falls back to UTF-8; this is
//!   never an error, a wrong guess only produces somewhat wrong names.
//! * Decoding is lossy and total: invalid sequences become `U+FFFD`, exactly
//!   like a browser would render them, so a hostile listing can never abort a
//!   directory read.
//!
//! [`ConnectionSpec::encoding`]: crate::ConnectionSpec::encoding

use encoding_rs::{Encoding, UTF_8};

/// A resolved character set for one connection.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Charset {
    /// The `encoding_rs` encoding used for decoding and encoding names.
    encoding: &'static Encoding,
}

impl Charset {
    /// Resolve a spec label (empty/`auto`/unknown ⇒ UTF-8).
    pub(crate) fn for_label(label: &str) -> Charset {
        let trimmed = label.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
            return Charset { encoding: UTF_8 };
        }
        if let Some(encoding) = Encoding::for_label(trimmed.as_bytes()) {
            return Charset { encoding };
        }
        // `latin-1`, `utf_8` and `cp-1252` are common spellings that WHATWG does
        // not list as labels; retry with the separators removed.
        let squeezed: String = trimmed.chars().filter(|c| *c != '-' && *c != '_').collect();
        Charset {
            encoding: Encoding::for_label(squeezed.as_bytes()).unwrap_or(UTF_8),
        }
    }

    /// Canonical name of the resolved encoding (`"utf-8"`, `"windows-1252"`, …).
    #[allow(dead_code)] // used by the tests and by diagnostics in a debugger
    pub(crate) fn name(self) -> &'static str {
        self.encoding.name()
    }

    /// Decode server bytes into a `String`, replacing invalid byte sequences.
    pub(crate) fn decode(self, bytes: &[u8]) -> String {
        if self.encoding == UTF_8 {
            if let Ok(text) = std::str::from_utf8(bytes) {
                return text.to_string();
            }
            // Some servers claim UTF-8 but still emit raw latin-1; a mixed
            // stream is far more useful decoded as latin-1 than as U+FFFD.
            return decode_single_byte(bytes);
        }
        let (text, _had_errors) = self.encoding.decode_without_bom_handling(bytes);
        text.into_owned()
    }

    /// Encode a Rust string into bytes the server expects for a path.
    ///
    /// Characters that the target encoding cannot represent become `?`: the
    /// alternatives are to fail the whole operation on one exotic name or to
    /// ship WHATWG's `&#NNNNN;` character references, which are meaningless
    /// inside a file name.
    pub(crate) fn encode(self, text: &str) -> Vec<u8> {
        if self.encoding == UTF_8 {
            return text.as_bytes().to_vec();
        }
        let (bytes, _used, had_errors) = self.encoding.encode(text);
        if !had_errors {
            return bytes.into_owned();
        }
        let mut out = Vec::with_capacity(text.len());
        for c in text.chars() {
            let mut buffer = [0u8; 4];
            let piece = c.encode_utf8(&mut buffer);
            let (encoded, _used, failed) = self.encoding.encode(piece);
            if failed {
                out.push(b'?');
            } else {
                out.extend_from_slice(&encoded);
            }
        }
        out
    }
}

/// Decode bytes as ISO-8859-1 (byte value == code point).
fn decode_single_byte(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_is_the_default_for_empty_and_unknown_labels() {
        assert_eq!(Charset::for_label("").name(), "UTF-8");
        assert_eq!(Charset::for_label("  ").name(), "UTF-8");
        assert_eq!(Charset::for_label("auto").name(), "UTF-8");
        assert_eq!(Charset::for_label("no-such-charset").name(), "UTF-8");
        assert_eq!(Charset::for_label("utf-8").name(), "UTF-8");
        assert_eq!(Charset::for_label("UTF8").name(), "UTF-8");
    }

    #[test]
    fn latin1_labels_resolve_to_the_western_code_page() {
        for label in [
            "latin-1",
            "iso-8859-1",
            "ISO8859-1",
            "l1",
            "cp819",
            "windows-1252",
        ] {
            assert_eq!(
                Charset::for_label(label).name(),
                "windows-1252",
                "label {label}"
            );
        }
        assert_eq!(Charset::for_label("windows-1250").name(), "windows-1250");
        assert_eq!(Charset::for_label("cp1251").name(), "windows-1251");
    }

    #[test]
    fn utf8_decoding_is_exact_and_falls_back_to_latin1() {
        let cs = Charset::for_label("utf-8");
        assert_eq!(cs.decode("příliš.txt".as_bytes()), "příliš.txt");
        // U+00E9 in latin-1 is a lone 0xE9 byte; a UTF-8 decoder must not fail.
        assert_eq!(cs.decode(&[b'c', b'a', b'f', 0xE9]), "caf\u{e9}");
        assert_eq!(cs.encode("příliš.txt"), "příliš.txt".as_bytes().to_vec());
    }

    #[test]
    fn latin1_round_trips_single_byte_names() {
        let cs = Charset::for_label("latin-1");
        let bytes = cs.encode("caf\u{e9} \u{20ac}");
        // windows-1252 represents € at 0x80 and é at 0xE9.
        assert_eq!(bytes, vec![b'c', b'a', b'f', 0xE9, b' ', 0x80]);
        assert_eq!(cs.decode(&bytes), "caf\u{e9} \u{20ac}");
    }

    #[test]
    fn unrepresentable_characters_degrade_instead_of_failing() {
        let cs = Charset::for_label("latin-1");
        let bytes = cs.encode("文件.txt");
        assert_eq!(bytes, b"??.txt".to_vec());
        // ASCII names are unaffected.
        assert_eq!(cs.encode("plain.txt"), b"plain.txt".to_vec());
    }

    #[test]
    fn decoding_is_lossy_for_invalid_bytes() {
        // 0xFF is not a valid UTF-8 lead byte and also not assigned in
        // windows-1252, so the decoder must produce a replacement character
        // rather than an error.
        let text = Charset::for_label("windows-1250").decode(&[0x41, 0xFF, 0x42]);
        assert!(
            text.starts_with('A') && text.ends_with('B'),
            "text: {text:?}"
        );
    }
}

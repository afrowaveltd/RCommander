//! Text encodings used to transport binary files: MIME base64, UUencode and
//! XXencode.
//!
//! The three formats share the `begin`/`end` envelope, so the decoder reads the
//! envelope and decides from the payload which alphabet was used. That makes
//! `decode` work on files produced by other tools as well.

use crate::error::{CoreError, CoreResult};

/// Supported text encodings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    /// MIME base64 (the `begin`/`end` envelope is optional on decode).
    Mime,
    /// Classic UUencode.
    Uu,
    /// XXencode, the variant with a different alphabet.
    Xx,
}

impl TextEncoding {
    /// Upper-case identifier (`MIME`, `UUE`, `XXE`).
    pub fn id(self) -> &'static str {
        match self {
            TextEncoding::Mime => "MIME",
            TextEncoding::Uu => "UUE",
            TextEncoding::Xx => "XXE",
        }
    }

    /// Parse an identifier, case-insensitively (`mime`, `base64`, `uu`, `xx`).
    pub fn parse(text: &str) -> Option<TextEncoding> {
        match text.trim().to_ascii_lowercase().as_str() {
            "mime" | "base64" | "b64" => Some(TextEncoding::Mime),
            "uu" | "uue" | "uuencode" => Some(TextEncoding::Uu),
            "xx" | "xxe" | "xxencode" => Some(TextEncoding::Xx),
            _ => None,
        }
    }

    /// Suggested output file extension.
    pub fn extension(self) -> &'static str {
        match self {
            TextEncoding::Mime => "b64",
            TextEncoding::Uu => "uue",
            TextEncoding::Xx => "xxe",
        }
    }
}

const UU_ALPHABET: &[u8] = b"`!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_";
const XX_ALPHABET: &[u8] = b"+-0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode `data` with `kind`, using `name` in the envelope.
///
/// Lines are wrapped at 60 characters for base64 and 61 encoded characters (45
/// bytes) for UU/XX, which is what the classic tools produce.
pub fn encode(data: &[u8], kind: TextEncoding, name: &str) -> String {
    let mut out = String::new();
    out.push_str("begin 644 ");
    out.push_str(name);
    out.push('\n');
    match kind {
        TextEncoding::Mime => {
            let encoded = base64_encode(data);
            for chunk in encoded.as_bytes().chunks(60) {
                out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
                out.push('\n');
            }
        }
        TextEncoding::Uu | TextEncoding::Xx => {
            let alphabet = if kind == TextEncoding::Uu {
                UU_ALPHABET
            } else {
                XX_ALPHABET
            };
            for chunk in data.chunks(45) {
                encode_uue_line(chunk, alphabet, &mut out);
                out.push('\n');
            }
        }
    }
    out.push_str("`\nend\n");
    out
}

fn encode_uue_line(chunk: &[u8], alphabet: &[u8], out: &mut String) {
    let length_char = alphabet[chunk.len() & 0x3f];
    out.push(length_char as char);
    for triple in chunk.chunks(3) {
        let b0 = triple[0];
        let b1 = triple.get(1).copied().unwrap_or(0);
        let b2 = triple.get(2).copied().unwrap_or(0);
        let indexes = [
            (b0 >> 2) & 0x3f,
            ((b0 << 4) | (b1 >> 4)) & 0x3f,
            ((b1 << 2) | (b2 >> 6)) & 0x3f,
            b2 & 0x3f,
        ];
        for index in indexes {
            out.push(alphabet[index as usize] as char);
        }
    }
}

/// Decode a text file produced by [`encode`] (or by an equivalent tool).
///
/// The envelope is optional: a bare base64 document is accepted too. The payload
/// alphabet decides between base64, UU and XX.
pub fn decode(text: &str) -> CoreResult<Vec<u8>> {
    let mut payload_lines: Vec<&str> = Vec::new();
    let mut saw_envelope = false;
    for line in text.lines() {
        let trimmed = line.trim_end();
        let lower = trimmed.trim_start().to_ascii_lowercase();
        if lower.starts_with("begin ") {
            saw_envelope = true;
            continue;
        }
        if lower == "end" || lower == "`" || lower == " " || lower.is_empty() {
            continue;
        }
        payload_lines.push(trimmed);
    }
    if payload_lines.is_empty() {
        // An empty file encodes to just the envelope, which is valid.
        return if saw_envelope {
            Ok(Vec::new())
        } else {
            Err(CoreError::Invalid("no encoded payload found".to_string()))
        };
    }
    // A UU/XX line starts with a length character, so a payload whose declared
    // lengths are all consistent wins over a base64 interpretation (UUencoded
    // text is often made only of characters that are legal base64 as well).
    let uu_hits = unique_hits(&payload_lines, UU_ALPHABET, XX_ALPHABET);
    let xx_hits = unique_hits(&payload_lines, XX_ALPHABET, UU_ALPHABET);
    let order: [&[u8]; 2] = if xx_hits > uu_hits {
        [XX_ALPHABET, UU_ALPHABET]
    } else {
        [UU_ALPHABET, XX_ALPHABET]
    };
    for alphabet in order {
        if let Some(bytes) = decode_uue_strict(&payload_lines, alphabet) {
            let _ = saw_envelope;
            return Ok(bytes);
        }
    }
    // Otherwise the payload must be base64.
    if payload_lines.iter().all(|line| {
        line.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
    }) {
        return base64_decode(&payload_lines.join(""));
    }
    Err(CoreError::Invalid(
        "unrecognised encoded payload".to_string(),
    ))
}

/// Count payload characters that are in `alphabet` but not in `other`, which is
/// what distinguishes UUencode from XXencode.
fn unique_hits(lines: &[&str], alphabet: &[u8], other: &[u8]) -> usize {
    lines
        .iter()
        .flat_map(|line| line.chars())
        .filter(|ch| alphabet.contains(&(*ch as u8)) && !other.contains(&(*ch as u8)))
        .count()
}

/// Decode UU/XX only when every line is self-consistent (the declared length
/// equals the number of bytes the line decodes to).
fn decode_uue_strict(lines: &[&str], alphabet: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut lookup = [255u8; 128];
    for (index, byte) in alphabet.iter().enumerate() {
        lookup[*byte as usize] = index as u8;
    }
    let mut parsed_any = false;
    for line in lines {
        let bytes = line.as_bytes();
        if bytes.is_empty() {
            continue;
        }
        let length = lookup.get(bytes[0] as usize).copied().unwrap_or(255);
        if length == 255 {
            return None;
        }
        let expected = length as usize;
        let mut decoded = Vec::with_capacity(expected);
        for group in bytes[1..].chunks(4) {
            if group.len() < 4 {
                return None;
            }
            let mut values = [0u8; 4];
            for (index, byte) in group.iter().enumerate() {
                let value = lookup.get(*byte as usize).copied().unwrap_or(255);
                if value == 255 {
                    return None;
                }
                values[index] = value;
            }
            decoded.push((values[0] << 2) | (values[1] >> 4));
            decoded.push(((values[1] & 0x0f) << 4) | (values[2] >> 2));
            decoded.push(((values[2] & 0x03) << 6) | values[3]);
        }
        if decoded.len() < expected {
            return None;
        }
        decoded.truncate(expected);
        out.extend_from_slice(&decoded);
        parsed_any = true;
    }
    if parsed_any { Some(out) } else { None }
}

/// Standard base64 encoding without padding-free variants.
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        let n = ((b0 as u32) << 16) | ((b1.unwrap_or(0) as u32) << 8) | b2.unwrap_or(0) as u32;
        out.push(BASE64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if b1.is_some() {
            BASE64_ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if b2.is_some() {
            BASE64_ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Standard base64 decoding; whitespace and `=` padding are ignored.
pub fn base64_decode(text: &str) -> CoreResult<Vec<u8>> {
    let mut values: Vec<u8> = Vec::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_whitespace() || ch == '=' {
            continue;
        }
        match BASE64_ALPHABET.iter().position(|byte| *byte as char == ch) {
            Some(index) => values.push(index as u8),
            None => {
                return Err(CoreError::Invalid(format!(
                    "invalid base64 character {ch:?}"
                )));
            }
        }
    }
    let mut out = Vec::with_capacity(values.len() * 3 / 4);
    for group in values.chunks(4) {
        match group.len() {
            4 => {
                out.push((group[0] << 2) | (group[1] >> 4));
                out.push(((group[1] & 0x0f) << 4) | (group[2] >> 2));
                out.push(((group[2] & 0x03) << 6) | group[3]);
            }
            3 => {
                out.push((group[0] << 2) | (group[1] >> 4));
                out.push(((group[1] & 0x0f) << 4) | (group[2] >> 2));
            }
            2 => out.push((group[0] << 2) | (group[1] >> 4)),
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_reference_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(base64_decode("Zm9v YmFy\n").unwrap(), b"foobar");
        assert!(base64_decode("not base64!").is_err());
    }

    #[test]
    fn mime_round_trip_restores_binary_data() {
        let data: Vec<u8> = (0..=255u8).collect();
        let text = encode(&data, TextEncoding::Mime, "data.bin");
        assert!(text.starts_with("begin 644 data.bin\n"));
        assert!(text.ends_with("`\nend\n"));
        assert_eq!(decode(&text).unwrap(), data);
    }

    #[test]
    fn uue_and_xxe_round_trip() {
        for kind in [TextEncoding::Uu, TextEncoding::Xx] {
            let data: Vec<u8> = (0..=255u8).chain([0u8, 1, 2, 3]).collect();
            let text = encode(&data, kind, "file.bin");
            assert!(text.starts_with("begin 644 file.bin\n"), "{kind:?}: {text}");
            assert_eq!(decode(&text).unwrap(), data, "{kind:?}");
        }
    }

    #[test]
    fn short_payloads_round_trip_for_every_format() {
        for kind in [TextEncoding::Mime, TextEncoding::Uu, TextEncoding::Xx] {
            for length in [0usize, 1, 2, 3, 4, 44, 45, 46, 90] {
                let data: Vec<u8> = (0..length).map(|i| (i * 7 % 251) as u8).collect();
                let text = encode(&data, kind, "x");
                assert_eq!(decode(&text).unwrap(), data, "{kind:?} length {length}");
            }
        }
    }

    #[test]
    fn uuencoded_lines_follow_the_classic_layout() {
        // Three identical bytes "abc" -> one line with the length character and
        // four alphabet characters.
        let text = encode(b"abc", TextEncoding::Uu, "x");
        let line = text.lines().nth(1).expect("payload line");
        assert!(line.len() > 1);
        assert_eq!(
            line.chars().next(),
            Some(UU_ALPHABET[3] as char),
            "length prefix is 3 bytes"
        );
        assert_eq!(line.chars().count(), 5, "one partial group of three bytes");
    }

    #[test]
    fn a_bare_base64_document_decodes() {
        let text = "Zm9vYmFy\n";
        assert_eq!(decode(text).unwrap(), b"foobar");
    }

    #[test]
    fn identifiers_parse_and_have_extensions() {
        assert_eq!(TextEncoding::parse("MIME"), Some(TextEncoding::Mime));
        assert_eq!(TextEncoding::parse("base64"), Some(TextEncoding::Mime));
        assert_eq!(TextEncoding::parse("uue"), Some(TextEncoding::Uu));
        assert_eq!(TextEncoding::parse("xxe"), Some(TextEncoding::Xx));
        assert_eq!(TextEncoding::parse("rot13"), None);
        assert_eq!(TextEncoding::Mime.extension(), "b64");
        assert_eq!(TextEncoding::Uu.extension(), "uue");
        assert_eq!(TextEncoding::Xx.id(), "XXE");
    }

    #[test]
    fn malformed_input_is_rejected_without_panicking() {
        assert!(decode("").is_err(), "an empty document has no payload");
        // An envelope with an empty payload is the encoding of an empty file.
        assert_eq!(decode("begin 644 x\nend\n").unwrap(), Vec::<u8>::new());
        assert!(decode("!!!!").is_err());
        assert!(
            decode("hello world\n").is_err(),
            "plain text is not an encoded payload"
        );
        // A line whose length prefix is inconsistent is rejected.
        assert!(decode("begin 644 x\nM!!\nend\n").is_err());
    }
}

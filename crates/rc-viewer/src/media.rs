//! Descriptive models for content the viewer does not render itself: media
//! containers and recognised binary formats.
//!
//! These carry enough metadata for the GUI to show an icon, a status line and —
//! where it is cheap and safe — a few parsed facts such as the sample rate of a
//! WAV file or the brand of an MP4 container.

use crate::hex::{HexDocument, hex_document};

/// How many leading bytes of a binary file are pre-formatted as a preview.
pub(crate) const BINARY_PREVIEW_BYTES: usize = 256;

/// An audio or video container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDocument {
    /// Coarse class, always `"audio"` or `"video"`.
    pub kind: String,
    /// One-line description: container, parsed facts and size, e.g.
    /// `"WAVE audio · 44100 Hz · 2 channels · 16 bit · 1.5 MiB"`.
    pub details: String,
}

/// A recognised binary format the viewer does not render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryDocument {
    /// Human readable kind, e.g. `"PDF document"`, `"ELF executable"`,
    /// `"ZIP archive"`, `"SQLite database"`.
    pub kind: String,
    /// File size in bytes.
    pub size: u64,
    /// Hexdump of the leading 256 bytes of the file.
    pub head_hex: HexDocument,
}

/// Build a [`MediaDocument`] from the probe of a media file.
pub(crate) fn media_document(hint: &str, head: &[u8], size: u64) -> MediaDocument {
    let class = match hint {
        "avi" | "mp4" | "mov" | "matroska" | "flv" | "asf" | "mpeg" | "3gp" => "video",
        _ => "audio",
    };
    let size_text = rc_core::format::format_size_unit(size);
    let detail = match hint {
        "wav" => match wav_details(head) {
            Some(facts) => format!("WAVE audio · {facts} · {size_text}"),
            None => format!("WAVE audio · {size_text}"),
        },
        "mp3" => {
            let tag = match head.get(3) {
                Some(&major) if head.starts_with(b"ID3") => format!("ID3v2.{major} tag"),
                _ => "MPEG audio stream".to_string(),
            };
            format!("MPEG audio · {tag} · {size_text}")
        }
        "mp4" | "m4a" | "mov" | "3gp" => match head.get(8..12) {
            Some(brand) if head.get(4..8) == Some(b"ftyp") => {
                let brand = String::from_utf8_lossy(brand).trim().to_string();
                format!("ISO base media · brand {brand} · {size_text}")
            }
            _ => format!("ISO base media · {size_text}"),
        },
        "ogg" => format!("Ogg container · {size_text}"),
        "flac" => format!("FLAC lossless audio · {size_text}"),
        "midi" => format!("MIDI sequence · {size_text}"),
        "aiff" => format!("AIFF audio · {size_text}"),
        "avi" => format!("AVI video · {size_text}"),
        "matroska" => format!("Matroska/WebM video · {size_text}"),
        "flv" => format!("Flash video · {size_text}"),
        "asf" => format!("ASF/WMV video · {size_text}"),
        "mpeg" => format!("MPEG program stream · {size_text}"),
        "amr" => format!("AMR audio · {size_text}"),
        "wavpack" => format!("WavPack audio · {size_text}"),
        other => format!("{other} media · {size_text}"),
    };
    MediaDocument {
        kind: class.to_string(),
        details: detail,
    }
}

/// Parse the `fmt ` chunk of a RIFF/WAVE header into a short fact list.
fn wav_details(head: &[u8]) -> Option<String> {
    // RIFF(4) size(4) WAVE(4) then chunks: id(4) size(4) payload
    let mut cursor = 12usize;
    while cursor + 8 <= head.len() {
        let id = head.get(cursor..cursor + 4)?;
        let size = u32::from_le_bytes(head.get(cursor + 4..cursor + 8)?.try_into().ok()?) as usize;
        let payload_start = cursor + 8;
        if id == b"fmt " {
            let payload = head.get(payload_start..payload_start + size.min(16))?;
            if payload.len() < 16 {
                return None;
            }
            let channels = u16::from_le_bytes([payload[2], payload[3]]);
            let sample_rate = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
            let bits = u16::from_le_bytes([payload[14], payload[15]]);
            let channel_text = match channels {
                1 => "1 channel (mono)".to_string(),
                2 => "2 channels (stereo)".to_string(),
                n => format!("{n} channels"),
            };
            return Some(format!("{sample_rate} Hz · {channel_text} · {bits} bit"));
        }
        // Chunks are word aligned.
        cursor = payload_start.checked_add(size)? + (size & 1);
    }
    None
}

/// Build a [`BinaryDocument`] for a recognised binary container.
pub(crate) fn binary_document(
    hint: &str,
    head: &[u8],
    size: u64,
    bytes_per_line: usize,
) -> BinaryDocument {
    let preview = head
        .get(..BINARY_PREVIEW_BYTES.min(head.len()))
        .unwrap_or(head);
    BinaryDocument {
        kind: binary_kind(hint).unwrap_or(hint).to_string(),
        size,
        head_hex: hex_document(preview, 0, bytes_per_line),
    }
}

/// Map a `format_hint` to a human readable kind name, if it is a known one.
fn binary_kind(hint: &str) -> Option<&'static str> {
    Some(match hint {
        "pdf" => "PDF document",
        "elf" => "ELF executable",
        "pe" => "PE (Windows) executable",
        "macho" => "Mach-O executable",
        "java-class" => "Java class file",
        "wasm" => "WebAssembly module",
        "zip" => "ZIP archive",
        "gzip" => "gzip stream",
        "bzip2" => "bzip2 stream",
        "xz" => "XZ stream",
        "zstd" => "Zstandard stream",
        "lz4" => "LZ4 frame",
        "7z" => "7-Zip archive",
        "rar" => "RAR archive",
        "cab" => "Windows cabinet archive",
        "ar" => "Unix ar archive",
        "tar" => "tar archive",
        "sqlite" => "SQLite database",
        "ole2" => "OLE2 compound document",
        "font" => "font file",
        "windows-registry" => "Windows registry hive",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 44-byte RIFF/WAVE header with a `fmt ` chunk (stereo, 44100 Hz, 16 bit).
    fn wav_head() -> Vec<u8> {
        let mut head = Vec::new();
        head.extend_from_slice(b"RIFF");
        head.extend_from_slice(&36u32.to_le_bytes());
        head.extend_from_slice(b"WAVE");
        head.extend_from_slice(b"fmt ");
        head.extend_from_slice(&16u32.to_le_bytes());
        head.extend_from_slice(&1u16.to_le_bytes()); // PCM
        head.extend_from_slice(&2u16.to_le_bytes()); // channels
        head.extend_from_slice(&44_100u32.to_le_bytes()); // sample rate
        head.extend_from_slice(&176_400u32.to_le_bytes()); // byte rate
        head.extend_from_slice(&4u16.to_le_bytes()); // block align
        head.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        head.extend_from_slice(b"data");
        head.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(head.len(), 44);
        head
    }

    #[test]
    fn wav_details_are_parsed() {
        let doc = media_document("wav", &wav_head(), 1_500_000);
        assert_eq!(doc.kind, "audio");
        assert!(
            doc.details.contains("WAVE audio"),
            "details: {}",
            doc.details
        );
        assert!(doc.details.contains("44100 Hz"), "details: {}", doc.details);
        assert!(doc.details.contains("stereo"), "details: {}", doc.details);
        assert!(doc.details.contains("16 bit"), "details: {}", doc.details);
        assert!(doc.details.contains("MiB"), "details: {}", doc.details);
    }

    #[test]
    fn truncated_wav_header_still_describes_the_file() {
        let head = &wav_head()[..20];
        let doc = media_document("wav", head, 1000);
        assert_eq!(doc.kind, "audio");
        assert!(
            doc.details.starts_with("WAVE audio"),
            "details: {}",
            doc.details
        );
    }

    #[test]
    fn mono_wav_says_mono() {
        let mut head = wav_head();
        head[22..24].copy_from_slice(&1u16.to_le_bytes());
        let doc = media_document("wav", &head, 10);
        assert!(doc.details.contains("mono"), "details: {}", doc.details);
    }

    #[test]
    fn mp3_reports_its_id3_tag() {
        let head = b"ID3\x04\x00\x00\x00\x00\x00\x00";
        let doc = media_document("mp3", head, 3 * 1024 * 1024);
        assert_eq!(doc.kind, "audio");
        assert!(doc.details.contains("ID3v2.4"), "details: {}", doc.details);
        assert!(doc.details.contains("3.00 MiB"), "details: {}", doc.details);

        let raw = [0xFFu8, 0xFB, 0x90, 0x64];
        let doc = media_document("mp3", &raw, 10);
        assert!(
            doc.details.contains("MPEG audio stream"),
            "details: {}",
            doc.details
        );
    }

    #[test]
    fn video_containers_are_classed_as_video() {
        for hint in ["mp4", "mov", "avi", "matroska", "flv", "asf", "mpeg", "3gp"] {
            let doc = media_document(hint, b"", 1024);
            assert_eq!(doc.kind, "video", "hint {hint}");
            assert!(!doc.details.is_empty());
        }
        for hint in [
            "wav", "mp3", "ogg", "flac", "midi", "aiff", "amr", "wavpack", "m4a",
        ] {
            assert_eq!(media_document(hint, b"", 1024).kind, "audio", "hint {hint}");
        }
    }

    #[test]
    fn mp4_brand_is_reported() {
        let mut head = vec![0x00, 0x00, 0x00, 0x18];
        head.extend_from_slice(b"ftypisom");
        let doc = media_document("mp4", &head, 2048);
        assert!(
            doc.details.contains("brand isom"),
            "details: {}",
            doc.details
        );
    }

    #[test]
    fn unknown_media_hint_is_described_generically() {
        let doc = media_document("mystery", b"", 512);
        assert_eq!(doc.kind, "audio");
        assert!(doc.details.contains("mystery"), "details: {}", doc.details);
        assert!(doc.details.contains("512 B"), "details: {}", doc.details);
    }

    #[test]
    fn binary_kind_names_are_human_readable() {
        assert_eq!(binary_kind("pdf"), Some("PDF document"));
        assert_eq!(binary_kind("elf"), Some("ELF executable"));
        assert_eq!(binary_kind("zip"), Some("ZIP archive"));
        assert_eq!(binary_kind("sqlite"), Some("SQLite database"));
        assert_eq!(binary_kind("tar"), Some("tar archive"));
        assert_eq!(binary_kind("font"), Some("font file"));
        assert_eq!(binary_kind("something-else"), None);
        // Unknown hints are kept verbatim instead of being invented.
        assert_eq!(binary_document("weird", b"x", 1, 16).kind, "weird");
    }

    #[test]
    fn binary_document_previews_the_head() {
        let head: Vec<u8> = (0u8..=255).collect();
        let doc = binary_document("pdf", &head, 10_000, 16);
        assert_eq!(doc.kind, "PDF document");
        assert_eq!(doc.size, 10_000);
        // The preview is capped, the reported size is not.
        assert_eq!(doc.head_hex.bytes_shown(), BINARY_PREVIEW_BYTES as u64);
        assert_eq!(doc.head_hex.line_count(), BINARY_PREVIEW_BYTES / 16);
        assert_eq!(
            doc.head_hex.lines[0].hex,
            "00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f"
        );
    }

    #[test]
    fn binary_document_handles_short_and_empty_heads() {
        let doc = binary_document("elf", b"\x7FELF", 4, 16);
        assert_eq!(doc.head_hex.line_count(), 1);
        assert_eq!(doc.head_hex.lines[0].hex, "7f 45 4c 46");
        assert_eq!(doc.head_hex.lines[0].ascii, ".ELF");

        let doc = binary_document("elf", b"", 0, 16);
        assert_eq!(doc.head_hex.line_count(), 0);
        assert_eq!(doc.size, 0);
    }

    #[test]
    fn binary_document_honours_the_row_size() {
        let doc = binary_document("zip", &(0u8..32).collect::<Vec<u8>>(), 32, 8);
        assert_eq!(doc.head_hex.bytes_per_line, 8);
        assert_eq!(doc.head_hex.line_count(), 4);
    }
}

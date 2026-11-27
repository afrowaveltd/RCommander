//! The F3 dispatch: classify a file, then build the render-ready model.

use std::path::Path;

use rc_core::format::format_size_unit;

use crate::classify::{self, ContentKind, FileFormat};
use crate::hex::{HexDocument, hex_document, load_hex};
use crate::img::{ImageDocument, decode_image_bytes, load_image};
use crate::media::{BinaryDocument, MediaDocument, binary_document, media_document};
use crate::options::ViewerOptions;
use crate::text::{TextDocument, load_text};

/// How many leading bytes [`load`] probes for classification.
///
/// Magic bytes and text statistics both live in the first few kilobytes; 8 KiB
/// is enough to judge an encoding without paying for a second read.
const PROBE_BYTES: usize = 8192;

/// The render-ready payload of a loaded file.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewerContent {
    /// A zero-byte file: nothing to render, the GUI shows "empty".
    Empty,
    /// Decoded, indexed text.
    Text(TextDocument),
    /// Decoded RGBA pixels.
    Image(ImageDocument),
    /// A hexdump, used for opaque bytes and as the fallback preview.
    Hex(HexDocument),
    /// Audio/video container metadata.
    Media(MediaDocument),
    /// Recognised binary container metadata plus a hex preview.
    Binary(BinaryDocument),
    /// The content was recognised but cannot be rendered; the string explains
    /// why (`"decoding image failed: …"`).
    Unsupported(String),
}

impl ViewerContent {
    /// Whether this content can be painted as-is by the GUI.
    pub fn is_renderable(&self) -> bool {
        matches!(
            self,
            ViewerContent::Text(_)
                | ViewerContent::Image(_)
                | ViewerContent::Hex(_)
                | ViewerContent::Empty
        )
    }
}

/// A file that has been classified and turned into a render-ready model.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedViewer {
    /// What [`detect`](crate::detect) decided about the file.
    pub format: FileFormat,
    /// What the GUI should render.
    pub content: ViewerContent,
}

/// Load a file the way F3 should: images as images, text as text, unknown
/// binaries as a hexdump and everything else as descriptive metadata.
///
/// The file is probed internally (the first 8 KiB), so no caller-supplied hint
/// is needed. I/O errors are returned; a file that cannot be *rendered* is not
/// an error and becomes [`ViewerContent::Unsupported`].
pub fn load(path: &Path, opts: &ViewerOptions) -> std::io::Result<LoadedViewer> {
    let head = classify::probe(path, PROBE_BYTES)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let format = classify::detect(&name, &head);
    let content = content_from_path(path, &format, &head, opts)?;
    Ok(LoadedViewer { format, content })
}

/// Classify and render an in-memory buffer (archive member, clipboard, remote
/// preview).
///
/// Never fails: undecodable content becomes [`ViewerContent::Unsupported`].
pub fn open_bytes(name: &str, bytes: &[u8], opts: &ViewerOptions) -> LoadedViewer {
    let format = classify::detect(name, bytes);
    let content = content_from_bytes(&format, bytes, opts);
    LoadedViewer { format, content }
}

/// A one-line summary for the viewer status bar.
///
/// Always non-empty and never panics, whatever the content variant is.
///
/// ```
/// let loaded = rc_viewer::open_bytes("notes.txt", b"a\nb\n", &Default::default());
/// assert!(rc_viewer::describe(&loaded).contains("2 lines"));
/// ```
pub fn describe(loaded: &LoadedViewer) -> String {
    match &loaded.content {
        ViewerContent::Empty => "Empty file".to_string(),
        ViewerContent::Text(document) => {
            let mut out = format!(
                "{} · {} · {}",
                loaded.format.format_hint,
                format_size_unit(document.total_bytes),
                count_unit(document.line_count(), "line")
            );
            if document.is_rtl {
                out.push_str(" · RTL");
            }
            if document.had_bom {
                out.push_str(" · BOM");
            }
            if document.truncated {
                out.push_str(" · truncated");
            }
            out
        }
        ViewerContent::Image(document) => {
            let mut out = format!(
                "{} image · {}×{} px · {} RGBA",
                display_format(&document.format_hint),
                document.width,
                document.height,
                format_size_unit(document.rgba.len() as u64)
            );
            if document.downscaled {
                out.push_str(" · downscaled");
            }
            out
        }
        ViewerContent::Hex(document) => {
            let mut out = format!(
                "hex dump · {} B/row · {}",
                document.bytes_per_line,
                format_size_unit(document.total_bytes)
            );
            if document.truncated {
                out.push_str(&format!(
                    " (showing {})",
                    format_size_unit(document.bytes_shown())
                ));
            }
            out
        }
        ViewerContent::Media(document) => {
            // The details usually name the class already ("WAVE audio, …");
            // only prefix it when they do not.
            let details = document.details.trim();
            if details.is_empty() {
                document.kind.clone()
            } else if details.to_ascii_lowercase().contains(&document.kind) {
                details.to_string()
            } else {
                format!("{} · {}", document.kind, details)
            }
        }
        ViewerContent::Binary(document) => {
            format!("{} · {}", document.kind, format_size_unit(document.size))
        }
        ViewerContent::Unsupported(reason) => {
            if reason.trim().is_empty() {
                "Unsupported content".to_string()
            } else {
                format!("Unsupported: {reason}")
            }
        }
    }
}

/// Build the content model for a file on disk.
fn content_from_path(
    path: &Path,
    format: &FileFormat,
    head: &[u8],
    opts: &ViewerOptions,
) -> std::io::Result<ViewerContent> {
    let size = || match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta.len(),
        _ => 0,
    };
    match format.kind {
        ContentKind::Empty => Ok(ViewerContent::Empty),
        ContentKind::Text => Ok(ViewerContent::Text(load_text(path, opts)?)),
        ContentKind::Hex => Ok(ViewerContent::Hex(load_hex(path, opts)?)),
        ContentKind::Image => Ok(match load_image(path, opts.max_image_pixels) {
            Ok(image) => ViewerContent::Image(image),
            Err(reason) => ViewerContent::Unsupported(reason),
        }),
        ContentKind::Media => Ok(ViewerContent::Media(media_document(
            &format.format_hint,
            head,
            size(),
        ))),
        ContentKind::Binary => Ok(ViewerContent::Binary(binary_document(
            &format.format_hint,
            head,
            size(),
            opts.bytes_per_line(),
        ))),
        ContentKind::Unsupported => Ok(ViewerContent::Unsupported(format.format_hint.clone())),
    }
}

/// Build the content model for an in-memory buffer.
fn content_from_bytes(format: &FileFormat, bytes: &[u8], opts: &ViewerOptions) -> ViewerContent {
    match format.kind {
        ContentKind::Empty => ViewerContent::Empty,
        ContentKind::Text => ViewerContent::Text(crate::text::document(bytes, opts)),
        ContentKind::Hex => {
            let shown = opts.max_bytes_read.min(bytes.len());
            let prefix = bytes.get(..shown).unwrap_or(bytes);
            let mut document = hex_document(prefix, 0, opts.bytes_per_line());
            document.total_bytes = bytes.len() as u64;
            document.truncated = bytes.len() > shown;
            ViewerContent::Hex(document)
        }
        ContentKind::Image => {
            match decode_image_bytes(bytes, &format.format_hint, opts.max_image_pixels) {
                Ok(image) => ViewerContent::Image(image),
                Err(reason) => ViewerContent::Unsupported(reason),
            }
        }
        ContentKind::Media => ViewerContent::Media(media_document(
            &format.format_hint,
            bytes,
            bytes.len() as u64,
        )),
        ContentKind::Binary => ViewerContent::Binary(binary_document(
            &format.format_hint,
            bytes,
            bytes.len() as u64,
            opts.bytes_per_line(),
        )),
        ContentKind::Unsupported => ViewerContent::Unsupported(format.format_hint.clone()),
    }
}

/// Upper-case the short container names (`png` → `PNG`) for display.
fn display_format(hint: &str) -> String {
    if hint.len() <= 5 && hint.chars().all(|c| c.is_ascii_alphanumeric()) {
        hint.to_ascii_uppercase()
    } else {
        hint.to_string()
    }
}

/// `1 line` / `2 lines`.
fn count_unit(count: usize, unit: &str) -> String {
    if count == 1 {
        format!("{count} {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::EncodingChoice;
    use image::{ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut img = RgbaImage::new(width, height);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgba([(x % 256) as u8, (y % 256) as u8, 0x80, 0xFF]);
        }
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .expect("encode png");
        out.into_inner()
    }

    /// Deterministic pseudo-random bytes that are definitely not text.
    fn noise() -> Vec<u8> {
        let mut state = 0xDEAD_BEEFu32;
        let mut out = Vec::with_capacity(256);
        for index in 0..256 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            // Sprinkle in NUL bytes so the text heuristic is unambiguous.
            out.push(if index % 7 == 0 {
                0
            } else {
                (state & 0xFF) as u8
            });
        }
        out
    }

    #[test]
    fn netpbm_and_farbfeld_reach_the_visual_viewer() {
        let opts = ViewerOptions::default();
        let dir = tempfile::tempdir().expect("tempdir");
        let pnm = b"P6\n1 1\n255\n\xff\x00\x00";
        let mut ff = b"farbfeld".to_vec();
        ff.extend_from_slice(&1u32.to_be_bytes());
        ff.extend_from_slice(&1u32.to_be_bytes());
        for channel in [u16::MAX, 0, 0, u16::MAX] {
            ff.extend_from_slice(&channel.to_be_bytes());
        }
        for (name, bytes) in [("sample.ppm", &pnm[..]), ("sample.ff", ff.as_slice())] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).expect("fixture");
            for loaded in [load(&path, &opts).expect("load"), open_bytes(name, bytes, &opts)] {
                assert_eq!(loaded.format.kind, ContentKind::Image);
                let ViewerContent::Image(image) = loaded.content else { panic!("visual preview expected") };
                assert_eq!((image.width, image.height), (1, 1));
                assert_eq!(image.rgba, vec![255, 0, 0, 255]);
            }
        }
    }

    #[test]
    fn load_dispatches_each_kind_of_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let opts = ViewerOptions::default();

        let png_path = dir.path().join("picture.png");
        std::fs::write(&png_path, png_bytes(4, 3)).expect("write png");
        let txt_path = dir.path().join("notes.txt");
        std::fs::write(&txt_path, b"alpha\nbeta\ngamma\n").expect("write txt");
        let bin_path = dir.path().join("blob.bin");
        std::fs::write(&bin_path, noise()).expect("write bin");
        let empty_path = dir.path().join("empty.dat");
        std::fs::write(&empty_path, b"").expect("write empty");

        let png = load(&png_path, &opts).expect("load png");
        assert_eq!(png.format.kind, ContentKind::Image);
        match &png.content {
            ViewerContent::Image(image) => {
                assert_eq!((image.width, image.height), (4, 3));
                assert_eq!(image.rgba.len(), 48);
            }
            other => panic!("expected an image, got {other:?}"),
        }

        let txt = load(&txt_path, &opts).expect("load txt");
        assert_eq!(txt.format.kind, ContentKind::Text);
        match &txt.content {
            ViewerContent::Text(document) => {
                assert_eq!(document.line_count(), 3);
                assert_eq!(document.line(1), Some("beta"));
                assert_eq!(document.encoding, "utf-8");
            }
            other => panic!("expected text, got {other:?}"),
        }

        let bin = load(&bin_path, &opts).expect("load bin");
        assert_eq!(bin.format.kind, ContentKind::Hex);
        match &bin.content {
            ViewerContent::Hex(document) => {
                assert!(!document.is_empty());
                assert_eq!(document.total_bytes, 256);
                assert_eq!(document.bytes_per_line, 16);
            }
            other => panic!("expected a hexdump, got {other:?}"),
        }

        let empty = load(&empty_path, &opts).expect("load empty");
        assert_eq!(empty.format.kind, ContentKind::Empty);
        assert_eq!(empty.content, ViewerContent::Empty);
    }

    #[test]
    fn load_reports_descriptive_metadata_for_binaries_and_media() {
        let dir = tempfile::tempdir().expect("tempdir");
        let opts = ViewerOptions::default();

        let pdf = dir.path().join("doc.pdf");
        let pdf_bytes = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\ntrailer\n%%EOF\n";
        std::fs::write(&pdf, pdf_bytes).expect("write");
        let loaded = load(&pdf, &opts).expect("load pdf");
        assert_eq!(loaded.format.kind, ContentKind::Binary);
        match &loaded.content {
            ViewerContent::Binary(document) => {
                assert_eq!(document.kind, "PDF document");
                assert_eq!(document.size, pdf_bytes.len() as u64);
                assert!(!document.head_hex.is_empty());
            }
            other => panic!("expected binary metadata, got {other:?}"),
        }

        let ogg = dir.path().join("sound.ogg");
        std::fs::write(&ogg, b"OggS\x00\x02\x00\x00\x00\x00\x00\x00").expect("write");
        let loaded = load(&ogg, &opts).expect("load ogg");
        assert_eq!(loaded.format.kind, ContentKind::Media);
        match &loaded.content {
            ViewerContent::Media(document) => {
                assert_eq!(document.kind, "audio");
                assert!(
                    document.details.contains("Ogg"),
                    "details: {}",
                    document.details
                );
            }
            other => panic!("expected media metadata, got {other:?}"),
        }
    }

    #[test]
    fn load_turns_a_corrupt_image_into_unsupported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("broken.png");
        let mut bytes = png_bytes(8, 8);
        bytes.truncate(20); // keeps the png signature, loses the frame
        std::fs::write(&path, &bytes).expect("write");

        let loaded = load(&path, &ViewerOptions::default()).expect("load");
        assert_eq!(loaded.format.kind, ContentKind::Image);
        match &loaded.content {
            ViewerContent::Unsupported(reason) => assert!(!reason.is_empty()),
            other => panic!("expected unsupported, got {other:?}"),
        }
        assert!(!describe(&loaded).is_empty());
    }

    #[test]
    fn huge_file_is_previewed_without_reading_all_of_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("huge.log");
        let line = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde\n";
        let mut content = String::with_capacity(9 * 1024 * 1024 + 64);
        while content.len() < 9 * 1024 * 1024 {
            content.push_str(line);
        }
        std::fs::write(&path, &content).expect("write");

        let loaded = load(&path, &ViewerOptions::default()).expect("load");
        assert_eq!(loaded.format.kind, ContentKind::Text);
        match &loaded.content {
            ViewerContent::Text(document) => {
                assert!(document.truncated);
                assert_eq!(document.total_bytes, content.len() as u64);
                // Exactly the preview limit, not the whole 9 MiB file.
                assert_eq!(document.text.len(), 8 * 1024 * 1024);
                assert!(document.line_count() > 100_000);
            }
            other => panic!("expected text, got {other:?}"),
        }
        let summary = describe(&loaded);
        assert!(summary.contains("truncated"), "description: {summary}");
    }

    #[test]
    fn binary_preview_of_a_large_file_stays_bounded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("huge.bin");
        // Structured binary data: NULs everywhere, no text heuristic can win.
        let mut bytes = vec![0u8; 3 * 1024 * 1024];
        for (index, byte) in bytes.iter_mut().enumerate() {
            if index % 4 == 1 {
                *byte = 0xFF;
            } else if index % 4 == 2 {
                *byte = (index % 251) as u8;
            }
        }
        std::fs::write(&path, &bytes).expect("write");

        let opts = ViewerOptions {
            max_bytes_read: 4096,
            ..ViewerOptions::default()
        };
        let loaded = load(&path, &opts).expect("load");
        match &loaded.content {
            ViewerContent::Hex(document) => {
                assert!(document.truncated);
                assert_eq!(document.total_bytes, bytes.len() as u64);
                assert_eq!(document.bytes_shown(), 4096);
                assert_eq!(document.line_count(), 256);
            }
            other => panic!("expected a hexdump, got {other:?}"),
        }
    }

    #[test]
    fn load_propagates_io_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let opts = ViewerOptions::default();
        assert!(load(&dir.path().join("missing.png"), &opts).is_err());
        // A directory is not a viewable file either.
        assert!(load(dir.path(), &opts).is_err());
    }

    #[test]
    fn open_bytes_renders_an_in_memory_png() {
        let loaded = open_bytes("clipboard.png", &png_bytes(5, 2), &ViewerOptions::default());
        assert_eq!(loaded.format.kind, ContentKind::Image);
        match &loaded.content {
            ViewerContent::Image(image) => {
                assert_eq!((image.width, image.height), (5, 2));
                assert_eq!(image.rgba.len(), 5 * 2 * 4);
                assert_eq!(image.format_hint, "png");
            }
            other => panic!("expected an image, got {other:?}"),
        }
    }

    #[test]
    fn open_bytes_handles_text_hex_and_empty() {
        let opts = ViewerOptions::default();
        let text = open_bytes("a.txt", b"one\ntwo", &opts);
        assert!(matches!(text.content, ViewerContent::Text(_)));
        assert_eq!(describe(&text), "utf-8 text · 7 B · 2 lines");

        let hex = open_bytes("a.bin", &noise(), &opts);
        assert!(matches!(hex.content, ViewerContent::Hex(_)));

        let empty = open_bytes("a.dat", b"", &opts);
        assert_eq!(empty.content, ViewerContent::Empty);
        assert_eq!(describe(&empty), "Empty file");
    }

    #[test]
    fn open_bytes_of_a_corrupt_image_does_not_panic() {
        let loaded = open_bytes(
            "broken.png",
            b"\x89PNG\r\n\x1a\ngarbage",
            &ViewerOptions::default(),
        );
        assert!(matches!(loaded.content, ViewerContent::Unsupported(_)));
    }

    #[test]
    fn open_bytes_respects_preview_limits() {
        let opts = ViewerOptions {
            max_bytes_read: 16,
            ..ViewerOptions::default()
        };
        let bytes: Vec<u8> = (0u8..64).map(|b| b | 0x80).collect();
        let loaded = open_bytes("blob.bin", &bytes, &opts);
        match &loaded.content {
            ViewerContent::Hex(document) => {
                assert!(document.truncated);
                assert_eq!(document.total_bytes, 64);
                assert_eq!(document.bytes_shown(), 16);
            }
            other => panic!("expected a hexdump, got {other:?}"),
        }
    }

    #[test]
    fn open_bytes_marks_truncated_text() {
        let opts = ViewerOptions {
            max_bytes_read: 4,
            ..ViewerOptions::default()
        };
        let loaded = open_bytes("a.txt", b"abcdefgh", &opts);
        match &loaded.content {
            ViewerContent::Text(document) => {
                assert_eq!(document.text, "abcd");
                assert!(document.truncated);
                assert_eq!(document.total_bytes, 8);
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn open_bytes_honours_an_explicit_encoding() {
        let opts = ViewerOptions {
            encoding: EncodingChoice::Explicit("windows-1250"),
            ..ViewerOptions::default()
        };
        let loaded = open_bytes("czech.txt", &[0xE8, b'a', b'u'], &opts);
        match &loaded.content {
            ViewerContent::Text(document) => {
                assert_eq!(document.encoding, "windows-1250");
                assert_eq!(document.text, "čau");
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn describe_is_non_empty_for_every_variant() {
        let loaded_variants: Vec<ViewerContent> = vec![
            ViewerContent::Empty,
            ViewerContent::Text(TextDocument {
                text: "x\ny".to_string(),
                encoding: "utf-8",
                had_bom: false,
                is_rtl: false,
                truncated: false,
                total_bytes: 3,
                line_starts: vec![0, 2],
            }),
            ViewerContent::Image(ImageDocument {
                width: 2,
                height: 2,
                rgba: vec![0; 16],
                format_hint: "png".to_string(),
                downscaled: false,
            }),
            ViewerContent::Hex(hex_document(&[0, 1, 2], 0, 16)),
            ViewerContent::Media(MediaDocument {
                kind: "audio".to_string(),
                details: "WAVE audio · 1.0 KiB".to_string(),
            }),
            ViewerContent::Binary(BinaryDocument {
                kind: "PDF document".to_string(),
                size: 1234,
                head_hex: hex_document(b"%PDF", 0, 16),
            }),
            ViewerContent::Unsupported("decoding image failed".to_string()),
            ViewerContent::Unsupported(String::new()),
        ];
        for content in loaded_variants {
            let loaded = LoadedViewer {
                format: FileFormat {
                    kind: ContentKind::Text,
                    format_hint: "utf-8 text".to_string(),
                    mime: "text/plain".to_string(),
                },
                content: content.clone(),
            };
            let text = describe(&loaded);
            assert!(!text.trim().is_empty(), "empty description for {content:?}");
            assert!(!text.contains('\n'), "description must be one line: {text}");
        }
    }

    #[test]
    fn describe_mentions_the_interesting_facts() {
        let opts = ViewerOptions::default();
        let text = describe(&open_bytes("a.txt", b"a\nb\n", &opts));
        assert!(text.contains("utf-8"), "description: {text}");
        assert!(text.contains("2 lines"), "description: {text}");

        let html = describe(&open_bytes(
            "page.html",
            b"<html><body>x</body></html>",
            &opts,
        ));
        assert!(html.starts_with("html"), "description: {html}");

        let image = describe(&open_bytes("p.png", &png_bytes(3, 3), &opts));
        assert!(image.contains("PNG image"), "description: {image}");
        assert!(image.contains("3×3 px"), "description: {image}");

        let hex = describe(&open_bytes("b.bin", &noise(), &opts));
        assert!(hex.starts_with("hex dump"), "description: {hex}");
        assert!(hex.contains("B/row"), "description: {hex}");

        let unsupported = describe(&LoadedViewer {
            format: FileFormat {
                kind: ContentKind::Image,
                format_hint: "heic".to_string(),
                mime: "image/heic".to_string(),
            },
            content: ViewerContent::Unsupported("unsupported container".to_string()),
        });
        assert!(
            unsupported.starts_with("Unsupported: "),
            "description: {unsupported}"
        );
    }

    #[test]
    fn describe_flags_rtl_bom_and_truncation() {
        let mut document = TextDocument {
            text: "\u{05D0}".to_string(),
            encoding: "utf-8",
            had_bom: true,
            is_rtl: true,
            truncated: true,
            total_bytes: 400,
            line_starts: vec![0],
        };
        let loaded = LoadedViewer {
            format: FileFormat {
                kind: ContentKind::Text,
                format_hint: "utf-8 text".to_string(),
                mime: "text/plain".to_string(),
            },
            content: ViewerContent::Text(document.clone()),
        };
        let text = describe(&loaded);
        assert!(text.contains("RTL"), "description: {text}");
        assert!(text.contains("BOM"), "description: {text}");
        assert!(text.contains("truncated"), "description: {text}");

        document.is_rtl = false;
        document.had_bom = false;
        document.truncated = false;
        let loaded = LoadedViewer {
            format: loaded.format,
            content: ViewerContent::Text(document),
        };
        let text = describe(&loaded);
        assert!(!text.contains("RTL"));
        assert!(!text.contains("truncated"));
    }

    #[test]
    fn describe_reports_downscaled_images() {
        let loaded = LoadedViewer {
            format: FileFormat {
                kind: ContentKind::Image,
                format_hint: "png".to_string(),
                mime: "image/png".to_string(),
            },
            content: ViewerContent::Image(ImageDocument {
                width: 4,
                height: 4,
                rgba: vec![0; 64],
                format_hint: "png".to_string(),
                downscaled: true,
            }),
        };
        assert!(describe(&loaded).contains("downscaled"));
    }

    #[test]
    fn is_renderable_classifies_the_variants() {
        assert!(ViewerContent::Empty.is_renderable());
        assert!(ViewerContent::Hex(hex_document(b"", 0, 16)).is_renderable());
        assert!(!ViewerContent::Unsupported("x".to_string()).is_renderable());
        assert!(
            !ViewerContent::Media(MediaDocument {
                kind: "audio".to_string(),
                details: String::new(),
            })
            .is_renderable()
        );
    }

    #[test]
    fn count_unit_pluralises() {
        assert_eq!(count_unit(0, "line"), "0 lines");
        assert_eq!(count_unit(1, "line"), "1 line");
        assert_eq!(count_unit(2, "line"), "2 lines");
    }

    #[test]
    fn display_format_uppercases_short_names() {
        assert_eq!(display_format("png"), "PNG");
        assert_eq!(display_format("webp"), "WEBP");
        assert_eq!(display_format("screenshot"), "screenshot");
        assert_eq!(display_format(""), "");
    }

    #[test]
    fn extension_does_not_override_a_known_container_through_the_loader() {
        // A PNG saved as .txt must still render as an image.
        let loaded = open_bytes("fake.txt", &png_bytes(2, 2), &ViewerOptions::default());
        assert_eq!(loaded.format.kind, ContentKind::Image);
        assert!(matches!(loaded.content, ViewerContent::Image(_)));

        // And text saved as .png must render as text.
        let loaded = open_bytes("fake.png", b"just text\n", &ViewerOptions::default());
        assert_eq!(loaded.format.kind, ContentKind::Text);
        assert!(matches!(loaded.content, ViewerContent::Text(_)));
    }
}

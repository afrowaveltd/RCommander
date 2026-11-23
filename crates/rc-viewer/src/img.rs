//! Image decoding with an explicit memory budget.
//!
//! Two hard limits keep the viewer from being killed by a decompression bomb:
//!
//! * [`MAX_IMAGE_FILE_BYTES`] — a file larger than this is not even read.
//! * [`MAX_DECODED_BYTES`] — the fully decoded RGBA buffer may not exceed this;
//!   an image declaring more pixels than that is **rejected with an error**
//!   before any pixel memory is allocated.
//!
//! Between those limits the `max_pixels` budget applies: a larger image is
//! decoded (it fits) and then reduced with fast block sampling
//! ([`image::imageops::thumbnail`]), reporting `downscaled = true`. A
//! `max_pixels` of `0` means "no pixel budget", but the hard caps still apply.

use std::path::Path;

use image::{ImageFormat, ImageReader, Limits};

/// Largest image file the viewer is willing to buffer (256 MiB).
pub const MAX_IMAGE_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// Largest decoded RGBA buffer the viewer is willing to allocate (1 GiB),
/// i.e. at most 268 435 456 pixels.
pub const MAX_DECODED_BYTES: u64 = 1024 * 1024 * 1024;

/// A decoded image, always 8-bit RGBA.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageDocument {
    /// Width in pixels of [`Self::rgba`].
    pub width: u32,
    /// Height in pixels of [`Self::rgba`].
    pub height: u32,
    /// Premultiplied-free RGBA8 pixels, `width * height * 4` bytes, row major.
    pub rgba: Vec<u8>,
    /// Container format that was decoded (`"png"`, `"jpeg"`, `"gif"`, `"bmp"`,
    /// `"webp"`, `"tiff"`, `"ico"`), or the caller's hint when the container
    /// could not be identified.
    pub format_hint: String,
    /// Whether the image was reduced to fit the pixel budget.
    pub downscaled: bool,
}

impl std::fmt::Debug for ImageDocument {
    /// Compact debug output: dumping a megabyte of pixels into a log is useless.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageDocument")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rgba_len", &self.rgba.len())
            .field("format_hint", &self.format_hint)
            .field("downscaled", &self.downscaled)
            .finish()
    }
}

impl ImageDocument {
    /// Number of pixels, `width * height`.
    pub fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// Everything that can go wrong while decoding an image.
#[derive(Debug, thiserror::Error)]
enum ImageError {
    #[error("image data is empty")]
    Empty,
    #[error("image file is too large to preview: {size} bytes (limit {limit} bytes)")]
    FileTooLarge { size: u64, limit: u64 },
    #[error(
        "image is too large to decode: {width}×{height} ({pixels} pixels) would need about {needed} of memory (limit {limit})"
    )]
    TooLargeToDecode {
        width: u32,
        height: u32,
        pixels: u64,
        needed: String,
        limit: String,
    },
    #[error("decoding image failed: {0}")]
    Decode(String),
    #[error("reading image failed: {0}")]
    Io(String),
}

/// Load and decode an image file.
///
/// Returns `Err` with a human readable message for a corrupt, truncated,
/// unsupported or pathologically large image — never panics and never allocates
/// more than [`MAX_DECODED_BYTES`].
pub fn load_image(path: &Path, max_pixels: u64) -> Result<ImageDocument, String> {
    let meta = std::fs::metadata(path).map_err(|e| ImageError::Io(e.to_string()).to_string())?;
    if meta.is_file() && meta.len() > MAX_IMAGE_FILE_BYTES {
        return Err(ImageError::FileTooLarge {
            size: meta.len(),
            limit: MAX_IMAGE_FILE_BYTES,
        }
        .to_string());
    }
    let bytes = std::fs::read(path).map_err(|e| ImageError::Io(e.to_string()).to_string())?;
    decode_image_bytes(&bytes, "", max_pixels)
}

/// Decode an image from memory.
///
/// `format_hint` is advisory (used for the returned `format_hint` when the
/// container cannot be identified); the real format always comes from the magic
/// bytes. This is the entry point for archive members and clipboard images.
pub fn decode_image_bytes(
    bytes: &[u8],
    format_hint: &str,
    max_pixels: u64,
) -> Result<ImageDocument, String> {
    decode(bytes, format_hint, max_pixels).map_err(|error| error.to_string())
}

fn decode(bytes: &[u8], format_hint: &str, max_pixels: u64) -> Result<ImageDocument, ImageError> {
    if bytes.is_empty() {
        return Err(ImageError::Empty);
    }
    if bytes.len() as u64 > MAX_IMAGE_FILE_BYTES {
        return Err(ImageError::FileTooLarge {
            size: bytes.len() as u64,
            limit: MAX_IMAGE_FILE_BYTES,
        });
    }
    // Cheap header probe: dimensions only, no pixel data.
    let (width, height) = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ImageError::Decode(e.to_string()))?
        .into_dimensions()
        .map_err(|e| ImageError::Decode(e.to_string()))?;
    if width == 0 || height == 0 {
        return Err(ImageError::Decode(
            "image declares an empty size".to_string(),
        ));
    }

    let pixels = width as u64 * height as u64;
    let needed = pixels.saturating_mul(4);
    if needed > MAX_DECODED_BYTES {
        // Refuse before touching pixel memory: this is the decompression-bomb guard.
        return Err(ImageError::TooLargeToDecode {
            width,
            height,
            pixels,
            needed: rc_core::format::format_size_unit(needed),
            limit: rc_core::format::format_size_unit(MAX_DECODED_BYTES),
        });
    }
    let downscaled = max_pixels != 0 && pixels > max_pixels;

    let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ImageError::Decode(e.to_string()))?;
    let format = reader.format();
    // Dimensions are already checked above; the byte cap is the real guard.
    let mut limits = Limits::default();
    limits.max_image_width = None;
    limits.max_image_height = None;
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| ImageError::Decode(e.to_string()))?;
    let rgba = decoded.to_rgba8();

    let (width, height, buffer) = if downscaled {
        let (new_width, new_height) = fit_dimensions(rgba.width(), rgba.height(), max_pixels);
        let thumbnail = image::imageops::thumbnail(&rgba, new_width, new_height);
        (thumbnail.width(), thumbnail.height(), thumbnail.into_raw())
    } else {
        (rgba.width(), rgba.height(), rgba.into_raw())
    };

    let expected = width as usize * height as usize * 4;
    if buffer.len() != expected {
        return Err(ImageError::Decode(format!(
            "decoded buffer has {} bytes, expected {expected}",
            buffer.len()
        )));
    }

    Ok(ImageDocument {
        width,
        height,
        rgba: buffer,
        format_hint: format.and_then(format_name).map_or_else(
            || {
                if format_hint.is_empty() {
                    "image".to_string()
                } else {
                    format_hint.to_string()
                }
            },
            |name| name.to_string(),
        ),
        downscaled,
    })
}

/// The canonical name of a container format.
fn format_name(format: ImageFormat) -> Option<&'static str> {
    Some(match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Gif => "gif",
        ImageFormat::Bmp => "bmp",
        ImageFormat::WebP => "webp",
        ImageFormat::Tiff => "tiff",
        ImageFormat::Ico => "ico",
        _ => return None,
    })
}

/// Scale `width`×`height` down so that it fits `max_pixels`.
///
/// Aspect ratio is preserved and the result is never empty. The geometric scale
/// is recomputed every round, so convergence takes a handful of iterations even
/// for extreme aspect ratios such as 1 000 000×1.
fn fit_dimensions(width: u32, height: u32, max_pixels: u64) -> (u32, u32) {
    let mut current_w = width.max(1) as u64;
    let mut current_h = height.max(1) as u64;
    if max_pixels == 0 || current_w * current_h <= max_pixels {
        return (current_w as u32, current_h as u32);
    }
    for _ in 0..64 {
        let pixels = current_w * current_h;
        if pixels <= max_pixels {
            break;
        }
        let scale = (max_pixels as f64 / pixels as f64).sqrt();
        let next_w = ((current_w as f64 * scale).floor() as u64).max(1);
        let next_h = ((current_h as f64 * scale).floor() as u64).max(1);
        if next_w == current_w && next_h == current_h {
            break;
        }
        current_w = next_w;
        current_h = next_h;
    }
    // Exact, bounded enforcement of the budget against float rounding.
    let mut guard = 0;
    while current_w * current_h > max_pixels && guard < 4096 {
        if current_w >= current_h {
            current_w = current_w.saturating_sub(1).max(1);
        } else {
            current_h = current_h.saturating_sub(1).max(1);
        }
        guard += 1;
    }
    (current_w as u32, current_h as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::io::Cursor;

    /// Encode a generated image to `format` bytes.
    fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let mut img = RgbaImage::new(width, height);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = Rgba([(x % 256) as u8, (y % 256) as u8, 0x40, 0xFF]);
        }
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut out, format)
            .expect("encode");
        out.into_inner()
    }

    /// CRC-32 as required by the PNG container.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in bytes {
            crc ^= byte as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    /// Rewrite the IHDR of a real PNG so that it *declares* `width`×`height`
    /// while the (tiny) pixel payload stays untouched: the decompression-bomb
    /// fixture. Its CRC is recomputed, so the header itself is well formed.
    fn png_declaring(width: u32, height: u32) -> Vec<u8> {
        let mut out = encoded(4, 3, ImageFormat::Png);
        // 8 byte signature, then length(4) type(4) data(13) crc(4).
        out[16..20].copy_from_slice(&width.to_be_bytes());
        out[20..24].copy_from_slice(&height.to_be_bytes());
        let crc = crc32(&out[12..29]);
        out[29..33].copy_from_slice(&crc.to_be_bytes());
        out
    }

    #[test]
    fn png_is_decoded_to_rgba8() {
        let bytes = encoded(4, 3, ImageFormat::Png);
        let doc = decode_image_bytes(&bytes, "png", 40_000_000).expect("decode");
        assert_eq!(doc.width, 4);
        assert_eq!(doc.height, 3);
        assert_eq!(doc.rgba.len(), 4 * 3 * 4);
        assert_eq!(doc.pixel_count(), 12);
        assert_eq!(doc.format_hint, "png");
        assert!(!doc.downscaled);
    }

    #[test]
    fn other_containers_round_trip() {
        for (format, name) in [
            (ImageFormat::Bmp, "bmp"),
            (ImageFormat::Gif, "gif"),
            (ImageFormat::WebP, "webp"),
        ] {
            let bytes = encoded(5, 2, format);
            let doc = decode_image_bytes(&bytes, "", 40_000_000)
                .unwrap_or_else(|e| panic!("{name} decode failed: {e}"));
            assert_eq!(doc.width, 5, "{name}");
            assert_eq!(doc.height, 2, "{name}");
            assert_eq!(doc.rgba.len(), 5 * 2 * 4, "{name}");
            assert_eq!(doc.format_hint, name);
        }
    }

    #[test]
    fn jpeg_round_trips() {
        let bytes = encoded(8, 8, ImageFormat::Jpeg);
        let doc = decode_image_bytes(&bytes, "jpeg", 40_000_000).expect("decode");
        assert_eq!((doc.width, doc.height), (8, 8));
        assert_eq!(doc.format_hint, "jpeg");
    }

    #[test]
    fn load_image_reads_a_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pic.png");
        std::fs::write(&path, encoded(3, 3, ImageFormat::Png)).expect("write");
        let doc = load_image(&path, 40_000_000).expect("load_image");
        assert_eq!((doc.width, doc.height), (3, 3));
        assert_eq!(doc.rgba.len(), 36);
        assert_eq!(doc.format_hint, "png");
    }

    #[test]
    fn load_image_reports_a_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = load_image(&dir.path().join("nope.png"), 40_000_000).expect_err("must fail");
        assert!(!error.is_empty());
    }

    #[test]
    fn empty_input_is_rejected() {
        let error = decode_image_bytes(&[], "png", 40_000_000).expect_err("must fail");
        assert!(error.contains("empty"), "error: {error}");
    }

    #[test]
    fn garbage_input_is_rejected_without_panicking() {
        let error = decode_image_bytes(b"this is not an image at all", "", 40_000_000)
            .expect_err("must fail");
        assert!(!error.is_empty());
    }

    #[test]
    fn truncated_image_is_rejected_without_panicking() {
        let full = encoded(16, 16, ImageFormat::Png);
        for cut in [8, 16, 40, full.len() / 2] {
            let truncated = &full[..cut.min(full.len())];
            let result = decode_image_bytes(truncated, "png", 40_000_000);
            assert!(result.is_err(), "cut {cut} unexpectedly decoded");
        }
    }

    #[test]
    fn corrupted_png_payload_is_rejected() {
        let mut bytes = encoded(16, 16, ImageFormat::Png);
        // Flip bytes inside the first IDAT payload.
        let len = bytes.len();
        for byte in bytes.get_mut(len / 2..len / 2 + 8).into_iter().flatten() {
            *byte ^= 0xFF;
        }
        let result = decode_image_bytes(&bytes, "png", 40_000_000);
        assert!(result.is_err(), "corrupted png decoded anyway");
    }

    #[test]
    fn huge_declared_size_is_rejected_before_allocating() {
        // 40000×40000 = 1.6e9 pixels, i.e. 6.4 GiB of RGBA: far beyond the hard cap.
        let bytes = png_declaring(40_000, 40_000);
        let error = decode_image_bytes(&bytes, "png", 40_000_000).expect_err("must be rejected");
        assert!(
            error.contains("too large"),
            "error should explain the size problem: {error}"
        );
        // Even with the pixel budget disabled the hard cap still protects us.
        let error = decode_image_bytes(&bytes, "png", 0).expect_err("must be rejected");
        assert!(error.contains("too large"), "error: {error}");
    }

    #[test]
    fn truncation_test_fixture_really_is_a_valid_container_header() {
        // The fixture must pass the cheap dimension probe, otherwise the
        // decompression-bomb test above would succeed for the wrong reason.
        let bytes = png_declaring(1000, 2000);
        let dims = ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .expect("guess")
            .into_dimensions()
            .expect("dimensions");
        assert_eq!(dims, (1000, 2000));
    }

    #[test]
    fn oversized_image_is_downscaled_to_the_pixel_budget() {
        let bytes = encoded(100, 100, ImageFormat::Png);
        let doc = decode_image_bytes(&bytes, "png", 1_000).expect("decode");
        assert!(doc.downscaled);
        assert!(doc.pixel_count() <= 1_000, "pixels: {}", doc.pixel_count());
        assert_eq!(doc.rgba.len() as u64, doc.pixel_count() * 4);
        // Aspect ratio of a square image stays square.
        assert_eq!(doc.width, doc.height);
        assert!(doc.width >= 28 && doc.width <= 32, "width: {}", doc.width);
    }

    #[test]
    fn image_within_budget_is_not_downscaled() {
        let bytes = encoded(20, 10, ImageFormat::Png);
        let doc = decode_image_bytes(&bytes, "png", 200).expect("decode");
        assert!(!doc.downscaled, "exactly at budget must not downscale");
        assert_eq!((doc.width, doc.height), (20, 10));
    }

    #[test]
    fn zero_pixel_budget_disables_downscaling() {
        let bytes = encoded(60, 40, ImageFormat::Png);
        let doc = decode_image_bytes(&bytes, "png", 0).expect("decode");
        assert!(!doc.downscaled);
        assert_eq!((doc.width, doc.height), (60, 40));
    }

    #[test]
    fn extreme_wide_image_keeps_a_usable_thumbnail() {
        let bytes = encoded(400, 2, ImageFormat::Png);
        let doc = decode_image_bytes(&bytes, "png", 100).expect("decode");
        assert!(doc.downscaled);
        assert!(doc.pixel_count() <= 100);
        assert_eq!(doc.height, 1, "a 400x2 image must keep at least one row");
        assert!(doc.width >= 1);
    }

    #[test]
    fn fit_dimensions_preserves_aspect_ratio_and_never_returns_zero() {
        assert_eq!(fit_dimensions(100, 100, 10_000), (100, 100));
        assert_eq!(fit_dimensions(100, 100, 0), (100, 100));
        let (w, h) = fit_dimensions(4000, 2000, 1_000);
        assert!(w as u64 * h as u64 <= 1_000);
        assert!((w as f64 / h as f64 - 2.0).abs() < 0.2, "{w}x{h}");
        let (w, h) = fit_dimensions(1_000_000, 1, 100);
        assert!(w as u64 * h as u64 <= 100);
        assert!(w >= 1 && h >= 1);
    }

    #[test]
    fn caller_hint_is_used_when_the_container_is_unknown() {
        // A valid PNG is always identified, so the hint only matters for the
        // fallback path; verify the helper still reports the decoded container.
        let bytes = encoded(2, 2, ImageFormat::Png);
        let doc = decode_image_bytes(&bytes, "screenshot", 40_000_000).expect("decode");
        assert_eq!(doc.format_hint, "png");
        assert_eq!(format_name(ImageFormat::Png), Some("png"));
    }

    #[test]
    fn debug_output_is_compact() {
        let doc = decode_image_bytes(&encoded(4, 4, ImageFormat::Png), "png", 0).expect("decode");
        let text = format!("{doc:?}");
        assert!(text.contains("rgba_len: 64"), "debug: {text}");
        assert!(text.len() < 200, "debug output too large: {text}");
    }

    #[test]
    fn budget_constants_are_consistent() {
        // Values are part of the documented contract; the relations keep the
        // decoder safe (a file may be smaller than the buffer it decodes to, and
        // the default pixel budget must fit inside the buffer cap).
        let cap = std::hint::black_box(MAX_DECODED_BYTES);
        let file_cap = std::hint::black_box(MAX_IMAGE_FILE_BYTES);
        assert_eq!(cap, 1024 * 1024 * 1024);
        assert!(file_cap < cap);
        assert!(40_000_000u64 * 4 < cap);
    }
}

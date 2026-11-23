//! Viewer configuration.

/// How the text decoder should choose an encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingChoice {
    /// Sniff the encoding: BOM first, then UTF-8 validity, then a BOM-less
    /// UTF-16 heuristic, then statistical detection (`chardetng`).
    Auto,
    /// Use this encoding label unless the stream carries an explicit BOM, which
    /// always wins. Labels are the usual names accepted by the WHATWG encoding
    /// standard (`"utf-8"`, `"utf-16le"`, `"windows-1250"`, `"iso-8859-2"`,
    /// `"koi8-r"`, …). `"utf-32"`/`"utf-32le"`/`"utf-32be"` are handled by the
    /// crate itself because the standard has no UTF-32. An unknown label falls
    /// back to [`EncodingChoice::Auto`] instead of failing.
    Explicit(&'static str),
}

/// Tunables for the viewer engine.
///
/// All values are plain data so the GUI can persist them verbatim. The defaults
/// are deliberately conservative: 8 MiB of preview text, a 16-byte hexdump row
/// and a 40 Mpixel image budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewerOptions {
    /// Upper bound on how many bytes are read for a text or hex preview.
    ///
    /// Anything beyond it is reported through the `truncated` flags, so a
    /// 20 GiB log file is as cheap to display as a 8 MiB one. `0` disables the
    /// preview entirely (nothing is read).
    pub max_bytes_read: usize,
    /// How many bytes one hexdump row shows. Values outside `1..=64` are
    /// clamped by [`crate::hex_document`] and [`crate::load_hex`].
    pub hex_bytes_per_line: usize,
    /// Encoding selection for text content.
    pub encoding: EncodingChoice,
    /// Whether the GUI should soft-wrap long lines (a display hint only; the
    /// engine never re-flows text).
    pub wrap_text: bool,
    /// Pixel budget for images: `width * height` above this is downscaled.
    /// `0` means "no pixel budget" (the hard decoded-buffer cap still applies).
    pub max_image_pixels: u64,
}

impl Default for ViewerOptions {
    fn default() -> Self {
        ViewerOptions {
            max_bytes_read: 8 * 1024 * 1024,
            hex_bytes_per_line: 16,
            encoding: EncodingChoice::Auto,
            wrap_text: true,
            max_image_pixels: 40_000_000,
        }
    }
}

impl ViewerOptions {
    /// The hexdump row size actually used, clamped to the supported `1..=64`
    /// range.
    pub(crate) fn bytes_per_line(&self) -> usize {
        self.hex_bytes_per_line.clamp(1, 64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_documented_values() {
        let o = ViewerOptions::default();
        assert_eq!(o.max_bytes_read, 8 * 1024 * 1024);
        assert_eq!(o.hex_bytes_per_line, 16);
        assert_eq!(o.encoding, EncodingChoice::Auto);
        assert!(o.wrap_text);
        assert_eq!(o.max_image_pixels, 40_000_000);
    }

    #[test]
    fn bytes_per_line_is_clamped_to_supported_range() {
        for (requested, expected) in [
            (0usize, 1usize),
            (1, 1),
            (16, 16),
            (64, 64),
            (65, 64),
            (usize::MAX, 64),
        ] {
            let options = ViewerOptions {
                hex_bytes_per_line: requested,
                ..ViewerOptions::default()
            };
            assert_eq!(options.bytes_per_line(), expected, "requested {requested}");
        }
    }

    #[test]
    fn options_are_copyable_plain_data() {
        let a = ViewerOptions::default();
        let b = a;
        assert_eq!(a, b);
        let c = ViewerOptions {
            encoding: EncodingChoice::Explicit("windows-1250"),
            ..a
        };
        assert_ne!(a, c);
        assert!(matches!(
            c.encoding,
            EncodingChoice::Explicit("windows-1250")
        ));
    }
}

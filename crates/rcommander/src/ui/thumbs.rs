//! Thumbnail cache for the thumbnail view mode.
//!
//! Decoding an image costs milliseconds, so decoded thumbnails are kept as GPU
//! textures keyed by path and size. The cache is bounded: the oldest entries are
//! dropped when it grows past [`MAX_ENTRIES`], which keeps memory flat while
//! scrolling through a directory with thousands of photos.
//!
//! Every decode goes through `rc-viewer`, which enforces a pixel budget, so a
//! 200-megapixel image is downscaled instead of exhausting memory.

use egui::{Context, TextureHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Upper bound on cached thumbnails.
pub const MAX_ENTRIES: usize = 512;

/// One cached thumbnail.
struct Cached {
    texture: TextureHandle,
    /// Insertion counter, used to evict the oldest entry.
    age: u64,
}

/// Thumbnail cache shared by both panels.
#[derive(Default)]
pub struct ThumbnailCache {
    entries: HashMap<(PathBuf, u32), Cached>,
    counter: u64,
    /// Number of decodes performed, handy for tests and diagnostics.
    pub decodes: u64,
}

impl ThumbnailCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cached texture for `path` at `size`, if it was decoded before.
    pub fn get(&self, path: &Path, size: u32) -> Option<&TextureHandle> {
        self.entries
            .get(&(path.to_path_buf(), size))
            .map(|entry| &entry.texture)
    }

    /// Return the thumbnail for `path`, decoding it when necessary.
    ///
    /// Returns `None` when the file is not a decodable image; the caller then
    /// falls back to the file name only.
    pub fn thumbnail(&mut self, ctx: &Context, path: &Path, size: u32) -> Option<&TextureHandle> {
        let key = (path.to_path_buf(), size);
        if !self.entries.contains_key(&key) {
            let decoded = rc_viewer::load_image(path, 40_000_000).ok()?;
            let image = if decoded.width.max(decoded.height) > size {
                downscale(&decoded, size)
            } else {
                egui::ColorImage::from_rgba_unmultiplied(
                    [decoded.width as usize, decoded.height as usize],
                    &decoded.rgba,
                )
            };
            let texture = ctx.load_texture(
                format!("thumb-{}", path.display()),
                image,
                egui::TextureOptions::LINEAR,
            );
            self.counter += 1;
            self.decodes += 1;
            self.entries.insert(
                key.clone(),
                Cached {
                    texture,
                    age: self.counter,
                },
            );
            self.evict();
        }
        // Touch the entry so recently used thumbnails survive eviction.
        if let Some(entry) = self.entries.get_mut(&key) {
            self.counter += 1;
            entry.age = self.counter;
        }
        self.entries.get(&key).map(|entry| &entry.texture)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn evict(&mut self) {
        while self.entries.len() > MAX_ENTRIES {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.age)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }
}

/// Nearest-neighbour downscale to fit `size` pixels on the longer edge.
fn downscale(image: &rc_viewer::ImageDocument, size: u32) -> egui::ColorImage {
    let longest = image.width.max(image.height).max(1);
    let scale = size as f32 / longest as f32;
    let out_w = ((image.width as f32 * scale).round() as usize).max(1);
    let out_h = ((image.height as f32 * scale).round() as usize).max(1);
    let mut pixels = Vec::with_capacity(out_w * out_h);
    for y in 0..out_h {
        let src_y = (y as f32 / scale) as u32;
        for x in 0..out_w {
            let src_x = (x as f32 / scale) as u32;
            let sx = src_x.min(image.width.saturating_sub(1));
            let sy = src_y.min(image.height.saturating_sub(1));
            let index = ((sy * image.width + sx) * 4) as usize;
            let r = image.rgba.get(index).copied().unwrap_or(0);
            let g = image.rgba.get(index + 1).copied().unwrap_or(0);
            let b = image.rgba.get(index + 2).copied().unwrap_or(0);
            let a = image.rgba.get(index + 3).copied().unwrap_or(255);
            pixels.push(egui::Color32::from_rgba_unmultiplied(r, g, b, a));
        }
    }
    egui::ColorImage {
        size: [out_w, out_h],
        pixels,
        source_size: egui::vec2(image.width as f32, image.height as f32),
    }
}

/// True when the name looks like an image that is worth decoding.
pub fn looks_like_image(name: &str) -> bool {
    matches!(
        name.rsplit('.')
            .next()
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default()
            .as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tif" | "tiff"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, width: u32, height: u32) {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, 128, 255]);
            }
        }
        let image: image::RgbaImage =
            image::ImageBuffer::from_raw(width, height, rgba).expect("buffer");
        image.save(path).expect("png written");
    }

    #[test]
    fn image_extensions_are_detected() {
        assert!(looks_like_image("photo.PNG"));
        assert!(looks_like_image("a.jpeg"));
        assert!(!looks_like_image("a.txt"));
        assert!(!looks_like_image("noext"));
        assert!(!looks_like_image(""));
    }

    #[test]
    fn cache_starts_empty() {
        let cache = ThumbnailCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.get(Path::new("/nothing"), 64).is_none());
    }

    #[test]
    fn downscale_fits_the_requested_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.png");
        write_png(&path, 64, 32);
        let decoded = rc_viewer::load_image(&path, 40_000_000).unwrap();
        let small = downscale(&decoded, 16);
        assert_eq!(small.size, [16, 8]);
        assert_eq!(small.pixels.len(), 16 * 8);
        // A size larger than the image keeps the aspect ratio rounded up.
        let same = downscale(&decoded, 128);
        assert_eq!(same.size, [128, 64]);
    }

    #[test]
    fn eviction_keeps_the_cache_bounded() {
        let mut cache = ThumbnailCache::new();
        // Insert more entries than the bound, without requiring a GPU.
        for index in 0..(MAX_ENTRIES + 20) {
            cache.counter += 1;
            cache.entries.insert(
                (PathBuf::from(format!("/img{index}.png")), 64),
                Cached {
                    texture: dummy_texture(),
                    age: cache.counter,
                },
            );
            cache.evict();
        }
        assert!(cache.len() <= MAX_ENTRIES, "cache grew to {}", cache.len());
    }

    fn dummy_texture() -> TextureHandle {
        // A TextureHandle needs a Context; use a standalone one for the test.
        let ctx = egui::Context::default();
        let image = egui::ColorImage::new([1, 1], vec![egui::Color32::TRANSPARENT]);
        ctx.load_texture("dummy", image, egui::TextureOptions::NEAREST)
    }
}

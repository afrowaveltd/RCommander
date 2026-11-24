//! Colours, spacing and fonts.
//!
//! The palette mirrors the classic commander look: white lists with a dark blue
//! cursor bar, blue directories, grey hidden entries and red marks.

use egui::{Color32, FontId, TextStyle};

/// A resolved palette for one theme.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub background: Color32,
    pub foreground: Color32,
    pub cursor_background: Color32,
    pub cursor_foreground: Color32,
    pub selected_background: Color32,
    pub selected_foreground: Color32,
    pub directory: Color32,
    pub hidden: Color32,
    pub readonly: Color32,
    pub archive: Color32,
    pub symlink: Color32,
    pub header_background: Color32,
    pub panel_border: Color32,
    pub status_background: Color32,
    pub accent: Color32,
    /// Background of every second row, so a long list stays readable.
    pub row_alternate: Color32,
    /// Destructive warnings: factory resets, overwrite prompts, failed checks.
    pub warning: Color32,
}

impl Palette {
    /// Classic light theme.
    pub fn light() -> Self {
        Palette {
            background: Color32::from_rgb(0xff, 0xff, 0xff),
            foreground: Color32::from_rgb(0x00, 0x00, 0x00),
            cursor_background: Color32::from_rgb(0x00, 0x00, 0x80),
            cursor_foreground: Color32::from_rgb(0xff, 0xff, 0xff),
            selected_background: Color32::from_rgb(0xc8, 0x10, 0x10),
            selected_foreground: Color32::from_rgb(0xff, 0xff, 0xff),
            directory: Color32::from_rgb(0x00, 0x00, 0x80),
            hidden: Color32::from_rgb(0x80, 0x80, 0x80),
            readonly: Color32::from_rgb(0xa0, 0x00, 0x00),
            archive: Color32::from_rgb(0x00, 0x70, 0x00),
            symlink: Color32::from_rgb(0x00, 0x80, 0x80),
            header_background: Color32::from_rgb(0xd4, 0xd0, 0xc8),
            panel_border: Color32::from_rgb(0x80, 0x80, 0x80),
            status_background: Color32::from_rgb(0xd4, 0xd0, 0xc8),
            accent: Color32::from_rgb(0x00, 0x00, 0x80),
            row_alternate: Color32::from_rgb(0xf0, 0xf2, 0xf8),
            warning: Color32::from_rgb(0xb0, 0x20, 0x20),
        }
    }

    /// Dark theme, used by the "Dark <-> Normal" command.
    pub fn dark() -> Self {
        Palette {
            background: Color32::from_rgb(0x1e, 0x1e, 0x1e),
            foreground: Color32::from_rgb(0xd8, 0xd8, 0xd8),
            cursor_background: Color32::from_rgb(0x26, 0x4f, 0x78),
            cursor_foreground: Color32::from_rgb(0xff, 0xff, 0xff),
            selected_background: Color32::from_rgb(0x8b, 0x1a, 0x1a),
            selected_foreground: Color32::from_rgb(0xff, 0xff, 0xff),
            directory: Color32::from_rgb(0x9c, 0xd0, 0xff),
            hidden: Color32::from_rgb(0x88, 0x88, 0x78),
            readonly: Color32::from_rgb(0xff, 0x8a, 0x80),
            archive: Color32::from_rgb(0x77, 0xdd, 0x77),
            symlink: Color32::from_rgb(0x66, 0xd9, 0xd9),
            header_background: Color32::from_rgb(0x2d, 0x2d, 0x2d),
            panel_border: Color32::from_rgb(0x55, 0x55, 0x55),
            status_background: Color32::from_rgb(0x2d, 0x2d, 0x2d),
            accent: Color32::from_rgb(0x9c, 0xd0, 0xff),
            row_alternate: Color32::from_rgb(0x27, 0x27, 0x2b),
            warning: Color32::from_rgb(0xff, 0x9a, 0x8a),
        }
    }

    /// Choose a palette by name (`light`, `dark`, anything else → light).
    pub fn by_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "dark" => Self::dark(),
            _ => Self::light(),
        }
    }
}

/// Text roles used by the file list renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowStyle {
    Normal,
    Directory,
    Hidden,
    Readonly,
    Archive,
    Symlink,
}

impl RowStyle {
    /// Decide the style for an entry, honouring the precedence used by the
    /// classic interface: directories and links first, then attributes.
    pub fn for_entry(entry: &rc_core::entry::Entry) -> Self {
        if entry.kind.is_link() {
            return RowStyle::Symlink;
        }
        if entry.is_dir() {
            return RowStyle::Directory;
        }
        if entry.attrs.readonly {
            return RowStyle::Readonly;
        }
        if entry.is_hidden() {
            return RowStyle::Hidden;
        }
        if entry.extension() == "zip" || entry.extension() == "7z" || entry.extension() == "gz" {
            return RowStyle::Archive;
        }
        RowStyle::Normal
    }

    /// Foreground colour of the style.
    pub fn color(self, palette: &Palette) -> Color32 {
        match self {
            RowStyle::Normal => palette.foreground,
            RowStyle::Directory => palette.directory,
            RowStyle::Hidden => palette.hidden,
            RowStyle::Readonly => palette.readonly,
            RowStyle::Archive => palette.archive,
            RowStyle::Symlink => palette.symlink,
        }
    }
}

/// Build an egui [`egui::Visuals`] for the palette and font size.
pub fn visuals(palette: &Palette, dark: bool) -> egui::Visuals {
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = palette.background;
    v.window_fill = palette.background;
    v.extreme_bg_color = palette.background;
    v.faint_bg_color = palette.header_background;
    v.override_text_color = Some(palette.foreground);
    v.selection.bg_fill = palette.cursor_background;
    v.selection.stroke = egui::Stroke::new(1.0, palette.cursor_foreground);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, palette.panel_border);
    v.window_stroke = egui::Stroke::new(1.0, palette.panel_border);
    v.window_corner_radius = egui::CornerRadius::same(2);
    v
}

/// Register the bundled icon font as the lowest fallback of both families.
///
/// Text keeps the normal proportional font; only the private-use code points of
/// [`crate::icons`] come from the bundled font. Without this, egui refuses to
/// render a character it cannot find (it panics with "No glyph for …"), so the
/// icons are also a correctness fix.
pub fn install_icon_font(ctx: &egui::Context) {
    use egui::{FontData, FontFamily, FontPriority};
    ctx.add_font(egui::epaint::text::FontInsert::new(
        crate::icons::FONT_NAME,
        FontData::from_static(crate::icons::FONT_BYTES),
        vec![
            egui::epaint::text::InsertFontFamily {
                family: FontFamily::Proportional,
                priority: FontPriority::Lowest,
            },
            egui::epaint::text::InsertFontFamily {
                family: FontFamily::Monospace,
                priority: FontPriority::Lowest,
            },
        ],
    ));
}

/// Format an icon for use in a label.
pub fn icon_text(icon: crate::icons::Icon) -> String {
    icon.glyph().to_string()
}

/// Register the monospace and proportional text styles.
pub fn install_text_styles(ctx: &egui::Context, font_size: f32) {
    let styles = [
        (TextStyle::Heading, FontId::proportional(font_size + 4.0)),
        (TextStyle::Body, FontId::proportional(font_size)),
        (TextStyle::Button, FontId::proportional(font_size)),
        (
            TextStyle::Small,
            FontId::proportional((font_size - 2.0).max(8.0)),
        ),
        (TextStyle::Monospace, FontId::monospace(font_size)),
    ];
    ctx.all_styles_mut(|style| {
        style.text_styles = styles.iter().cloned().collect();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rc_core::entry::{Entry, EntryKind};
    use std::path::PathBuf;

    fn entry(name: &str, kind: EntryKind) -> Entry {
        Entry::new(name, PathBuf::from("/").join(name), kind, 0)
    }

    #[test]
    fn palettes_are_distinct_and_named() {
        assert_ne!(Palette::light().background, Palette::dark().background);
        assert_eq!(
            Palette::by_name("dark").background,
            Palette::dark().background
        );
        assert_eq!(
            Palette::by_name("LIGHT").background,
            Palette::light().background
        );
        assert_eq!(
            Palette::by_name("nonsense").background,
            Palette::light().background
        );
    }

    #[test]
    fn row_style_precedence() {
        assert_eq!(
            RowStyle::for_entry(&entry("d", EntryKind::Dir)),
            RowStyle::Directory
        );
        assert_eq!(
            RowStyle::for_entry(&entry("l", EntryKind::SymlinkToFile)),
            RowStyle::Symlink
        );
        let mut ro = entry("f.txt", EntryKind::File);
        ro.attrs.readonly = true;
        assert_eq!(RowStyle::for_entry(&ro), RowStyle::Readonly);
        assert_eq!(
            RowStyle::for_entry(&entry(".hidden", EntryKind::File)),
            RowStyle::Hidden
        );
        assert_eq!(
            RowStyle::for_entry(&entry("a.zip", EntryKind::File)),
            RowStyle::Archive
        );
        assert_eq!(
            RowStyle::for_entry(&entry("a.txt", EntryKind::File)),
            RowStyle::Normal
        );
    }

    #[test]
    fn row_colours_come_from_the_palette() {
        let p = Palette::light();
        assert_eq!(RowStyle::Directory.color(&p), p.directory);
        assert_eq!(RowStyle::Normal.color(&p), p.foreground);
        let d = Palette::dark();
        assert_ne!(RowStyle::Hidden.color(&d), RowStyle::Hidden.color(&p));
    }

    #[test]
    fn the_alternate_row_colour_is_a_subtle_shift_from_the_background() {
        for palette in [Palette::light(), Palette::dark()] {
            assert_ne!(
                palette.row_alternate, palette.background,
                "zebra rows must be visible"
            );
            let (bg, alt) = (palette.background, palette.row_alternate);
            let distance = (bg.r() as i32 - alt.r() as i32).abs()
                + (bg.g() as i32 - alt.g() as i32).abs()
                + (bg.b() as i32 - alt.b() as i32).abs();
            assert!(
                (6..=60).contains(&distance),
                "the shift should be subtle but visible: {distance}"
            );
        }
    }

    #[test]
    fn visuals_take_the_palette_colours() {
        let p = Palette::dark();
        let v = visuals(&p, true);
        assert_eq!(v.panel_fill, p.background);
        assert_eq!(v.override_text_color, Some(p.foreground));
    }
}

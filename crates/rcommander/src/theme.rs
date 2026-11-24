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

fn colour(rgb: rc_core::theme::Rgb) -> Color32 {
    Color32::from_rgb(rgb.r, rgb.g, rgb.b)
}

impl Palette {
    /// Build the palette from a resolved theme file.
    ///
    /// This is the single place where theme data becomes egui colours, so a role
    /// added to `rc_core::theme` only has to be mapped once.
    pub fn from_resolved(resolved: &rc_core::theme::Resolved) -> Palette {
        use rc_core::theme::Role;
        Palette {
            background: colour(resolved.get(Role::Background)),
            foreground: colour(resolved.get(Role::Foreground)),
            cursor_background: colour(resolved.get(Role::CursorBackground)),
            cursor_foreground: colour(resolved.get(Role::CursorForeground)),
            selected_background: colour(resolved.get(Role::SelectedBackground)),
            selected_foreground: colour(resolved.get(Role::SelectedForeground)),
            directory: colour(resolved.get(Role::Directory)),
            hidden: colour(resolved.get(Role::Hidden)),
            readonly: colour(resolved.get(Role::Readonly)),
            archive: colour(resolved.get(Role::Archive)),
            symlink: colour(resolved.get(Role::Symlink)),
            header_background: colour(resolved.get(Role::HeaderBackground)),
            panel_border: colour(resolved.get(Role::PanelBorder)),
            status_background: colour(resolved.get(Role::StatusBackground)),
            accent: colour(resolved.get(Role::Accent)),
            row_alternate: colour(resolved.get(Role::RowAlternate)),
            warning: colour(resolved.get(Role::Warning)),
        }
    }

    /// The colour of a semantic role by name.
    ///
    /// Used where the caller only knows the role (the tool-bar icons), so the
    /// palette and the role table cannot drift apart.
    pub fn role(&self, role: rc_core::theme::Role) -> Color32 {
        use rc_core::theme::Role;
        match role {
            Role::Background => self.background,
            Role::Foreground => self.foreground,
            Role::CursorBackground => self.cursor_background,
            Role::CursorForeground => self.cursor_foreground,
            Role::SelectedBackground => self.selected_background,
            Role::SelectedForeground => self.selected_foreground,
            Role::Directory => self.directory,
            Role::Hidden => self.hidden,
            Role::Readonly => self.readonly,
            Role::Archive => self.archive,
            Role::Symlink => self.symlink,
            Role::HeaderBackground => self.header_background,
            Role::PanelBorder => self.panel_border,
            Role::StatusBackground => self.status_background,
            Role::Accent => self.accent,
            Role::RowAlternate => self.row_alternate,
            Role::Warning => self.warning,
        }
    }

    /// The palette of the theme the model has active, including custom files.
    pub fn for_model(model: &crate::model::Model) -> Palette {
        Palette::from_resolved(&model.resolved_theme())
    }

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
        // Built-in names only: a custom theme needs the loaded theme set, which
        // `Palette::for_model` has. `light()`/`dark()` stay as the canonical
        // values the engine's built-ins are compared against.
        match name.trim().to_ascii_lowercase().as_str() {
            "dark" => Self::dark(),
            _ => Self::light(),
        }
    }

    /// True when the palette reads better on a dark background.
    pub fn is_dark(&self) -> bool {
        (0.299 * self.background.r() as f32
            + 0.587 * self.background.g() as f32
            + 0.114 * self.background.b() as f32)
            < 128.0
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
    v.code_bg_color = palette.row_alternate;
    v.override_text_color = Some(palette.foreground);
    v.selection.bg_fill = palette.cursor_background;
    v.selection.stroke = egui::Stroke::new(1.0, palette.cursor_foreground);
    v.window_stroke = egui::Stroke::new(1.0, palette.panel_border);
    v.window_corner_radius = egui::CornerRadius::same(2);
    v.menu_corner_radius = egui::CornerRadius::same(2);

    // The widget styles decide what a *button* and a window title bar look like. They
    // used to be left at egui's defaults, so every theme with a dark palette kept white
    // buttons and a white title row — unreadable in half the combinations.
    let chrome = palette.header_background;
    let hover = mix(chrome, palette.accent, 0.35);
    let pressed = mix(chrome, palette.accent, 0.6);
    let button_stroke = egui::Stroke::new(1.0, palette.panel_border);
    let text = egui::Stroke::new(1.0, palette.foreground);

    v.widgets.noninteractive.bg_fill = chrome;
    v.widgets.noninteractive.weak_bg_fill = chrome;
    v.widgets.noninteractive.bg_stroke = button_stroke;
    v.widgets.noninteractive.fg_stroke = text;

    v.widgets.inactive.bg_fill = chrome;
    v.widgets.inactive.weak_bg_fill = chrome;
    v.widgets.inactive.bg_stroke = button_stroke;
    v.widgets.inactive.fg_stroke = text;

    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, palette.accent);
    v.widgets.hovered.fg_stroke = text;

    v.widgets.active.bg_fill = pressed;
    v.widgets.active.weak_bg_fill = pressed;
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, palette.accent);
    v.widgets.active.fg_stroke = text;

    // Window and menu title bars use the `open` state.
    v.widgets.open.bg_fill = chrome;
    v.widgets.open.weak_bg_fill = chrome;
    v.widgets.open.bg_stroke = button_stroke;
    v.widgets.open.fg_stroke = text;

    v
}

/// Blend two colours: `t == 0` is `a`, `t == 1` is `b`.
fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
    egui::Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
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
    fn buttons_and_title_bars_follow_the_theme() {
        // The regression this guards: every theme with a dark palette used to keep
        // egui's light button and title-bar colours.
        for palette in [Palette::light(), Palette::dark()] {
            let dark = palette.is_dark();
            let v = visuals(&palette, dark);
            assert_eq!(
                v.widgets.inactive.weak_bg_fill, palette.header_background,
                "button background"
            );
            assert_eq!(
                v.widgets.inactive.bg_fill, palette.header_background,
                "checkbox and slider background"
            );
            assert_eq!(
                v.widgets.open.weak_bg_fill, palette.header_background,
                "window title bar"
            );
            assert_eq!(v.widgets.noninteractive.bg_fill, palette.header_background);
            assert_eq!(v.widgets.inactive.fg_stroke.color, palette.foreground);
            // A dark theme must not leave a white button behind.
            if dark {
                assert!(
                    v.widgets.inactive.weak_bg_fill.r() < 0x80,
                    "the dark theme's buttons are dark: {:?}",
                    v.widgets.inactive.weak_bg_fill
                );
            }
            // Hover and press stay distinguishable from the resting state.
            assert_ne!(
                v.widgets.hovered.weak_bg_fill,
                v.widgets.inactive.weak_bg_fill
            );
            assert_ne!(
                v.widgets.active.weak_bg_fill,
                v.widgets.hovered.weak_bg_fill
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

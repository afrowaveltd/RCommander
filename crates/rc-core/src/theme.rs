//! Colour themes as data (Phase 14).
//!
//! A theme is a TOML file with a `[meta]` section and one hex colour per semantic
//! role. Roles are inherited from a base theme, so a theme file only has to name
//! the colours it actually changes; a missing role never becomes black or
//! transparent by accident.
//!
//! The engine is deliberately free of any user-interface dependency: it parses,
//! validates, resolves and writes themes, and the application maps the resolved
//! roles onto its palette.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One semantic colour role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Background,
    Foreground,
    CursorBackground,
    CursorForeground,
    SelectedBackground,
    SelectedForeground,
    Directory,
    Hidden,
    Readonly,
    Archive,
    Symlink,
    HeaderBackground,
    PanelBorder,
    StatusBackground,
    Accent,
    RowAlternate,
    Warning,
    /// Syntax highlighting: keywords, strings, comments, numbers, types and calls.
    CodeKeyword,
    CodeString,
    CodeComment,
    CodeNumber,
    CodeType,
    CodeFunction,
}

impl Role {
    /// Every role, in the order the editor lists them.
    pub const ALL: [Role; 23] = [
        Role::Background,
        Role::Foreground,
        Role::CursorBackground,
        Role::CursorForeground,
        Role::SelectedBackground,
        Role::SelectedForeground,
        Role::Directory,
        Role::Hidden,
        Role::Readonly,
        Role::Archive,
        Role::Symlink,
        Role::HeaderBackground,
        Role::PanelBorder,
        Role::StatusBackground,
        Role::Accent,
        Role::RowAlternate,
        Role::Warning,
        Role::CodeKeyword,
        Role::CodeString,
        Role::CodeComment,
        Role::CodeNumber,
        Role::CodeType,
        Role::CodeFunction,
    ];

    /// Name used in the TOML file and the localisation key.
    pub fn name(self) -> &'static str {
        match self {
            Role::Background => "background",
            Role::Foreground => "foreground",
            Role::CursorBackground => "cursor_background",
            Role::CursorForeground => "cursor_foreground",
            Role::SelectedBackground => "selected_background",
            Role::SelectedForeground => "selected_foreground",
            Role::Directory => "directory",
            Role::Hidden => "hidden",
            Role::Readonly => "readonly",
            Role::Archive => "archive",
            Role::Symlink => "symlink",
            Role::HeaderBackground => "header_background",
            Role::PanelBorder => "panel_border",
            Role::StatusBackground => "status_background",
            Role::Accent => "accent",
            Role::RowAlternate => "row_alternate",
            Role::Warning => "warning",
            Role::CodeKeyword => "code_keyword",
            Role::CodeString => "code_string",
            Role::CodeComment => "code_comment",
            Role::CodeNumber => "code_number",
            Role::CodeType => "code_type",
            Role::CodeFunction => "code_function",
        }
    }

    /// Look a role up by its file name.
    pub fn from_name(name: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|role| role.name() == name)
    }
}

/// An `#rrggbb` colour, kept as bytes so the engine needs no UI types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    /// Parse `#rrggbb`, `rrggbb` or the short `#rgb` form.
    pub fn parse(text: &str) -> Result<Rgb, String> {
        let text = text.trim();
        let hex = text.strip_prefix('#').unwrap_or(text);
        let bad = || format!("“{text}” is not a colour (expected #rrggbb)");
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| bad())?;
                let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| bad())?;
                let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| bad())?;
                Ok(Rgb::new(r, g, b))
            }
            3 => {
                let mut parts = [0u8; 3];
                for (index, ch) in hex.chars().enumerate() {
                    let value = ch.to_digit(16).ok_or_else(bad)? as u8;
                    // `#abc` means `#aabbcc`.
                    parts[index] = value * 17;
                }
                Ok(Rgb::new(parts[0], parts[1], parts[2]))
            }
            _ => Err(bad()),
        }
    }

    /// `#rrggbb`, lower case.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Relative luminance (0 = black, 255 = white), used for contrast warnings.
    pub fn luminance(self) -> f32 {
        // The perceptual weights of ITU-R BT.601: good enough for a warning.
        0.299 * self.r as f32 + 0.587 * self.g as f32 + 0.114 * self.b as f32
    }

    /// WCAG-ish contrast ratio between two colours (1.0 … 21.0).
    pub fn contrast(self, other: Rgb) -> f32 {
        fn channel(value: u8) -> f32 {
            let v = value as f32 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        fn relative(colour: Rgb) -> f32 {
            0.2126 * channel(colour.r) + 0.7152 * channel(colour.g) + 0.0722 * channel(colour.b)
        }
        let a = relative(self) + 0.05;
        let b = relative(other) + 0.05;
        if a > b { a / b } else { b / a }
    }
}

/// `[meta]` of a theme file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub name: String,
    /// `light`, `dark` or the name of another theme the roles are inherited from.
    pub base: String,
    pub author: String,
    pub version: String,
    /// Free-form note shown in the theme list.
    pub description: String,
}

/// A theme as written on disk: only the roles it overrides are present.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub meta: Meta,
    /// `role name = "#rrggbb"`.
    pub colors: BTreeMap<String, String>,
    /// `mask = "#rrggbb"` rules for file types.
    pub file_types: BTreeMap<String, String>,
}

impl Theme {
    /// A theme that only names its base.
    pub fn with_base(name: impl Into<String>, base: impl Into<String>) -> Theme {
        Theme {
            meta: Meta {
                name: name.into(),
                base: base.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// A theme that spells out every role (used for the shipped themes and the
    /// editor's "save as").
    pub fn complete(name: impl Into<String>, base: impl Into<String>, roles: &Resolved) -> Theme {
        let mut theme = Theme::with_base(name, base);
        for role in Role::ALL {
            theme
                .colors
                .insert(role.name().to_string(), roles.get(role).to_hex());
        }
        theme
    }

    /// Parse and validate a theme file.
    pub fn parse(text: &str) -> Result<Theme, String> {
        let theme: Theme = toml::from_str(text).map_err(|e| format!("invalid theme: {e}"))?;
        theme.validate()?;
        Ok(theme)
    }

    /// Check every role name and colour so a typo cannot silently do nothing.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in &self.colors {
            if Role::from_name(name).is_none() {
                return Err(format!(
                    "unknown colour role “{name}” (known roles: {})",
                    Role::ALL
                        .iter()
                        .map(|role| role.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            Rgb::parse(value)?;
        }
        for (mask, value) in &self.file_types {
            if mask.trim().is_empty() {
                return Err("a file-type rule needs a mask".to_string());
            }
            Rgb::parse(value)?;
        }
        if !self.meta.base.trim().is_empty() {
            let base = self.meta.base.to_ascii_lowercase();
            let known = matches!(base.as_str(), "light" | "dark" | "high-contrast");
            if !known && Rgb::parse(&base).is_ok() {
                return Err("the base of a theme is a theme name, not a colour".to_string());
            }
        }
        Ok(())
    }

    /// Write the theme as TOML.
    pub fn to_text(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Save next to the other themes.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_text())
    }

    /// Load and validate one file.
    pub fn load(path: &Path) -> Result<Theme, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Theme::parse(&text)
    }

    /// The name this theme is known by: `[meta] name`, else the file stem.
    pub fn display_name(&self, path: &Path) -> String {
        if self.meta.name.trim().is_empty() {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "theme".to_string())
        } else {
            self.meta.name.trim().to_string()
        }
    }

    /// Resolve every role against `base`, which has to be fully known.
    pub fn resolve(&self, base: &Resolved) -> Resolved {
        let mut resolved = base.clone();
        for (name, value) in &self.colors {
            if let Some(role) = Role::from_name(name)
                && let Ok(colour) = Rgb::parse(value)
            {
                resolved.set(role, colour);
            }
        }
        resolved.file_types = self.file_types.clone();
        resolved
    }
}

/// Every role with a concrete colour: the result of resolving a theme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    roles: BTreeMap<Role, Rgb>,
    /// Per-file-type colours, most specific mask wins (evaluated by the caller).
    pub file_types: BTreeMap<String, String>,
}

impl Resolved {
    /// Build from a role table; every role must be present.
    pub fn from_roles(roles: BTreeMap<Role, Rgb>) -> Resolved {
        Resolved {
            roles,
            file_types: BTreeMap::new(),
        }
    }

    pub fn get(&self, role: Role) -> Rgb {
        self.roles.get(&role).copied().unwrap_or(Rgb::new(0, 0, 0))
    }

    pub fn set(&mut self, role: Role, colour: Rgb) {
        self.roles.insert(role, colour);
    }

    /// Set from a hex string, ignoring invalid input (the theme already validated).
    pub fn set_hex(&mut self, role: Role, text: &str) {
        if let Ok(colour) = Rgb::parse(text) {
            self.roles.insert(role, colour);
        }
    }

    /// True when every role has a colour.
    pub fn is_complete(&self) -> bool {
        Role::ALL.iter().all(|role| self.roles.contains_key(role))
    }

    /// Contrast problems worth warning about, as `(role, role, ratio)` triples.
    pub fn contrast_warnings(&self) -> Vec<(Role, Role, f32)> {
        let pairs = [
            (Role::Foreground, Role::Background),
            (Role::CursorForeground, Role::CursorBackground),
            (Role::SelectedForeground, Role::SelectedBackground),
            (Role::Foreground, Role::RowAlternate),
            (Role::Hidden, Role::Background),
            (Role::Accent, Role::HeaderBackground),
        ];
        let mut warnings = Vec::new();
        for (front, back) in pairs {
            let ratio = self.get(front).contrast(self.get(back));
            if ratio < 3.0 {
                warnings.push((front, back, ratio));
            }
        }
        warnings
    }

    /// The classic light palette.
    pub fn light() -> Resolved {
        let mut resolved = Resolved {
            roles: BTreeMap::new(),
            file_types: BTreeMap::new(),
        };
        for (role, colour) in [
            (Role::Background, Rgb::new(0xff, 0xff, 0xff)),
            (Role::Foreground, Rgb::new(0x00, 0x00, 0x00)),
            (Role::CursorBackground, Rgb::new(0x00, 0x00, 0x80)),
            (Role::CursorForeground, Rgb::new(0xff, 0xff, 0xff)),
            (Role::SelectedBackground, Rgb::new(0xc8, 0x10, 0x10)),
            (Role::SelectedForeground, Rgb::new(0xff, 0xff, 0xff)),
            (Role::Directory, Rgb::new(0x00, 0x00, 0x80)),
            (Role::Hidden, Rgb::new(0x80, 0x80, 0x80)),
            (Role::Readonly, Rgb::new(0xa0, 0x00, 0x00)),
            (Role::Archive, Rgb::new(0x00, 0x70, 0x00)),
            (Role::Symlink, Rgb::new(0x00, 0x80, 0x80)),
            (Role::HeaderBackground, Rgb::new(0xd4, 0xd0, 0xc8)),
            (Role::PanelBorder, Rgb::new(0x80, 0x80, 0x80)),
            (Role::StatusBackground, Rgb::new(0xd4, 0xd0, 0xc8)),
            (Role::Accent, Rgb::new(0x00, 0x00, 0x80)),
            (Role::RowAlternate, Rgb::new(0xf0, 0xf2, 0xf8)),
            (Role::Warning, Rgb::new(0xb0, 0x20, 0x20)),
            // Chosen for contrast on white: keywords blue, strings green, comments grey.
            (Role::CodeKeyword, Rgb::new(0x00, 0x00, 0xa0)),
            (Role::CodeString, Rgb::new(0x00, 0x70, 0x30)),
            (Role::CodeComment, Rgb::new(0x70, 0x70, 0x70)),
            (Role::CodeNumber, Rgb::new(0xa0, 0x40, 0x00)),
            (Role::CodeType, Rgb::new(0x60, 0x20, 0x90)),
            (Role::CodeFunction, Rgb::new(0x00, 0x50, 0x80)),
        ] {
            resolved.set(role, colour);
        }
        resolved
    }

    /// The dark palette.
    pub fn dark() -> Resolved {
        let mut resolved = Resolved {
            roles: BTreeMap::new(),
            file_types: BTreeMap::new(),
        };
        for (role, colour) in [
            (Role::Background, Rgb::new(0x1e, 0x1e, 0x1e)),
            (Role::Foreground, Rgb::new(0xd8, 0xd8, 0xd8)),
            (Role::CursorBackground, Rgb::new(0x30, 0x50, 0x80)),
            (Role::CursorForeground, Rgb::new(0xff, 0xff, 0xff)),
            (Role::SelectedBackground, Rgb::new(0x80, 0x30, 0x30)),
            (Role::SelectedForeground, Rgb::new(0xff, 0xff, 0xff)),
            (Role::Directory, Rgb::new(0x9c, 0xd0, 0xff)),
            (Role::Hidden, Rgb::new(0x88, 0x88, 0x88)),
            (Role::Readonly, Rgb::new(0xff, 0x90, 0x90)),
            (Role::Archive, Rgb::new(0x90, 0xd0, 0x90)),
            (Role::Symlink, Rgb::new(0x80, 0xd0, 0xd0)),
            (Role::HeaderBackground, Rgb::new(0x2b, 0x2b, 0x2b)),
            (Role::PanelBorder, Rgb::new(0x55, 0x55, 0x55)),
            (Role::StatusBackground, Rgb::new(0x2b, 0x2b, 0x2b)),
            (Role::Accent, Rgb::new(0x9c, 0xd0, 0xff)),
            (Role::RowAlternate, Rgb::new(0x27, 0x27, 0x2b)),
            (Role::Warning, Rgb::new(0xff, 0x9a, 0x8a)),
            // The same hues, lifted so they read on the dark background.
            (Role::CodeKeyword, Rgb::new(0x8a, 0xb4, 0xff)),
            (Role::CodeString, Rgb::new(0x9c, 0xd0, 0x90)),
            (Role::CodeComment, Rgb::new(0x8a, 0x8a, 0x8a)),
            (Role::CodeNumber, Rgb::new(0xff, 0xb0, 0x70)),
            (Role::CodeType, Rgb::new(0xd0, 0xa0, 0xff)),
            (Role::CodeFunction, Rgb::new(0x80, 0xc8, 0xe0)),
        ] {
            resolved.set(role, colour);
        }
        resolved
    }

    /// The built-in base palettes by name.
    pub fn builtin(name: &str) -> Option<Resolved> {
        match name.trim().to_ascii_lowercase().as_str() {
            "light" | "" => Some(Resolved::light()),
            "dark" => Some(Resolved::dark()),
            // The shipped high-contrast theme is a file; its base is light.
            "high-contrast" => Some(Resolved::light()),
            _ => None,
        }
    }

    /// True when the theme reads better on a dark background.
    pub fn is_dark(&self) -> bool {
        self.get(Role::Background).luminance() < 128.0
    }
}

/// Every theme found in a directory, with the built-ins always available.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeSet {
    /// `name -> theme`, sorted by name.
    pub themes: BTreeMap<String, Theme>,
    /// Files that could not be read, as `path: message`.
    pub problems: Vec<String>,
}

impl ThemeSet {
    /// Built-ins only; used when no theme directory exists.
    pub fn with_builtins() -> ThemeSet {
        let mut set = ThemeSet::default();
        set.themes.insert(
            "dark".to_string(),
            Theme::complete("dark", "dark", &Resolved::dark()),
        );
        set.themes.insert(
            "light".to_string(),
            Theme::complete("light", "light", &Resolved::light()),
        );
        set
    }

    pub fn names(&self) -> Vec<String> {
        self.themes.keys().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        let wanted = name.trim().to_ascii_lowercase();
        self.themes
            .iter()
            .find(|(key, theme)| {
                key.to_ascii_lowercase() == wanted || theme.meta.name.eq_ignore_ascii_case(name)
            })
            .map(|(_, theme)| theme)
    }

    /// Load every `*.toml` in `dir`, sorted by file name so duplicates resolve the
    /// same way on every platform (the first file wins and the duplicate is
    /// reported).
    pub fn load_dir(dir: &Path) -> ThemeSet {
        let mut set = ThemeSet::with_builtins();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return set;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .map(|ext| ext.eq_ignore_ascii_case("toml"))
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();
        for path in paths {
            match Theme::load(&path) {
                Ok(theme) => {
                    let name = theme.display_name(&path).to_ascii_lowercase();
                    if set.themes.contains_key(&name) && name != "light" && name != "dark" {
                        set.problems.push(format!(
                            "{}: a theme named “{name}” already exists",
                            path.display()
                        ));
                        continue;
                    }
                    if name == "light" || name == "dark" {
                        // A file may refine a built-in theme: it wins.
                        set.themes.insert(name, theme);
                    } else {
                        set.themes.insert(name, theme);
                    }
                }
                Err(message) => set.problems.push(format!("{}: {message}", path.display())),
            }
        }
        set
    }

    /// Resolve `name` to a full palette, following one level of `base` inheritance
    /// (and further if the base is itself a theme in the set).
    pub fn resolve(&self, name: &str) -> Resolved {
        self.resolve_depth(name, 0)
    }

    fn resolve_depth(&self, name: &str, depth: usize) -> Resolved {
        if depth > 8 {
            return Resolved::light();
        }
        let Some(theme) = self.get(name) else {
            return Resolved::builtin(name).unwrap_or_else(Resolved::light);
        };
        let base_name = if theme.meta.base.trim().is_empty() {
            "light".to_string()
        } else {
            theme.meta.base.trim().to_string()
        };
        let base = match Resolved::builtin(&base_name) {
            Some(builtin) => {
                // A theme with the same name as the built-in base replaces it, so
                // resolving its own name through the base would recurse; use the
                // built-in in that case.
                if self.get(&base_name).is_some() && !base_name.eq_ignore_ascii_case(name) {
                    self.resolve_depth(&base_name, depth + 1)
                } else {
                    builtin
                }
            }
            None => self.resolve_depth(&base_name, depth + 1),
        };
        theme.resolve(&base)
    }

    /// Write a theme into `dir` under `name`, replacing an existing file of that
    /// name.
    pub fn store(&mut self, dir: &Path, name: &str, theme: &Theme) -> Result<PathBuf, String> {
        let file = theme_file_name(name);
        let path = dir.join(file);
        theme
            .save(&path)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        self.themes.insert(name.to_ascii_lowercase(), theme.clone());
        Ok(path)
    }

    /// Delete a theme file; the built-ins cannot be removed.
    pub fn remove(&mut self, dir: &Path, name: &str) -> Result<bool, String> {
        let key = name.to_ascii_lowercase();
        if key == "light" || key == "dark" {
            return Err("the built-in themes cannot be deleted".to_string());
        }
        let path = dir.join(theme_file_name(name));
        let existed = path.is_file();
        if existed {
            std::fs::remove_file(&path)
                .map_err(|e| format!("cannot delete {}: {e}", path.display()))?;
        }
        self.themes.remove(&key);
        Ok(existed)
    }
}

/// File name for a theme: lower-case, spaces and slashes replaced by `-`.
pub fn theme_file_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => ch,
            _ => '-',
        })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() {
        "theme.toml".to_string()
    } else {
        format!("{cleaned}.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rc-theme-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn colours_parse_in_every_accepted_form() {
        assert_eq!(Rgb::parse("#ff8000").unwrap(), Rgb::new(255, 128, 0));
        assert_eq!(Rgb::parse("ff8000").unwrap(), Rgb::new(255, 128, 0));
        assert_eq!(Rgb::parse("#f80").unwrap(), Rgb::new(255, 136, 0));
        assert_eq!(Rgb::parse(" #FFF ").unwrap(), Rgb::new(255, 255, 255));
        assert_eq!(Rgb::new(1, 2, 3).to_hex(), "#010203");
        assert!(Rgb::parse("#gg0000").is_err());
        assert!(Rgb::parse("#ff00").is_err());
        assert!(Rgb::parse("").is_err());
        assert!(Rgb::parse("blue").is_err());
    }

    #[test]
    fn a_theme_file_only_names_the_roles_it_changes() {
        let text = r##"
[meta]
name = "Sunset"
base = "light"
author = "me"

[colors]
background = "#fff0e0"
directory = "#a03000"
"##;
        let theme = Theme::parse(text).unwrap();
        assert_eq!(theme.meta.name, "Sunset");
        assert_eq!(theme.meta.base, "light");
        let resolved = theme.resolve(&Resolved::light());
        assert_eq!(resolved.get(Role::Background), Rgb::new(0xff, 0xf0, 0xe0));
        assert_eq!(resolved.get(Role::Directory), Rgb::new(0xa0, 0x30, 0x00));
        // Everything else is inherited, so nothing is left black by accident.
        assert_eq!(
            resolved.get(Role::Foreground),
            Resolved::light().get(Role::Foreground)
        );
        assert_eq!(
            resolved.get(Role::CursorBackground),
            Resolved::light().get(Role::CursorBackground)
        );
        assert!(resolved.is_complete());
    }

    #[test]
    fn unknown_roles_and_bad_colours_are_rejected() {
        let unknown = Theme::parse("[meta]\nname='x'\n[colors]\nbackgrund='#fff'\n");
        let message = unknown.unwrap_err();
        assert!(
            message.contains("backgrund"),
            "message names the role: {message}"
        );
        assert!(
            message.contains("background"),
            "message lists known roles: {message}"
        );

        let bad = Theme::parse("[meta]\nname='x'\n[colors]\nbackground='rgb(1,2,3)'\n");
        assert!(bad.is_err());

        let bad_type = Theme::parse("[meta]\nname='x'\n[file_types]\n'*.rs'='nope'\n");
        assert!(bad_type.is_err());

        let empty_mask = Theme::parse("[meta]\nname='x'\n[file_types]\n' '='#ff0000'\n");
        assert!(empty_mask.is_err());

        // A base must be a name, not a colour.
        let colour_base = Theme::parse("[meta]\nname='x'\nbase='#ffffff'\n");
        assert!(colour_base.is_err());
    }

    #[test]
    fn a_shipped_style_theme_round_trips_through_a_file() {
        let dir = temp_dir("roundtrip");
        let theme = Theme::complete("Solar", "light", &Resolved::light());
        assert_eq!(theme.colors.len(), Role::ALL.len());
        let path = dir.join("solar.toml");
        theme.save(&path).unwrap();
        let loaded = Theme::load(&path).unwrap();
        assert_eq!(loaded, theme);
        assert_eq!(
            loaded.resolve(&Resolved::light()).get(Role::Accent),
            Resolved::light().get(Role::Accent)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_builtin_palettes_are_complete_and_legible() {
        for (name, resolved) in [("light", Resolved::light()), ("dark", Resolved::dark())] {
            assert!(resolved.is_complete(), "{name} has every role");
            let warnings = resolved.contrast_warnings();
            assert!(
                warnings.is_empty(),
                "{name} has unreadable pairs: {warnings:?}"
            );
            assert_eq!(resolved.is_dark(), name == "dark");
        }
        assert_eq!(Resolved::builtin("light").unwrap(), Resolved::light());
        assert!(Resolved::builtin("nonsense").is_none());
    }

    #[test]
    fn contrast_warnings_find_unreadable_pairs() {
        let mut resolved = Resolved::light();
        resolved.set(Role::Foreground, Rgb::new(0xfe, 0xfe, 0xfe));
        let warnings = resolved.contrast_warnings();
        assert!(
            warnings
                .iter()
                .any(|(front, back, _)| *front == Role::Foreground && *back == Role::Background),
            "white text on white has to be reported: {warnings:?}"
        );
        assert!(Rgb::new(0, 0, 0).contrast(Rgb::new(255, 255, 255)) > 20.0);
        assert!(Rgb::new(0, 0, 0).contrast(Rgb::new(0, 0, 0)) < 1.1);
    }

    #[test]
    fn a_directory_of_themes_is_discovered_and_duplicates_resolve_deterministically() {
        let dir = temp_dir("discover");
        std::fs::write(
            dir.join("a-harbour.toml"),
            "[meta]\nname = 'Harbour'\nbase = 'light'\n[colors]\naccent = '#005080'\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("b-harbour.toml"),
            "[meta]\nname = 'Harbour'\nbase = 'light'\n[colors]\naccent = '#500080'\n",
        )
        .unwrap();
        std::fs::write(dir.join("broken.toml"), "[meta\nname='x'").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a theme").unwrap();

        let set = ThemeSet::load_dir(&dir);
        assert!(set.get("harbour").is_some(), "the first file wins");
        assert_eq!(
            set.resolve("harbour").get(Role::Accent),
            Rgb::new(0x00, 0x50, 0x80),
            "the alphabetically first file is the one that loads"
        );
        assert_eq!(set.problems.len(), 2, "one duplicate and one broken file");
        assert!(set.get("light").is_some(), "built-ins stay available");
        // Loading the same directory twice gives the same answer.
        let again = ThemeSet::load_dir(&dir);
        assert_eq!(set.resolve("harbour"), again.resolve("harbour"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_theme_can_inherit_from_another_theme_and_from_dark() {
        let dir = temp_dir("inherit");
        std::fs::write(
            dir.join("base.toml"),
            "[meta]\nname='Base'\nbase='dark'\n[colors]\nbackground='#101820'\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("child.toml"),
            "[meta]\nname='Child'\nbase='Base'\n[colors]\naccent='#ff0000'\n",
        )
        .unwrap();
        let set = ThemeSet::load_dir(&dir);
        let child = set.resolve("child");
        assert_eq!(
            child.get(Role::Background),
            Rgb::new(0x10, 0x18, 0x20),
            "inherited through the chain"
        );
        assert_eq!(child.get(Role::Accent), Rgb::new(0xff, 0x00, 0x00));
        assert_eq!(
            child.get(Role::Directory),
            Resolved::dark().get(Role::Directory)
        );
        assert!(child.is_complete());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_theme_can_override_a_builtin_without_recursing() {
        let dir = temp_dir("override");
        std::fs::write(
            dir.join("light.toml"),
            "[meta]\nname='light'\nbase='light'\n[colors]\nbackground='#fefefe'\n",
        )
        .unwrap();
        let set = ThemeSet::load_dir(&dir);
        let resolved = set.resolve("light");
        assert_eq!(resolved.get(Role::Background), Rgb::new(0xfe, 0xfe, 0xfe));
        assert_eq!(
            resolved.get(Role::Foreground),
            Resolved::light().get(Role::Foreground)
        );
        assert!(resolved.is_complete());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn storing_and_removing_themes_respects_the_builtins() {
        let dir = temp_dir("store");
        let mut set = ThemeSet::load_dir(&dir);
        let theme = Theme::complete("My Theme", "light", &Resolved::dark());
        let path = set.store(&dir, "My Theme", &theme).unwrap();
        assert_eq!(path.file_name().unwrap(), "my-theme.toml");
        assert!(path.is_file());
        assert_eq!(
            set.resolve("my theme").get(Role::Background),
            Resolved::dark().get(Role::Background)
        );

        assert!(set.remove(&dir, "my theme").unwrap());
        assert!(!path.exists());
        assert!(
            set.remove(&dir, "light").is_err(),
            "built-ins are protected"
        );
        assert!(set.remove(&dir, "never existed").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn theme_file_names_are_safe() {
        assert_eq!(theme_file_name("Solar"), "solar.toml");
        assert_eq!(theme_file_name("My Theme!"), "my-theme.toml");
        assert_eq!(theme_file_name("../etc/passwd"), "etc-passwd.toml");
        assert_eq!(theme_file_name("  "), "theme.toml");
    }

    #[test]
    fn a_set_without_a_directory_still_offers_the_builtins() {
        let set = ThemeSet::load_dir(Path::new("/nonexistent/themes"));
        assert_eq!(set.names(), vec!["dark".to_string(), "light".to_string()]);
        assert!(set.resolve("dark").is_dark());
        // An unknown name falls back to the light palette instead of failing.
        assert_eq!(set.resolve("no-such-theme"), Resolved::light());
    }
}

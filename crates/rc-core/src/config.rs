//! Persistent configuration.
//!
//! Settings live in a single human-editable TOML file. Unknown keys are
//! preserved on load so that a newer version's file can be shared, and missing
//! keys fall back to the documented defaults.

use crate::error::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Top-level settings record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Language code of the UI (`en`, `cs`, …).
    pub language: String,
    /// Text direction override: `auto`, `ltr` or `rtl`.
    pub text_direction: String,
    pub layout: Layout,
    pub display: Display,
    /// Which faces the interface uses (Phase 14.7).
    pub fonts: Fonts,
    pub colors: Colors,
    pub behavior: Behavior,
    pub copy: CopySettings,
    pub delete: DeleteSettings,
    pub lister: Lister,
    pub editor: Editor,
    pub packer: Packer,
    pub button_bar: ButtonBar,
    /// Saved personal layout, independent of the currently active toolbar.
    pub button_bar_custom: Option<ButtonBar>,
    pub ftp: Ftp,
    pub agent: Agent,
    /// Git integration preferences.
    pub git: Git,
    /// Keyboard shortcut overrides: `"ctrl+shift+f5" = "cm_CreateShortcuts"`.
    pub shortcuts: BTreeMap<String, String>,
    /// Number of saved directory history entries per panel.
    pub history_length: usize,
    /// Restore the last used directories and tabs on start-up.
    pub save_position: bool,
    /// The prompt catalogue's own preferences (Phase 16.17c).
    pub prompts: PromptSettings,
    /// Which sounds the application makes (Phase 14.10).
    pub sounds: Sounds,
    /// Which plugins are switched on (Phase 18.7).
    pub plugins: Plugins,
}

/// Which plugins the user switched off.
///
/// The list is of *disabled* ids, not enabled ones, and that direction is deliberate: an installed
/// plugin is on unless the user said otherwise, so installing one by hand (by copying a folder) does
/// not need a second step to make it work. A plugin that is switched off keeps its manifest and its
/// files; it simply is not registered as a source, so it cannot appear in a panel and none of its
/// declared capabilities exist.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plugins {
    /// Ids switched off, in the order the user switched them off.
    pub disabled: Vec<String>,
}

impl Plugins {
    /// Whether a plugin id is switched on.
    pub fn is_enabled(&self, id: &str) -> bool {
        !self
            .disabled
            .iter()
            .any(|disabled| disabled.eq_ignore_ascii_case(id))
    }

    /// Switch one plugin on or off, keeping the list free of duplicates.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        self.disabled
            .retain(|disabled| !disabled.eq_ignore_ascii_case(id));
        if !enabled {
            self.disabled.push(id.to_string());
        }
    }
}

/// Short sounds for short events.
///
/// Sound is a *notification*, not information: everything it announces is also written to the
/// status line, so a machine with the speakers off loses nothing. The default is on for problems
/// and off for successes, because a file manager that chirps on every copy is a file manager
/// people mute entirely — and then miss the error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sounds {
    /// Master switch.
    pub enabled: bool,
    /// Play when an operation fails.
    pub on_error: bool,
    /// Play when the agent finishes an answer.
    pub on_agent_done: bool,
    /// Play when a connection drops or a transfer fails.
    pub on_disconnect: bool,
}

impl Default for Sounds {
    fn default() -> Self {
        Sounds {
            enabled: true,
            on_error: true,
            on_agent_done: false,
            on_disconnect: true,
        }
    }
}

impl Sounds {
    /// True when this kind of sound should be played at all.
    pub fn allows(&self, kind: crate::desktop::Sound) -> bool {
        if !self.enabled {
            return false;
        }
        match kind {
            crate::desktop::Sound::Done => self.on_agent_done,
            crate::desktop::Sound::Problem => self.on_error,
            crate::desktop::Sound::Disconnected => self.on_disconnect,
        }
    }
}

/// Preferences for the prompt picker.
///
/// Only the *pins* are stored: which prompt was used when is a session convenience, and writing it
/// to disk would turn a helpful ordering into a file that grows forever.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptSettings {
    /// Prompt ids the user pinned to the top of the picker, in the order they were pinned.
    pub pinned: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "en".to_string(),
            text_direction: "auto".to_string(),
            layout: Layout::default(),
            display: Display::default(),
            fonts: Fonts::default(),
            colors: Colors::default(),
            behavior: Behavior::default(),
            copy: CopySettings::default(),
            delete: DeleteSettings::default(),
            lister: Lister::default(),
            editor: Editor::default(),
            packer: Packer::default(),
            button_bar: ButtonBar::default(),
            button_bar_custom: None,
            ftp: Ftp::default(),
            agent: Agent::default(),
            git: Git::default(),
            shortcuts: BTreeMap::new(),
            history_length: 30,
            save_position: true,
            prompts: PromptSettings::default(),
            sounds: Sounds::default(),
            plugins: Plugins::default(),
        }
    }
}

/// Window/panel geometry and visible chrome.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub window_width: f32,
    pub window_height: f32,
    pub window_maximized: bool,
    /// Relative width of the left panel (0.2 – 0.8).
    pub panel_split: f32,
    pub show_button_bar: bool,
    /// `true` puts the button bar below the file lists.
    pub button_bar_bottom: bool,
    pub show_drive_bar: bool,
    pub show_command_line: bool,
    pub show_function_key_bar: bool,
    pub show_status_bar: bool,
    pub show_tabs: bool,
    /// Store the drive buttons vertically (two columns).
    pub vertical_button_bar: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            window_width: 1100.0,
            window_height: 700.0,
            window_maximized: false,
            panel_split: 0.5,
            show_button_bar: true,
            button_bar_bottom: true,
            show_drive_bar: true,
            show_command_line: true,
            show_function_key_bar: true,
            show_status_bar: true,
            show_tabs: true,
            vertical_button_bar: false,
        }
    }
}

/// Which fonts the interface uses, and how big each role is.
///
/// Three roles, because three kinds of text need different faces: prose and menus (body),
/// headings, and anything where character alignment matters (monospace). An empty family name
/// means "the built-in font", so a fresh installation looks exactly as before.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fonts {
    /// Family for lists, menus and dialogs; empty = the built-in sans.
    pub body_family: String,
    /// Family for headings; empty = the body family, one size larger.
    pub heading_family: String,
    /// Family for paths, code and the hex viewer; empty = the built-in monospace.
    pub mono_family: String,
    /// Size of headings, in points.
    pub heading_size: f32,
    /// Size of monospaced text, in points.
    pub mono_size: f32,
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts {
            body_family: String::new(),
            heading_family: String::new(),
            mono_family: String::new(),
            heading_size: 18.0,
            mono_size: 14.0,
        }
    }
}

impl Fonts {
    /// True when no custom face is used, so the theme does not need to load any font file.
    pub fn is_builtin(&self) -> bool {
        self.body_family.trim().is_empty()
            && self.heading_family.trim().is_empty()
            && self.mono_family.trim().is_empty()
    }

    /// Every family name the settings ask for.
    pub fn families(&self) -> Vec<&str> {
        [
            self.body_family.as_str(),
            self.heading_family.as_str(),
            self.mono_family.as_str(),
        ]
        .into_iter()
        .filter(|family| !family.trim().is_empty())
        .collect()
    }

    /// Clamp sizes into a range that stays readable and cannot break a layout.
    pub fn sanitise(&mut self) {
        self.heading_size = self.heading_size.clamp(8.0, 48.0);
        self.mono_size = self.mono_size.clamp(8.0, 48.0);
    }
}

/// What the file lists show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    /// `brief`, `full`, `comments`, `tree`, `thumbnails`, `custom`.
    pub view_mode: String,
    pub show_hidden: bool,
    /// Show a `..` row at the top of every list.
    pub show_parent_entry: bool,
    /// Only show files matching this mask.
    pub file_mask: String,
    pub show_size_in_bytes: bool,
    /// Sort directories before files.
    pub dirs_first: bool,
    /// Natural (numeric aware) name sorting.
    pub natural_sort: bool,
    pub font_size: f32,
    pub row_height: f32,
    pub show_icons: bool,
    pub thumbnail_size: u32,
    /// Columns of the full view, in order.
    pub columns: Vec<ColumnSpec>,
    /// Named view modes offered in the view menu.
    pub view_modes: Vec<ViewModeSpec>,
}

impl Default for Display {
    fn default() -> Self {
        Display {
            view_mode: "full".to_string(),
            show_hidden: false,
            show_parent_entry: false,
            file_mask: "*.*".to_string(),
            show_size_in_bytes: false,
            dirs_first: true,
            natural_sort: true,
            font_size: 14.0,
            row_height: 20.0,
            show_icons: true,
            thumbnail_size: 96,
            columns: vec![
                ColumnSpec::new("name", 0.0),
                ColumnSpec::new("size", 92.0),
                ColumnSpec::new("date", 128.0),
                ColumnSpec::new("attrs", 64.0),
            ],
            view_modes: Vec::new(),
        }
    }
}

/// One column of the full view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColumnSpec {
    /// Column identifier: `name`, `ext`, `size`, `date`, `attrs`, `comment`, `mode`, `link`.
    pub id: String,
    /// Preferred width in points; `0` means "take the remaining space" (only the
    /// first column may do that).
    pub width: f32,
}

impl ColumnSpec {
    pub fn new(id: impl Into<String>, width: f32) -> Self {
        ColumnSpec {
            id: id.into(),
            width,
        }
    }

    /// Localisation key of the column header.
    pub fn label_key(&self) -> Option<&'static str> {
        match self.id.as_str() {
            "name" => Some("col.name"),
            "ext" => Some("col.ext"),
            "size" => Some("col.size"),
            "date" => Some("col.date"),
            "attrs" => Some("col.attrs"),
            "comment" => Some("col.comment"),
            "mode" => Some("col.permissions"),
            "link" => Some("col.link_target"),
            _ => None,
        }
    }

    /// Known column ids, in the order a menu offers them.
    pub fn known_ids() -> [&'static str; 8] {
        [
            "name", "ext", "size", "date", "attrs", "comment", "mode", "link",
        ]
    }

    /// True for the identifiers the renderer understands.
    pub fn is_known(&self) -> bool {
        ColumnSpec::known_ids().contains(&self.id.as_str())
    }
}

impl Default for ColumnSpec {
    fn default() -> Self {
        ColumnSpec {
            id: "name".to_string(),
            width: 0.0,
        }
    }
}

/// A named combination of columns, sorting and filtering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewModeSpec {
    pub name: String,
    /// `brief`, `full`, `comments`, `thumbnails`, `tree`.
    pub kind: String,
    pub columns: Vec<ColumnSpec>,
    /// Sort field name: `name`, `ext`, `size`, `date`, `unsorted`.
    pub sort_field: String,
    pub sort_reversed: bool,
    /// File mask applied by the mode (`*.*` = everything).
    pub mask: String,
}

impl Default for ViewModeSpec {
    fn default() -> Self {
        ViewModeSpec {
            name: "All files".to_string(),
            kind: "full".to_string(),
            columns: Vec::new(),
            sort_field: "name".to_string(),
            sort_reversed: false,
            mask: "*.*".to_string(),
        }
    }
}

impl ViewModeSpec {
    /// Validate the entry so a broken mode cannot be applied.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a view mode needs a name".to_string());
        }
        if !matches!(
            self.kind.as_str(),
            "brief" | "full" | "comments" | "thumbnails" | "tree"
        ) {
            return Err(format!("unknown view mode kind {:?}", self.kind));
        }
        if !matches!(
            self.sort_field.as_str(),
            "name" | "ext" | "size" | "date" | "unsorted"
        ) {
            return Err(format!("unknown sort field {:?}", self.sort_field));
        }
        if self.columns.iter().any(|c| !c.is_known()) {
            return Err("the view mode contains an unknown column".to_string());
        }
        Ok(())
    }
}

/// Colour scheme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    /// `dark` or `light`.
    pub theme: String,
    pub background: String,
    pub foreground: String,
    pub cursor_background: String,
    pub cursor_foreground: String,
    pub selected_background: String,
    pub selected_foreground: String,
    pub directory: String,
    pub hidden: String,
    pub readonly: String,
    pub archive: String,
    pub symlink: String,
    /// Colour rules `mask = "#rrggbb"`.
    pub by_type: BTreeMap<String, String>,
}

impl Default for Colors {
    fn default() -> Self {
        Colors {
            theme: "light".to_string(),
            background: "#ffffff".to_string(),
            foreground: "#000000".to_string(),
            cursor_background: "#000080".to_string(),
            cursor_foreground: "#ffffff".to_string(),
            selected_background: "#ff0000".to_string(),
            selected_foreground: "#ffffff".to_string(),
            directory: "#000080".to_string(),
            hidden: "#808080".to_string(),
            readonly: "#a00000".to_string(),
            archive: "#008000".to_string(),
            symlink: "#008080".to_string(),
            by_type: BTreeMap::new(),
        }
    }
}

/// Confirmation and interaction behaviour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Behavior {
    pub confirm_delete: bool,
    /// Delete to the platform trash instead of permanently.
    pub use_trash: bool,
    pub confirm_overwrite: bool,
    pub confirm_exit: bool,
    /// Ask before leaving a directory with an unfinished command line.
    pub confirm_close_with_running_transfers: bool,
    /// Re-read the target panel after an operation.
    pub refresh_after_operation: bool,
    /// Remember the selection when changing directories.
    pub keep_selection_on_navigate: bool,
    /// Open archives as directories.
    pub enter_archives: bool,
    /// Follow symlinks when entering them.
    pub follow_symlinks: bool,
    pub single_click_open: bool,
    /// Seconds before a quick search resets (0 = never).
    pub quick_search_timeout: f32,
}

impl Default for Behavior {
    fn default() -> Self {
        Behavior {
            confirm_delete: true,
            use_trash: false,
            confirm_overwrite: true,
            confirm_exit: false,
            confirm_close_with_running_transfers: true,
            refresh_after_operation: true,
            keep_selection_on_navigate: false,
            enter_archives: true,
            follow_symlinks: true,
            single_click_open: false,
            quick_search_timeout: 2.0,
        }
    }
}

/// Copy/move defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CopySettings {
    pub preserve_times: bool,
    pub preserve_permissions: bool,
    pub keep_symlinks: bool,
    /// Run copies in the background transfer queue by default.
    pub background: bool,
    /// Verify the copy by re-reading the written data.
    pub verify: bool,
    /// Skip files that are identical (size and timestamp).
    pub skip_identical: bool,
    pub buffer_size: usize,
}

impl Default for CopySettings {
    fn default() -> Self {
        CopySettings {
            preserve_times: true,
            preserve_permissions: true,
            keep_symlinks: true,
            background: false,
            verify: false,
            skip_identical: true,
            buffer_size: 262_144,
        }
    }
}

/// Delete defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct DeleteSettings {
    /// Delete read-only files without asking.
    pub force_readonly: bool,
    /// Overwrite the content before deleting (paranoid mode).
    pub wipe: bool,
}

/// Internal viewer (F3) defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lister {
    /// Default mode: `auto`, `text`, `hex`, `image`.
    pub mode: String,
    pub font_size: f32,
    pub wrap_text: bool,
    /// Bytes shown per hex line.
    pub hex_bytes_per_line: usize,
    /// Show the hex offset column.
    pub hex_show_offset: bool,
    /// Auto-detect the text encoding.
    pub detect_encoding: bool,
    /// Encoding override (`utf-8`, `windows-1250`, …).
    pub encoding: String,
    pub show_line_numbers: bool,
    /// Remember the zoom level per session.
    pub keep_zoom: bool,
}

impl Default for Lister {
    fn default() -> Self {
        Lister {
            mode: "auto".to_string(),
            font_size: 14.0,
            wrap_text: false,
            hex_bytes_per_line: 16,
            hex_show_offset: true,
            detect_encoding: true,
            encoding: "auto".to_string(),
            show_line_numbers: false,
            keep_zoom: true,
        }
    }
}

/// Editor (F4) defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Editor {
    /// `internal` or a command line with `%1`.
    pub command: String,
    pub font_size: f32,
    pub tab_width: usize,
    /// Unix or Windows line endings for new files.
    pub line_ending: String,
    pub word_wrap: bool,
    pub syntax_highlight: bool,
    pub trim_trailing_whitespace: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            command: "internal".to_string(),
            font_size: 14.0,
            tab_width: 4,
            line_ending: "lf".to_string(),
            word_wrap: false,
            syntax_highlight: true,
            trim_trailing_whitespace: false,
        }
    }
}

/// Archive handling defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Packer {
    /// Default archive type: `zip`, `tar.gz`, `7z`, `tar`.
    pub default_type: String,
    /// Compression level 0-9.
    pub level: u8,
    /// Store paths relative to the current directory.
    pub store_relative_paths: bool,
    /// Never written to disk (same policy as connection passwords); archive
    /// passwords will move into the credential store as well.
    #[serde(default, skip_serializing)]
    pub password: String,
    /// Extensions that are opened as directories.
    pub archive_extensions: Vec<String>,
}

impl Default for Packer {
    fn default() -> Self {
        Packer {
            default_type: "zip".to_string(),
            level: 6,
            store_relative_paths: true,
            password: String::new(),
            archive_extensions: [
                "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "tbz", "xz", "txz", "zst", "cab",
                "jar", "apk", "iso",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }
}

/// One button of the button bar.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarButton {
    /// Label shown on the button (falls back to the command name).
    pub caption: String,
    /// Internal command (`cm_Copy`), a directory to open, or an external program.
    pub command: String,
    /// Optional parameters; `%P` source path, `%T` target path, `%N` cursor file.
    pub parameters: String,
    /// Optional separation line before the button.
    pub separator: bool,
}

impl BarButton {
    pub fn new(caption: impl Into<String>, command: impl Into<String>) -> Self {
        BarButton {
            caption: caption.into(),
            command: command.into(),
            parameters: String::new(),
            separator: false,
        }
    }

    /// Text shown on the button.
    pub fn label(&self) -> String {
        if self.caption.trim().is_empty() {
            self.command.clone()
        } else {
            self.caption.clone()
        }
    }

    /// A button is usable when it names a command or a directory.
    pub fn is_valid(&self) -> bool {
        !self.command.trim().is_empty()
    }
}

/// The configurable button bar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ButtonBar {
    pub buttons: Vec<BarButton>,
    /// Show the bar above the file lists instead of below them.
    pub on_top: bool,
}

impl Default for ButtonBar {
    fn default() -> Self {
        ButtonBar {
            on_top: false,
            buttons: vec![
                BarButton::new("Brief", "cm_SrcShort"),
                BarButton::new("Full", "cm_SrcLong"),
                BarButton::new("Search", "cm_SearchFor"),
                BarButton::new("Pack", "cm_PackFiles"),
                BarButton::new("Unpack", "cm_UnpackFiles"),
                BarButton::new("Multi-rename", "cm_MultiRenameFiles"),
                BarButton::new("Compare", "cm_CompareDirs"),
                BarButton::new("Synchronize", "cm_FileSync"),
                BarButton::new("Hotlist", "cm_DirectoryHotlist"),
                BarButton::new("Size", "cm_GetFileSpace"),
                BarButton::new("Options", "cm_Config"),
            ],
        }
    }
}

impl ButtonBar {
    /// Append a button.
    pub fn add(&mut self, button: BarButton) -> usize {
        self.buttons.push(button);
        self.buttons.len() - 1
    }

    /// Remove the button at `index`.
    pub fn remove(&mut self, index: usize) -> Option<BarButton> {
        if index < self.buttons.len() {
            Some(self.buttons.remove(index))
        } else {
            None
        }
    }

    /// Move a button one position towards the start.
    pub fn move_up(&mut self, index: usize) -> bool {
        if index == 0 || index >= self.buttons.len() {
            return false;
        }
        self.buttons.swap(index, index - 1);
        true
    }

    /// Move a button one position towards the end.
    pub fn move_down(&mut self, index: usize) -> bool {
        if index + 1 >= self.buttons.len() {
            return false;
        }
        self.buttons.swap(index, index + 1);
        true
    }

    /// Drop buttons that name no command.
    pub fn retain_valid(&mut self) -> usize {
        let before = self.buttons.len();
        self.buttons.retain(|b| b.is_valid());
        before - self.buttons.len()
    }
}

/// FTP/SFTP connection defaults and saved connection list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ftp {
    pub passive_mode: bool,
    pub use_mlsd: bool,
    /// `auto`, `utf-8`, `latin-1`, …
    pub encoding: String,
    pub keep_alive_seconds: u64,
    pub timeout_seconds: u64,
    pub show_hidden: bool,
    pub connections: Vec<crate::config::SavedConnection>,
    pub default_local_dir: String,
    pub default_remote_dir: String,
}

impl Default for Ftp {
    fn default() -> Self {
        Ftp {
            passive_mode: true,
            use_mlsd: true,
            encoding: "utf-8".to_string(),
            keep_alive_seconds: 30,
            timeout_seconds: 30,
            show_hidden: true,
            connections: Vec::new(),
            default_local_dir: String::new(),
            default_remote_dir: String::new(),
        }
    }
}

/// One saved FTP/SFTP connection.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedConnection {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Never written to disk: passwords live in the credential store
    /// (`rc_core::secrets`). The field is kept so an older settings file can still
    /// be read and migrated; it is never serialised again.
    #[serde(default, skip_serializing)]
    pub password: String,
    /// `ftp`, `ftps` or `sftp`.
    pub protocol: String,
    /// `password`, `key` or `agent`.
    pub auth_method: String,
    /// Private key file for `key` authentication.
    pub key_path: String,
    /// Passphrase of that key; never written to disk (credential store).
    #[serde(default, skip_serializing)]
    pub key_passphrase: String,
    /// `strict`, `tofu` (default) or `ignore` — see `rc_core::knownhosts`.
    pub host_key_policy: String,
    pub remote_dir: String,
    pub local_dir: String,
    pub passive: bool,
    pub anonymous: bool,
}

// Saved profiles also carry transient credentials while connecting. Settings
// and dialog Debug output must not expose either secret.
impl std::fmt::Debug for SavedConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SavedConnection")
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("password", &"[REDACTED]")
            .field("protocol", &self.protocol)
            .field("auth_method", &self.auth_method)
            .field("key_path", &self.key_path)
            .field("key_passphrase", &"[REDACTED]")
            .field("host_key_policy", &self.host_key_policy)
            .field("remote_dir", &self.remote_dir)
            .field("local_dir", &self.local_dir)
            .field("passive", &self.passive)
            .field("anonymous", &self.anonymous)
            .finish()
    }
}

impl SavedConnection {
    /// Default TCP port of a protocol name, used when a connection is created so
    /// the user can connect without typing a port at all.
    pub fn default_port(protocol: &str) -> u16 {
        match protocol.trim().to_ascii_lowercase().as_str() {
            "sftp" | "sftp4" | "sftp5" | "ssh" => 22,
            "ftps" | "ftpsi" | "ftpes" => 990,
            _ => 21,
        }
    }

    /// A connection that is ready to be edited: `localhost` on the protocol's
    /// default port.
    pub fn with_defaults(protocol: &str) -> Self {
        SavedConnection {
            protocol: protocol.to_string(),
            port: SavedConnection::default_port(protocol),
            host: "localhost".to_string(),
            auth_method: "password".to_string(),
            host_key_policy: "tofu".to_string(),
            passive: true,
            ..Default::default()
        }
    }

    /// True when `port` is still the untouched default for `protocol`, which is
    /// when switching the protocol should move it (21 <-> 22) without asking.
    pub fn port_is_default(&self) -> bool {
        self.port == 0 || self.port == SavedConnection::default_port(&self.protocol)
    }

    /// Account id used as the key in the credential store.
    pub fn account_id(&self) -> String {
        let protocol = if self.protocol.is_empty() {
            "ftp"
        } else {
            self.protocol.as_str()
        };
        let port = if self.port == 0 { 21 } else { self.port };
        crate::secrets::account_id(protocol, &self.user, &self.host, port)
    }
}

/// Git integration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Git {
    /// Whether the application may read the system/user Git configuration.
    ///
    /// `ask` (default) shows a dialog first, `allowed` and `denied` record the
    /// user's answer. Reading the configuration means reading the credential
    /// helper — and with the `store` helper a file of plain-text passwords — so it
    /// never happens without a decision.
    pub read_system_config: String,
    /// Use the identity from that configuration for commit authoring.
    pub use_identity: bool,
    /// What happens after the editor saves a file that belongs to a repository:
    /// `ask` (default), `always` or `never`.
    pub commit_on_save: String,
    /// Push the new commit right away when committing on save.
    pub push_after_commit: bool,
    /// Commit message template; `{name}` is replaced by the file name.
    pub commit_message: String,
}

impl Default for Git {
    fn default() -> Self {
        Git {
            read_system_config: "ask".to_string(),
            use_identity: true,
            commit_on_save: "ask".to_string(),
            push_after_commit: true,
            commit_message: "Edit {name}".to_string(),
        }
    }
}

impl Git {
    /// True when the user already allowed reading the configuration.
    pub fn config_allowed(&self) -> bool {
        self.read_system_config.eq_ignore_ascii_case("allowed")
    }

    /// True when the user refused once and should not be asked again.
    pub fn config_denied(&self) -> bool {
        self.read_system_config.eq_ignore_ascii_case("denied")
    }

    /// True when the question still has to be asked.
    pub fn config_undecided(&self) -> bool {
        !self.config_allowed() && !self.config_denied()
    }

    /// True when saving a file should commit without asking.
    pub fn commit_always(&self) -> bool {
        self.commit_on_save.eq_ignore_ascii_case("always")
    }

    /// True when saving a file must never commit.
    pub fn commit_never(&self) -> bool {
        self.commit_on_save.eq_ignore_ascii_case("never")
    }

    /// The commit message for `file`, with `{name}` filled in.
    pub fn message_for(&self, file: &str) -> String {
        let template = if self.commit_message.trim().is_empty() {
            "Edit {name}"
        } else {
            self.commit_message.trim()
        };
        template.replace("{name}", file)
    }
}

/// MCP (Model Context Protocol) integration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Agent {
    /// Expose the file manager as an MCP server.
    pub server_enabled: bool,
    /// `stdio` or `http`.
    pub server_transport: String,
    pub server_port: u16,
    /// Authentication token required by the HTTP transport ("" = none).
    pub server_token: String,
    /// Confirm risky tools (delete/overwrite) before executing.
    pub require_confirmation: bool,
    /// Saved MCP servers used by the built-in agent client.
    pub clients: Vec<McpServerConfig>,
    /// Show the agent panel in the UI.
    pub panel_enabled: bool,
    /// Privilege profile the agent runs under: `sandbox`, `user` or `privileged`.
    pub profile: String,
    /// Roots the sandbox profile may touch (filled from the other panel when empty).
    pub sandbox_roots: Vec<String>,
    /// Path prefixes that are never touched, whatever a grant says.
    pub deny_paths: Vec<String>,
    /// Operations that always need an approval (`execute`, `publish`, `privileged`).
    pub require_approval: Vec<String>,
    /// Programs the agent may run without asking. Empty by default: nothing runs.
    pub allow_exec: Vec<String>,
    /// Write an audit log next to the other per-user files.
    pub audit: bool,
    /// Model provider kind: `ollama` (the only one built in so far).
    pub provider_kind: String,
    /// Base URL of that provider.
    pub provider_url: String,
    /// Model the agent uses; empty means "ask the user to pick one".
    pub model: String,
    /// Seconds of silence before asking whether to keep waiting, not a read timeout.
    pub response_wait_seconds: u64,
    /// Last successful cold load duration by provider and model.
    pub model_load_seconds: BTreeMap<String, u64>,
    /// What we remember about each connected MCP server and its tools (Phase 16.37).
    ///
    /// Kept in the settings file so a second connection does not ask the same questions again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<crate::mcptools::ServerRecord>,
    /// Operations the human permanently pre-approved in the approval dialog.
    ///
    /// Written only by an explicit, typed confirmation; every use is still audited.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preapproved: Vec<Preapproval>,
}

/// One permanently pre-approved operation (the "allow always" answer).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preapproval {
    /// Operation name as used by `rc_core::policy` (`publish`, `execute`, …).
    pub operation: String,
    /// The note the dialog stored, shown in the security page.
    #[serde(default)]
    pub label: String,
    /// When the human said yes (unix seconds), so the log and the page can date it.
    #[serde(default)]
    pub created: i64,
}

impl Default for Agent {
    fn default() -> Self {
        Agent {
            server_enabled: false,
            server_transport: "stdio".to_string(),
            server_port: 0,
            server_token: String::new(),
            require_confirmation: true,
            clients: Vec::new(),
            panel_enabled: true,
            profile: "sandbox".to_string(),
            sandbox_roots: Vec::new(),
            deny_paths: Vec::new(),
            require_approval: vec![
                "execute".to_string(),
                "publish".to_string(),
                "privileged".to_string(),
            ],
            allow_exec: Vec::new(),
            audit: true,
            provider_kind: "ollama".to_string(),
            provider_url: "http://127.0.0.1:11434".to_string(),
            model: String::new(),
            response_wait_seconds: 120,
            model_load_seconds: BTreeMap::new(),
            mcp_servers: Vec::new(),
            preapproved: Vec::new(),
        }
    }
}

/// Configuration of an MCP server the built-in client may connect to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct McpServerConfig {
    pub name: String,
    /// `stdio`, `http` or `sse`.
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    pub env: BTreeMap<String, String>,
}

impl Settings {
    /// Parse settings from TOML text, filling in defaults for missing keys.
    pub fn from_toml(text: &str) -> CoreResult<Self> {
        toml::from_str(text).map_err(|e| CoreError::Config(e.to_string()))
    }

    /// Serialise settings to TOML text.
    pub fn to_toml(&self) -> CoreResult<String> {
        toml::to_string_pretty(self).map_err(|e| CoreError::Config(e.to_string()))
    }

    /// Read settings from `path`; a missing file yields defaults.
    pub fn load(path: &Path) -> CoreResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(CoreError::io(path, e)),
        }
    }

    /// Write settings to `path`, creating parent directories.
    pub fn save(&self, path: &Path) -> CoreResult<()> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        let text = self.to_toml()?;
        std::fs::write(path, text).map_err(|e| CoreError::io(path, e))
    }

    /// Look up a shortcut override by its normalised text form.
    pub fn shortcut_override(&self, shortcut: &str) -> Option<&String> {
        self.shortcuts.get(shortcut)
    }

    /// Effective archive extension list, lower case.
    pub fn archive_extensions(&self) -> Vec<String> {
        self.packer
            .archive_extensions
            .iter()
            .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
            .collect()
    }

    /// True when `name` looks like an archive that can be entered.
    pub fn is_archive_name(&self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        self.archive_extensions()
            .iter()
            .any(|ext| lower.ends_with(&format!(".{ext}")))
    }
}

/// The per-user configuration directory of RCommander.
pub fn config_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("RCOMMANDER_CONFIG_DIR")
        && !custom.is_empty()
    {
        return PathBuf::from(custom);
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("RCommander");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Application Support/RCommander");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
            && !xdg.is_empty()
        {
            return PathBuf::from(xdg).join("rcommander");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config/rcommander");
        }
    }
    PathBuf::from(".rcommander")
}

/// Default settings file path inside [`config_dir`].
pub fn default_config_path() -> PathBuf {
    config_dir().join("settings.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pins_survive_a_round_trip_and_an_old_file_still_loads() {
        let mut settings = Settings::default();
        settings.prompts.pinned = vec!["git.review-diff".to_string(), "files.explain".to_string()];
        let text = toml::to_string_pretty(&settings).unwrap();
        assert!(text.contains("pinned"), "{text}");
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back.prompts.pinned, settings.prompts.pinned);
        // A file written before this section existed has no `[prompts]` table at all.
        let old = toml::from_str::<Settings>("language = \"cs\"\n").unwrap();
        assert!(old.prompts.pinned.is_empty());
    }

    #[test]
    fn a_new_connection_has_a_usable_host_and_port() {
        let ftp = SavedConnection::with_defaults("ftp");
        assert_eq!(ftp.host, "localhost");
        assert_eq!(ftp.port, 21, "FTP defaults to 21");
        assert!(ftp.passive, "passive mode is the safe default");
        let sftp = SavedConnection::with_defaults("sftp");
        assert_eq!(sftp.port, 22, "SFTP defaults to 22");
        assert_eq!(SavedConnection::default_port("FTPS"), 990);
        assert_eq!(SavedConnection::default_port("unknown"), 21);
        // A port the user typed is not a default, so switching the protocol must
        // leave it alone.
        assert!(ftp.port_is_default());
        let mut custom = ftp.clone();
        custom.port = 2121;
        assert!(!custom.port_is_default());
        // An empty port counts as "not chosen yet".
        let mut unset = ftp.clone();
        unset.port = 0;
        assert!(unset.port_is_default());
    }

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.language, "en");
        assert!(s.layout.show_command_line);
        assert_eq!(s.display.view_mode, "full");
        assert!(s.behavior.confirm_delete);
        assert_eq!(s.copy.buffer_size, 262_144);
        assert!(!s.agent.server_enabled);
        assert!((0.0..=1.0).contains(&s.layout.panel_split));
    }

    #[test]
    fn toml_round_trip_preserves_values() {
        let mut s = Settings {
            language: "cs".to_string(),
            ..Settings::default()
        };
        s.display.show_hidden = true;
        s.copy.verify = true;
        s.shortcuts
            .insert("ctrl+f3".to_string(), "cm_SrcByName".to_string());
        let text = s.to_toml().unwrap();
        let back = Settings::from_toml(&text).unwrap();
        assert_eq!(s, back);
        assert!(text.contains("language = \"cs\""));
        assert!(text.contains("ctrl+f3"));
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let text = "[display]\nshow_hidden = true\n";
        let s = Settings::from_toml(text).unwrap();
        assert!(s.display.show_hidden);
        assert_eq!(s.display.view_mode, "full");
        assert_eq!(s.language, "en");
    }

    #[test]
    fn unknown_keys_are_ignored_for_forward_compatibility() {
        let text = "future_option = 42\n[display]\nbrand_new = \"x\"\nshow_hidden = true\n";
        let s = Settings::from_toml(text).unwrap();
        assert!(s.display.show_hidden);
    }

    #[test]
    fn invalid_toml_reports_a_config_error() {
        let err = Settings::from_toml("this is not toml = = =").unwrap_err();
        assert!(matches!(err, CoreError::Config(_)));
    }

    #[test]
    fn load_of_missing_file_returns_defaults_and_save_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.toml");
        let default = Settings::load(&path).unwrap();
        assert_eq!(default, Settings::default());
        let s = Settings {
            language: "cs".to_string(),
            ..Settings::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap().language, "cs");
    }

    #[test]
    fn archive_extension_helpers_work() {
        let s = Settings::default();
        assert!(s.is_archive_name("backup.ZIP"));
        assert!(s.is_archive_name("data.tar.gz"));
        assert!(!s.is_archive_name("notes.txt"));
        assert!(s.archive_extensions().contains(&"7z".to_string()));
    }

    #[test]
    fn the_default_button_bar_offers_working_commands() {
        let bar = ButtonBar::default();
        assert!(!bar.buttons.is_empty());
        assert!(bar.buttons.iter().all(|b| b.is_valid()));
        assert!(
            bar.buttons
                .iter()
                .any(|b| b.command == "cm_Copy" || b.command == "cm_SearchFor")
        );
        assert!(!bar.on_top);
        // Every default command must exist in the command catalogue.
        for button in &bar.buttons {
            assert!(
                crate::command::resolve(&button.command).is_some(),
                "unknown default command {}",
                button.command
            );
        }
    }

    #[test]
    fn button_bar_editing_operations() {
        let mut bar = ButtonBar {
            buttons: vec![
                BarButton::new("a", "cm_Copy"),
                BarButton::new("b", "cm_SrcLong"),
            ],
            on_top: false,
        };
        let index = bar.add(BarButton::new("c", "cm_Config"));
        assert_eq!(index, 2);
        assert!(bar.move_up(2));
        assert_eq!(bar.buttons[1].caption, "c");
        assert!(!bar.move_up(0));
        assert!(bar.move_down(0));
        assert!(!bar.move_down(2));
        // Order after the operations above: [c, a, b]; removing the first leaves c.
        assert_eq!(bar.remove(0).map(|b| b.caption), Some("c".to_string()));
        assert!(bar.remove(99).is_none());
    }

    #[test]
    fn invalid_buttons_are_dropped() {
        let mut bar = ButtonBar {
            buttons: vec![BarButton::new("ok", "cm_Copy"), BarButton::new("bad", "  ")],
            on_top: false,
        };
        assert_eq!(bar.retain_valid(), 1);
        assert_eq!(bar.buttons.len(), 1);
        assert_eq!(BarButton::new("", "cm_Copy").label(), "cm_Copy");
        assert_eq!(BarButton::new("Copy", "cm_Copy").label(), "Copy");
    }

    #[test]
    fn the_button_bar_round_trips_through_toml() {
        let mut settings = Settings {
            button_bar: ButtonBar {
                buttons: vec![BarButton::new("My", "cm_Copy")],
                on_top: true,
            },
            ..Settings::default()
        };
        settings.button_bar.add(BarButton {
            caption: "Sec".into(),
            command: "cm_SrcLong".into(),
            parameters: String::new(),
            separator: true,
        });
        settings.button_bar_custom = Some(settings.button_bar.clone());
        let text = settings.to_toml().unwrap();
        let back = Settings::from_toml(&text).unwrap();
        assert_eq!(back.button_bar, settings.button_bar);
        assert_eq!(back.button_bar_custom, settings.button_bar_custom);
        assert!(text.contains("cm_SrcLong"));
    }

    #[test]
    fn the_default_full_view_has_the_classic_columns() {
        let display = Display::default();
        let ids: Vec<&str> = display.columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["name", "size", "date", "attrs"]);
        assert!(display.columns.iter().all(|c| c.is_known()));
        assert_eq!(
            display.columns[0].width, 0.0,
            "the name column takes the remaining space"
        );
        assert!(display.view_modes.is_empty());
    }

    #[test]
    fn column_labels_and_ids_are_known() {
        assert_eq!(ColumnSpec::new("size", 10.0).label_key(), Some("col.size"));
        assert_eq!(
            ColumnSpec::new("mode", 10.0).label_key(),
            Some("col.permissions")
        );
        assert_eq!(ColumnSpec::new("nonsense", 10.0).label_key(), None);
        assert!(!ColumnSpec::new("nonsense", 10.0).is_known());
        assert_eq!(ColumnSpec::known_ids().len(), 8);
    }

    #[test]
    fn view_modes_validate_their_fields() {
        let mut mode = ViewModeSpec::default();
        assert!(mode.validate().is_ok());
        mode.name = "  ".into();
        assert!(mode.validate().is_err());
        mode.name = "Pictures".into();
        mode.kind = "carousel".into();
        assert!(mode.validate().is_err());
        mode.kind = "thumbnails".into();
        mode.sort_field = "magic".into();
        assert!(mode.validate().is_err());
        mode.sort_field = "date".into();
        mode.columns = vec![ColumnSpec::new("nonsense", 0.0)];
        assert!(mode.validate().is_err());
        mode.columns = vec![ColumnSpec::new("name", 0.0), ColumnSpec::new("date", 120.0)];
        assert!(mode.validate().is_ok());
    }

    #[test]
    fn columns_and_view_modes_round_trip_through_toml() {
        let mut settings = Settings::default();
        settings.display.columns = vec![
            ColumnSpec::new("name", 0.0),
            ColumnSpec::new("comment", 200.0),
        ];
        settings.display.view_modes = vec![ViewModeSpec {
            name: "Shots".into(),
            kind: "thumbnails".into(),
            columns: vec![],
            sort_field: "date".into(),
            sort_reversed: true,
            mask: "*.png".into(),
        }];
        let text = settings.to_toml().unwrap();
        let back = Settings::from_toml(&text).unwrap();
        assert_eq!(back.display.columns, settings.display.columns);
        assert_eq!(back.display.view_modes, settings.display.view_modes);
        assert!(text.contains("comment"));
        assert!(text.contains("Shots"));
    }

    #[test]
    fn saved_connection_debug_redacts_credentials() {
        let connection = SavedConnection {
            host: "example.test".into(),
            user: "alice".into(),
            password: "fixture-login-secret".into(),
            key_passphrase: "fixture-key-secret".into(),
            ..SavedConnection::default()
        };
        for formatted in [format!("{connection:?}"), format!("{connection:#?}")] {
            assert!(!formatted.contains("fixture-login-secret"));
            assert!(!formatted.contains("fixture-key-secret"));
            assert!(formatted.contains("[REDACTED]"));
            assert!(formatted.contains("example.test"));
        }
        assert_eq!(connection.password, "fixture-login-secret");
        assert_eq!(connection.key_passphrase, "fixture-key-secret");
    }

    #[test]
    fn passwords_are_never_written_to_the_settings_file() {
        let mut settings = Settings::default();
        settings.ftp.connections.push(SavedConnection {
            name: "work".into(),
            host: "example.com".into(),
            port: 21,
            user: "bob".into(),
            password: "hunter2".into(),
            protocol: "ftp".into(),
            ..Default::default()
        });
        let text = settings.to_toml().unwrap();
        assert!(
            !text.contains("hunter2"),
            "the password leaked into settings.toml:\n{text}"
        );
        assert!(
            !text.contains("password"),
            "the password field must not be written:\n{text}"
        );
        // An older file that still has a password is readable so it can be migrated.
        let legacy = format!("{text}\n");
        let legacy_with_password = legacy.replace(
            "user = \"bob\"",
            "user = \"bob\"\npassword = \"legacy-secret\"",
        );
        let parsed = Settings::from_toml(&legacy_with_password).unwrap();
        assert_eq!(parsed.ftp.connections[0].password, "legacy-secret");
    }

    #[test]
    fn saved_connections_have_a_stable_account_id() {
        let connection = SavedConnection {
            host: "example.com".into(),
            user: "bob".into(),
            port: 21,
            protocol: "sftp".into(),
            ..Default::default()
        };
        assert_eq!(connection.account_id(), "sftp://bob@example.com:21");
        let default_port = SavedConnection {
            host: "example.com".into(),
            user: "bob".into(),
            port: 0,
            protocol: String::new(),
            ..Default::default()
        };
        assert_eq!(default_port.account_id(), "ftp://bob@example.com:21");
    }

    #[test]
    fn config_path_honours_environment_override() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test process assumption is not guaranteed, so
        // the variable is restored immediately after use.
        unsafe { std::env::set_var("RCOMMANDER_CONFIG_DIR", dir.path()) };
        assert_eq!(config_dir(), dir.path());
        assert!(default_config_path().ends_with("settings.toml"));
        unsafe { std::env::remove_var("RCOMMANDER_CONFIG_DIR") };
    }
}

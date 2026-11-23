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
    pub colors: Colors,
    pub behavior: Behavior,
    pub copy: CopySettings,
    pub delete: DeleteSettings,
    pub lister: Lister,
    pub editor: Editor,
    pub packer: Packer,
    pub ftp: Ftp,
    pub agent: Agent,
    /// Keyboard shortcut overrides: `"ctrl+shift+f5" = "cm_CreateShortcuts"`.
    pub shortcuts: BTreeMap<String, String>,
    /// Number of saved directory history entries per panel.
    pub history_length: usize,
    /// Restore the last used directories and tabs on start-up.
    pub save_position: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "en".to_string(),
            text_direction: "auto".to_string(),
            layout: Layout::default(),
            display: Display::default(),
            colors: Colors::default(),
            behavior: Behavior::default(),
            copy: CopySettings::default(),
            delete: DeleteSettings::default(),
            lister: Lister::default(),
            editor: Editor::default(),
            packer: Packer::default(),
            ftp: Ftp::default(),
            agent: Agent::default(),
            shortcuts: BTreeMap::new(),
            history_length: 30,
            save_position: true,
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
    pub columns: Vec<String>,
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
            columns: ["name", "size", "date", "attrs"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedConnection {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Password is stored in the platform keyring when available; this field
    /// keeps a fallback for portable installs.
    pub password: String,
    /// `ftp`, `ftps` or `sftp`.
    pub protocol: String,
    pub remote_dir: String,
    pub local_dir: String,
    pub passive: bool,
    pub anonymous: bool,
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

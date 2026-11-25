//! The UI-free contract between the MCP server and the RCommander GUI.
//!
//! Nothing in this module touches egui, eframe or any windowing type: the
//! desktop front-end (or the Android front-end) implements [`FileManagerHost`]
//! and the server drives it through this trait. That keeps the whole MCP
//! protocol layer testable without a display.

use serde::{Deserialize, Serialize};

/// Which of the two file panels a request refers to.
///
/// RCommander is a dual-pane manager: every tool that inspects or moves data
/// may name a panel explicitly, and defaults to the active one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelSide {
    /// The left panel.
    Left,
    /// The right panel.
    Right,
}

impl PanelSide {
    /// Both panels, left first.
    pub const ALL: [PanelSide; 2] = [PanelSide::Left, PanelSide::Right];

    /// Parse the tool-argument spelling (`"left"`/`"right"`, or the single
    /// letter shorthand `"l"`/`"r"`), case-insensitively.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "left" | "l" => Some(PanelSide::Left),
            "right" | "r" => Some(PanelSide::Right),
            _ => None,
        }
    }

    /// The spelling used in tool arguments and tool output.
    pub fn as_str(self) -> &'static str {
        match self {
            PanelSide::Left => "left",
            PanelSide::Right => "right",
        }
    }

    /// The other panel.
    pub fn opposite(self) -> Self {
        match self {
            PanelSide::Left => PanelSide::Right,
            PanelSide::Right => PanelSide::Left,
        }
    }
}

impl std::fmt::Display for PanelSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a [`FileManagerHost::select`] call combines the mask with the current
/// selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectMode {
    /// Replace the selection with the matching entries.
    Set,
    /// Add the matching entries to the selection.
    Add,
    /// Remove the matching entries from the selection.
    Remove,
    /// Select everything that is currently unselected and vice versa.
    InvertAll,
}

impl SelectMode {
    /// Parse the tool-argument spelling, case-insensitively. `"invert"` and
    /// `"invertall"` are accepted as aliases of `"invert_all"`.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "set" | "replace" => Some(SelectMode::Set),
            "add" | "include" => Some(SelectMode::Add),
            "remove" | "exclude" => Some(SelectMode::Remove),
            "invert_all" | "invertall" | "invert" => Some(SelectMode::InvertAll),
            _ => None,
        }
    }

    /// The spelling used in tool arguments and tool output.
    pub fn as_str(self) -> &'static str {
        match self {
            SelectMode::Set => "set",
            SelectMode::Add => "add",
            SelectMode::Remove => "remove",
            SelectMode::InvertAll => "invert_all",
        }
    }
}

impl std::fmt::Display for SelectMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A snapshot of one panel, as reported by [`FileManagerHost::panels`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPanel {
    /// Which panel this is.
    pub side: PanelSide,
    /// Current directory of the panel.
    pub path: String,
    /// Whether this is the focused (active) panel.
    pub active: bool,
    /// View mode identifier, e.g. `"brief"`, `"full"` or `"thumbnails"`.
    pub view_mode: String,
    /// Number of selected entries.
    pub selected: usize,
    /// Number of entries in the listing, including `..`.
    pub entries: usize,
}

/// Outcome counters of a copy, move or delete operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostOpResult {
    /// Regular files that were processed.
    pub files: u64,
    /// Directories that were processed.
    pub dirs: u64,
    /// Total number of bytes transferred (files only).
    pub bytes: u64,
    /// Entries that were skipped, e.g. because the target existed and
    /// overwriting was not requested.
    pub skipped: u64,
}

impl HostOpResult {
    /// One-line human readable summary, used as tool output.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!(
            "{} file{}",
            self.files,
            if self.files == 1 { "" } else { "s" }
        ));
        if self.dirs > 0 {
            parts.push(format!(
                "{} director{}",
                self.dirs,
                if self.dirs == 1 { "y" } else { "ies" }
            ));
        }
        if self.bytes > 0 {
            parts.push(rc_core::format::format_size_bytes(self.bytes));
        }
        if self.skipped > 0 {
            parts.push(format!("{} skipped", self.skipped));
        }
        parts.join(", ")
    }
}

/// A recursive search request handed to [`FileManagerHost::search`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    /// Directory the search starts in.
    pub root: String,
    /// Name mask in [`rc_core::mask`] syntax (`*.txt;*.md`); `*` matches all.
    pub name_mask: String,
    /// Optional case-insensitive (or case-sensitive) substring a file has to
    /// contain to match. `None` searches by name only.
    pub content: Option<String>,
    /// Whether sub-directories are descended into.
    pub recursive: bool,
    /// Whether the name mask and the content needle are case sensitive.
    pub case_sensitive: bool,
    /// Upper bound on the number of returned paths.
    pub max_results: usize,
}

impl SearchRequest {
    /// A name-only, recursive search for `name_mask` below `root`.
    pub fn new(root: impl Into<String>, name_mask: impl Into<String>) -> Self {
        SearchRequest {
            root: root.into(),
            name_mask: name_mask.into(),
            content: None,
            recursive: true,
            case_sensitive: false,
            max_results: 200,
        }
    }
}

/// Everything the MCP server needs from the file manager, implemented by the
/// GUI (or by any other front-end, or by a test double).
///
/// All methods are synchronous: the MCP server is called from a background
/// thread, and the implementation is responsible for hopping to the UI thread
/// when it needs to touch widgets. Fallible methods return `Err(message)` with
/// a message that is shown to the requesting agent verbatim, so it should name
/// the offending path.
pub trait FileManagerHost: Send + Sync {
    /// Snapshot of both panels.
    fn panels(&self) -> Vec<HostPanel>;

    /// Snapshot of the focused panel.
    fn active_panel(&self) -> HostPanel;

    /// Entries of the active (or given) panel; `path` `None` = current dir.
    ///
    /// The returned vector should include the `..` pseudo-entry when the
    /// panel displays one; the server does not add it.
    fn list_directory(
        &self,
        panel: Option<PanelSide>,
        path: Option<&str>,
    ) -> Result<Vec<rc_core::entry::Entry>, String>;

    /// Navigate a panel to `path`.
    fn open_directory(&self, panel: PanelSide, path: &str) -> Result<(), String>;

    /// Full paths of the selected entries of a panel.
    fn selection(&self, panel: Option<PanelSide>) -> Result<Vec<String>, String>;

    /// Apply a mask to the selection and report how many entries are selected
    /// afterwards (or how many were affected, depending on the implementation;
    /// the server only reports the number).
    fn select(
        &self,
        panel: Option<PanelSide>,
        mask: &str,
        mode: SelectMode,
    ) -> Result<usize, String>;

    /// Copy (`move_it == false`) or move the sources into `dest`.
    fn copy(
        &self,
        sources: &[String],
        dest: &str,
        move_it: bool,
        overwrite: bool,
    ) -> Result<HostOpResult, String>;

    /// Delete the paths, preferentially into the platform trash.
    fn delete(&self, paths: &[String], use_trash: bool) -> Result<HostOpResult, String>;

    /// Create a directory (including missing parents).
    fn create_directory(&self, path: &str) -> Result<(), String>;

    /// Rename/move a single entry inside the same parent.
    fn rename(&self, from: &str, to: &str) -> Result<(), String>;

    /// Read at most `max_bytes` bytes of a text file.
    fn read_text(&self, path: &str, max_bytes: usize) -> Result<String, String>;

    /// Replace the content of a (text) file and report how many bytes were
    /// written.
    fn write_text(&self, path: &str, content: &str) -> Result<u64, String>;

    /// Free/total space of the volume holding `path`; `None` when the platform
    /// cannot report it.
    fn disk_space(&self, path: &str) -> Result<Option<rc_core::drive::Space>, String>;

    /// Recursive search; returns matching full paths.
    fn search(&self, request: &SearchRequest) -> Result<Vec<String>, String>;

    /// Help topics as `(id, title)` pairs.
    fn help_topics(&self) -> Vec<(String, String)>;

    /// Body of a help topic in a given language, `None` when unknown.
    fn help_body(&self, topic: &str, language: &str) -> Option<String>;

    /// Identifier of the language pack that is active right now.
    fn current_language(&self) -> String;

    /// Identifiers of every installed language pack.
    fn available_languages(&self) -> Vec<String>;
}

/// Approval gate for destructive tools.
///
/// The GUI registers an implementation; the server calls [`Confirmer::confirm`]
/// before running a tool whose [`crate::ToolDescriptor::requires_confirmation`]
/// is set. Implementations typically show a modal dialog and block until the
/// user answers, or return `false` immediately when the agent runs unattended.
pub trait Confirmer: Send + Sync {
    /// Return `true` only when the operation may proceed.
    fn confirm(&self, tool: &str, args: &serde_json::Value) -> bool;
}

/// One changed file as the Git tools report it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostGitFile {
    /// Path relative to the repository root.
    pub path: String,
    /// Two-letter status in the style of `git status --short` (`M `, ` M`, `??`, `!!`).
    pub codes: String,
    /// Human readable state (`modified`, `staged addition`, `ignored`, …).
    pub state: String,
}

/// Optional capability: read and change a Git repository.
///
/// Kept out of [`FileManagerHost`] on purpose: a front-end without a Git engine simply does
/// not register one, and the server then publishes none of the Git tools. Every mutating
/// method goes through the application's policy engine, so a tool call that the user has not
/// granted fails with an explanation instead of quietly doing the work.
pub trait GitHost: Send + Sync {
    /// Changed files of the repository containing `path` (`"M "`, `"??"`, …).
    fn git_status(&self, path: &str) -> Result<Vec<HostGitFile>, String>;

    /// The last `limit` commits as `(short id, summary, author)`.
    fn git_log(&self, path: &str, limit: usize) -> Result<Vec<(String, String, String)>, String>;

    /// Local branches and the current one.
    fn git_branches(&self, path: &str) -> Result<(Vec<String>, String), String>;

    /// Put `files` into the index; returns how many were staged.
    fn git_stage(&self, path: &str, files: &[String]) -> Result<usize, String>;

    /// Stage `files` (when given) and commit them; returns the short commit id.
    fn git_commit(&self, path: &str, message: &str, files: &[String]) -> Result<String, String>;

    /// Check out `branch`.
    fn git_checkout(&self, path: &str, branch: &str) -> Result<(), String>;

    /// Fetch and fast-forward the current branch; returns how many commits arrived.
    fn git_pull(&self, path: &str) -> Result<usize, String>;

    /// Push the current branch.
    fn git_push(&self, path: &str) -> Result<(), String>;
}

/// Optional capability: changing the UI language.
///
/// It is deliberately not part of [`FileManagerHost`]: a front-end that cannot
/// switch languages at runtime simply never registers one, and the server then
/// does not publish the `set_language` tool at all.
pub trait LanguageHost: Send + Sync {
    /// Switch to `language`, or explain why that is impossible.
    fn set_language(&self, language: &str) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_side_parses_names_and_letters() {
        assert_eq!(PanelSide::parse("LEFT"), Some(PanelSide::Left));
        assert_eq!(PanelSide::parse(" r "), Some(PanelSide::Right));
        assert_eq!(PanelSide::parse("middle"), None);
        assert_eq!(PanelSide::Left.opposite(), PanelSide::Right);
        assert_eq!(PanelSide::ALL.len(), 2);
        assert_eq!(PanelSide::Left.to_string(), "left");
    }

    #[test]
    fn select_mode_parses_aliases() {
        assert_eq!(SelectMode::parse("Set"), Some(SelectMode::Set));
        assert_eq!(SelectMode::parse("invert-all"), Some(SelectMode::InvertAll));
        assert_eq!(SelectMode::parse("invert"), Some(SelectMode::InvertAll));
        assert_eq!(SelectMode::parse("everything"), None);
        assert_eq!(SelectMode::InvertAll.as_str(), "invert_all");
        assert_eq!(SelectMode::Add.to_string(), "add");
    }

    #[test]
    fn op_result_summary_counts_units() {
        let r = HostOpResult {
            files: 2,
            dirs: 0,
            bytes: 2048,
            skipped: 0,
        };
        assert!(r.summary().contains("2 files"), "{}", r.summary());
        let d = HostOpResult {
            files: 0,
            dirs: 1,
            bytes: 0,
            skipped: 1,
        };
        let s = d.summary();
        assert!(s.contains("1 directory") && s.contains("1 skipped"), "{s}");
    }
}

//! Modal dialogs of the application.
//!
//! A dialog is plain state plus validation logic; the view layer renders it and
//! writes the field values back before calling the matching model action.

use crate::model::ViewerMode;
use rc_core::config::SavedConnection;
use rc_core::ops::OverwriteMode;

/// Every dialog the application can show.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Dialog {
    /// Copy (`F5`) / move (`F6`) with options.
    Transfer(TransferDialogState),
    /// Overwrite confirmation for one file.
    Overwrite(OverwriteDialogState),
    /// Create directory (`F7`).
    MakeDir(MakeDirState),
    /// Rename in place (`Shift+F6`).
    Rename(RenameState),
    /// Delete confirmation (`F8`).
    Delete(DeleteState),
    /// Search (`Alt+F7`).
    Search(SearchState),
    /// Pack (`Alt+F5`).
    Pack(PackState),
    /// Unpack (`Alt+F6`).
    Unpack(UnpackState),
    /// Multi-rename tool (`Ctrl+M`).
    MultiRename(MultiRenameState),
    /// Change attributes (`Files > Change Attributes`).
    Attributes(AttributesState),
    /// Edit the file comment (`Ctrl+Z`).
    Comment(CommentState),
    /// Directory hotlist (`Ctrl+D`).
    Hotlist(HotlistState),
    /// Options (`Configuration > Options`), one tab at a time.
    Options(OptionsState),
    /// FTP quick connect (`Ctrl+F`).
    FtpConnect(FtpConnectState),
    /// Synchronise directories.
    Synchronize(SyncState),
    /// About box.
    About,
    /// Generic confirmation (used by the view layer).
    Confirm {
        title_key: String,
        message: String,
        action: ConfirmAction,
    },
    /// Progress of a running transfer (non-modal in spirit, but it blocks input).
    Progress(ProgressState),
}

/// Which tab of the options dialog is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsTab {
    Display,
    Language,
    Colors,
    Editor,
    Packer,
    Ftp,
    Agent,
    Shortcuts,
}

impl OptionsTab {
    pub fn all() -> [OptionsTab; 8] {
        [
            OptionsTab::Display,
            OptionsTab::Language,
            OptionsTab::Colors,
            OptionsTab::Editor,
            OptionsTab::Packer,
            OptionsTab::Ftp,
            OptionsTab::Agent,
            OptionsTab::Shortcuts,
        ]
    }

    pub fn key(self) -> &'static str {
        match self {
            OptionsTab::Display => "dialog.config.display",
            OptionsTab::Language => "dialog.config.language",
            OptionsTab::Colors => "dialog.config.colors",
            OptionsTab::Editor => "dialog.config.editor",
            OptionsTab::Packer => "dialog.config.packer",
            OptionsTab::Ftp => "dialog.config.ftp",
            OptionsTab::Agent => "dialog.config.agent",
            OptionsTab::Shortcuts => "dialog.config.shortcuts",
        }
    }
}

/// Copy/move dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferDialogState {
    pub move_it: bool,
    pub destination: String,
    pub sources: Vec<String>,
    /// Copy in the background queue.
    pub background: bool,
    pub verify: bool,
    pub preserve_times: bool,
    pub overwrite: OverwriteMode,
    /// Ask the user before overwriting (per file).
    pub ask_overwrite: bool,
}

impl TransferDialogState {
    pub fn new(move_it: bool, sources: Vec<String>, destination: impl Into<String>) -> Self {
        TransferDialogState {
            move_it,
            destination: destination.into(),
            sources,
            background: false,
            verify: false,
            preserve_times: true,
            overwrite: OverwriteMode::Fail,
            ask_overwrite: true,
        }
    }

    /// Validate the dialog before running it.
    pub fn validate(&self) -> Result<(), String> {
        if self.sources.is_empty() {
            return Err("no files selected".to_string());
        }
        if self.destination.trim().is_empty() {
            return Err("empty destination".to_string());
        }
        Ok(())
    }

    pub fn title_key(&self) -> &'static str {
        if self.move_it {
            "dialog.move.title"
        } else {
            "dialog.copy.title"
        }
    }

    /// Summary line: `3 files (1.2 MiB)`.
    pub fn summary(&self) -> String {
        format!("{} file(s)", self.sources.len())
    }
}

/// Overwrite question for one destination file.
#[derive(Clone, Debug, PartialEq)]
pub struct OverwriteDialogState {
    pub source: String,
    pub destination: String,
    pub source_size: u64,
    pub destination_size: u64,
    pub source_time: Option<i64>,
    pub destination_time: Option<i64>,
    /// Index of the file in the running batch and the batch size.
    pub index: usize,
    pub total: usize,
}

impl OverwriteDialogState {
    /// True when the new file is newer than the old one.
    pub fn source_is_newer(&self) -> bool {
        match (self.source_time, self.destination_time) {
            (Some(a), Some(b)) => a > b,
            _ => true,
        }
    }

    /// True when the sizes differ.
    pub fn size_differs(&self) -> bool {
        self.source_size != self.destination_size
    }
}

/// `F7` create-directory dialog.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MakeDirState {
    pub name: String,
    /// Create the directory in the target panel instead of the source.
    pub in_target_panel: bool,
}

impl MakeDirState {
    /// Validate the directory name: rejects empty names and path separators.
    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("empty name".to_string());
        }
        if name.contains('/') || name.contains('\\') {
            return Err("name must not contain a path separator".to_string());
        }
        if name == "." || name == ".." {
            return Err("reserved name".to_string());
        }
        Ok(())
    }
}

/// `Shift+F6` rename dialog.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RenameState {
    pub original: String,
    pub name: String,
    /// Rename the extension only.
    pub keep_extension: bool,
}

impl RenameState {
    pub fn new(original: impl Into<String>) -> Self {
        let original = original.into();
        RenameState {
            name: original.clone(),
            original,
            keep_extension: false,
        }
    }

    /// The effective new name, honouring "keep extension".
    pub fn effective_name(&self) -> String {
        if !self.keep_extension {
            return self.name.trim().to_string();
        }
        let new = self.name.trim();
        if let Some(dot) = self.original.rfind('.')
            && dot > 0
            && !new.contains('.')
        {
            return format!("{new}{}", &self.original[dot..]);
        }
        new.to_string()
    }

    pub fn validate(&self) -> Result<(), String> {
        let name = self.effective_name();
        if name.is_empty() {
            return Err("empty name".to_string());
        }
        if name.contains('/') || name.contains('\\') {
            return Err("name must not contain a path separator".to_string());
        }
        if name == self.original {
            return Err("name unchanged".to_string());
        }
        Ok(())
    }
}

/// Delete confirmation.
#[derive(Clone, Debug, PartialEq)]
pub struct DeleteState {
    pub paths: Vec<String>,
    /// Delete permanently instead of using the recycle bin.
    pub permanent: bool,
}

impl DeleteState {
    pub fn new(paths: Vec<String>, permanent: bool) -> Self {
        DeleteState { paths, permanent }
    }

    pub fn count(&self) -> usize {
        self.paths.len()
    }
}

/// Search dialog (`Alt+F7`).
#[derive(Clone, Debug, PartialEq)]
pub struct SearchState {
    pub root: String,
    pub name_mask: String,
    pub content: String,
    pub case_sensitive: bool,
    pub whole_words: bool,
    pub regex: bool,
    pub subdirs: bool,
    pub search_archives: bool,
    pub search_hidden: bool,
    pub follow_links: bool,
    pub max_results: usize,
    /// Results collected so far: (path, matched line).
    pub results: Vec<SearchHit>,
    pub running: bool,
    pub truncated: bool,
}

/// One search result row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub path: String,
    pub line: Option<String>,
    pub size: u64,
}

impl SearchState {
    pub fn new(root: impl Into<String>) -> Self {
        SearchState {
            root: root.into(),
            name_mask: "*.*".to_string(),
            content: String::new(),
            case_sensitive: false,
            whole_words: false,
            regex: false,
            subdirs: true,
            search_archives: false,
            search_hidden: false,
            follow_links: false,
            max_results: 10_000,
            results: Vec::new(),
            running: false,
            truncated: false,
        }
    }

    /// True when the search also looks inside file contents.
    pub fn is_content_search(&self) -> bool {
        !self.content.is_empty()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.root.trim().is_empty() {
            return Err("empty start directory".to_string());
        }
        if self.name_mask.trim().is_empty() && self.content.trim().is_empty() {
            return Err("nothing to search for".to_string());
        }
        Ok(())
    }
}

/// Pack dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct PackState {
    pub archive_name: String,
    /// `zip`, `tar.gz`, `7z`, `tar`.
    pub format: String,
    pub level: u8,
    pub password: String,
    pub sources: Vec<String>,
    pub delete_after: bool,
}

impl PackState {
    pub fn new(sources: Vec<String>, default_name: impl Into<String>) -> Self {
        PackState {
            archive_name: default_name.into(),
            format: "zip".to_string(),
            level: 6,
            password: String::new(),
            sources,
            delete_after: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.sources.is_empty() {
            return Err("no files selected".to_string());
        }
        if self.archive_name.trim().is_empty() {
            return Err("empty archive name".to_string());
        }
        if self.level > 9 {
            return Err("compression level must be 0-9".to_string());
        }
        Ok(())
    }
}

/// Unpack dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct UnpackState {
    pub archive: String,
    pub destination: String,
    /// Only these members (empty = everything).
    pub members: Vec<String>,
    pub password: String,
    pub extract_paths: bool,
}

impl UnpackState {
    pub fn new(archive: impl Into<String>, destination: impl Into<String>) -> Self {
        UnpackState {
            archive: archive.into(),
            destination: destination.into(),
            members: Vec::new(),
            password: String::new(),
            extract_paths: true,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.archive.trim().is_empty() {
            return Err("no archive selected".to_string());
        }
        if self.destination.trim().is_empty() {
            return Err("empty destination".to_string());
        }
        Ok(())
    }
}

/// Multi-rename tool state.
#[derive(Clone, Debug, PartialEq)]
pub struct MultiRenameState {
    /// Name pattern with the supported placeholders.
    pub pattern: String,
    /// Extension pattern (`[E]` keeps the original extension).
    pub extension_pattern: String,
    pub search_for: String,
    pub replace_with: String,
    pub use_regex: bool,
    pub counter_start: u64,
    pub counter_step: u64,
    pub counter_digits: usize,
    pub sources: Vec<String>,
    /// Rows previewed in the dialog: (original, new).
    pub preview: Vec<(String, String)>,
}

impl MultiRenameState {
    pub fn new(sources: Vec<String>) -> Self {
        MultiRenameState {
            pattern: "[N]".to_string(),
            extension_pattern: "[E]".to_string(),
            search_for: String::new(),
            replace_with: String::new(),
            use_regex: false,
            counter_start: 1,
            counter_step: 1,
            counter_digits: 0,
            sources,
            preview: Vec::new(),
        }
    }

    /// Recompute the preview rows.
    pub fn refresh_preview(&mut self) {
        self.preview = self
            .sources
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let counter = self.counter_start as i64 + self.counter_step as i64 * i as i64;
                let new = crate::model::commands::apply_rename_pattern(
                    name,
                    &self.pattern,
                    &self.extension_pattern,
                    counter.max(0) as u64,
                    self.counter_digits,
                    &self.search_for,
                    &self.replace_with,
                );
                (name.clone(), new)
            })
            .collect();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.sources.is_empty() {
            return Err("no files selected".to_string());
        }
        if self.pattern.trim().is_empty() {
            return Err("empty name pattern".to_string());
        }
        Ok(())
    }
}

/// Change-attributes dialog.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AttributesState {
    pub readonly: Option<bool>,
    pub hidden: Option<bool>,
    pub system: Option<bool>,
    pub archive: Option<bool>,
    pub unix_mode: Option<u32>,
    pub recursive: bool,
    pub paths: Vec<String>,
}

impl AttributesState {
    pub fn validate(&self) -> Result<(), String> {
        if self.paths.is_empty() {
            return Err("no files selected".to_string());
        }
        if self.readonly.is_none()
            && self.hidden.is_none()
            && self.system.is_none()
            && self.archive.is_none()
            && self.unix_mode.is_none()
        {
            return Err("no attribute changed".to_string());
        }
        if let Some(mode) = self.unix_mode
            && mode > 0o7777
        {
            return Err("invalid mode".to_string());
        }
        Ok(())
    }
}

/// File-comment dialog.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CommentState {
    pub path: String,
    pub name: String,
    pub text: String,
    pub original: String,
}

impl CommentState {
    pub fn new(path: impl Into<String>, name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        CommentState {
            path: path.into(),
            name: name.into(),
            text: text.clone(),
            original: text,
        }
    }

    pub fn is_modified(&self) -> bool {
        self.text != self.original
    }
}

/// Directory hotlist dialog/editor.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HotlistState {
    pub selected: Option<usize>,
    pub edit_title: String,
    pub edit_path: String,
    pub adding: bool,
}

/// FTP quick-connect dialog.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FtpConnectState {
    pub connection: SavedConnection,
    pub save_password: bool,
    pub status: String,
}

impl FtpConnectState {
    pub fn validate(&self) -> Result<(), String> {
        if self.connection.host.trim().is_empty() {
            return Err("empty host".to_string());
        }
        Ok(())
    }

    /// `user@host:port` summary.
    pub fn summary(&self) -> String {
        let user = if self.connection.user.is_empty() {
            "anonymous"
        } else {
            self.connection.user.as_str()
        };
        format!("{user}@{}:{}", self.connection.host, self.connection.port)
    }
}

/// Synchronise-directories dialog.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SyncState {
    pub left: String,
    pub right: String,
    pub ignore_dates: bool,
    pub asymmetric: bool,
    pub by_content: bool,
    pub dry_run: bool,
    pub rows: Vec<SyncRow>,
}

/// One comparison row of the synchronise dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncRow {
    pub name: String,
    pub left_size: Option<u64>,
    pub right_size: Option<u64>,
    pub left_time: Option<i64>,
    pub right_time: Option<i64>,
    /// Direction: `->`, `<-`, `=`, `!=`, `newer`.
    pub status: String,
}

impl SyncState {
    pub fn validate(&self) -> Result<(), String> {
        if self.left.trim().is_empty() || self.right.trim().is_empty() {
            return Err("both directories are required".to_string());
        }
        Ok(())
    }

    /// Count rows that need a copy in either direction.
    pub fn pending(&self) -> usize {
        self.rows.iter().filter(|r| r.status != "=").count()
    }
}

/// Generic confirmation action ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
    Quit,
    DeletePermanent,
    Overwrite,
    CloseWithTransfers,
    SaveSettings,
    DiscardEditorChanges,
}

/// Progress of a long running operation shown in a modal dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressState {
    pub title_key: String,
    pub current_file: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub cancellable: bool,
}

impl Default for ProgressState {
    fn default() -> Self {
        ProgressState {
            title_key: "status.copying".to_string(),
            current_file: String::new(),
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total: 0,
            cancellable: true,
        }
    }
}

impl ProgressState {
    pub fn fraction(&self) -> f32 {
        if self.bytes_total == 0 {
            if self.files_total == 0 {
                0.0
            } else {
                (self.files_done as f32 / self.files_total as f32).clamp(0.0, 1.0)
            }
        } else {
            (self.bytes_done as f32 / self.bytes_total as f32).clamp(0.0, 1.0)
        }
    }

    pub fn percent(&self) -> u8 {
        (self.fraction() * 100.0).round() as u8
    }
}

/// Options dialog state: the settings are edited in a copy so Cancel works.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionsState {
    pub tab: OptionsTab,
    pub draft: rc_core::config::Settings,
    pub language_choice: String,
    pub shortcuts_edit: Vec<(String, String)>,
}

impl OptionsState {
    pub fn new(settings: &rc_core::config::Settings, tab: OptionsTab) -> Self {
        let mut shortcuts_edit: Vec<(String, String)> = settings
            .shortcuts
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        shortcuts_edit.sort();
        OptionsState {
            tab,
            draft: settings.clone(),
            language_choice: settings.language.clone(),
            shortcuts_edit,
        }
    }

    /// Write the shortcut table back into the draft settings.
    pub fn commit_shortcuts(&mut self) {
        self.draft.shortcuts = self.shortcuts_edit.iter().cloned().collect();
    }
}

/// Which viewer mode a dialog asks for (used by the open-viewer command).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewerRequest {
    Auto,
    Internal,
    External,
}

impl From<ViewerRequest> for ViewerMode {
    fn from(r: ViewerRequest) -> ViewerMode {
        match r {
            ViewerRequest::Auto => ViewerMode::Auto,
            ViewerRequest::Internal => ViewerMode::Auto,
            ViewerRequest::External => ViewerMode::Auto,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_dialog_validation_and_title() {
        let mut d = TransferDialogState::new(false, vec!["a".into()], "/tmp");
        assert!(d.validate().is_ok());
        assert_eq!(d.title_key(), "dialog.copy.title");
        d.move_it = true;
        assert_eq!(d.title_key(), "dialog.move.title");
        d.destination = "  ".to_string();
        assert!(d.validate().is_err());
        let empty = TransferDialogState::new(false, vec![], "/tmp");
        assert!(empty.validate().is_err());
        assert_eq!(d.summary(), "1 file(s)");
    }

    #[test]
    fn overwrite_dialog_reports_newer_and_size_difference() {
        let d = OverwriteDialogState {
            source: "a".into(),
            destination: "b".into(),
            source_size: 10,
            destination_size: 5,
            source_time: Some(20),
            destination_time: Some(10),
            index: 0,
            total: 1,
        };
        assert!(d.source_is_newer());
        assert!(d.size_differs());
        let older = OverwriteDialogState {
            source_time: Some(1),
            destination_time: Some(2),
            ..d.clone()
        };
        assert!(!older.source_is_newer());
        let unknown = OverwriteDialogState {
            source_time: None,
            destination_time: None,
            ..d
        };
        assert!(unknown.source_is_newer());
    }

    #[test]
    fn mkdir_validation_rejects_bad_names() {
        let mut d = MakeDirState::default();
        assert!(d.validate().is_err());
        d.name = "newdir".into();
        assert!(d.validate().is_ok());
        for bad in ["a/b", "a\\b", ".", "..", "  "] {
            d.name = bad.into();
            assert!(d.validate().is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn rename_dialog_keeps_extension_when_asked() {
        let mut d = RenameState::new("report.txt");
        assert_eq!(d.name, "report.txt");
        d.name = "summary".into();
        assert_eq!(d.effective_name(), "summary");
        d.keep_extension = true;
        assert_eq!(d.effective_name(), "summary.txt");
        d.name = "summary.md".into();
        assert_eq!(d.effective_name(), "summary.md");
        assert!(d.validate().is_ok());
        d.name = "report.txt".into();
        assert!(d.validate().is_err(), "unchanged name is rejected");
        d.name = "a/b.txt".into();
        assert!(d.validate().is_err());
    }

    #[test]
    fn search_state_defaults_and_validation() {
        let mut s = SearchState::new("/tmp");
        assert!(!s.is_content_search());
        assert!(s.validate().is_ok());
        s.name_mask = " ".into();
        assert!(s.validate().is_err());
        s.content = "needle".into();
        assert!(s.is_content_search());
        assert!(s.validate().is_ok());
        let empty = SearchState::new("");
        assert!(empty.validate().is_err());
        assert!(SearchState::new("/x").subdirs);
    }

    #[test]
    fn pack_and_unpack_validation() {
        let mut p = PackState::new(vec!["a.txt".into()], "out.zip");
        assert!(p.validate().is_ok());
        p.level = 12;
        assert!(p.validate().is_err());
        p.level = 6;
        p.sources.clear();
        assert!(p.validate().is_err());
        let mut u = UnpackState::new("a.zip", "/tmp");
        assert!(u.validate().is_ok());
        u.destination = "".into();
        assert!(u.validate().is_err());
        assert!(UnpackState::new("", "/tmp").validate().is_err());
    }

    #[test]
    fn multi_rename_preview_uses_the_counter() {
        let mut m = MultiRenameState::new(vec!["a.txt".into(), "b.txt".into()]);
        m.pattern = "file[C]".into();
        m.counter_start = 5;
        m.counter_digits = 3;
        m.refresh_preview();
        assert_eq!(m.preview.len(), 2);
        assert_eq!(m.preview[0].1, "file005.txt");
        assert_eq!(m.preview[1].1, "file006.txt");
        assert!(m.validate().is_ok());
        let mut empty = MultiRenameState::new(vec![]);
        assert!(empty.validate().is_err());
        empty.pattern = "".into();
        empty.sources = vec!["x".into()];
        assert!(empty.validate().is_err());
    }

    #[test]
    fn attributes_requires_a_change() {
        let mut a = AttributesState {
            paths: vec!["x".into()],
            ..Default::default()
        };
        assert!(a.validate().is_err());
        a.readonly = Some(true);
        assert!(a.validate().is_ok());
        a.unix_mode = Some(0o7777);
        assert!(a.validate().is_ok());
        a.unix_mode = Some(0o10000);
        assert!(a.validate().is_err());
        let no_paths = AttributesState {
            readonly: Some(true),
            ..Default::default()
        };
        assert!(no_paths.validate().is_err());
    }

    #[test]
    fn comment_state_tracks_modification() {
        let c = CommentState::new("/tmp/a", "a", "hello");
        assert!(!c.is_modified());
        let mut c2 = c.clone();
        c2.text = "changed".into();
        assert!(c2.is_modified());
    }

    #[test]
    fn ftp_dialog_summary_and_validation() {
        let mut f = FtpConnectState::default();
        assert!(f.validate().is_err());
        f.connection.host = "example.com".into();
        f.connection.port = 21;
        assert!(f.validate().is_ok());
        assert_eq!(f.summary(), "anonymous@example.com:21");
        f.connection.user = "bob".into();
        assert_eq!(f.summary(), "bob@example.com:21");
        f.connection.port = 0;
        f.connection.host = String::new();
        assert!(f.validate().is_err());
    }

    #[test]
    fn sync_state_counts_pending_rows() {
        let mut s = SyncState::default();
        assert!(s.validate().is_err());
        s.left = "/a".into();
        s.right = "/b".into();
        assert!(s.validate().is_ok());
        s.rows = vec![
            SyncRow {
                name: "a".into(),
                left_size: Some(1),
                right_size: Some(1),
                left_time: None,
                right_time: None,
                status: "=".into(),
            },
            SyncRow {
                name: "b".into(),
                left_size: Some(1),
                right_size: None,
                left_time: None,
                right_time: None,
                status: "->".into(),
            },
        ];
        assert_eq!(s.pending(), 1);
    }

    #[test]
    fn progress_percentage_math() {
        let mut p = ProgressState::default();
        assert_eq!(p.percent(), 0);
        p.bytes_total = 100;
        p.bytes_done = 50;
        assert_eq!(p.percent(), 50);
        assert!((p.fraction() - 0.5).abs() < 1e-6);
        p.bytes_total = 0;
        p.files_total = 4;
        p.files_done = 1;
        assert_eq!(p.percent(), 25);
        p.bytes_done = 999;
        assert_eq!(
            p.percent(),
            25,
            "byte progress is ignored when the total is unknown"
        );
    }

    #[test]
    fn options_state_starts_from_settings_and_commits_shortcuts() {
        let mut settings = rc_core::config::Settings::default();
        settings
            .shortcuts
            .insert("ctrl+f3".into(), "cm_SrcByName".into());
        let mut o = OptionsState::new(&settings, OptionsTab::Display);
        assert_eq!(o.language_choice, "en");
        assert_eq!(o.shortcuts_edit.len(), 1);
        o.shortcuts_edit.push(("f9".into(), "cm_Test".into()));
        o.commit_shortcuts();
        assert_eq!(o.draft.shortcuts.len(), 2);
        assert_eq!(
            o.draft.shortcuts.get("f9").map(|s| s.as_str()),
            Some("cm_Test")
        );
        assert_eq!(OptionsTab::all().len(), 8);
        assert!(OptionsTab::Language.key().starts_with("dialog.config."));
    }

    #[test]
    fn hotlist_state_defaults_are_empty() {
        let h = HotlistState::default();
        assert!(h.selected.is_none());
        assert!(!h.adding);
    }
}

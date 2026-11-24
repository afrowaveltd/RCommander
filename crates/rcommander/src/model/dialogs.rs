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
    /// Directory history of the active panel (`Alt+Down`).
    History(HistoryState),
    /// Ask for a quick filter mask (`Ctrl+F12`).
    MaskPrompt(MaskPromptState),
    /// Result of comparing the two panels (`Shift+F2`).
    Compare(CompareState),
    /// Directory tree browser (`Alt+F10`).
    Tree(TreeState),
    /// Button bar editor (`Configuration ▸ Button Bar`).
    ButtonBar(ButtonBarState),
    /// Columns and view modes editor.
    ViewModes(ViewModeDialogState),
    /// Split a file (`Files ▸ Split File`).
    Split(SplitState),
    /// Combine parts (`Files ▸ Combine Files`).
    Combine(CombineState),
    /// Create checksums (`Files ▸ Create Checksum File`).
    Checksum(ChecksumState),
    /// Verify checksums from a file.
    VerifyChecksum(VerifyChecksumState),
    /// Compare two files by content.
    CompareFiles(CompareFilesState),
    /// Duplicate finder.
    Duplicates(DuplicatesState),
    /// Unlock (or create) the encrypted credential store.
    UnlockSecrets(UnlockSecretsState),
    /// Encode a file as text (MIME/UUE/XXE).
    Encode(EncodeState),
    /// Decode a text file back to binary.
    Decode(DecodeState),
    /// Save the selection to a file.
    SaveSelection(SaveSelectionState),
    /// Load a selection from a file.
    LoadSelection(LoadSelectionState),
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
    /// Factory resets; the page warns loudly because nothing here can be undone.
    Dangerous,
}

impl OptionsTab {
    pub fn all() -> [OptionsTab; 9] {
        [
            OptionsTab::Display,
            OptionsTab::Language,
            OptionsTab::Colors,
            OptionsTab::Editor,
            OptionsTab::Packer,
            OptionsTab::Ftp,
            OptionsTab::Agent,
            OptionsTab::Shortcuts,
            OptionsTab::Dangerous,
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
            OptionsTab::Dangerous => "dialog.config.dangerous",
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

/// FTP/SFTP quick-connect dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct FtpConnectState {
    pub connection: SavedConnection,
    pub save_password: bool,
    pub status: String,
}

impl Default for FtpConnectState {
    fn default() -> Self {
        FtpConnectState {
            connection: SavedConnection {
                protocol: "ftp".to_string(),
                auth_method: "password".to_string(),
                host_key_policy: "tofu".to_string(),
                ..Default::default()
            },
            save_password: true,
            status: String::new(),
        }
    }
}

impl FtpConnectState {
    /// True when the SSH-specific fields should be shown.
    pub fn is_ssh(&self) -> bool {
        matches!(
            self.connection
                .protocol
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "sftp" | "sftp4" | "sftp5"
        ) || self
            .connection
            .protocol
            .trim()
            .to_ascii_lowercase()
            .starts_with("sftp")
    }

    /// Validation that also covers the SSH-specific choices.
    pub fn validate(&self) -> Result<(), String> {
        if self.is_ssh() {
            if self.connection.user.trim().is_empty() {
                return Err("SFTP needs a user name".to_string());
            }
            if self
                .connection
                .auth_method
                .trim()
                .eq_ignore_ascii_case("key")
                && self.connection.key_path.trim().is_empty()
            {
                return Err("key authentication needs a key file".to_string());
            }
        }
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

/// Directory history list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryState {
    pub entries: Vec<HistoryRow>,
    pub selected: Option<usize>,
}

/// One row of the history list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRow {
    pub path: String,
    pub cursor_name: Option<String>,
}

impl HistoryState {
    pub fn new(rows: Vec<HistoryRow>) -> Self {
        HistoryState {
            entries: rows,
            selected: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The mask entry used by `Ctrl+F12` (only selected files / quick filter).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaskPromptState {
    pub mask: String,
    pub hide_non_matching: bool,
}

impl MaskPromptState {
    pub fn new(mask: impl Into<String>) -> Self {
        MaskPromptState {
            mask: mask.into(),
            hide_non_matching: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.mask.trim().is_empty() {
            return Err("empty mask".to_string());
        }
        rc_core::mask::MaskSet::parse(self.mask.trim()).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Comparison of the two panels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompareState {
    pub rows: Vec<CompareRow>,
    /// Mark entries that differ in the source panel.
    pub mark_different: bool,
}

/// One compared name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareRow {
    pub name: String,
    /// `=`, `!=`, `<` (only left), `>` (only right), `<DIR>` markers.
    pub status: String,
    pub left_size: Option<u64>,
    pub right_size: Option<u64>,
}

impl CompareState {
    pub fn different(&self) -> usize {
        self.rows.iter().filter(|r| r.status != "=").count()
    }

    pub fn summary(&self) -> String {
        format!("{} of {} entries differ", self.different(), self.rows.len())
    }
}

/// A browsable directory tree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeState {
    /// Directory the tree was built from.
    pub root: String,
    pub entries: Vec<String>,
    pub selected: Option<usize>,
    pub include_files: bool,
}

impl TreeState {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Button bar editor state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ButtonBarState {
    pub selected: Option<usize>,
    pub edit_caption: String,
    pub edit_command: String,
    pub edit_parameters: String,
    pub adding: bool,
}

impl ButtonBarState {
    pub fn new() -> Self {
        ButtonBarState::default()
    }

    /// Start editing the button at `index`.
    pub fn edit(&mut self, index: usize, button: &rc_core::config::BarButton) {
        self.selected = Some(index);
        self.adding = false;
        self.edit_caption = button.caption.clone();
        self.edit_command = button.command.clone();
        self.edit_parameters = button.parameters.clone();
    }

    /// Start adding a new button.
    pub fn start_add(&mut self) {
        self.adding = true;
        self.selected = None;
        self.edit_caption = String::new();
        self.edit_command = String::new();
        self.edit_parameters = String::new();
    }

    /// The button described by the editor fields.
    pub fn draft(&self) -> rc_core::config::BarButton {
        rc_core::config::BarButton {
            caption: self.edit_caption.trim().to_string(),
            command: self.edit_command.trim().to_string(),
            parameters: self.edit_parameters.trim().to_string(),
            separator: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.edit_command.trim().is_empty() {
            return Err("a button needs a command".to_string());
        }
        Ok(())
    }
}

/// Columns / view modes editor state.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewModeDialogState {
    pub name: String,
    pub kind: String,
    pub mask: String,
    pub sort_field: String,
    pub sort_reversed: bool,
    /// Column ids the mode shows (empty = keep the panel's columns).
    pub columns: Vec<String>,
    pub selected_mode: Option<usize>,
}

impl Default for ViewModeDialogState {
    fn default() -> Self {
        ViewModeDialogState {
            name: String::new(),
            kind: "full".to_string(),
            mask: "*.*".to_string(),
            sort_field: "name".to_string(),
            sort_reversed: false,
            columns: Vec::new(),
            selected_mode: None,
        }
    }
}

impl ViewModeDialogState {
    /// Build the state from an existing mode.
    pub fn from_mode(mode: &rc_core::config::ViewModeSpec, index: usize) -> Self {
        ViewModeDialogState {
            name: mode.name.clone(),
            kind: mode.kind.clone(),
            mask: mode.mask.clone(),
            sort_field: mode.sort_field.clone(),
            sort_reversed: mode.sort_reversed,
            columns: mode.columns.iter().map(|c| c.id.clone()).collect(),
            selected_mode: Some(index),
        }
    }

    /// Convert the editor fields into a validated view mode.
    pub fn to_mode(&self) -> rc_core::config::ViewModeSpec {
        rc_core::config::ViewModeSpec {
            name: self.name.trim().to_string(),
            kind: self.kind.clone(),
            columns: self
                .columns
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    rc_core::config::ColumnSpec::new(
                        id.clone(),
                        if index == 0 { 0.0 } else { 90.0 },
                    )
                })
                .collect(),
            sort_field: self.sort_field.clone(),
            sort_reversed: self.sort_reversed,
            mask: if self.mask.trim().is_empty() {
                "*.*".to_string()
            } else {
                self.mask.trim().to_string()
            },
        }
    }
}

/// Split-file dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitState {
    pub source: String,
    pub part_size: u64,
    pub destination: String,
    /// Number of parts reported by the last run.
    pub parts_created: usize,
}

impl SplitState {
    pub fn new(source: impl Into<String>, destination: impl Into<String>) -> Self {
        SplitState {
            source: source.into(),
            part_size: rc_core::split::DEFAULT_PART_SIZE,
            destination: destination.into(),
            parts_created: 0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.source.trim().is_empty() {
            return Err("no file selected".to_string());
        }
        if self.part_size == 0 {
            return Err("part size must be greater than zero".to_string());
        }
        Ok(())
    }

    /// Part size in a human readable form.
    pub fn size_label(&self) -> String {
        rc_core::format::format_size_unit(self.part_size)
    }
}

/// Combine-parts dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct CombineState {
    /// The first part (`.001`) whose siblings are combined.
    pub first_part: String,
    pub destination: String,
    pub discovered: usize,
}

impl CombineState {
    pub fn new(first_part: impl Into<String>, destination: impl Into<String>) -> Self {
        CombineState {
            first_part: first_part.into(),
            destination: destination.into(),
            discovered: 0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.first_part.trim().is_empty() {
            return Err("no part selected".to_string());
        }
        if self.destination.trim().is_empty() {
            return Err("empty destination".to_string());
        }
        Ok(())
    }
}

/// Create-checksums dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct ChecksumState {
    pub algorithm: String,
    pub file_name: String,
    pub sources: Vec<String>,
    pub base: String,
}

impl ChecksumState {
    pub fn new(sources: Vec<String>, base: impl Into<String>) -> Self {
        let algorithm = rc_core::checksum::Algorithm::Md5;
        let paths: Vec<std::path::PathBuf> = sources.iter().map(std::path::PathBuf::from).collect();
        ChecksumState {
            algorithm: algorithm.id().to_string(),
            file_name: rc_core::checksum::suggested_name(&paths, algorithm),
            sources,
            base: base.into(),
        }
    }

    pub fn parsed_algorithm(&self) -> Option<rc_core::checksum::Algorithm> {
        rc_core::checksum::Algorithm::parse(&self.algorithm)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.sources.is_empty() {
            return Err("no files selected".to_string());
        }
        if self.parsed_algorithm().is_none() {
            return Err(format!("unknown checksum algorithm {:?}", self.algorithm));
        }
        if self.file_name.trim().is_empty() {
            return Err("empty checksum file name".to_string());
        }
        Ok(())
    }
}

/// Verify-checksums dialog.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct VerifyChecksumState {
    pub checksum_file: String,
    pub base: String,
    pub checked: usize,
    pub ok: usize,
    pub failures: Vec<(String, String)>,
}

impl VerifyChecksumState {
    pub fn new(checksum_file: impl Into<String>, base: impl Into<String>) -> Self {
        VerifyChecksumState {
            checksum_file: checksum_file.into(),
            base: base.into(),
            ..Default::default()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.checksum_file.trim().is_empty() {
            return Err("no checksum file selected".to_string());
        }
        Ok(())
    }

    pub fn summary(&self) -> String {
        if self.checked == 0 {
            "no checksums read yet".to_string()
        } else {
            format!(
                "{} ok, {} failed of {}",
                self.ok,
                self.failures.len(),
                self.checked
            )
        }
    }
}

/// Compare-two-files dialog.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CompareFilesState {
    pub left: String,
    pub right: String,
    /// `None` = not compared yet, `Some(None)` = identical, `Some(Some(n))` = differ at n.
    pub result: Option<Option<u64>>,
}

impl CompareFilesState {
    pub fn new(left: impl Into<String>, right: impl Into<String>) -> Self {
        CompareFilesState {
            left: left.into(),
            right: right.into(),
            result: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.left.trim().is_empty() || self.right.trim().is_empty() {
            return Err("both files are required".to_string());
        }
        Ok(())
    }

    /// Text describing the comparison result.
    pub fn result_text(&self) -> String {
        match self.result {
            None => "not compared yet".to_string(),
            Some(None) => "the files are identical".to_string(),
            Some(Some(offset)) => format!("the files differ at byte {offset}"),
        }
    }
}

/// Duplicate finder dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct DuplicatesState {
    pub root: String,
    pub min_size: u64,
    pub algorithm: String,
    pub include_hidden: bool,
    pub groups: Vec<rc_core::dupes::DuplicateGroup>,
    pub scanned: u64,
}

impl DuplicatesState {
    pub fn new(root: impl Into<String>) -> Self {
        DuplicatesState {
            root: root.into(),
            min_size: 1,
            algorithm: rc_core::checksum::Algorithm::Md5.id().to_string(),
            include_hidden: false,
            groups: Vec::new(),
            scanned: 0,
        }
    }

    pub fn parsed_algorithm(&self) -> Option<rc_core::checksum::Algorithm> {
        rc_core::checksum::Algorithm::parse(&self.algorithm)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.root.trim().is_empty() {
            return Err("empty directory".to_string());
        }
        if self.parsed_algorithm().is_none() {
            return Err(format!("unknown checksum algorithm {:?}", self.algorithm));
        }
        Ok(())
    }

    pub fn summary(&self) -> String {
        let files: usize = self.groups.iter().map(|g| g.paths.len()).sum();
        let wasted: u64 = self.groups.iter().map(|g| g.wasted_bytes()).sum();
        format!(
            "{} group(s), {} file(s), {}",
            self.groups.len(),
            files,
            rc_core::format::format_size_unit(wasted)
        )
    }
}

/// Encode dialog state.
#[derive(Clone, Debug, PartialEq)]
pub struct EncodeState {
    pub sources: Vec<String>,
    /// `MIME`, `UUE` or `XXE`.
    pub encoding: String,
}

impl EncodeState {
    pub fn new(sources: Vec<String>) -> Self {
        EncodeState {
            sources,
            encoding: "MIME".to_string(),
        }
    }

    pub fn parsed_encoding(&self) -> Option<rc_core::encode::TextEncoding> {
        rc_core::encode::TextEncoding::parse(&self.encoding)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.sources.is_empty() {
            return Err("no files selected".to_string());
        }
        if self.parsed_encoding().is_none() {
            return Err(format!("unknown encoding {:?}", self.encoding));
        }
        Ok(())
    }
}

/// Decode dialog state.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodeState {
    pub source: String,
    /// File name to restore; taken from the envelope when empty.
    pub target_name: String,
    pub decoded_bytes: u64,
}

impl DecodeState {
    pub fn new(source: impl Into<String>) -> Self {
        DecodeState {
            source: source.into(),
            target_name: String::new(),
            decoded_bytes: 0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.source.trim().is_empty() {
            return Err("no file selected".to_string());
        }
        Ok(())
    }
}

/// Save-selection dialog state.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveSelectionState {
    pub file_name: String,
    pub count: usize,
}

impl SaveSelectionState {
    pub fn new(count: usize) -> Self {
        SaveSelectionState {
            file_name: "selection.txt".to_string(),
            count,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.count == 0 {
            return Err("nothing is selected".to_string());
        }
        if self.file_name.trim().is_empty() {
            return Err("empty file name".to_string());
        }
        Ok(())
    }
}

/// Load-selection dialog state.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadSelectionState {
    pub file_name: String,
    pub loaded: usize,
}

impl LoadSelectionState {
    pub fn new() -> Self {
        LoadSelectionState {
            file_name: "selection.txt".to_string(),
            loaded: 0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.file_name.trim().is_empty() {
            return Err("empty file name".to_string());
        }
        Ok(())
    }
}

impl Default for LoadSelectionState {
    fn default() -> Self {
        LoadSelectionState::new()
    }
}

/// Unlock/create dialog for the encrypted credential store.
#[derive(Clone, Debug, PartialEq)]
pub struct UnlockSecretsState {
    /// The store file already exists, so the passphrase must be entered. When the
    /// user cannot remember it, `force_create` switches to creating a new store
    /// (which discards the old secrets after an explicit confirmation).
    pub existing: bool,
    /// Create a fresh store even though an old one exists.
    pub force_create: bool,
    pub passphrase: String,
    /// Confirmation field, used when creating a new store.
    pub confirm: String,
    pub error: Option<String>,
}

impl UnlockSecretsState {
    pub fn new(existing: bool) -> Self {
        UnlockSecretsState {
            existing,
            force_create: false,
            passphrase: String::new(),
            confirm: String::new(),
            error: None,
        }
    }

    /// True when the dialog creates a store (missing file, or the user asked to
    /// start over because the passphrase is lost).
    pub fn is_create(&self) -> bool {
        !self.existing || self.force_create
    }

    /// Switch to creating a new store, discarding the old one.
    pub fn start_over(&mut self) {
        self.force_create = true;
        self.passphrase.clear();
        self.confirm.clear();
        self.error = None;
    }

    /// Minimum passphrase length; enforced only when creating.
    pub const MIN_LENGTH: usize = 8;

    pub fn validate(&self) -> Result<(), String> {
        if self.passphrase.is_empty() {
            return Err("the passphrase must not be empty".to_string());
        }
        if self.is_create() {
            if self.passphrase.chars().count() < Self::MIN_LENGTH {
                return Err(format!("use at least {} characters", Self::MIN_LENGTH));
            }
            if self.passphrase != self.confirm {
                return Err("the two passphrases differ".to_string());
            }
        }
        Ok(())
    }

    pub fn title(&self) -> &'static str {
        if self.is_create() {
            "dialog.secrets.create"
        } else {
            "dialog.secrets.unlock"
        }
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
    /// Reset the user asked for and has not confirmed yet. A reset is only ever
    /// one click away from a second click that actually wipes something.
    pub pending_reset: Option<super::ResetScope>,
    /// Scopes that were already reset in this dialog, so the page can show a
    /// "done" mark instead of leaving the button looking untouched.
    pub applied_resets: Vec<super::ResetScope>,
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
            pending_reset: None,
            applied_resets: Vec::new(),
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
        assert_eq!(OptionsTab::all().len(), 9);
        assert!(OptionsTab::Language.key().starts_with("dialog.config."));
        // The reset page has to stay last: it is the one nobody opens by accident.
        assert_eq!(*OptionsTab::all().last().unwrap(), OptionsTab::Dangerous);
        assert!(o.pending_reset.is_none(), "a reset needs an explicit click");
    }

    #[test]
    fn hotlist_state_defaults_are_empty() {
        let h = HotlistState::default();
        assert!(h.selected.is_none());
        assert!(!h.adding);
    }
}

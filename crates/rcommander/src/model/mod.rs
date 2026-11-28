//! Headless application model.
//!
//! Everything the user can do is expressed here as a method that mutates the
//! model and returns a [`CoreResult`] or a [`CommandOutcome`]. The egui layer is
//! a pure view over this state, which is what makes the behaviour testable
//! without a display.

pub mod actions;
pub mod agentworkspace;
pub mod ai;
/// Re-export of the sound kinds, so the model can name one without importing the whole desktop
/// module at every call site.
pub mod desktop_sound {
    pub use rc_core::desktop::Sound;
}
pub mod chat;
pub mod chatpane;
pub mod columns;
pub mod commands;
pub mod dialogs;
pub mod fonts;
pub mod git;
pub mod mcptools;
pub mod plugins;
pub mod policy;
pub mod prompts;
pub mod remote;
pub mod reset;
pub mod routing;
pub mod search;
pub mod sessions;
pub mod sources;
pub mod surfaces;
pub mod workspacereview;
use sources::{SourceAction, SourceMenuAction};
pub use surfaces::{FnKey, Surface};
pub mod themes;

use crate::i18n::{Direction, Strings};
use crate::ops_runner::OpHandle;
pub use crate::ops_runner::OpResult;
use rc_core::config::Settings;
use rc_core::drive::Space;
use rc_core::entry::{Entry, EntryKind};
use rc_core::error::{CoreError, CoreResult};
use rc_core::history::DirHistory;
use rc_core::keymap::{KeyMap, Shortcut};
use rc_core::listing::{Listing, ReadOptions};
use rc_core::location::Location;
use rc_core::mask::MaskSet;
use rc_core::quicksearch::QuickSearch;
use rc_core::secrets::{MemoryStore, SecretStore};
use rc_core::selection::Selection;
use rc_core::sorting::{SortField, SortSpec};
use rc_core::tabs::TabBar;

pub use dialogs::Dialog;
pub use git::GitConfigGate;
pub use reset::{ResetReport, ResetScope};

/// Which of the two panels is meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

/// How the file list is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    /// Names only, wrapped in columns.
    Brief,
    /// Name, size, date, attributes.
    Full,
    /// Full plus the file comment.
    Comments,
    /// Thumbnails grid.
    Thumbnails,
    /// Flat list of a directory tree (branch view).
    Tree,
    /// Columns configured by the user.
    Custom,
}

impl ViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewMode::Brief => "brief",
            ViewMode::Full => "full",
            ViewMode::Comments => "comments",
            ViewMode::Thumbnails => "thumbnails",
            ViewMode::Tree => "tree",
            ViewMode::Custom => "custom",
        }
    }

    /// Parse a view-mode identifier (`brief`, `full`, `comments`, …).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> ViewMode {
        match text.trim().to_ascii_lowercase().as_str() {
            "brief" | "short" => ViewMode::Brief,
            "comments" => ViewMode::Comments,
            "thumbnails" | "thumbs" => ViewMode::Thumbnails,
            "tree" => ViewMode::Tree,
            "custom" => ViewMode::Custom,
            _ => ViewMode::Full,
        }
    }
}

/// One panel: a tab strip plus the listing, marks and history of the active tab.
#[derive(Clone, Debug)]
pub struct Panel {
    pub tabs: TabBar,
    /// Git status of the entries in the current listing (empty for plain folders).
    pub git_status: std::collections::HashMap<String, rc_git::StatusEntry>,
    /// Branch or revision shown in the header, for a Git panel.
    pub git_label: String,
    pub listing: Listing,
    pub selection: Selection,
    pub history: DirHistory,
    // Chronological navigation is separate from the deduplicated MRU list.
    back_locations: Vec<Location>,
    forward_locations: Vec<Location>,
    pub quick: QuickSearch,
    pub view_mode: ViewMode,
    /// Mask used to filter files (`*.*` = everything).
    pub mask_spec: String,
    /// Last read error, shown in the panel instead of entries.
    pub error: Option<String>,
    /// Path typed into the inline path editor (None = not editing).
    pub path_edit: Option<String>,
    /// Show the agent chat in this panel instead of the file list (Phase 16.1).
    ///
    /// The listing is untouched while the chat is shown, so switching back restores the directory,
    /// the cursor and the selection exactly.
    pub chat: bool,
}

impl Panel {
    /// Create a panel showing `location`.
    pub fn new(location: Location, settings: &Settings) -> Self {
        let mask_spec = settings.display.file_mask.clone();
        let view_mode = ViewMode::from_str(&settings.display.view_mode);
        let sort = SortSpec {
            dirs_first: settings.display.dirs_first,
            natural: settings.display.natural_sort,
            ..SortSpec::default()
        };
        let read_options = ReadOptions {
            show_hidden: settings.display.show_hidden,
            show_parent: settings.display.show_parent_entry,
            mask: MaskSet::parse_or_all(&mask_spec).unwrap_or_default(),
        };
        let listing = Listing::read(&location, &read_options, sort).unwrap_or_else(|_| {
            Listing::from_entries(location.clone(), Vec::new(), &read_options, sort)
        });
        let mut history = DirHistory::new(settings.history_length);
        if let Some(p) = location.as_path() {
            history.record(p.to_path_buf(), None, 0);
        }
        Panel {
            tabs: TabBar::new(location),
            git_status: std::collections::HashMap::new(),
            git_label: String::new(),
            listing,
            selection: Selection::new(),
            history,
            back_locations: Vec::new(),
            forward_locations: Vec::new(),
            quick: QuickSearch::new(),
            view_mode,
            mask_spec,
            error: None,
            path_edit: None,
            chat: false,
        }
    }

    pub fn location(&self) -> &Location {
        self.listing.location()
    }

    pub fn entries(&self) -> &[Entry] {
        self.listing.entries()
    }

    pub fn cursor(&self) -> usize {
        self.listing.cursor()
    }

    pub fn cursor_entry(&self) -> Option<&Entry> {
        self.listing.cursor_entry()
    }

    /// The entry under the cursor, ignoring the `..` pseudo row.
    pub fn current(&self) -> Option<&Entry> {
        self.cursor_entry().filter(|e| e.name != "..")
    }

    /// Whether the panel has an earlier chronological visit.
    pub fn can_go_back(&self) -> bool {
        !self.back_locations.is_empty()
    }

    /// Whether the panel has a later chronological visit.
    pub fn can_go_forward(&self) -> bool {
        !self.forward_locations.is_empty()
    }

    fn record_navigation(&mut self, target: &Location, capacity: usize) {
        if self.location() == target {
            return;
        }
        // Remote locations are tied to a live session, not merely a path.
        // Do not replay a stale remote identity via the current connection.
        if self.location().is_remote() || target.is_remote() {
            self.clear_navigation_history();
            return;
        }
        let previous = self.location().clone();
        self.back_locations.push(previous);
        if self.back_locations.len() > capacity.max(1) {
            let excess = self.back_locations.len() - capacity.max(1);
            self.back_locations.drain(..excess);
        }
        self.forward_locations.clear();
    }

    pub fn clear_navigation_history(&mut self) {
        self.back_locations.clear();
        self.forward_locations.clear();
    }

    pub fn sort(&self) -> SortSpec {
        self.listing.sort_spec()
    }

    pub fn read_options(&self, settings: &Settings) -> ReadOptions {
        ReadOptions {
            show_hidden: settings.display.show_hidden,
            show_parent: settings.display.show_parent_entry,
            mask: MaskSet::parse_or_all(&self.mask_spec).unwrap_or_default(),
        }
    }

    /// Open a location whose entries come from somewhere other than the file system.
    ///
    /// A plugin source lists its own items; everything the panel does with them — cursor, sorting,
    /// marks, tabs — is the same code as a directory, which is why this is a small function and not
    /// a second panel implementation.
    pub fn open_with_entries(
        &mut self,
        location: Location,
        entries: Vec<Entry>,
        settings: &Settings,
    ) -> CoreResult<()> {
        let opts = self.read_options(settings);
        let sort = self.sort();
        self.git_status.clear();
        self.git_label.clear();
        self.record_navigation(&location, settings.history_length);
        self.listing = Listing::from_entries(location.clone(), entries, &opts, sort);
        self.error = None;
        self.selection.clear();
        self.quick.reset();
        self.tabs.set_active_location(location);
        Ok(())
    }

    /// Re-read the current directory, keeping the marks that still apply.
    pub fn reread(&mut self, settings: &Settings) -> CoreResult<()> {
        self.listing
            .set_mask(MaskSet::parse_or_all(&self.mask_spec).unwrap_or_default());
        let result = self.listing.reread();
        self.selection.retain_existing(self.listing.entries());
        match &result {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(e.to_string()),
        }
        self.apply_display_options(settings);
        result
    }

    /// Apply the hidden/parent/format options from settings to the listing.
    pub fn apply_display_options(&mut self, settings: &Settings) {
        let opts = self.read_options(settings);
        self.listing.set_show_hidden(opts.show_hidden);
        self.view_mode = ViewMode::from_str(&settings.display.view_mode);
    }

    /// Change the sort order of the panel.
    pub fn set_sort(&mut self, spec: SortSpec) {
        self.listing.resort(spec);
    }

    /// Sort by a field, toggling the direction when the field is already active.
    pub fn sort_by(&mut self, field: SortField) {
        let current = self.sort();
        let spec = if current.field == field {
            current.reversed()
        } else {
            SortSpec {
                field,
                reversed: false,
                ..current
            }
        };
        self.listing.resort(spec);
    }

    /// Move the cursor by `delta` rows.
    pub fn move_cursor(&mut self, delta: isize, wrap: bool) {
        self.listing.move_cursor(delta, wrap);
    }

    pub fn set_cursor(&mut self, index: usize) {
        self.listing.set_cursor(index);
    }

    /// Jump to the first/last row.
    pub fn cursor_home(&mut self) {
        self.listing.set_cursor(0);
    }

    pub fn cursor_end(&mut self) {
        let n = self.listing.len();
        self.listing.set_cursor(n.saturating_sub(1));
    }

    /// Page up/down by a number of rows.
    pub fn page(&mut self, rows: usize, down: bool) {
        let delta = rows as isize * if down { 1 } else { -1 };
        self.listing.move_cursor(delta, false);
    }

    /// Mark or unmark the entry under the cursor and advance (the `Space` key).
    pub fn toggle_and_advance(&mut self) {
        if let Some(e) = self.current().cloned() {
            self.selection.toggle(&e.name);
            self.move_cursor(1, false);
        } else {
            self.move_cursor(1, false);
        }
    }

    /// Mark or unmark the entry under the cursor without moving (`Insert`).
    pub fn toggle_current(&mut self) {
        if let Some(e) = self.current() {
            let name = e.name.clone();
            self.selection.toggle(&name);
        }
    }

    /// Entries that an operation should act on: the marks, or the cursor row.
    pub fn operation_targets(&self) -> Vec<Entry> {
        let marked = self.selection.selected_entries(self.entries());
        if !marked.is_empty() {
            return marked.into_iter().cloned().collect();
        }
        self.current().cloned().into_iter().collect()
    }

    /// Marks as name list (for the status line and MCP).
    pub fn selected_names(&self) -> Vec<String> {
        self.selection
            .selected_entries(self.entries())
            .into_iter()
            .map(|e| e.name.clone())
            .collect()
    }

    /// Number of marks and the bytes they add up to.
    pub fn selection_stats(&self) -> (usize, u64) {
        self.listing.selection_statistics(&self.selection)
    }

    /// Status text: `1 234 k in 2 files, 5 dirs` style summary.
    pub fn statistics(&self) -> (usize, usize, u64) {
        self.listing.statistics()
    }

    /// Quick-search keystroke handling; returns the index to jump to.
    pub fn quick_search_push(&mut self, ch: char) -> Option<usize> {
        let names = self.listing.name_list();
        let cursor = self.cursor();
        let hit = self.quick.push(ch, &names, cursor);
        if let Some(i) = hit {
            self.set_cursor(i);
        }
        hit
    }

    /// Switch the tab and re-read.
    pub fn activate_tab(&mut self, index: usize, settings: &Settings) -> CoreResult<()> {
        if !self.tabs.activate(index) {
            return Err(CoreError::Invalid(format!("no tab {index}")));
        }
        let loc = self.tabs.active().location.clone();
        self.open(loc, settings)
    }

    /// Open a location in the active tab. Archive locations are listed through
    /// the archive backend, everything else through the file system.
    pub fn open(&mut self, location: Location, settings: &Settings) -> CoreResult<()> {
        let opts = self.read_options(settings);
        let sort = self.sort();
        // `branches/main` is not a directory inside a tree: it names a revision, so
        // the panel switches to that revision's root instead.
        let location = normalise_git_location(location);
        let listing = if location.is_git() {
            let listing = crate::git_bridge::read_listing(&location).map_err(CoreError::Invalid)?;
            self.git_status = listing.status;
            self.git_label = listing.label;
            Listing::from_entries(location.clone(), listing.entries, &opts, sort)
        } else {
            self.git_status.clear();
            self.git_label.clear();
            match crate::archive_bridge::read_archive_listing(&location) {
                Some(result) => {
                    let entries = result.map_err(CoreError::Invalid)?;
                    Listing::from_entries(location.clone(), entries, &opts, sort)
                }
                None => Listing::read(&location, &opts, sort)?,
            }
        };
        self.record_navigation(&location, settings.history_length);
        self.listing = listing;
        self.error = None;
        self.selection.clear();
        self.quick.reset();
        self.tabs.set_active_location(location.clone());
        if let Some(p) = location.as_path() {
            self.history
                .record(p.to_path_buf(), None, chrono::Local::now().timestamp());
        }
        Ok(())
    }
}

/// Whether the audit log is switched on in the settings.
fn settings_audit_enabled(model: &Model) -> bool {
    model.settings.agent.audit
}

/// Turn `<root>/branches/<name>` into a location for that revision.
fn normalise_git_location(location: Location) -> Location {
    if !location.is_git() {
        return location;
    }
    let Some((root, rev, path)) = location.git_parts() else {
        return location;
    };
    if !rev.is_empty() {
        return location;
    }
    if let Some((folder, name)) = path.split_once('/')
        && crate::git_bridge::is_virtual_folder(folder)
    {
        let base = Location::git(root, "", folder);
        if let Some(inner) = crate::git_bridge::location_for_reference(&base, folder, name) {
            return inner;
        }
    }
    location
}

/// The whole application state.
pub struct Model {
    pub settings: Settings,
    pub strings: Box<dyn Strings>,
    pub keymap: KeyMap,
    pub left: Panel,
    pub right: Panel,
    pub focus: Side,
    /// Modal dialog stack; only the last one is interactive.
    pub dialogs: Vec<Dialog>,
    pub status: String,
    /// Command line content below the panels.
    pub command_line: String,
    pub command_history: Vec<String>,
    pub viewer: Option<ViewerState>,
    pub editor: Option<EditorState>,
    pub help: Option<HelpState>,
    pub quit_requested: bool,
    /// Free space of the active panel's volume, refreshed by the view.
    pub free_space: Option<Space>,
    /// Files marked in the *inactive* panel are the default copy targets.
    pub last_error: Option<String>,
    /// Connected remote panels, indexed by [`Side`].
    pub remote_sessions: [Option<remote::RemoteSession>; 2],
    /// Quick-view panel is shown instead of the inactive panel (`Ctrl+Q`).
    pub quick_view: bool,
    /// Directory tree panel replaces the inactive panel.
    pub tree_view: bool,
    /// Running background operations (copies, moves, deletes).
    pub transfers: Vec<OpHandle>,
    /// Counter for operation ids.
    pub next_job_id: u64,
    /// The MCP client panel is open.
    pub show_agent_panel: bool,
    /// The agent chat window is open (Phase 16.1).
    pub show_chat_panel: bool,
    /// Running search, if any.
    pub search_job: Option<search::SearchJob>,
    /// The background transfer manager window is open.
    pub show_transfer_manager: bool,
    /// Selection captured by "save selection" (in-memory restore).
    pub saved_selection: Option<Selection>,
    /// Text the view layer should put on the clipboard (drained every frame).
    pub clipboard: Vec<String>,
    /// Settings changed since they were last written to disk.
    pub settings_dirty: bool,
    /// Where connection credentials are kept. Defaults to an in-memory store, so
    /// nothing is written to disk until the user unlocks the encrypted file.
    pub secrets: Box<dyn SecretStore>,
    /// Master passphrase of the encrypted store, kept in memory for this session
    /// only so newly remembered passwords can be written out immediately.
    pub secret_passphrase: Option<String>,
    /// Secrets waiting for the store to be unlocked: `(account, secret)`.
    pub pending_secrets: Vec<(String, String)>,
    /// Override of the encrypted store location (tests, portable installs).
    pub secrets_path_override: Option<std::path::PathBuf>,
    /// Override of the per-user config directory (tests, portable installs).
    pub config_dir_override: Option<std::path::PathBuf>,
    /// Saved connection profiles (`sessions.toml`), loaded lazily.
    pub sessions_store: rc_core::sessions::SessionStore,
    /// The session store changed on disk since it was loaded.
    pub sessions_dirty: bool,
    /// A connection attempt running on a worker thread.
    pub connect_job: Option<sessions::ConnectJob>,
    /// The user's Git configuration, loaded only after approval.
    pub git_config: Option<rc_git::GitConfig>,
    /// Cached model list of the configured provider, for the model picker.
    pub ai_models: ai::AiModelCache,
    /// Provider override (tests, future provider selection).
    pub ai_provider: Option<std::sync::Arc<dyn rc_llm::Provider>>,
    /// Transport override for routed chat (tests); the real one is `LlmBackend`.
    pub ai_backend: Option<std::sync::Arc<dyn rc_runtime::Backend>>,
    /// The durable token ledger, loaded from this profile's catalogue on first use.
    ///
    /// Shared with the worker thread that dispatches a chat request, so the reservation is taken
    /// and reconciled exactly where the request runs.
    pub quota_ledger: Option<std::sync::Arc<std::sync::Mutex<rc_core::quota::QuotaLedger>>>,
    /// Why the ledger could not be loaded, so a broken profile is reported instead of ignored.
    pub quota_ledger_error: Option<String>,
    /// The last answer of a model test, so the dialog can show it.
    pub last_ai_reply: Option<ai::AiTestResult>,
    /// A model test running on a worker thread.
    pub ai_test_job: Option<ai::AiTestJob>,
    /// Capability policy for everything with side effects (Phase 16.0).
    pub policy: rc_core::policy::Policy,
    /// Append-only trail of what was allowed, refused and approved.
    pub audit: rc_core::audit::AuditLog,
    /// A push started by "commit on save", running on a worker thread.
    pub git_push_job: Option<git::GitNetJob>,
    /// Colour themes found on disk plus the built-ins.
    pub theme_set: rc_core::theme::ThemeSet,
    /// Where themes are read from (tests and portable installs point elsewhere).
    pub themes_dir_override: Option<std::path::PathBuf>,
    /// The agent chat (Phase 16.1).
    pub chat_session: chat::ChatSession,
    /// Requests from a running `rcommander mcp` client waiting for this window's answer.
    pub bridge_requests: std::collections::HashMap<u64, rc_agent::bridge::IncomingRequest>,
    /// Requests that arrived while another dialog was open.
    pub bridge_backlog: std::collections::VecDeque<(
        policy::PendingApproval,
        rc_core::policy::Request,
        String,
        u64,
    )>,
    /// Id for the next bridge request.
    pub next_bridge_id: u64,
    /// Fonts found on this machine, scanned on first use.
    pub fonts: Vec<rc_core::font::FontFile>,
    /// Whether the font scan has happened.
    pub fonts_scanned: bool,
    /// Font choices that could not be loaded, so the options page can say which one failed.
    pub font_problems: Vec<String>,
    /// The prompt catalogue: built-in prompts plus the user's own (Phase 16.16).
    pub prompts: rc_core::prompt::Catalogue,
    /// Whether the catalogue has been read yet.
    pub prompts_loaded: bool,
    /// State of the prompt picker in the chat.
    pub prompt_picker: prompts::PromptPicker,
    /// The prompt editor on the Prompts page (Phase 16.17b).
    pub prompt_editor: prompts::PromptEditor,
    /// A message the user should not miss (error, finished work).
    pub alert: Option<Alert>,
    /// The open "process with agent" workspace (Phase 16.10a).
    pub agent_workspace: Option<AgentWorkspaceState>,
    /// Plugin-provided panel sources (Phase 18.2a). One location variant, many sources.
    pub sources: sources::SourceRegistry,
    /// Every installed plugin, switched on or off, from the last scan.
    pub plugins_loaded: Vec<plugins::PluginSummary>,
    /// Plugin-owned dictionaries, separate from the host translator.
    plugin_strings: std::collections::BTreeMap<String, Vec<rc_i18n::Locale>>,
    /// Manifests the last scan refused, with the reason, for the status line and diagnostics.
    pub plugin_problems: Vec<rc_core::plugin::PluginProblem>,
}

/// Placement of a document surface within the main workspace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocumentPlacement {
    #[default]
    Workspace,
    Panel(Side),
    Floating,
}

impl DocumentPlacement {
    pub fn owns_focus(self, side: Side) -> bool {
        match self {
            Self::Panel(panel) => panel == side,
            _ => true,
        }
    }
}

/// An open F3 viewer.
#[derive(Clone, Debug, Default)]
pub struct ViewerState {
    pub placement: DocumentPlacement,
    pub path: String,
    pub mode: ViewerMode,
    pub scroll: f32,
    pub zoom: f32,
    pub wraps: bool,
    /// Bytes per hex line.
    pub hex_bytes: usize,
    /// Row the user picked in the hexdump, for the detail panel.
    pub hex_selected: usize,
    /// Where "go to offset" points, as typed.
    pub hex_goto: String,
    pub find: Option<String>,
    pub encoding: String,
    pub title: String,
}

/// Viewer display modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewerMode {
    #[default]
    Auto,
    Text,
    Hex,
    Image,
}

/// An open F4 editor.
#[derive(Clone, Debug, Default)]
pub struct EditorState {
    pub placement: DocumentPlacement,
    pub path: String,
    pub text: String,
    pub modified: bool,
    pub dirty_since_open: bool,
    pub line_ending: String,
    pub cursor_line: usize,
    /// The file is not text (NUL bytes or invalid UTF-8), so the editor shows it as such instead of
    /// pretending the bytes are characters.
    pub binary: bool,
    /// Where the window sits: collapsed, maximized, pinned (Phase 14.9).
    pub chrome: crate::ui::workspace_window::Chrome,
}

/// A running "process with agent" workspace.
#[derive(Clone, Debug)]
pub struct AgentWorkspaceState {
    /// The copies, their hashes and the directory they live in.
    pub workspace: rc_core::workspace::Workspace,
    /// What could not be copied, said in the transcript.
    pub skipped: Vec<rc_core::workspace::Skipped>,
    /// Panel hosting the chat of this workspace.
    pub side: Side,
}

/// A message the user should not miss.
#[derive(Clone, Debug)]
pub struct Alert {
    pub text: String,
    pub kind: AlertKind,
    /// When it happened, so the view can fade it or keep it until the next action.
    pub at: std::time::Instant,
    /// Sound to play with it, when the settings allow it.
    pub sound: Option<crate::model::desktop_sound::Sound>,
}

/// How serious an alert is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    /// Something finished.
    Done,
    /// Something went wrong.
    Error,
}

/// The in-app help window.
#[derive(Clone, Debug, Default)]
pub struct HelpState {
    pub topic: Option<String>,
    pub language: String,
    pub search: String,
}

impl Model {
    /// Cold start with a settings record, a translator and two directories.
    pub fn boot(
        settings: Settings,
        strings: Box<dyn Strings>,
        left: Location,
        right: Location,
    ) -> Self {
        let keymap = {
            let mut km = KeyMap::defaults();
            km.apply_overrides(&settings.shortcuts);
            km
        };
        let mut model = Model {
            left: Panel::new(left, &settings),
            right: Panel::new(right, &settings),
            focus: Side::Left,
            dialogs: Vec::new(),
            status: String::new(),
            command_line: String::new(),
            command_history: Vec::new(),
            viewer: None,
            editor: None,
            help: None,
            quit_requested: false,
            free_space: None,
            last_error: None,
            remote_sessions: [None, None],
            quick_view: false,
            tree_view: false,
            transfers: Vec::new(),
            next_job_id: 1,
            show_agent_panel: false,
            show_chat_panel: false,
            search_job: None,
            show_transfer_manager: false,
            saved_selection: None,
            clipboard: Vec::new(),
            settings_dirty: false,
            secrets: Box::new(MemoryStore::new()),
            secret_passphrase: None,
            pending_secrets: Vec::new(),
            secrets_path_override: None,
            config_dir_override: None,
            sessions_store: rc_core::sessions::SessionStore::new(),
            sessions_dirty: false,
            connect_job: None,
            ai_backend: None,
            quota_ledger: None,
            quota_ledger_error: None,
            git_config: None,
            ai_models: ai::AiModelCache::default(),
            ai_provider: None,
            last_ai_reply: None,
            ai_test_job: None,
            policy: rc_core::policy::Policy::restrictive(),
            audit: rc_core::audit::AuditLog::new(crate::paths::audit_path(), false),
            git_push_job: None,
            theme_set: rc_core::theme::ThemeSet::with_builtins(),
            themes_dir_override: None,
            chat_session: chat::ChatSession::default(),
            bridge_requests: std::collections::HashMap::new(),
            bridge_backlog: std::collections::VecDeque::new(),
            next_bridge_id: 1,
            fonts: Vec::new(),
            fonts_scanned: false,
            font_problems: Vec::new(),
            prompts: rc_core::prompt::Catalogue::default(),
            prompts_loaded: false,
            prompt_picker: prompts::PromptPicker::default(),
            prompt_editor: prompts::PromptEditor::default(),
            alert: None,
            agent_workspace: None,
            sources: sources::SourceRegistry::new(),
            plugins_loaded: Vec::new(),
            plugin_strings: std::collections::BTreeMap::new(),
            plugin_problems: Vec::new(),
            keymap,
            settings,
            strings,
        };
        model.load_sessions();
        model.load_themes();
        model.audit = rc_core::audit::AuditLog::new(
            model.config_file("audit.log"),
            settings_audit_enabled(&model),
        );
        model.refresh_policy();
        model.status = model.strings.get("status.ready");
        model
    }

    /// Convenience constructor used by tests: two panels in temp directories.
    pub fn for_test(settings: Settings, left: Location, right: Location) -> Self {
        Model::boot(
            settings,
            Box::new(crate::i18n::KeyEchoStrings::default()),
            left,
            right,
        )
    }

    pub fn panel(&self, side: Side) -> &Panel {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    pub fn panel_mut(&mut self, side: Side) -> &mut Panel {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    pub fn active(&self) -> &Panel {
        self.panel(self.focus)
    }

    pub fn active_mut(&mut self) -> &mut Panel {
        let side = self.focus;
        self.panel_mut(side)
    }

    pub fn inactive(&self) -> &Panel {
        self.panel(self.focus.other())
    }

    pub fn inactive_mut(&mut self) -> &mut Panel {
        let side = self.focus.other();
        self.panel_mut(side)
    }

    /// Switch the active panel (the `Tab` key).
    pub fn switch_focus(&mut self) {
        self.focus = self.focus.other();
    }

    /// The directory of the inactive panel — the default copy target.
    pub fn target_location(&self) -> Location {
        self.inactive().location().clone()
    }

    pub fn direction(&self) -> Direction {
        self.strings.direction()
    }

    /// Text with the current language, ready to display (menu mnemonic markers
    /// removed).
    pub fn t(&self, key: &str) -> String {
        crate::i18n::strip_mnemonics(&self.strings.get(key))
    }

    /// Text with placeholders, ready to display.
    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        crate::i18n::strip_mnemonics(&self.strings.format(key, args))
    }

    /// Open `location` in `side`, keeping the cursor on the previous name when
    /// it still exists.
    pub fn open_in(&mut self, side: Side, location: Location) -> CoreResult<()> {
        if location.is_plugin() {
            // A plugin location is not a file system: the registry on the model is the only thing
            // that knows what is inside, and an unknown source is an error rather than an empty
            // panel. `Panel` itself stays unaware of plugins, which is the point of one variant.
            let settings = self.settings.clone();
            let entries = self.sources.list(&location)?;
            let panel = self.panel_mut(side);
            panel.open_with_entries(location, entries, &settings)?;
            return Ok(());
        }
        if location.is_remote() {
            let keep = self.panel(side).cursor_entry().map(|e| e.name.clone());
            self.open_remote(side, location)?;
            if let Some(name) = keep
                && let Some(index) = self.panel(side).listing.find(&name)
            {
                self.panel_mut(side).set_cursor(index);
            }
            return Ok(());
        }
        let settings = self.settings.clone();
        let panel = self.panel_mut(side);
        let keep = panel.cursor_entry().map(|e| e.name.clone());
        panel.open(location, &settings)?;
        if let Some(name) = keep
            && let Some(i) = panel.listing.find(&name)
        {
            panel.set_cursor(i);
        }
        Ok(())
    }

    /// Open `location` in the active panel.
    pub fn navigate(&mut self, location: Location) -> CoreResult<()> {
        let side = self.focus;
        self.open_in(side, location)
    }

    /// Open `location` in the inactive panel.
    pub fn navigate_other(&mut self, location: Location) -> CoreResult<()> {
        let side = self.focus.other();
        self.open_in(side, location)
    }

    /// Traverse chronological visits in the specified panel, preserving the
    /// forward branch and leaving both stacks unchanged on an unsuccessful open.
    pub fn go_history(&mut self, side: Side, forward: bool) -> CommandOutcome {
        let panel = self.panel(side);
        let current = panel.location().clone();
        let mut back = panel.back_locations.clone();
        let mut next = panel.forward_locations.clone();
        let target = if forward { next.pop() } else { back.pop() };
        let Some(target) = target else {
            return CommandOutcome::None;
        };
        if let Err(error) = self.open_in(side, target) {
            return CommandOutcome::Failed(error.to_string());
        }
        if forward {
            back.push(current);
        } else {
            next.push(current);
        }
        let panel = self.panel_mut(side);
        panel.back_locations = back;
        panel.forward_locations = next;
        self.focus = side;
        CommandOutcome::Ok
    }

    /// Go to the parent directory. The cursor is placed on the directory we
    /// came from, which is what makes `Backspace` navigation pleasant.
    pub fn go_parent(&mut self) -> CoreResult<()> {
        let came_from = self.active().location().file_name();
        let parent = self
            .active()
            .location()
            .parent()
            .ok_or_else(|| CoreError::Invalid("already at root".to_string()))?;
        self.navigate(parent)?;
        if let Some(name) = came_from
            && let Some(index) = self.active().listing.find(&name)
        {
            self.active_mut().set_cursor(index);
        }
        Ok(())
    }

    /// Open the entry under the cursor: directories and (optionally) archives
    /// are entered, files open in the viewer.
    pub fn enter(&mut self) -> CommandOutcome {
        // Parent is navigable, but must stay excluded from ordinary file operations.
        if self.active().cursor_entry().is_some_and(|entry| entry.name == "..") {
            return match self.go_parent() {
                Ok(()) => CommandOutcome::Ok,
                Err(error) => CommandOutcome::Failed(error.to_string()),
            };
        }
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::None;
        };
        // A plugin source answers this question itself: its rows are not always paths, and
        // "open" may mean "go deeper", "show that in the other panel" or "send it".
        if self.active().location().is_plugin() {
            let side = self.focus;
            return self.open_source_entry(side, &entry);
        }
        if entry.is_dir() {
            let location = match self.active().location().child(&entry.name) {
                Some(l) => l,
                None => return CommandOutcome::Failed("invalid name".to_string()),
            };
            return match self.navigate(location) {
                Ok(()) => CommandOutcome::Ok,
                Err(e) => CommandOutcome::Failed(e.to_string()),
            };
        }
        if self.settings.behavior.enter_archives && self.settings.is_archive_name(&entry.name) {
            let location = Location::archive(entry.path.clone(), "");
            return match self.navigate(location) {
                Ok(()) => CommandOutcome::Ok,
                Err(e) => CommandOutcome::Failed(e.to_string()),
            };
        }
        // A plain file: Enter and a double-click mean "do what the system would do", which is what
        // the user expects from a file manager. The built-in viewer stays behind F3.
        CommandOutcome::OpenExternally(entry.path)
    }

    /// Ask the source what opening one of its rows means, and do it.
    ///
    /// The three answers are all the panel can express: go deeper in this panel, show it in the
    /// *other* panel (the preview pairing — a photo, a message body), or run a verb the source
    /// owns. Nothing here is a permission; a source verb that changes something still has to get
    /// through whatever the source itself requires.
    pub fn open_source_entry(&mut self, side: Side, entry: &Entry) -> CommandOutcome {
        let location = self.panel(side).location().clone();
        let Some(source) = self.sources.source_of(&location) else {
            return CommandOutcome::Failed(format!("no plugin provides “{}”", location.display()));
        };
        // Resolve the answer before touching the model: the source borrow ends here.
        let answer = source.open(entry);
        match answer {
            Ok(SourceAction::Into(target)) => match self.open_in(side, target) {
                Ok(()) => CommandOutcome::Ok,
                Err(error) => CommandOutcome::Failed(error.to_string()),
            },
            Ok(SourceAction::Preview(target)) => {
                let other = side.other();
                match self.open_in(other, target) {
                    Ok(()) => {
                        self.focus = other;
                        CommandOutcome::Ok
                    }
                    Err(error) => CommandOutcome::Failed(error.to_string()),
                }
            }
            Ok(SourceAction::Command(command)) => self.run_source_command(side, &command),
            Ok(SourceAction::Done) => CommandOutcome::Ok,
            Err(error) => CommandOutcome::Failed(error.to_string()),
        }
    }

    /// Run a verb the source offered in its row menu.
    ///
    /// The listing is re-read afterwards, because a verb like "mark as read" or "file this
    /// elsewhere" changes what the source shows — and a panel that keeps showing the old rows
    /// would be lying about the result.
    pub fn run_source_command(&mut self, side: Side, command: &str) -> CommandOutcome {
        let location = self.panel(side).location().clone();
        match self.sources.run(&location, command) {
            Ok(()) => {
                if let Err(error) = self.open_in(side, location) {
                    return CommandOutcome::Failed(error.to_string());
                }
                CommandOutcome::Ok
            }
            Err(error) => CommandOutcome::Failed(error.to_string()),
        }
    }

    /// The source verbs the row under the cursor offers, if the panel is a plugin source.
    pub fn source_actions(&self, side: Side) -> Vec<SourceMenuAction> {
        let Some(entry) = self.panel(side).current() else {
            return Vec::new();
        };
        let location = self.panel(side).location();
        self.sources.actions(location, entry)
    }

    /// Re-read both panels.
    pub fn refresh_all(&mut self) {
        let settings = self.settings.clone();
        let _ = self.left.reread(&settings);
        let _ = self.right.reread(&settings);
    }

    /// Re-read the active panel.
    /// Volumes the panel selector offers (system and virtual ones excluded).
    pub fn selectable_volumes(&self) -> Vec<rc_core::drive::Volume> {
        rc_core::drive::pickable_volumes()
    }

    /// The volume the panel's directory sits on, for the selector's current value.
    pub fn current_volume(&self, side: Side) -> Option<rc_core::drive::Volume> {
        let path = self.panel(side).location().as_path()?;
        rc_core::drive::volume_of(path)
    }

    /// Move a panel to a volume root.
    pub fn open_volume(&mut self, side: Side, root: &std::path::Path) -> CommandOutcome {
        match self.open_in(side, Location::local(root)) {
            Ok(()) => CommandOutcome::Ok,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    pub fn refresh_active(&mut self) {
        let settings = self.settings.clone();
        let side = self.focus;
        let _ = self.panel_mut(side).reread(&settings);
    }

    /// Status line text for the active panel.
    pub fn status_text(&self) -> String {
        let panel = self.active();
        let (files, dirs, bytes) = panel.statistics();
        let (count, sel_bytes) = panel.selection_stats();
        let files_text = self.tf("panel.files", &[("count", &files.to_string())]);
        let dirs_text = self.tf("panel.dirs", &[("count", &dirs.to_string())]);
        let size_text = rc_core::format::format_size_bytes(bytes);
        if count == 0 {
            format!("{files_text} {dirs_text} {size_text}")
        } else {
            let sel = rc_core::format::format_size_bytes(sel_bytes);
            self.tf(
                "panel.selection_of",
                &[
                    ("count", &count.to_string()),
                    ("total", &files.to_string()),
                    ("bytes", &sel),
                ],
            )
        }
    }

    /// Window title: application name plus both paths.
    pub fn window_title(&self) -> String {
        format!(
            "RCommander — {} | {}",
            self.left.location().display(),
            self.right.location().display()
        )
    }

    // ----- dialogs -------------------------------------------------------

    /// Open the rescue picker above any current dialog without discarding it.
    pub fn open_language_picker(&mut self) {
        if !matches!(self.top_dialog(), Some(Dialog::LanguagePicker)) {
            self.open_dialog(Dialog::LanguagePicker);
        }
    }

    pub fn open_dialog(&mut self, dialog: Dialog) {
        self.dialogs.push(dialog);
    }

    pub fn close_top_dialog(&mut self) -> Option<Dialog> {
        self.dialogs.pop()
    }

    pub fn close_dialogs(&mut self) {
        self.dialogs.clear();
    }

    pub fn top_dialog(&self) -> Option<&Dialog> {
        self.dialogs.last()
    }

    pub fn top_dialog_mut(&mut self) -> Option<&mut Dialog> {
        self.dialogs.last_mut()
    }

    pub fn has_dialog(&self) -> bool {
        !self.dialogs.is_empty()
    }

    /// The modal blocker: when a dialog is open the panels ignore keys.
    pub fn modal(&self) -> bool {
        self.has_dialog()
    }

    // ----- errors --------------------------------------------------------

    pub fn report_error(&mut self, error: impl std::fmt::Display) {
        let text = error.to_string();
        self.last_error = Some(text.clone());
        self.status = text.clone();
        // An error is also an *alert*: it stays visible until something else happens, and it is
        // the one kind of message the user may also want to hear.
        self.alert = Some(Alert {
            text,
            kind: AlertKind::Error,
            at: std::time::Instant::now(),
            sound: Some(crate::model::desktop_sound::Sound::Problem),
        });
    }

    /// Announce a finished operation that the user was waiting for.
    pub fn report_done(
        &mut self,
        text: impl Into<String>,
        sound: Option<crate::model::desktop_sound::Sound>,
    ) {
        let text = text.into();
        self.status = text.clone();
        self.alert = Some(Alert {
            text,
            kind: AlertKind::Done,
            at: std::time::Instant::now(),
            sound,
        });
    }

    /// Take the pending alert, if one is worth showing or playing.
    pub fn take_alert(&mut self) -> Option<Alert> {
        self.alert.take()
    }

    /// Directory holding per-user files: settings, hotlist, known hosts.
    ///
    /// Tests and portable installs point this somewhere else, so a factory
    /// reset never touches the real profile of the person running the suite.
    pub fn config_dir(&self) -> std::path::PathBuf {
        self.config_dir_override
            .clone()
            .unwrap_or_else(rc_core::config::config_dir)
    }

    /// True when the owner configured model routing, so a chat request must be admitted first.
    ///
    /// An installation that has not migrated declares no connections and keeps the legacy
    /// single-provider path, exactly as before this feature existed.
    pub fn routing_enabled(&self) -> bool {
        !self.settings.agent.model_catalog.connections.is_empty()
    }

    /// The durable token ledger for this profile, loaded once and shared with workers.
    ///
    /// `Ok(None)` means routing is not enabled. `Err` means the owner enabled routing but the
    /// ledger cannot be read: the chat then refuses to send rather than spending unaccounted
    /// tokens, because an unusable ledger is not the same as no budget.
    pub fn quota_ledger(
        &mut self,
    ) -> Result<Option<std::sync::Arc<std::sync::Mutex<rc_core::quota::QuotaLedger>>>, String> {
        if let Some(ledger) = &self.quota_ledger {
            return Ok(Some(ledger.clone()));
        }
        if !self.routing_enabled() {
            return Ok(None);
        }
        if let Some(error) = &self.quota_ledger_error {
            return Err(error.clone());
        }
        let directory = self.config_dir();
        match rc_core::quota::QuotaLedger::load_in(&directory, &self.settings.agent.model_catalog) {
            Ok(ledger) => {
                let shared = std::sync::Arc::new(std::sync::Mutex::new(ledger));
                self.quota_ledger = Some(shared.clone());
                Ok(Some(shared))
            }
            Err(error) => {
                let message = format!("{}: {error}", directory.display());
                self.quota_ledger_error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// Remember that the settings changed; the view layer persists them.
    pub fn mark_settings_dirty(&mut self) {
        self.settings_dirty = true;
    }

    /// True while a search is running, so the status line can say so.
    pub fn is_searching(&self) -> bool {
        self.search_job.is_some()
    }

    pub fn report(&mut self, text: impl Into<String>) {
        self.status = text.into();
        // A plain message replaces an older alert: the newest thing said is the one that matters.
        self.alert = None;
    }
}

/// Result of executing one internal command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandOutcome {
    /// Nothing happened (unknown command, no-op).
    None,
    /// The command ran successfully.
    Ok,
    /// The command failed with a message.
    Failed(String),
    /// A file should be opened in the viewer by the view layer.
    OpenFile(std::path::PathBuf),
    /// Hand the file to the platform's default action: run it if it is a program, otherwise open
    /// it in whatever the system associates with it.
    ///
    /// This is what Enter and a double-click mean; `OpenFile` is the built-in viewer behind F3.
    OpenExternally(std::path::PathBuf),
    /// The application should exit.
    Quit,
    /// The window should be minimized.
    Minimize,
}

impl CommandOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, CommandOutcome::Ok)
    }
}

/// Build the read options for a location without a panel (used by helpers).
pub fn default_read_options(settings: &Settings) -> ReadOptions {
    ReadOptions {
        show_hidden: settings.display.show_hidden,
        show_parent: settings.display.show_parent_entry,
        mask: MaskSet::parse_or_all(&settings.display.file_mask).unwrap_or_default(),
    }
}

/// Keyboard support: translate a shortcut into a command name using the model's
/// key map. Returns `None` when the key is not bound.
pub fn command_for_key(model: &Model, shortcut: &Shortcut) -> Option<String> {
    model.keymap.lookup(shortcut).map(|s| s.to_string())
}

/// Map an entry to the kind of operation the UI should offer.
pub fn entry_kind_of(entry: &Entry) -> EntryKind {
    entry.kind
}

#[cfg(test)]
mod tests;

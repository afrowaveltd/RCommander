//! Headless application model.
//!
//! Everything the user can do is expressed here as a method that mutates the
//! model and returns a [`CoreResult`] or a [`CommandOutcome`]. The egui layer is
//! a pure view over this state, which is what makes the behaviour testable
//! without a display.

pub mod commands;
pub mod dialogs;
pub mod remote;
pub mod search;

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
use rc_core::selection::Selection;
use rc_core::sorting::{SortField, SortSpec};
use rc_core::tabs::TabBar;

pub use dialogs::Dialog;

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
    pub listing: Listing,
    pub selection: Selection,
    pub history: DirHistory,
    pub quick: QuickSearch,
    pub view_mode: ViewMode,
    /// Mask used to filter files (`*.*` = everything).
    pub mask_spec: String,
    /// Last read error, shown in the panel instead of entries.
    pub error: Option<String>,
    /// Path typed into the inline path editor (None = not editing).
    pub path_edit: Option<String>,
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
            listing,
            selection: Selection::new(),
            history,
            quick: QuickSearch::new(),
            view_mode,
            mask_spec,
            error: None,
            path_edit: None,
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
        let listing = match crate::archive_bridge::read_archive_listing(&location) {
            Some(result) => {
                let entries = result.map_err(CoreError::Invalid)?;
                Listing::from_entries(location.clone(), entries, &opts, sort)
            }
            None => Listing::read(&location, &opts, sort)?,
        };
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
    /// Running search, if any.
    pub search_job: Option<search::SearchJob>,
}

/// An open F3 viewer.
#[derive(Clone, Debug, Default)]
pub struct ViewerState {
    pub path: String,
    pub mode: ViewerMode,
    pub scroll: f32,
    pub zoom: f32,
    pub wraps: bool,
    /// Bytes per hex line.
    pub hex_bytes: usize,
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
    pub path: String,
    pub text: String,
    pub modified: bool,
    pub dirty_since_open: bool,
    pub line_ending: String,
    pub cursor_line: usize,
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
            search_job: None,
            keymap,
            settings,
            strings,
        };
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

    /// Text with the current language.
    pub fn t(&self, key: &str) -> String {
        self.strings.get(key)
    }

    /// Text with placeholders.
    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.strings.format(key, args)
    }

    /// Open `location` in `side`, keeping the cursor on the previous name when
    /// it still exists.
    pub fn open_in(&mut self, side: Side, location: Location) -> CoreResult<()> {
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
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::None;
        };
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
        // A plain file: the view layer opens the F3 window.
        CommandOutcome::OpenFile(entry.path)
    }

    /// Re-read both panels.
    pub fn refresh_all(&mut self) {
        let settings = self.settings.clone();
        let _ = self.left.reread(&settings);
        let _ = self.right.reread(&settings);
    }

    /// Re-read the active panel.
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
        self.status = text;
    }

    pub fn report(&mut self, text: impl Into<String>) {
        self.status = text.into();
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

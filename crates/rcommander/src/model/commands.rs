//! Internal command dispatch.
//!
//! Every user action (menu entry, keyboard shortcut, button bar, MCP tool) is
//! funnelled through [`Model::execute`], which takes a command identifier such
//! as `cm_Copy` and returns what the view layer should do next.

use crate::model::dialogs::*;
use crate::model::{CommandOutcome, Model, Side, ViewMode};
use rc_core::entry::Entry;
use rc_core::error::CoreError;
use rc_core::keymap::Shortcut;
use rc_core::location::Location;
use rc_core::mask::MaskSet;
use rc_core::ops::{self, CancelToken, OverwriteMode, TransferOptions};
use rc_core::sorting::{SortField, SortSpec};
use std::path::{Path, PathBuf};

/// Substitute multi-rename placeholders in `name`.
///
/// Supported: `[N]` name without extension, `[n]` lower-case name, `[U]`
/// upper-case name, `[E]` extension (empty for none), `[P]` parent directory
/// name, `[T1]`…`[T9]` directory levels from the file upwards, `[C]` counter,
/// `[C<n>]`, `[C+<step>]`, `[C-<step>]`, `[C<digits>]`, and the date parts
/// `[Y] [M] [D] [h] [m] [s]`. Unknown placeholders stay verbatim.
pub fn apply_rename_pattern(
    name: &str,
    pattern: &str,
    ext_pattern: &str,
    counter: u64,
    digits: usize,
    search: &str,
    replace: &str,
) -> String {
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();

    let parent_name = std::path::Path::new(&std::env::current_dir().unwrap_or_default())
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let render = |pat: &str, counter: u64, digits: usize| -> String {
        let mut out = String::new();
        let mut rest = pat;
        while let Some(start) = rest.find('[') {
            out.push_str(&rest[..start]);
            let tail = &rest[start..];
            let Some(end) = tail.find(']') else {
                out.push_str(tail);
                return out;
            };
            let token = &tail[1..end];
            rest = &tail[end + 1..];
            let value = match token {
                "N" => stem.clone(),
                "n" => stem.to_lowercase(),
                "U" => stem.to_uppercase(),
                "E" => ext.clone(),
                "e" => ext.to_lowercase(),
                "P" => parent_name.clone(),
                "C" => format_counter(counter, digits),
                "Y" => chrono::Local::now().format("%Y").to_string(),
                "M" => chrono::Local::now().format("%m").to_string(),
                "D" => chrono::Local::now().format("%d").to_string(),
                "h" => chrono::Local::now().format("%H").to_string(),
                "mi" => chrono::Local::now().format("%M").to_string(),
                "s" => chrono::Local::now().format("%S").to_string(),
                other => {
                    if let Some(lvl) = other
                        .strip_prefix('T')
                        .and_then(|n| n.parse::<usize>().ok())
                    {
                        let mut parts: Vec<String> = Vec::new();
                        let mut p = Path::new(name);
                        while let Some(parent) = p.parent() {
                            if let Some(n) = parent.file_name() {
                                parts.push(n.to_string_lossy().to_string());
                            }
                            if parent.as_os_str().is_empty() {
                                break;
                            }
                            p = parent;
                        }
                        parts
                            .get(lvl.saturating_sub(1))
                            .cloned()
                            .unwrap_or_default()
                    } else if let Some(spec) = other.strip_prefix('C') {
                        counter_spec(spec, counter, digits)
                    } else {
                        format!("[{other}]")
                    }
                }
            };
            out.push_str(&value);
        }
        out.push_str(rest);
        out
    };

    let mut result = render(pattern, counter, digits);
    if !search.is_empty() {
        result = result.replace(search, replace);
    }
    let ext_text = render(ext_pattern, counter, digits);
    if ext_text.trim().is_empty() {
        result
    } else {
        format!("{result}.{}", ext_text.trim_start_matches('.'))
    }
}

fn format_counter(value: u64, digits: usize) -> String {
    if digits == 0 {
        value.to_string()
    } else {
        format!("{value:0digits$}")
    }
}

fn counter_spec(spec: &str, counter: u64, digits: usize) -> String {
    // [C+2] steps forward, [C-2] steps back, [C4] pads to 4 digits.
    if let Some(step) = spec.strip_prefix('+') {
        let step = step.parse::<u64>().unwrap_or(0);
        return format_counter(counter.saturating_add(step), digits);
    }
    if let Some(step) = spec.strip_prefix('-') {
        let step = step.parse::<u64>().unwrap_or(0);
        return format_counter(counter.saturating_sub(step), digits);
    }
    if let Ok(d) = spec.parse::<usize>() {
        return format_counter(counter, d);
    }
    format_counter(counter, digits)
}

impl Model {
    /// Execute one internal command by name.
    pub fn execute(&mut self, command: &str) -> CommandOutcome {
        let normalized = normalize_command(command);
        let outcome = match normalized.as_str() {
            // ---- panel navigation -------------------------------------
            "cm_gotoparent" => match self.go_parent() {
                Ok(()) => CommandOutcome::Ok,
                Err(e) => CommandOutcome::Failed(e.to_string()),
            },
            "cm_rereadsource" => {
                self.refresh_active();
                CommandOutcome::Ok
            }
            "cm_goback" => self.go_back(),
            "cm_goforward" => self.go_forward(),
            "cm_opendirinotherpanel" => self.open_cursor_in_other_panel(),
            "cm_exchange" => {
                std::mem::swap(&mut self.left, &mut self.right);
                CommandOutcome::Ok
            }
            "cm_matchsrc" => self.match_target_to_source(),
            // ---- tabs -------------------------------------------------
            "cm_newtab" => self.new_tab(),
            "cm_closetab" => self.close_tab(),
            "cm_nexttab" => self.cycle_tab(true),
            "cm_prevtab" => self.cycle_tab(false),

            // ---- extra panels ----------------------------------------
            "cm_srcquickview" => {
                self.quick_view = !self.quick_view;
                if self.quick_view {
                    self.tree_view = false;
                }
                CommandOutcome::Ok
            }
            "cm_separatetree1" => {
                self.tree_view = !self.tree_view;
                if self.tree_view {
                    self.quick_view = false;
                }
                CommandOutcome::Ok
            }

            // ---- dialogs ---------------------------------------------
            "cm_historylist" => {
                let side = self.focus;
                let rows: Vec<HistoryRow> = self
                    .panel(side)
                    .history
                    .entries()
                    .iter()
                    .map(|e| HistoryRow {
                        path: e.path.to_string_lossy().to_string(),
                        cursor_name: e.cursor.clone(),
                    })
                    .collect();
                self.open_dialog(Dialog::History(HistoryState::new(rows)));
                CommandOutcome::Ok
            }
            "cm_cdtree" => {
                let root = self.active().location().display();
                let mut state = TreeState {
                    root: root.clone(),
                    include_files: false,
                    ..Default::default()
                };
                state.entries = self.collect_tree_entries();
                self.open_dialog(Dialog::Tree(state));
                CommandOutcome::Ok
            }
            "cm_comparedirs" => self.compare_directories(),
            "cm_srcuserdef" => {
                let mask = self.active().mask_spec.clone();
                self.open_dialog(Dialog::MaskPrompt(MaskPromptState::new(mask)));
                CommandOutcome::Ok
            }
            "cm_opendiropenarchive" | "cm_opendirorarchive" => self.enter(),
            "cm_openincommandline" => self.insert_cursor_into_command_line(),

            "cm_switchpanel" => {
                self.switch_focus();
                CommandOutcome::Ok
            }
            "cm_enter" => self.enter(),

            // ---- view modes -------------------------------------------
            "cm_srcshort" => self.set_view_mode(ViewMode::Brief),
            "cm_srclong" => self.set_view_mode(ViewMode::Full),
            "cm_srccomments" => self.set_view_mode(ViewMode::Comments),
            "cm_srcthumbs" => self.set_view_mode(ViewMode::Thumbnails),
            "cm_srctree" => self.set_view_mode(ViewMode::Tree),
            "cm_srcallfiles" => self.set_filter("*.*"),
            "cm_srcexecs" => self.set_filter("*"),
            "cm_branchview" => self.set_branch_view(),

            // ---- sorting ----------------------------------------------
            "cm_srcbyname" => self.sort_active(SortField::Name),
            "cm_srcbyext" => self.sort_active(SortField::Extension),
            "cm_srcbysize" => self.sort_active(SortField::Size),
            "cm_srcbydatetime" => self.sort_active(SortField::ModifiedTime),
            "cm_srcunsorted" => self.sort_active(SortField::Unsorted),
            "cm_srcnegorder" => {
                let spec = self.active().sort();
                let panel = self.active_mut();
                panel.set_sort(spec.reversed());
                CommandOutcome::Ok
            }

            // ---- selection --------------------------------------------
            "cm_selectall" => {
                let entries: Vec<Entry> = self.active().entries().to_vec();
                self.active_mut().selection.select_all(&entries, true);
                CommandOutcome::Ok
            }
            "cm_clearall" => {
                self.active_mut().selection.clear();
                CommandOutcome::Ok
            }
            "cm_exchangeselection" => {
                let list: Vec<Entry> = self.active().entries().to_vec();
                self.active_mut().selection.invert(&list, true);
                CommandOutcome::Ok
            }
            "cm_toggleselection" => {
                self.active_mut().toggle_current();
                CommandOutcome::Ok
            }
            "cm_toggleselectionanddown" => {
                self.active_mut().toggle_and_advance();
                CommandOutcome::Ok
            }
            "cm_selectsameext" => self.select_same_extension(true),
            "cm_deselectsameext" => self.select_same_extension(false),
            "cm_restoreselection" => {
                self.report(self.t("status.ready"));
                CommandOutcome::Ok
            }

            // ---- file operations --------------------------------------
            "cm_copy" => self.open_transfer_dialog(false),
            "cm_copysamepanel" => self.open_transfer_dialog(false),
            "cm_renmov" => self.open_transfer_dialog(true),
            "cm_renameonly" => self.open_rename_dialog(),
            "cm_mkdir" => {
                self.open_dialog(Dialog::MakeDir(MakeDirState::default()));
                CommandOutcome::Ok
            }
            "cm_mkdirtarget" => {
                self.open_dialog(Dialog::MakeDir(MakeDirState {
                    in_target_panel: true,
                    ..Default::default()
                }));
                CommandOutcome::Ok
            }
            "cm_delete" => self.open_delete_dialog(self.settings.behavior.use_trash),
            "cm_deletepermanent" => self.open_delete_dialog(false),
            "cm_list" | "cm_listinternalonly" => self.open_viewer(),
            "cm_edit" => self.open_editor(false),
            "cm_editnew" => self.open_editor(true),
            "cm_printfile" => {
                self.report("printing is not implemented yet");
                CommandOutcome::None
            }

            // ---- dialogs ----------------------------------------------
            "cm_searchfor" | "cm_searchstandalone" => {
                let root = self.active().location().display();
                self.open_dialog(Dialog::Search(SearchState::new(root)));
                CommandOutcome::Ok
            }
            "cm_packfiles" => {
                let sources = self.selected_paths();
                let name = self.suggest_archive_name();
                self.open_dialog(Dialog::Pack(PackState::new(sources, name)));
                CommandOutcome::Ok
            }
            "cm_unpackfiles" => {
                let archive = self
                    .active()
                    .current()
                    .map(|e| e.path.to_string_lossy().to_string())
                    .unwrap_or_default();
                let dest = self.target_location().display();
                self.open_dialog(Dialog::Unpack(UnpackState::new(archive, dest)));
                CommandOutcome::Ok
            }
            "cm_testarchive" => self.test_archive(),
            "cm_multirename" => {
                let sources = self.active().selected_names();
                let mut state = MultiRenameState::new(sources);
                state.refresh_preview();
                self.open_dialog(Dialog::MultiRename(state));
                CommandOutcome::Ok
            }
            "cm_setattrib" => {
                let paths = self.selected_paths();
                self.open_dialog(Dialog::Attributes(AttributesState {
                    paths,
                    ..Default::default()
                }));
                CommandOutcome::Ok
            }
            "cm_editcomment" => self.open_comment_dialog(),
            "cm_directoryhotlist" => {
                self.open_dialog(Dialog::Hotlist(HotlistState::default()));
                CommandOutcome::Ok
            }
            "cm_config" => {
                self.open_dialog(Dialog::Options(OptionsState::new(
                    &self.settings,
                    OptionsTab::Display,
                )));
                CommandOutcome::Ok
            }
            "cm_about" => {
                self.open_dialog(Dialog::About);
                CommandOutcome::Ok
            }
            "cm_helpindex" => {
                self.open_help(None);
                CommandOutcome::Ok
            }
            "cm_versioninfo" => self.show_properties(),
            "cm_getfilespace" => self.calculate_space(),
            "cm_ftpdisconnect" => {
                let side = self.focus;
                match self.disconnect_remote(side) {
                    Ok(()) => CommandOutcome::Ok,
                    Err(e) => CommandOutcome::Failed(e.to_string()),
                }
            }
            "cm_ftpconnect" | "cm_ftpnewconnection" => {
                self.open_dialog(Dialog::FtpConnect(FtpConnectState::default()));
                CommandOutcome::Ok
            }
            "cm_synchronizedirs" => {
                let left = self.panel(Side::Left).location().display();
                let right = self.panel(Side::Right).location().display();
                self.open_dialog(Dialog::Synchronize(SyncState {
                    left,
                    right,
                    ..Default::default()
                }));
                CommandOutcome::Ok
            }
            "cm_themetoggle" => {
                let next = if self.settings.colors.theme == "dark" {
                    "light"
                } else {
                    "dark"
                };
                self.settings.colors.theme = next.to_string();
                CommandOutcome::Ok
            }
            "cm_exit" => CommandOutcome::Quit,
            "cm_minimize" => CommandOutcome::Minimize,
            "cm_savesettings" => {
                match self.settings.save(&rc_core::config::default_config_path()) {
                    Ok(()) => CommandOutcome::Ok,
                    Err(e) => CommandOutcome::Failed(e.to_string()),
                }
            }

            // ---- command line ----------------------------------------
            "cm_opencommandlineindir" => {
                self.command_line.clear();
                CommandOutcome::Ok
            }
            "cm_commandlinehistory" => {
                if let Some(last) = self.command_history.last().cloned() {
                    self.command_line = last;
                }
                CommandOutcome::Ok
            }

            "" => CommandOutcome::None,
            _ => CommandOutcome::None,
        };
        if let CommandOutcome::Failed(msg) = &outcome {
            self.report_error(msg);
        }
        outcome
    }

    // ----- helper commands -------------------------------------------

    fn set_view_mode(&mut self, mode: ViewMode) -> CommandOutcome {
        self.settings.display.view_mode = mode.as_str().to_string();
        self.active_mut().view_mode = mode;
        CommandOutcome::Ok
    }

    fn set_filter(&mut self, mask: &str) -> CommandOutcome {
        self.active_mut().mask_spec = mask.to_string();
        self.refresh_active();
        CommandOutcome::Ok
    }

    fn set_branch_view(&mut self) -> CommandOutcome {
        self.settings.display.view_mode = "tree".to_string();
        self.active_mut().view_mode = ViewMode::Tree;
        CommandOutcome::Ok
    }

    fn sort_active(&mut self, field: SortField) -> CommandOutcome {
        self.active_mut().sort_by(field);
        CommandOutcome::Ok
    }

    fn select_same_extension(&mut self, select: bool) -> CommandOutcome {
        let Some(name) = self.active().current().map(|e| e.name.clone()) else {
            return CommandOutcome::None;
        };
        let entries: Vec<Entry> = self.active().entries().to_vec();
        let panel = self.active_mut();
        if select {
            panel.selection.select_same_extension(&entries, &name, true);
        } else {
            panel
                .selection
                .deselect_same_extension(&entries, &name, true);
        }
        CommandOutcome::Ok
    }

    fn go_back(&mut self) -> CommandOutcome {
        let current = self.active().location().as_path().map(|p| p.to_path_buf());
        let Some(current) = current else {
            return CommandOutcome::None;
        };
        let target = self
            .active()
            .history
            .previous(&current)
            .map(|e| e.path.clone());
        match target {
            Some(path) => match self.navigate(Location::local(path)) {
                Ok(()) => CommandOutcome::Ok,
                Err(e) => CommandOutcome::Failed(e.to_string()),
            },
            None => CommandOutcome::None,
        }
    }

    fn go_forward(&mut self) -> CommandOutcome {
        let current = self.active().location().as_path().map(|p| p.to_path_buf());
        let Some(current) = current else {
            return CommandOutcome::None;
        };
        let target = self.active().history.next(&current).map(|e| e.path.clone());
        match target {
            Some(path) => match self.navigate(Location::local(path)) {
                Ok(()) => CommandOutcome::Ok,
                Err(e) => CommandOutcome::Failed(e.to_string()),
            },
            None => CommandOutcome::None,
        }
    }

    fn open_cursor_in_other_panel(&mut self) -> CommandOutcome {
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::None;
        };
        let location = if entry.is_dir() {
            match self.active().location().child(&entry.name) {
                Some(l) => l,
                None => return CommandOutcome::None,
            }
        } else {
            self.active().location().clone()
        };
        match self.navigate_other(location) {
            Ok(()) => CommandOutcome::Ok,
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    fn match_target_to_source(&mut self) -> CommandOutcome {
        let location = self.active().location().clone();
        let side = self.focus.other();
        match self.open_in(side, location) {
            Ok(()) => CommandOutcome::Ok,
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Absolute paths of the operation targets (marks, else cursor).
    pub fn selected_paths(&self) -> Vec<String> {
        self.active()
            .operation_targets()
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect()
    }

    /// Suggested archive name for the pack dialog.
    pub fn suggest_archive_name(&self) -> String {
        let base = self
            .active()
            .current()
            .map(|e| e.stem())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "archive".to_string());
        format!("{base}.zip")
    }

    fn open_transfer_dialog(&mut self, move_it: bool) -> CommandOutcome {
        let sources = self.selected_paths();
        if sources.is_empty() {
            self.report(self.t("error.not_found"));
            return CommandOutcome::None;
        }
        let dest = self.target_location().display();
        let mut state = TransferDialogState::new(move_it, sources, dest);
        state.background = self.settings.copy.background;
        state.verify = self.settings.copy.verify;
        state.preserve_times = self.settings.copy.preserve_times;
        self.open_dialog(Dialog::Transfer(state));
        CommandOutcome::Ok
    }

    fn open_rename_dialog(&mut self) -> CommandOutcome {
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::None;
        };
        self.open_dialog(Dialog::Rename(RenameState::new(entry.name)));
        CommandOutcome::Ok
    }

    fn open_delete_dialog(&mut self, use_trash: bool) -> CommandOutcome {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return CommandOutcome::None;
        }
        self.open_dialog(Dialog::Delete(DeleteState::new(paths, !use_trash)));
        CommandOutcome::Ok
    }

    fn open_viewer(&mut self) -> CommandOutcome {
        match self.active().current().cloned() {
            Some(entry) if !entry.is_dir() => CommandOutcome::OpenFile(entry.path),
            _ => CommandOutcome::None,
        }
    }

    fn open_editor(&mut self, new_file: bool) -> CommandOutcome {
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::None;
        };
        if entry.is_dir() {
            return CommandOutcome::None;
        }
        if new_file {
            let path = self
                .active()
                .location()
                .as_path()
                .map(|p| p.join("new.txt"));
            if let Some(path) = path {
                return CommandOutcome::OpenFile(path);
            }
            return CommandOutcome::None;
        }
        CommandOutcome::OpenFile(entry.path)
    }

    fn open_comment_dialog(&mut self) -> CommandOutcome {
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::None;
        };
        let comments = self
            .active()
            .location()
            .as_path()
            .map(rc_core::comment::Comments::load_dir)
            .unwrap_or_default();
        let text = comments.get(&entry.name).unwrap_or("").to_string();
        let path = self.active().location().display();
        self.open_dialog(Dialog::Comment(CommentState::new(path, entry.name, text)));
        CommandOutcome::Ok
    }

    fn show_properties(&mut self) -> CommandOutcome {
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::None;
        };
        let text = format!(
            "{} — {} — {}",
            entry.name,
            rc_core::format::format_size_bytes(entry.size),
            rc_core::format::format_datetime(entry.mtime)
        );
        self.report(text);
        CommandOutcome::Ok
    }

    fn calculate_space(&mut self) -> CommandOutcome {
        let targets = self.active().operation_targets();
        if targets.is_empty() {
            return CommandOutcome::None;
        }
        let cancel = CancelToken::new();
        let mut bytes = 0u64;
        let mut files = 0u64;
        for entry in &targets {
            match ops::measure(&entry.path, &cancel) {
                Ok((b, f)) => {
                    bytes += b;
                    files += f;
                }
                Err(_) => continue,
            }
        }
        self.report(format!(
            "{files} files, {}",
            rc_core::format::format_size_unit(bytes)
        ));
        CommandOutcome::Ok
    }

    fn test_archive(&mut self) -> CommandOutcome {
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::None;
        };
        if !self.settings.is_archive_name(&entry.name) {
            self.report("not an archive");
            return CommandOutcome::None;
        }
        self.report(format!("archive test of {} requested", entry.name));
        CommandOutcome::Ok
    }

    /// Open the in-app help window.
    pub fn open_help(&mut self, topic: Option<String>) {
        let language = self.settings.language.clone();
        self.help = Some(crate::model::HelpState {
            topic,
            language,
            search: String::new(),
        });
    }

    /// Open the F3 viewer for `path`.
    pub fn open_viewer_for(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        self.viewer = Some(crate::model::ViewerState {
            path: path.to_string_lossy().to_string(),
            hex_bytes: self.settings.lister.hex_bytes_per_line,
            encoding: self.settings.lister.encoding.clone(),
            title: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            ..Default::default()
        });
    }

    /// Open the F4 editor for `path`, loading the text.
    pub fn open_editor_for(&mut self, path: impl Into<PathBuf>) -> std::io::Result<()> {
        let path = path.into();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        self.editor = Some(crate::model::EditorState {
            path: path.to_string_lossy().to_string(),
            text,
            line_ending: "lf".to_string(),
            ..Default::default()
        });
        Ok(())
    }

    // ----- keyboard --------------------------------------------------

    /// Handle a normalised keyboard shortcut. Returns true when it was consumed.
    pub fn handle_key(&mut self, shortcut: &Shortcut) -> bool {
        if let Some(cmd) = self.keymap.lookup(shortcut).map(|s| s.to_string()) {
            let outcome = self.execute(&cmd);
            return !matches!(outcome, CommandOutcome::None);
        }
        false
    }

    /// Handle a printable character (quick search / command line).
    pub fn handle_char(&mut self, ch: char) -> bool {
        if ch.is_control() {
            return false;
        }
        let hit = self.active_mut().quick_search_push(ch);
        hit.is_some()
    }

    /// Clear the quick-search buffer (Esc).
    pub fn cancel_quick_search(&mut self) {
        self.active_mut().quick.reset();
    }

    // ----- command line ----------------------------------------------

    /// Execute the text in the command line: `cd <path>`, `cm_*` commands, or a
    /// shell command through the OS.
    pub fn run_command_line(&mut self) -> CommandOutcome {
        let text = self.command_line.trim().to_string();
        if text.is_empty() {
            return CommandOutcome::None;
        }
        self.command_history.push(text.clone());
        self.command_line.clear();
        if let Some(rest) = text.strip_prefix("cd ") {
            let target = rest.trim();
            let location = match Location::parse(target) {
                Ok(l) => l,
                Err(e) => return CommandOutcome::Failed(e.to_string()),
            };
            return match self.navigate(location) {
                Ok(()) => CommandOutcome::Ok,
                Err(e) => CommandOutcome::Failed(e.to_string()),
            };
        }
        if text.starts_with("cm_") {
            return self.execute(&text);
        }
        CommandOutcome::None
    }

    // ----- resolved operations (used by dialogs and tests) ------------

    /// Run a copy/move described by the transfer dialog.
    pub fn run_transfer(&mut self, state: &TransferDialogState) -> CommandOutcome {
        let destination = PathBuf::from(state.destination.trim());
        let mut options = TransferOptions {
            overwrite: state.overwrite,
            preserve_times: state.preserve_times,
            ..Default::default()
        };
        if state.ask_overwrite {
            options.overwrite = OverwriteMode::Skip;
        }
        let cancel = CancelToken::new();
        let mut files = 0u64;
        let mut failures: Vec<String> = Vec::new();
        for source in &state.sources {
            let src = PathBuf::from(source);
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let target = destination.join(&name);
            let result = if state.move_it {
                ops::move_path(&src, &target, &options, &cancel)
            } else {
                ops::copy(&src, &target, &options, &cancel, None)
            };
            match result {
                Ok(outcome) => files += outcome.files.max(1),
                Err(e) if e.is_cancelled() => break,
                Err(e) => failures.push(format!("{name}: {e}")),
            }
        }
        self.close_dialogs();
        if self.settings.behavior.refresh_after_operation {
            self.refresh_all();
        }
        if failures.is_empty() {
            self.report(self.tf("status.done", &[("count", &files.to_string())]));
            CommandOutcome::Ok
        } else {
            let message = failures.join("; ");
            self.report_error(&message);
            CommandOutcome::Failed(message)
        }
    }

    /// Run a delete described by the delete dialog.
    pub fn run_delete(&mut self, state: &DeleteState) -> CommandOutcome {
        let cancel = CancelToken::new();
        let mut removed = 0u64;
        let mut failures = Vec::new();
        for path in &state.paths {
            match ops::remove(Path::new(path), &cancel) {
                Ok(n) => removed += n,
                Err(e) if e.is_cancelled() => break,
                Err(e) => failures.push(format!("{path}: {e}")),
            }
        }
        self.close_dialogs();
        if self.settings.behavior.refresh_after_operation {
            self.refresh_all();
        }
        if failures.is_empty() {
            self.report(self.tf("status.done", &[("count", &removed.to_string())]));
            CommandOutcome::Ok
        } else {
            let message = failures.join("; ");
            self.report_error(&message);
            CommandOutcome::Failed(message)
        }
    }

    /// Create a directory from the dialog state.
    pub fn run_mkdir(&mut self, state: &MakeDirState) -> CommandOutcome {
        if let Err(e) = state.validate() {
            return CommandOutcome::Failed(e);
        }
        let side = if state.in_target_panel {
            self.focus.other()
        } else {
            self.focus
        };
        let base = self.panel(side).location().clone();
        let location = match base.child(state.name.trim()) {
            Some(l) => l,
            None => return CommandOutcome::Failed("invalid name".to_string()),
        };
        let Some(path) = location.as_path() else {
            return CommandOutcome::Failed("cannot create directories here".to_string());
        };
        let outcome = match ops::create_dir(path) {
            Ok(()) => CommandOutcome::Ok,
            Err(e) => CommandOutcome::Failed(e.to_string()),
        };
        self.close_dialogs();
        self.refresh_all();
        outcome
    }

    /// Rename the cursor entry from the dialog state.
    pub fn run_rename(&mut self, state: &RenameState) -> CommandOutcome {
        if let Err(e) = state.validate() {
            return CommandOutcome::Failed(e);
        }
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::Failed("nothing selected".to_string());
        };
        let new_name = state.effective_name();
        let Some(target) = entry.path.parent().map(|p| p.join(&new_name)) else {
            return CommandOutcome::Failed("cannot rename".to_string());
        };
        let outcome = match ops::rename(&entry.path, &target) {
            Ok(()) => CommandOutcome::Ok,
            Err(e) => CommandOutcome::Failed(e.to_string()),
        };
        self.close_dialogs();
        self.refresh_active();
        outcome
    }

    /// Apply the attributes dialog to the file system.
    pub fn run_attributes(&mut self, state: &AttributesState) -> CommandOutcome {
        if let Err(e) = state.validate() {
            return CommandOutcome::Failed(e);
        }
        let mut failures = Vec::new();
        for path in &state.paths {
            let p = Path::new(path);
            if let Some(ro) = state.readonly
                && let Ok(md) = std::fs::metadata(p)
            {
                let mut perms = md.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(ro);
                if let Err(e) = std::fs::set_permissions(p, perms) {
                    failures.push(format!("{path}: {e}"));
                }
            }
            #[cfg(unix)]
            if let Some(mode) = state.unix_mode {
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)) {
                    failures.push(format!("{path}: {e}"));
                }
            }
        }
        self.close_dialogs();
        self.refresh_all();
        if failures.is_empty() {
            CommandOutcome::Ok
        } else {
            CommandOutcome::Failed(failures.join("; "))
        }
    }

    /// Save a file comment to `descript.ion`.
    pub fn run_comment(&mut self, state: &CommentState) -> CommandOutcome {
        let dir = PathBuf::from(&state.path);
        let mut comments = rc_core::comment::Comments::load_dir(&dir);
        comments.set(state.name.clone(), state.text.clone());
        let outcome = match comments.save_dir(&dir) {
            Ok(()) => CommandOutcome::Ok,
            Err(e) => CommandOutcome::Failed(e.to_string()),
        };
        self.close_dialogs();
        outcome
    }

    /// Apply a multi-rename batch to the file system.
    pub fn run_multi_rename(&mut self, state: &MultiRenameState) -> CommandOutcome {
        if let Err(e) = state.validate() {
            return CommandOutcome::Failed(e);
        }
        let dir = self.active().location().as_path().map(|p| p.to_path_buf());
        let Some(dir) = dir else {
            return CommandOutcome::Failed("not a local directory".to_string());
        };
        let mut renames: Vec<(PathBuf, PathBuf)> = Vec::new();
        for (i, name) in state.sources.iter().enumerate() {
            let counter =
                (state.counter_start as i64 + state.counter_step as i64 * i as i64).max(0) as u64;
            let new_name = apply_rename_pattern(
                name,
                &state.pattern,
                &state.extension_pattern,
                counter,
                state.counter_digits,
                &state.search_for,
                &state.replace_with,
            );
            if new_name == *name || new_name.trim().is_empty() {
                continue;
            }
            renames.push((dir.join(name), dir.join(new_name)));
        }
        let mut failures = Vec::new();
        for (from, to) in renames {
            if to.exists() {
                failures.push(format!("{} exists", to.display()));
                continue;
            }
            if let Err(e) = ops::rename(&from, &to) {
                failures.push(format!("{}: {e}", from.display()));
            }
        }
        self.close_dialogs();
        self.refresh_active();
        if failures.is_empty() {
            CommandOutcome::Ok
        } else {
            CommandOutcome::Failed(failures.join("; "))
        }
    }

    // ----- tabs ------------------------------------------------------

    /// Open a new tab next to the active one, showing the same directory.
    pub fn new_tab(&mut self) -> CommandOutcome {
        let side = self.focus;
        let location = self.panel(side).location().clone();
        let tab = rc_core::tabs::PanelTab {
            sort: self.panel(side).sort(),
            mask: self.panel(side).mask_spec.clone(),
            ..rc_core::tabs::PanelTab::new(location)
        };
        self.panel_mut(side).tabs.add(tab, true);
        CommandOutcome::Ok
    }

    /// Close the active tab and show the tab that becomes active.
    pub fn close_tab(&mut self) -> CommandOutcome {
        let side = self.focus;
        let index = self.panel(side).tabs.active_index();
        if !self.panel_mut(side).tabs.close(index) {
            self.report("cannot close the last tab");
            return CommandOutcome::None;
        }
        let location = self.panel(side).tabs.active().location.clone();
        let settings = self.settings.clone();
        match self.panel_mut(side).open(location, &settings) {
            Ok(()) => CommandOutcome::Ok,
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Cycle the active tab forwards or backwards.
    pub fn cycle_tab(&mut self, forward: bool) -> CommandOutcome {
        let side = self.focus;
        let tabs = &mut self.panel_mut(side).tabs;
        if forward {
            tabs.next();
        } else {
            tabs.previous();
        }
        let location = tabs.active().location.clone();
        let settings = self.settings.clone();
        match self.panel_mut(side).open(location, &settings) {
            Ok(()) => CommandOutcome::Ok,
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    // ----- extra panels ---------------------------------------------

    /// Directory names below the current directory, used by the tree dialog.
    pub fn collect_tree_entries(&self) -> Vec<String> {
        let Some(root) = self.active().location().as_path().map(|p| p.to_path_buf()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        let mut guard = 0;
        while let Some(dir) = stack.pop() {
            guard += 1;
            if guard > 2000 {
                break;
            }
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut children: Vec<std::path::PathBuf> = read
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            children.sort();
            for child in children {
                out.push(child.to_string_lossy().to_string());
                stack.push(child);
            }
        }
        out
    }

    /// Remove a quick filter mask and show everything again.
    pub fn clear_mask(&mut self) -> CommandOutcome {
        self.active_mut().mask_spec = "*.*".to_string();
        self.refresh_active();
        CommandOutcome::Ok
    }

    /// Apply a mask from the mask dialog.
    pub fn apply_mask(&mut self, state: &MaskPromptState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        self.active_mut().mask_spec = state.mask.trim().to_string();
        self.close_dialogs();
        self.refresh_active();
        CommandOutcome::Ok
    }

    /// Put the file name under the cursor into the command line.
    pub fn insert_cursor_into_command_line(&mut self) -> CommandOutcome {
        let Some(entry) = self.active().current().cloned() else {
            return CommandOutcome::None;
        };
        if !self.command_line.is_empty() && !self.command_line.ends_with(' ') {
            self.command_line.push(' ');
        }
        self.command_line.push_str(&entry.name);
        CommandOutcome::Ok
    }

    /// Compare the two panels name by name and report the differences.
    pub fn compare_directories(&mut self) -> CommandOutcome {
        let left_entries = self.panel(Side::Left).entries().to_vec();
        let right_entries = self.panel(Side::Right).entries().to_vec();
        let mut rows: Vec<CompareRow> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in left_entries.iter().filter(|e| e.name != "..") {
            let other = right_entries.iter().find(|e| e.name == entry.name);
            let row = match other {
                None => CompareRow {
                    name: entry.name.clone(),
                    status: if entry.is_dir() {
                        "<DIR>".to_string()
                    } else {
                        "<".to_string()
                    },
                    left_size: Some(entry.size),
                    right_size: None,
                },
                Some(other) => {
                    let equal = entry.size == other.size
                        && entry.mtime == other.mtime
                        && entry.is_dir() == other.is_dir();
                    CompareRow {
                        name: entry.name.clone(),
                        status: if equal {
                            "=".to_string()
                        } else {
                            "!=".to_string()
                        },
                        left_size: Some(entry.size),
                        right_size: Some(other.size),
                    }
                }
            };
            seen.insert(entry.name.clone());
            rows.push(row);
        }
        for entry in right_entries
            .iter()
            .filter(|e| e.name != ".." && !seen.contains(&e.name))
        {
            rows.push(CompareRow {
                name: entry.name.clone(),
                status: if entry.is_dir() { "<DIR>" } else { ">" }.to_string(),
                left_size: None,
                right_size: Some(entry.size),
            });
        }
        rows.sort_by_key(|a| a.name.to_lowercase());
        let _different = rows.iter().filter(|r| r.status != "=").count();
        let state = CompareState {
            rows,
            mark_different: true,
        };
        if state.mark_different {
            let names: Vec<String> = state
                .rows
                .iter()
                .filter(|r| r.status != "=")
                .map(|r| r.name.clone())
                .collect();
            let side = self.focus;
            let panel = self.panel_mut(side);
            panel.selection.clear();
            for name in names {
                panel.selection.insert(name);
            }
        }
        self.report(state.summary());
        self.open_dialog(Dialog::Compare(state));
        CommandOutcome::Ok
    }

    /// Switch the UI language.
    pub fn set_language(&mut self, code: &str) {
        self.settings.language = code.to_string();
        self.report(self.t("status.ready"));
    }
}

/// Normalise a command identifier: trim, map a numeric id to its name, and
/// accept lower case variants.
pub fn normalize_command(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(info) = rc_core::command::resolve(trimmed) {
        return info.name.to_ascii_lowercase();
    }
    trimmed.to_ascii_lowercase()
}

/// Sort helper used by the view layer's column headers.
pub fn spec_for_column(spec: SortSpec, field: SortField) -> SortSpec {
    if spec.field == field {
        spec.reversed()
    } else {
        SortSpec {
            field,
            reversed: false,
            ..spec
        }
    }
}

/// Parse a mask string, falling back to "everything" on a syntax error.
pub fn safe_mask(spec: &str) -> MaskSet {
    MaskSet::parse_or_all(spec).unwrap_or_default()
}

/// Human readable error text for the status bar.
pub fn error_text(error: &CoreError) -> String {
    error.to_string()
}

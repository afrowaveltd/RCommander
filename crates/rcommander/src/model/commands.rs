//! Internal command dispatch.
//!
//! Every user action (menu entry, keyboard shortcut, button bar, MCP tool) is
//! funnelled through [`Model::execute`], which takes a command identifier such
//! as `cm_Copy` and returns what the view layer should do next.

use crate::model::dialogs::SearchHit;
use crate::model::dialogs::*;
use crate::model::{CommandOutcome, Model, Side, ViewMode};
use crate::ops_runner::{OpKind, OpResult};
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
        // `cm_Left…`/`cm_Right…` are the same commands bound to one panel and
        // activating it, exactly like `cm_Src…` acting on the source panel.
        // Explicit local navigation, never an external program. Keep case on Linux.
        if let Some(path) = command.strip_prefix("rc_OpenDirectory:") {
            let path = Path::new(path);
            if !path.is_absolute() || !path.is_dir() {
                return CommandOutcome::Failed(self.t("toolbar.directory_missing"));
            }
            return match self.open_in(self.focus, Location::local(path)) {
                Ok(()) => CommandOutcome::Ok,
                Err(e) => CommandOutcome::Failed(e.to_string()),
            };
        }
        let normalized = normalize_command(command);
        let (forced_side, normalized) = match panel_prefix(&normalized) {
            Some((side, rewritten)) => (Some(side), rewritten),
            None => (None, normalized),
        };
        if let Some(side) = forced_side {
            self.focus = side;
        }
        // A place that does not allow an operation refuses it here, before anything is
        // staged, recorded or asked about. A plugin source that never declared support for
        // deletion cannot have its rows deleted however the command arrives — function key,
        // menu, script or MCP tool — and no approval dialog pretends otherwise.
        if !self.allows(self.focus, &normalized) {
            let command = normalized.clone();
            return CommandOutcome::Failed(
                self.t("cmd.not_available_here").replace("{cmd}", &command),
            );
        }
        // Numbered families: `cm_ActivateTab3`, `cm_SortByCol2`, `cm_ViewMode4`.
        if let Some(number) = trailing_number(&normalized) {
            if normalized.starts_with("cm_activatetab")
                || normalized.starts_with("cm_srcactivatetab")
                || normalized.starts_with("cm_trgactivatetab")
            {
                return self.activate_tab_number(number);
            }
            if normalized.starts_with("cm_sortbycol")
                || normalized.starts_with("cm_srcsortbycol")
                || normalized.starts_with("cm_trgsortbycol")
            {
                return self.sort_by_column_number(number);
            }
            if normalized.starts_with("cm_viewmode")
                || normalized.starts_with("cm_srcviewmode")
                || normalized.starts_with("cm_trgviewmode")
            {
                return self.apply_view_mode_number(number);
            }
        }
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
            // Plugins are data: re-reading the manifest directory is the whole reload.
            "cm_reloadplugins" => {
                let summary = self.load_plugins();
                self.report(summary);
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
            "cm_opennewtab" | "cm_newtab" => self.new_tab(),
            "cm_closecurrenttab" | "cm_closetab" => self.close_tab(),
            "cm_switchtonexttab" | "cm_nexttab" => self.cycle_tab(true),
            "cm_switchtoprevioustab" | "cm_prevtab" => self.cycle_tab(false),

            // ---- extra panels ----------------------------------------
            "cm_srcquickview" => {
                self.quick_view = !self.quick_view;
                if self.quick_view {
                    self.tree_view = false;
                }
                CommandOutcome::Ok
            }
            "cm_toggleseparatetree1" | "cm_separatetree1" => {
                self.tree_view = !self.tree_view;
                if self.tree_view {
                    self.quick_view = false;
                }
                CommandOutcome::Ok
            }

            // ---- dialogs ---------------------------------------------
            "cm_directoryhistory" | "cm_historylist" => {
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
            "cm_addfilenametocmdline" | "cm_openincommandline" => {
                self.insert_cursor_into_command_line()
            }

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
            "cm_dirbranch" | "cm_branchview" => self.toggle_branch_view(),

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
            "cm_selectcurrentnameext" | "cm_selectsameext" => self.select_same_extension(true),
            "cm_unselectcurrentnameext" | "cm_deselectsameext" => self.select_same_extension(false),
            // ---- file operations --------------------------------------
            "cm_copy" => self.open_transfer_dialog(false),
            "cm_copysamepanel" => self.open_transfer_dialog(false),
            "cm_renmov" => self.open_transfer_dialog(true),
            "cm_renameonly" => self.open_rename_dialog(),
            "cm_mkdir" => {
                self.open_dialog(Dialog::MakeDir(MakeDirState::default()));
                CommandOutcome::Ok
            }
            "cm_mkdirother" | "cm_mkdirtarget" => {
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
            "cm_editnewfile" | "cm_editnew" => self.open_editor(true),
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
            "cm_multirenamefiles" | "cm_multirename" => {
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
            "cm_opentransfermanager" => {
                self.show_transfer_manager = true;
                CommandOutcome::Ok
            }
            "cm_encode" => {
                let sources = self.selected_paths();
                self.open_dialog(Dialog::Encode(EncodeState::new(sources)));
                CommandOutcome::Ok
            }
            "cm_decode" => {
                let source = self
                    .active()
                    .current()
                    .map(|e| e.path.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.open_dialog(Dialog::Decode(DecodeState::new(source)));
                CommandOutcome::Ok
            }
            "cm_saveselectiontofile" => {
                let count = self.active().selection.len();
                self.open_dialog(Dialog::SaveSelection(SaveSelectionState::new(count)));
                CommandOutcome::Ok
            }
            "cm_loadselectionfromfile" => {
                self.open_dialog(Dialog::LoadSelection(LoadSelectionState::new()));
                CommandOutcome::Ok
            }
            "cm_saveselection" => {
                self.saved_selection = Some(self.active().selection.clone());
                self.report(self.tf(
                    "panel.selection_of",
                    &[("count", &self.active().selection.len().to_string())],
                ));
                CommandOutcome::Ok
            }
            "cm_restoreselection" => match self.saved_selection.clone() {
                Some(selection) => {
                    let entries: Vec<Entry> = self.active().entries().to_vec();
                    let mut restored = selection;
                    restored.retain_existing(&entries);
                    let count = restored.len();
                    self.active_mut().selection = restored;
                    self.report(self.tf("panel.selection_of", &[("count", &count.to_string())]));
                    CommandOutcome::Ok
                }
                None => CommandOutcome::None,
            },
            "cm_split" | "cm_splitfile" => {
                let source = self
                    .active()
                    .current()
                    .map(|e| e.path.to_string_lossy().to_string())
                    .unwrap_or_default();
                let destination = self.active().location().display();
                self.open_dialog(Dialog::Split(SplitState::new(source, destination)));
                CommandOutcome::Ok
            }
            "cm_combine" | "cm_combinefiles" => {
                let first = self
                    .active()
                    .current()
                    .map(|e| e.path.to_string_lossy().to_string())
                    .unwrap_or_default();
                let destination = rc_core::split::combined_name(std::path::Path::new(&first))
                    .to_string_lossy()
                    .to_string();
                self.open_dialog(Dialog::Combine(CombineState::new(first, destination)));
                CommandOutcome::Ok
            }
            "cm_crccreate" | "cm_createchecksumfile" | "cm_checksums" => {
                let sources = self.selected_paths();
                let base = self.active().location().display();
                self.open_dialog(Dialog::Checksum(ChecksumState::new(sources, base)));
                CommandOutcome::Ok
            }
            "cm_crccheck" | "cm_verifychecksums" => {
                let file = self
                    .active()
                    .current()
                    .map(|e| e.path.to_string_lossy().to_string())
                    .unwrap_or_default();
                let base = self.active().location().display();
                self.open_dialog(Dialog::VerifyChecksum(VerifyChecksumState::new(file, base)));
                CommandOutcome::Ok
            }
            "cm_comparefilesbycontent" => {
                let left = self
                    .active()
                    .current()
                    .map(|e| e.path.to_string_lossy().to_string())
                    .unwrap_or_default();
                let right = self
                    .panel(self.focus.other())
                    .current()
                    .map(|e| e.path.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.open_dialog(Dialog::CompareFiles(CompareFilesState::new(left, right)));
                CommandOutcome::Ok
            }
            "cm_findduplicates" | "cm_duplicates" => {
                let root = self.active().location().display();
                self.open_dialog(Dialog::Duplicates(DuplicatesState::new(root)));
                CommandOutcome::Ok
            }
            "cm_customcolumnviewmenu" | "cm_configcustomcolumns" => {
                self.open_dialog(Dialog::ViewModes(ViewModeDialogState::default()));
                CommandOutcome::Ok
            }
            "cm_copynamestoclip" => self.copy_names_to_clipboard(false, false),
            "cm_copyfullnamestoclip" | "cm_copynetnamestoclip" => {
                self.copy_names_to_clipboard(true, false)
            }
            "cm_copyfilestoclip" | "cm_copydetailstoclip" | "cm_copyfpfiledetailstoclip" => {
                self.copy_names_to_clipboard(true, true)
            }
            "cm_copysrcpathtoclip" => {
                let path = self.panel(Side::Left).location().display();
                self.copy_to_clipboard(path);
                CommandOutcome::Ok
            }
            "cm_copytrgpathtoclip" => {
                let path = self.panel(Side::Right).location().display();
                self.copy_to_clipboard(path);
                CommandOutcome::Ok
            }
            "cm_visbuttonbar" | "cm_visbuttonbar2" => self.toggle_layout("buttonbar"),
            "cm_visdrivebuttons"
            | "cm_vistwodrivebuttons"
            | "cm_visflatdrivebuttons"
            | "cm_visdrivecombo" => self.toggle_layout("drivebuttons"),
            "cm_viscmdline" => self.toggle_layout("cmdline"),
            "cm_viskeybuttons" => self.toggle_layout("keybuttons"),
            "cm_visstatusbar" => self.toggle_layout("statusbar"),
            "cm_visdirtabs" => self.toggle_layout("dirtabs"),
            "cm_buttonadd" => {
                let mut state = ButtonBarState::new();
                state.start_add();
                self.open_dialog(Dialog::ButtonBar(state));
                CommandOutcome::Ok
            }
            "cm_buttonadddirectory" => self.add_current_directory_button(),
            "cm_mcptools" => {
                self.show_agent_panel = true;
                CommandOutcome::Ok
            }
            "cm_plugins" => self.open_options_on(OptionsTab::Plugins),
            "cm_secrets" => {
                self.open_dialog(Dialog::UnlockSecrets(UnlockSecretsState::new(
                    self.secrets_exist(),
                )));
                CommandOutcome::Ok
            }
            "cm_buttonconfig" => {
                self.open_dialog(Dialog::ButtonBar(ButtonBarState::new()));
                CommandOutcome::Ok
            }
            "cm_config" => self.open_options_on(OptionsTab::Display),
            // The options pages that grew into features of their own deserve a way in that does
            // not make the user find the page first.
            "cm_fonts" | "cm_fontoptions" => self.open_options_on(OptionsTab::Fonts),
            "cm_security" | "cm_securityoptions" => self.open_options_on(OptionsTab::Security),
            "cm_colors" | "cm_coloroptions" => self.open_options_on(OptionsTab::Colors),
            "cm_dangerous" | "cm_resets" => self.open_options_on(OptionsTab::Dangerous),
            "cm_prompts" | "cm_promptmanager" => self.open_options_on(OptionsTab::Prompts),
            "cm_about" => {
                self.open_dialog(Dialog::About);
                CommandOutcome::Ok
            }
            "cm_helpindex" => {
                // F1 answers for the surface it was pressed on: help about the chat when the chat
                // has the focus, about the editor when the editor does.
                let topic = self.surface().help_topic().to_string();
                self.open_help(Some(topic));
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
            "cm_ftpconnect" | "cm_ftpnew" | "cm_ftpnewconnection" => {
                self.open_dialog(Dialog::FtpConnect(FtpConnectState::default()));
                CommandOutcome::Ok
            }
            "cm_filesync" | "cm_synchronizedirs" => {
                let left = self.panel(Side::Left).location().display();
                let right = self.panel(Side::Right).location().display();
                self.open_dialog(Dialog::Synchronize(SyncState {
                    left,
                    right,
                    ..Default::default()
                }));
                CommandOutcome::Ok
            }
            "cm_gitconfig" | "cm_gitsettings" | "cm_git" => self.request_git_config(),
            "cm_gitpanel" | "cm_opengitrepository" => self.open_git_panel(),
            "cm_gitbranches" | "cm_gitcheckout" | "cm_gitswitchbranch" => self.open_git_branches(),
            "cm_gitremote" | "cm_gitfetch" | "cm_gitpull" | "cm_gitpush" => self.open_git_remote(),
            "cm_gitcopyout" => self.copy_from_git(),
            "cm_gitstage" => self.git_stage_selection(),
            "cm_gitunstage" => self.git_unstage_selection(),
            "cm_gitcommit" => self.open_git_commit(),
            "cm_gitclone" => self.open_git_clone(),
            "cm_aimodel" | "cm_aimodels" | "cm_model" => {
                let _ = self.refresh_ai_models();
                self.open_dialog(Dialog::AiModel(
                    crate::model::dialogs::AiModelState::default(),
                ));
                CommandOutcome::Ok
            }
            "cm_srcchat" | "cm_leftchat" => self.toggle_panel_chat(crate::model::Side::Left),
            "cm_trgchat" | "cm_rightchat" => self.toggle_panel_chat(crate::model::Side::Right),
            "cm_chatpane" | "cm_chatbesidefiles" => self.open_chat_beside_files(),
            "cm_chatfiles" | "cm_closechatpane" => {
                let focus = self.focus;
                let mut outcome = self.set_panel_chat(focus, false);
                // A second panel showing the chat would leave no file list at all, so close it
                // as well; asking twice for "give me the files back" must not need two keys.
                let other = focus.other();
                if self.panel_shows_chat(other) {
                    let second = self.set_panel_chat(other, false);
                    if matches!(outcome, CommandOutcome::None) {
                        outcome = second;
                    }
                }
                outcome
            }
            "cm_aichat" | "cm_chat" | "cm_agentchat" => {
                // Opening the chat is also the first place a model list is needed, so a
                // fresh installation does not have to open the picker first.
                if self.ai_models.models.is_empty() {
                    let _ = self.refresh_ai_models();
                }
                self.show_chat_panel = true;
                CommandOutcome::Ok
            }
            // The keyboard's own row over a conversation (Phase 14.9). These live in the model so
            // the bar, the shortcut and the future TUI all reach the same implementation.
            "cm_chatclear" => self.chat_clear(),
            "cm_chatstop" => self.chat_stop(),
            "cm_chatexport" => self.chat_export(),
            "cm_chatsaveprompt" => self.chat_save_prompt(),
            "cm_promptpicker" => {
                self.prompt_picker.open = true;
                CommandOutcome::Ok
            }
            "cm_editorsave" => self.editor_save(),
            "cm_editorsavecopy" => self.editor_save_copy(),
            "cm_editorreload" => self.editor_reload(),
            "cm_editorclose" => self.editor_close(),
            "cm_viewerclose" => {
                self.viewer = None;
                CommandOutcome::Ok
            }
            "cm_viewertext" | "cm_viewerhex" | "cm_viewerimage" => self.viewer_set_mode(command),
            "cm_viewerzoomin" | "cm_viewerzoomout" => {
                self.viewer_zoom(command == "cm_viewerzoomin")
            }
            // "Process with agent": the file-to-agent handoff (Phase 16.10a).
            "cm_processwithagent" | "cm_agentworkspace" => self.process_with_agent(),
            "cm_discardworkspace" => self.discard_workspace(),
            "cm_reviewworkspace" | "cm_applyworkspace" => {
                if self.agent_workspace.is_none() {
                    let message = self.t("workspace.none").to_string();
                    self.report_error(&message);
                    CommandOutcome::Failed(message)
                } else {
                    // A fresh review every time the dialog opens: a diff from an hour ago is not a
                    // review.
                    let review = self.review_workspace();
                    self.open_dialog(Dialog::WorkspaceReview(
                        crate::model::dialogs::WorkspaceReviewState::new(review),
                    ));
                    CommandOutcome::Ok
                }
            }
            "cm_openworkspace" => self.open_workspace_directory(),
            "cm_themetoggle" => self.toggle_theme(),
            "cm_themenext" => self.cycle_theme(1),
            "cm_themeprevious" => self.cycle_theme(-1),
            "cm_exit" => CommandOutcome::Quit,
            "cm_minimize" => CommandOutcome::Minimize,
            "cm_configsavesettings" | "cm_savesettings" => {
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
            "cm_showcmdlinehistory" | "cm_commandlinehistory" => {
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
        self.mark_settings_dirty();
        self.active_mut().view_mode = mode;
        CommandOutcome::Ok
    }

    fn set_filter(&mut self, mask: &str) -> CommandOutcome {
        self.active_mut().mask_spec = mask.to_string();
        self.refresh_active();
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
        if self.editor.as_ref().is_some_and(|state| state.modified) {
            return Err(std::io::Error::other(self.t("editor.modified")));
        }
        let path = path.into();
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        // A file that is not text is shown as binary rather than as mojibake, and saving it is
        // refused: an editor that silently rewrites bytes it could not read is a data-loss bug.
        let binary = looks_binary(&bytes);
        let text = if binary {
            String::new()
        } else {
            String::from_utf8_lossy(&bytes).to_string()
        };
        let line_ending = if text.contains("\r\n") { "crlf" } else { "lf" }.to_string();
        self.editor = Some(crate::model::EditorState {
            path: path.to_string_lossy().to_string(),
            text,
            line_ending,
            binary,
            ..Default::default()
        });
        Ok(())
    }

    /// Where a copy of `path` could be saved when the original cannot be written.
    ///
    /// The documents folder is the honest answer: it exists on every platform we support and the
    /// user certainly owns it. The file name keeps the original one so nothing is lost in the
    /// translation.
    pub fn suggest_copy_path(&self, path: &std::path::Path) -> PathBuf {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled.txt".to_string());
        let base = rc_core::drive::home_dir()
            .map(|home| home.join("Documents"))
            .filter(|documents| documents.is_dir())
            .or_else(rc_core::drive::home_dir)
            .unwrap_or_else(std::env::temp_dir);
        // Never overwrite an existing file with a copy: the user's documents folder is theirs.
        let mut candidate = base.join(&name);
        let mut counter = 1;
        while candidate.exists() {
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled".to_string());
            let extension = path
                .extension()
                .map(|extension| format!(".{}", extension.to_string_lossy()))
                .unwrap_or_default();
            candidate = base.join(format!("{stem} ({counter}){extension}"));
            counter += 1;
        }
        candidate
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

    // ----- background operations -------------------------------------

    /// Start a copy or move in the background and return the job id.
    ///
    /// The job is then advanced by [`Model::poll_transfers`], so the UI keeps
    /// running while gigabytes are copied.
    pub fn start_transfer(&mut self, state: &TransferDialogState) -> Option<u64> {
        if state.validate().is_err() {
            return None;
        }
        let id = self.next_job_id;
        self.next_job_id += 1;
        let sources: Vec<PathBuf> = state.sources.iter().map(PathBuf::from).collect();
        let destination = PathBuf::from(state.destination.trim());
        let kind = if state.move_it {
            OpKind::Move
        } else {
            OpKind::Copy
        };
        let handle =
            crate::ops_runner::spawn_transfer(id, kind, sources, destination, state.overwrite);
        self.transfers.push(handle);
        self.close_dialogs();
        self.report(self.t("status.copying"));
        Some(id)
    }

    /// Start a delete in the background.
    pub fn start_delete(&mut self, state: &DeleteState) -> u64 {
        let id = self.next_job_id;
        self.next_job_id += 1;
        let targets: Vec<PathBuf> = state.paths.iter().map(PathBuf::from).collect();
        self.transfers
            .push(crate::ops_runner::spawn_delete(id, targets));
        self.close_dialogs();
        id
    }

    /// Advance the running jobs and drop the finished ones. Returns true while
    /// at least one job is still running.
    pub fn poll_transfers(&mut self) -> bool {
        let mut finished: Vec<OpResult> = Vec::new();
        for handle in self.transfers.iter_mut() {
            if !handle.poll() {
                finished.push(handle.result.clone().unwrap_or(OpResult::Cancelled));
            }
        }
        self.transfers.retain(|handle| !handle.is_finished());
        let mut refresh = false;
        for result in finished {
            match result {
                OpResult::Finished { files, bytes } => {
                    let text = format!(
                        "{} ({})",
                        self.t("status.done"),
                        rc_core::format::format_size_bytes(bytes)
                    );
                    self.report(text);
                    let _ = files;
                    refresh = true;
                }
                OpResult::Cancelled => {
                    self.report(self.t("status.cancelled"));
                    refresh = true;
                }
                OpResult::Failed(message) => self.report_error(message),
            }
        }
        if refresh && self.settings.behavior.refresh_after_operation {
            self.refresh_all();
        }
        !self.transfers.is_empty()
    }

    /// Cancel one operation by its job id.
    pub fn cancel_transfer(&mut self, id: u64) -> bool {
        match self.transfers.iter().find(|handle| handle.id == id) {
            Some(handle) => {
                handle.cancel();
                true
            }
            None => false,
        }
    }

    /// Cancel every running operation.
    pub fn cancel_transfers(&mut self) {
        for handle in self.transfers.iter() {
            handle.cancel();
        }
    }

    /// Aggregate progress of the running jobs: (fraction 0..1, current file).
    pub fn transfer_progress(&self) -> Option<(f32, String)> {
        if self.transfers.is_empty() {
            return None;
        }
        let total: f32 = self.transfers.iter().map(|h| h.progress.fraction()).sum();
        let fraction = total / self.transfers.len() as f32;
        let current = self.transfers.iter().find_map(|h| {
            if h.progress.current_file.is_empty() {
                None
            } else {
                Some(h.progress.current_file.clone())
            }
        });
        Some((fraction, current.unwrap_or_default()))
    }

    /// True while a background operation runs.
    pub fn is_busy(&self) -> bool {
        !self.transfers.is_empty()
    }

    // ----- clipboard and layout toggles -------------------------------

    /// Request the view layer to copy `text` to the system clipboard.
    pub fn copy_to_clipboard(&mut self, text: impl Into<String>) {
        self.clipboard.push(text.into());
    }

    /// Copy the names of the marked files (or the cursor file) to the clipboard.
    pub fn copy_names_to_clipboard(
        &mut self,
        with_path: bool,
        with_details: bool,
    ) -> CommandOutcome {
        let entries = self.active().operation_targets();
        if entries.is_empty() {
            return CommandOutcome::None;
        }
        let lines: Vec<String> = entries
            .iter()
            .map(|entry| {
                let mut line = if with_path {
                    entry.path.to_string_lossy().to_string()
                } else {
                    entry.name.clone()
                };
                if with_details {
                    line.push_str(&format!(
                        " {}",
                        rc_core::format::format_size_bytes(entry.size)
                    ));
                    let date = rc_core::format::format_datetime(entry.mtime);
                    if !date.is_empty() {
                        line.push_str(&format!(" {date}"));
                    }
                }
                line
            })
            .collect();
        let count = lines.len();
        self.copy_to_clipboard(lines.join(
            "
",
        ));
        self.report(self.tf("panel.selection_of", &[("count", &count.to_string())]));
        CommandOutcome::Ok
    }

    /// Toggle one layout element and report the new state.
    fn toggle_layout(&mut self, element: &str) -> CommandOutcome {
        let layout = &mut self.settings.layout;
        let value = match element {
            "buttonbar" | "buttonbar2" => &mut layout.show_button_bar,
            "drivebuttons" | "twodrivebuttons" | "flatdrivebuttons" | "drivecombo" => {
                &mut layout.show_drive_bar
            }
            "cmdline" => &mut layout.show_command_line,
            "keybuttons" => &mut layout.show_function_key_bar,
            "statusbar" => &mut layout.show_status_bar,
            "dirtabs" => &mut layout.show_tabs,
            _ => return CommandOutcome::None,
        };
        *value = !*value;
        let state = if *value { "on" } else { "off" };
        self.mark_settings_dirty();
        self.report(format!("{element}: {state}"));
        CommandOutcome::Ok
    }

    // ----- numbered command families ----------------------------------

    /// Activate tab `number` (1-based) of the active panel.
    pub fn activate_tab_number(&mut self, number: usize) -> CommandOutcome {
        if number == 0 {
            return CommandOutcome::None;
        }
        let index = number - 1;
        let settings = self.settings.clone();
        let panel = self.active_mut();
        if index >= panel.tabs.len() {
            // A number beyond the last tab opens a new tab, like the reference does.
            let location = panel.location().clone();
            panel
                .tabs
                .add(rc_core::tabs::PanelTab::new(location), false);
        }
        let side = self.focus;
        match self.panel_mut(side).activate_tab(index, &settings) {
            Ok(()) => CommandOutcome::Ok,
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Sort by the column at position `number` (1-based) of the configured columns.
    pub fn sort_by_column_number(&mut self, number: usize) -> CommandOutcome {
        let columns = self.settings.display.columns.clone();
        let Some(column) = number.checked_sub(1).and_then(|index| columns.get(index)) else {
            return CommandOutcome::None;
        };
        let field = match column.id.as_str() {
            "ext" => SortField::Extension,
            "size" => SortField::Size,
            "date" => SortField::ModifiedTime,
            "name" => SortField::Name,
            _ => return CommandOutcome::None,
        };
        self.sort_active(field);
        CommandOutcome::Ok
    }

    /// Apply the `number`-th saved view mode (1-based).
    pub fn apply_view_mode_number(&mut self, number: usize) -> CommandOutcome {
        let modes = self.settings.display.view_modes.clone();
        match number
            .checked_sub(1)
            .and_then(|index| modes.get(index))
            .map(|mode| mode.name.clone())
        {
            Some(name) => self.apply_view_mode(&name),
            None => CommandOutcome::None,
        }
    }

    // ----- credentials -------------------------------------------------

    /// Path of the encrypted store file.
    pub fn secrets_file(&self) -> std::path::PathBuf {
        self.secrets_path_override
            .clone()
            .or_else(|| {
                self.config_dir_override
                    .as_ref()
                    .map(|dir| dir.join("secrets.enc"))
            })
            .unwrap_or_else(crate::paths::secrets_path)
    }

    /// True when a store file exists on disk (it then needs a passphrase).
    pub fn secrets_exist(&self) -> bool {
        self.secrets_file().is_file()
    }

    /// True when the encrypted store is unlocked in this session.
    pub fn secrets_unlocked(&self) -> bool {
        self.secret_passphrase.is_some()
    }

    /// Number of secrets the active store holds.
    pub fn secrets_count(&self) -> usize {
        self.secrets.accounts().len()
    }

    /// Delete the encrypted store file, for the "start over" path and the factory
    /// reset. Returns true when a file was removed.
    pub fn reset_secret_store(&mut self) -> bool {
        self.pending_secrets.clear();
        self.secret_passphrase = None;
        self.secrets = Box::new(rc_core::secrets::MemoryStore::new());
        let path = self.secrets_file();
        std::fs::remove_file(&path).is_ok()
    }

    /// Open the encrypted store with `passphrase`, creating it when it does not
    /// exist yet. Secrets that were waiting for the store are written out.
    pub fn unlock_secrets(&mut self, passphrase: &str) -> Result<(), String> {
        if passphrase.is_empty() {
            return Err("the passphrase must not be empty".to_string());
        }
        let path = self.secrets_file();
        let mut store = if path.is_file() {
            rc_core::secrets::EncryptedStore::load(&path, passphrase).map_err(|e| e.to_string())?
        } else {
            rc_core::secrets::EncryptedStore::new(path.clone())
        };
        // Move everything that was waiting into the store.
        let pending: Vec<(String, String)> = self.pending_secrets.drain(..).collect();
        for (account, secret) in pending {
            store.insert(account, secret);
        }
        if store.is_dirty() || !path.is_file() {
            store.save(passphrase).map_err(|e| e.to_string())?;
        }
        self.secrets = Box::new(store);
        self.secret_passphrase = Some(passphrase.to_string());
        let count = self.secrets.accounts().len().to_string();
        self.report(self.tf("dialog.secrets.unlocked", &[("count", &count)]));
        Ok(())
    }

    /// Forget the passphrase and drop back to the in-memory store.
    pub fn lock_secrets(&mut self) {
        if let Some(passphrase) = self.secret_passphrase.take() {
            // Persist anything that changed before locking.
            if let Err(e) = self.persist_secrets_with(&passphrase) {
                self.report_error(e);
            }
        }
        self.secrets = Box::new(rc_core::secrets::MemoryStore::new());
        self.report(self.t("dialog.secrets.locked"));
    }

    /// Write the store out again when a passphrase is known.
    pub fn persist_secrets(&mut self) -> Result<(), String> {
        match self.secret_passphrase.clone() {
            Some(passphrase) => self.persist_secrets_with(&passphrase),
            None => Ok(()),
        }
    }

    fn persist_secrets_with(&mut self, passphrase: &str) -> Result<(), String> {
        let path = self.secrets_file();
        // The trait store is opaque, so rebuild a fresh encrypted store from the
        // accounts and values the current store answers.
        let mut store = rc_core::secrets::EncryptedStore::new(path.clone());
        for account in self.secrets.accounts() {
            if let Some(secret) = self.secrets.get(&account).map_err(|e| e.to_string())? {
                store.insert(account, secret);
            }
        }
        store.save(passphrase).map_err(|e| e.to_string())
    }

    /// Replace the credential store (the view layer swaps in the encrypted file
    /// once the user has unlocked it).
    pub fn set_secret_store(&mut self, store: Box<dyn rc_core::secrets::SecretStore>) {
        self.secrets = store;
    }

    /// Store the password of a connection in the credential store.
    pub fn remember_password(
        &mut self,
        connection: &rc_core::config::SavedConnection,
    ) -> CommandOutcome {
        if connection.password.is_empty() {
            return CommandOutcome::None;
        }
        let account = connection.account_id();
        if let Err(e) = self.secrets.set(&account, &connection.password) {
            return CommandOutcome::Failed(e.to_string());
        }
        if self.secrets_unlocked() {
            if let Err(e) = self.persist_secrets() {
                return CommandOutcome::Failed(format!(
                    "the credential store could not be written: {e}"
                ));
            }
        } else {
            self.pending_secrets
                .push((account, connection.password.clone()));
            self.report(self.t("dialog.secrets.pending"));
        }
        CommandOutcome::Ok
    }

    /// Move every password that an older settings file still contains into the
    /// credential store and clear it from the settings.
    ///
    /// Returns the number of migrated credentials. The caller is responsible for
    /// persisting the store and the (now empty) settings file.
    pub fn migrate_plaintext_passwords(&mut self) -> usize {
        let mut migrated = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for connection in self.settings.ftp.connections.iter_mut() {
            if connection.password.is_empty() {
                continue;
            }
            let account = connection.account_id();
            match self.secrets.set(&account, &connection.password) {
                Ok(()) => {
                    connection.password.clear();
                    migrated += 1;
                }
                Err(e) => failures.push(format!("{account}: {e}")),
            }
        }
        if migrated > 0 {
            self.mark_settings_dirty();
        }
        if !failures.is_empty() {
            self.report_error(format!(
                "some passwords could not be migrated: {}",
                failures.join("; ")
            ));
        } else if migrated > 0 {
            self.report(format!(
                "{migrated} password(s) moved to the credential store"
            ));
        }
        migrated
    }

    /// Password for a saved connection, if the store has one.
    pub fn stored_password(&self, connection: &rc_core::config::SavedConnection) -> Option<String> {
        self.secrets.get(&connection.account_id()).ok().flatten()
    }

    // ----- encode / decode and saved selections ------------------------

    /// Encode the selected files as MIME/UUE/XXE text.
    pub fn run_encode(&mut self, state: &EncodeState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let Some(kind) = state.parsed_encoding() else {
            return CommandOutcome::Failed("unknown encoding".to_string());
        };
        let cancel = CancelToken::new();
        let mut written = 0u64;
        let mut failures: Vec<String> = Vec::new();
        for source in &state.sources {
            let path = PathBuf::from(source);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let data = match std::fs::read(&path) {
                Ok(data) => data,
                Err(e) => {
                    failures.push(format!("{name}: {e}"));
                    continue;
                }
            };
            if cancel.is_cancelled() {
                break;
            }
            let text = rc_core::encode::encode(&data, kind, &name);
            let mut target = path.clone();
            target.set_extension(format!(
                "{}.{}",
                name.rsplit('.').next().unwrap_or("bin"),
                kind.extension()
            ));
            let target = target.with_file_name(format!("{name}.{}", kind.extension()));
            if let Err(e) = std::fs::write(&target, text.as_bytes()) {
                failures.push(format!("{}: {e}", target.display()));
                continue;
            }
            written += 1;
        }
        self.close_dialogs();
        self.refresh_all();
        if failures.is_empty() {
            self.report(self.tf("status.done", &[("count", &written.to_string())]));
            CommandOutcome::Ok
        } else {
            let message = failures.join("; ");
            self.report_error(&message);
            CommandOutcome::Failed(message)
        }
    }

    /// Decode a MIME/UUE/XXE file back to its binary content.
    pub fn run_decode(&mut self, state: &mut DecodeState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let source = PathBuf::from(&state.source);
        let text = match std::fs::read_to_string(&source) {
            Ok(text) => text,
            Err(e) => return CommandOutcome::Failed(e.to_string()),
        };
        // The name from the envelope wins unless the user typed one.
        let envelope_name = text
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("begin "))
            .and_then(|rest| rest.split_whitespace().nth(2))
            .map(|s| s.to_string());
        let data = match rc_core::encode::decode(&text) {
            Ok(data) => data,
            Err(e) => return CommandOutcome::Failed(e.to_string()),
        };
        let target = if state.target_name.trim().is_empty() {
            let name = envelope_name.unwrap_or_else(|| {
                let mut stem = source
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "decoded".to_string());
                if let Some(position) = stem.rfind('.') {
                    stem.truncate(position);
                }
                stem
            });
            source.with_file_name(name)
        } else {
            source.with_file_name(state.target_name.trim())
        };
        match std::fs::write(&target, &data) {
            Ok(()) => {
                state.decoded_bytes = data.len() as u64;
                self.close_dialogs();
                self.refresh_all();
                self.report(self.tf("status.done", &[("count", &data.len().to_string())]));
                CommandOutcome::Ok
            }
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Write the current selection to a file in the active directory.
    pub fn run_save_selection(&mut self, state: &SaveSelectionState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let Some(dir) = self.active().location().as_path().map(|p| p.to_path_buf()) else {
            return CommandOutcome::Failed("only local directories are supported".to_string());
        };
        let text = self.active().selection.to_file_format();
        let target = dir.join(state.file_name.trim());
        match std::fs::write(&target, text) {
            Ok(()) => {
                self.close_dialogs();
                self.report(self.tf("status.done", &[("count", &state.count.to_string())]));
                CommandOutcome::Ok
            }
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Restore a selection from a file in the active directory.
    pub fn run_load_selection(&mut self, state: &mut LoadSelectionState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let Some(dir) = self.active().location().as_path().map(|p| p.to_path_buf()) else {
            return CommandOutcome::Failed("only local directories are supported".to_string());
        };
        let text = match std::fs::read_to_string(dir.join(state.file_name.trim())) {
            Ok(text) => text,
            Err(e) => return CommandOutcome::Failed(e.to_string()),
        };
        let selection = rc_core::selection::Selection::from_file_format(&text);
        let entries: Vec<Entry> = self.active().entries().to_vec();
        let mut restored = selection;
        restored.retain_existing(&entries);
        let count = restored.len();
        self.active_mut().selection = restored;
        state.loaded = count;
        self.close_dialogs();
        self.report(self.tf("panel.selection_of", &[("count", &count.to_string())]));
        CommandOutcome::Ok
    }

    /// Toggle the branch view of the active panel: a flat list of everything
    /// below the current directory, with paths as names.
    pub fn toggle_branch_view(&mut self) -> CommandOutcome {
        let leaving = self.active().view_mode == crate::model::ViewMode::Tree;
        let Some(root) = self.active().location().as_path().map(|p| p.to_path_buf()) else {
            return CommandOutcome::Failed("branch view needs a local directory".to_string());
        };
        let location = self.active().location().clone();
        let settings = self.settings.clone();
        if leaving {
            // Back to the normal listing of the same directory.
            return match self.navigate(location) {
                Ok(()) => {
                    self.active_mut().view_mode =
                        crate::model::ViewMode::from_str(&settings.display.view_mode);
                    CommandOutcome::Ok
                }
                Err(e) => CommandOutcome::Failed(e.to_string()),
            };
        }
        let cancel = CancelToken::new();
        match rc_core::branch::branch_listing(&root, 50_000, &cancel) {
            Ok((entries, truncated)) => {
                let panel = self.active_mut();
                let options = panel.read_options(&settings);
                let sort = panel.sort();
                panel.listing =
                    rc_core::listing::Listing::from_entries(location, entries, &options, sort);
                panel.view_mode = crate::model::ViewMode::Tree;
                panel.selection.clear();
                let count = panel.listing.len();
                self.report(self.tf("panel.files", &[("count", &count.to_string())]));
                if truncated {
                    self.report("branch view truncated at 50000 entries");
                }
                CommandOutcome::Ok
            }
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    // ----- file utilities ---------------------------------------------

    /// Split the cursor file (or every marked file) into numbered parts.
    pub fn run_split(&mut self, state: &SplitState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let source = PathBuf::from(&state.source);
        let destination = if state.destination.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(&state.destination))
        };
        let cancel = CancelToken::new();
        match rc_core::split::split_file(&source, state.part_size, destination.as_deref(), &cancel)
        {
            Ok(parts) => {
                let count = parts.len();
                if let Some(Dialog::Split(dialog)) = self.top_dialog_mut() {
                    dialog.parts_created = count;
                }
                self.report(self.tf("status.done", &[("count", &count.to_string())]));
                self.refresh_all();
                CommandOutcome::Ok
            }
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Combine numbered parts into one file.
    pub fn run_combine(&mut self, state: &CombineState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let first = PathBuf::from(&state.first_part);
        let destination = PathBuf::from(&state.destination);
        let cancel = CancelToken::new();
        let parts = match rc_core::split::discover_parts(&first) {
            Ok(parts) => parts,
            Err(e) => return CommandOutcome::Failed(e.to_string()),
        };
        if let Some(Dialog::Combine(dialog)) = self.top_dialog_mut() {
            dialog.discovered = parts.len();
        }
        match rc_core::split::combine_files(&parts, &destination, &cancel) {
            Ok(bytes) => {
                self.close_dialogs();
                self.report(self.tf("status.done", &[("count", &bytes.to_string())]));
                self.refresh_all();
                CommandOutcome::Ok
            }
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Write a checksum file for the selected files.
    pub fn run_create_checksums(&mut self, state: &ChecksumState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let Some(algorithm) = state.parsed_algorithm() else {
            return CommandOutcome::Failed("unknown algorithm".to_string());
        };
        let base = PathBuf::from(&state.base);
        let out = base.join(state.file_name.trim());
        let files: Vec<PathBuf> = state.sources.iter().map(PathBuf::from).collect();
        let cancel = CancelToken::new();
        match rc_core::checksum::create_checksum_file(&files, &out, &base, algorithm, &cancel) {
            Ok((_path, count)) => {
                self.close_dialogs();
                self.report(self.tf("status.done", &[("count", &count.to_string())]));
                self.refresh_all();
                CommandOutcome::Ok
            }
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Verify the checksums of the selected file.
    pub fn run_verify_checksums(&mut self, state: &mut VerifyChecksumState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let file = PathBuf::from(&state.checksum_file);
        let base = if state.base.trim().is_empty() {
            file.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        } else {
            PathBuf::from(&state.base)
        };
        let cancel = CancelToken::new();
        match rc_core::checksum::verify_checksum_file(&file, &base, &cancel) {
            Ok(report) => {
                state.checked = report.checked;
                state.ok = report.ok;
                state.failures = report.failures.clone();
                let summary = report.summary();
                if let Some(Dialog::VerifyChecksum(dialog)) = self.top_dialog_mut() {
                    dialog.checked = state.checked;
                    dialog.ok = state.ok;
                    dialog.failures = state.failures.clone();
                }
                if report.failures.is_empty() {
                    self.report(summary);
                } else {
                    self.report_error(summary);
                }
                CommandOutcome::Ok
            }
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Compare two files byte by byte.
    pub fn run_compare_files(&mut self, state: &mut CompareFilesState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let cancel = CancelToken::new();
        let left = PathBuf::from(&state.left);
        let right = PathBuf::from(&state.right);
        match rc_core::compare::first_difference(&left, &right, &cancel) {
            Ok(result) => {
                state.result = Some(result);
                if let Some(Dialog::CompareFiles(dialog)) = self.top_dialog_mut() {
                    dialog.result = state.result;
                }
                self.report(state.result_text());
                CommandOutcome::Ok
            }
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    /// Search the current directory for duplicate files.
    pub fn run_duplicates(&mut self, state: &mut DuplicatesState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let Some(algorithm) = state.parsed_algorithm() else {
            return CommandOutcome::Failed("unknown algorithm".to_string());
        };
        let cancel = CancelToken::new();
        let root = PathBuf::from(&state.root);
        match rc_core::dupes::find_duplicates(
            &root,
            state.min_size,
            algorithm,
            state.include_hidden,
            &cancel,
            None,
        ) {
            Ok(report) => {
                state.groups = report.groups.clone();
                state.scanned = report.scanned;
                let summary = report.summary();
                if let Some(Dialog::Duplicates(dialog)) = self.top_dialog_mut() {
                    dialog.groups = report.groups;
                    dialog.scanned = report.scanned;
                }
                self.report(summary);
                CommandOutcome::Ok
            }
            Err(e) => CommandOutcome::Failed(e.to_string()),
        }
    }

    // ----- columns and view modes -------------------------------------

    /// Append a column to the full view.
    pub fn add_column(&mut self, id: &str) -> CommandOutcome {
        let spec = rc_core::config::ColumnSpec::new(id, 90.0);
        if !spec.is_known() {
            return CommandOutcome::Failed(format!("unknown column {id:?}"));
        }
        if self.settings.display.columns.iter().any(|c| c.id == id) {
            return CommandOutcome::Ok;
        }
        self.settings.display.columns.push(spec);
        self.mark_settings_dirty();
        CommandOutcome::Ok
    }

    /// Remove a column; the name column cannot be removed.
    pub fn remove_column(&mut self, id: &str) -> CommandOutcome {
        if id == "name" {
            return CommandOutcome::Failed("the name column is required".to_string());
        }
        let before = self.settings.display.columns.len();
        self.settings.display.columns.retain(|c| c.id != id);
        if self.settings.display.columns.len() == before {
            CommandOutcome::None
        } else {
            self.mark_settings_dirty();
            CommandOutcome::Ok
        }
    }

    /// Move a column one slot towards the start (`up`) or the end.
    pub fn move_column(&mut self, id: &str, up: bool) -> CommandOutcome {
        let index = match self
            .settings
            .display
            .columns
            .iter()
            .position(|c| c.id == id)
        {
            Some(index) => index,
            None => return CommandOutcome::None,
        };
        let target = if up {
            if index == 0 {
                return CommandOutcome::None;
            }
            index - 1
        } else {
            if index + 1 >= self.settings.display.columns.len() {
                return CommandOutcome::None;
            }
            index + 1
        };
        self.settings.display.columns.swap(index, target);
        self.mark_settings_dirty();
        CommandOutcome::Ok
    }

    /// Set the width of a column.
    pub fn set_column_width(&mut self, id: &str, width: f32) -> CommandOutcome {
        match self
            .settings
            .display
            .columns
            .iter_mut()
            .find(|c| c.id == id)
        {
            Some(column) => {
                column.width = width.clamp(20.0, 800.0);
                self.mark_settings_dirty();
                CommandOutcome::Ok
            }
            None => CommandOutcome::Failed(format!("unknown column {id:?}")),
        }
    }

    /// Save the editor state as a view mode (replacing a mode of the same name).
    pub fn save_view_mode(&mut self, state: &ViewModeDialogState) -> CommandOutcome {
        let mode = state.to_mode();
        if let Err(message) = mode.validate() {
            return CommandOutcome::Failed(message);
        }
        match self
            .settings
            .display
            .view_modes
            .iter_mut()
            .find(|m| m.name == mode.name)
        {
            Some(existing) => *existing = mode,
            None => self.settings.display.view_modes.push(mode),
        }
        self.mark_settings_dirty();
        CommandOutcome::Ok
    }

    /// Apply a saved view mode to the active panel.
    pub fn apply_view_mode(&mut self, name: &str) -> CommandOutcome {
        let Some(mode) = self
            .settings
            .display
            .view_modes
            .iter()
            .find(|m| m.name == name)
            .cloned()
        else {
            return CommandOutcome::None;
        };
        self.settings.display.view_mode = mode.kind.clone();
        let panel = self.active_mut();
        panel.view_mode = crate::model::ViewMode::from_str(&mode.kind);
        panel.mask_spec = mode.mask.clone();
        let field = match mode.sort_field.as_str() {
            "ext" => SortField::Extension,
            "size" => SortField::Size,
            "date" => SortField::ModifiedTime,
            "unsorted" => SortField::Unsorted,
            _ => SortField::Name,
        };
        let spec = SortSpec {
            field,
            reversed: mode.sort_reversed,
            ..panel.sort()
        };
        panel.set_sort(spec);
        if !mode.columns.is_empty() {
            let current = self.settings.display.columns.clone();
            self.settings.display.columns = mode.columns.clone();
            if current.is_empty() {
                self.settings.display.columns = mode.columns.clone();
            }
        }
        self.refresh_active();
        CommandOutcome::Ok
    }

    /// Delete a saved view mode.
    pub fn remove_view_mode(&mut self, name: &str) -> CommandOutcome {
        let before = self.settings.display.view_modes.len();
        self.settings.display.view_modes.retain(|m| m.name != name);
        if self.settings.display.view_modes.len() == before {
            CommandOutcome::None
        } else {
            self.mark_settings_dirty();
            CommandOutcome::Ok
        }
    }

    // ----- drag & drop -------------------------------------------------

    /// Copy files dropped onto a panel into that panel's directory.
    ///
    /// Existing files are never overwritten: a colliding name gets a ` (2)`
    /// suffix instead, so an accidental drop cannot destroy data.
    pub fn drop_files_into(&mut self, side: Side, paths: Vec<PathBuf>) -> CommandOutcome {
        let Some(destination) = self
            .panel(side)
            .location()
            .as_path()
            .map(|p| p.to_path_buf())
        else {
            return CommandOutcome::Failed(
                "dropping is only supported on local directories".to_string(),
            );
        };
        if paths.is_empty() {
            return CommandOutcome::None;
        }
        let cancel = CancelToken::new();
        let options = TransferOptions {
            overwrite: OverwriteMode::Rename,
            ..Default::default()
        };
        let mut copied = 0u64;
        let mut failures: Vec<String> = Vec::new();
        for source in paths {
            let name = source.file_name().map(|n| n.to_owned()).unwrap_or_default();
            let target = destination.join(name);
            match ops::copy(&source, &target, &options, &cancel, None) {
                Ok(outcome) => copied += outcome.files.max(1),
                Err(e) => failures.push(format!("{}: {e}", source.display())),
            }
        }
        self.refresh_active();
        if failures.is_empty() {
            self.report(self.tf("status.done", &[("count", &copied.to_string())]));
            CommandOutcome::Ok
        } else {
            let message = failures.join("; ");
            self.report_error(&message);
            CommandOutcome::Failed(message)
        }
    }

    // ----- button bar --------------------------------------------------

    /// Add a local directory shortcut without executing it.
    pub fn add_current_directory_button(&mut self) -> CommandOutcome {
        let Some(path) = self.active().location().as_path() else {
            return CommandOutcome::Failed(self.t("toolbar.local_directory_only"));
        };
        if !path.is_absolute() || !path.is_dir() {
            return CommandOutcome::Failed(self.t("toolbar.directory_missing"));
        }
        let caption = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned();
        let Some(path_text) = path.to_str() else {
            return CommandOutcome::Failed(self.t("toolbar.directory_missing"));
        };
        let command = format!("rc_OpenDirectory:{path_text}");
        self.settings
            .button_bar
            .add(rc_core::config::BarButton::new(caption, command));
        self.mark_settings_dirty();
        CommandOutcome::Ok
    }

    /// Save a personal layout without changing the active one.
    pub fn save_custom_button_bar(&mut self) {
        self.settings.button_bar_custom = Some(self.settings.button_bar.clone());
        self.mark_settings_dirty();
    }

    /// Restore factory layout, keeping the previous personal layout.
    pub fn reset_button_bar(&mut self) {
        if self.settings.button_bar != rc_core::config::ButtonBar::default() {
            self.save_custom_button_bar();
        }
        self.settings.button_bar = rc_core::config::ButtonBar::default();
        self.mark_settings_dirty();
    }

    /// Restore a saved personal layout.
    pub fn restore_custom_button_bar(&mut self) -> bool {
        let Some(saved) = self.settings.button_bar_custom.clone() else {
            return false;
        };
        self.settings.button_bar = saved;
        self.mark_settings_dirty();
        true
    }

    /// Apply the editor: store the draft button and refresh the bar.
    pub fn apply_button_bar(&mut self, state: &mut ButtonBarState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let draft = state.draft();
        let bar = &mut self.settings.button_bar;
        match state.selected {
            Some(index) if index < bar.buttons.len() => {
                bar.buttons[index] = draft;
            }
            _ => {
                let index = bar.add(draft);
                state.selected = Some(index);
            }
        }
        state.adding = false;
        self.mark_settings_dirty();
        CommandOutcome::Ok
    }

    /// Remove the selected button from the bar.
    pub fn remove_button_bar(&mut self, state: &mut ButtonBarState) -> CommandOutcome {
        let Some(index) = state.selected else {
            return CommandOutcome::None;
        };
        match self.settings.button_bar.remove(index) {
            Some(_) => {
                state.selected = None;
                self.mark_settings_dirty();
                CommandOutcome::Ok
            }
            None => CommandOutcome::None,
        }
    }

    /// Move the selected button one slot up or down.
    pub fn move_button_bar(&mut self, state: &mut ButtonBarState, up: bool) -> CommandOutcome {
        let Some(index) = state.selected else {
            return CommandOutcome::None;
        };
        let moved = if up {
            self.settings.button_bar.move_up(index)
        } else {
            self.settings.button_bar.move_down(index)
        };
        if moved {
            state.selected = Some(if up { index - 1 } else { index + 1 });
            self.mark_settings_dirty();
            CommandOutcome::Ok
        } else {
            CommandOutcome::None
        }
    }

    // ----- synchronisation -------------------------------------------

    /// Fill `state.rows` by comparing the two directories it names.
    pub fn sync_compare(&mut self, state: &mut SyncState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let left = match self.read_dir_entries(&state.left) {
            Ok(entries) => entries,
            Err(message) => return CommandOutcome::Failed(message),
        };
        let right = match self.read_dir_entries(&state.right) {
            Ok(entries) => entries,
            Err(message) => return CommandOutcome::Failed(message),
        };
        let mut rows: Vec<SyncRow> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in left.iter().filter(|e| e.name != "..") {
            let other = right.iter().find(|e| e.name == entry.name);
            let row = match other {
                None => SyncRow {
                    name: entry.name.clone(),
                    left_size: Some(entry.size),
                    right_size: None,
                    left_time: entry.mtime,
                    right_time: None,
                    status: "->".to_string(),
                },
                Some(other) => {
                    let same_size = entry.size == other.size;
                    let same_time = match (state.ignore_dates, entry.mtime, other.mtime) {
                        (true, _, _) => true,
                        (false, Some(a), Some(b)) => a == b,
                        (false, None, None) => true,
                        _ => false,
                    };
                    let equal = same_size && same_time && entry.is_dir() == other.is_dir();
                    SyncRow {
                        name: entry.name.clone(),
                        left_size: Some(entry.size),
                        right_size: Some(other.size),
                        left_time: entry.mtime,
                        right_time: other.mtime,
                        status: if equal { "=" } else { "!=" }.to_string(),
                    }
                }
            };
            seen.insert(entry.name.clone());
            rows.push(row);
        }
        for entry in right
            .iter()
            .filter(|e| e.name != ".." && !seen.contains(&e.name))
        {
            rows.push(SyncRow {
                name: entry.name.clone(),
                left_size: None,
                right_size: Some(entry.size),
                left_time: None,
                right_time: entry.mtime,
                status: "<-".to_string(),
            });
        }
        rows.sort_by_key(|a| a.name.to_lowercase());
        state.rows = rows;
        let pending = state.pending();
        let snapshot = state.rows.clone();
        if let Some(Dialog::Synchronize(dialog)) = self.top_dialog_mut() {
            dialog.rows = snapshot;
        }
        self.report(self.tf("dialog.search.progress", &[("count", &pending.to_string())]));
        CommandOutcome::Ok
    }

    /// Copy the differing entries in the direction the comparison found.
    /// `state.dry_run` only reports what would happen.
    pub fn sync_execute(&mut self, state: &SyncState) -> CommandOutcome {
        if let Err(message) = state.validate() {
            return CommandOutcome::Failed(message);
        }
        let left_root = PathBuf::from(&state.left);
        let right_root = PathBuf::from(&state.right);
        let mut copied = 0u64;
        let mut failures: Vec<String> = Vec::new();
        let mut plans: Vec<(PathBuf, PathBuf)> = Vec::new();
        for row in &state.rows {
            let (from, to) = match row.status.as_str() {
                "->" => (left_root.join(&row.name), right_root.join(&row.name)),
                "<-" => (right_root.join(&row.name), left_root.join(&row.name)),
                "!=" => {
                    // Overwrite the older copy with the newer one.
                    match (row.left_time, row.right_time) {
                        (Some(a), Some(b)) if a >= b => {
                            (left_root.join(&row.name), right_root.join(&row.name))
                        }
                        (Some(_), Some(_)) => {
                            (right_root.join(&row.name), left_root.join(&row.name))
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };
            plans.push((from, to));
        }
        if state.dry_run {
            self.report(self.tf(
                "dialog.search.results_count",
                &[("count", &plans.len().to_string())],
            ));
            return CommandOutcome::Ok;
        }
        let cancel = CancelToken::new();
        let options = TransferOptions {
            overwrite: OverwriteMode::Overwrite,
            ..Default::default()
        };
        for (from, to) in plans {
            match ops::copy(&from, &to, &options, &cancel, None) {
                Ok(outcome) => copied += outcome.files.max(1),
                Err(e) => failures.push(format!("{}: {e}", from.display())),
            }
        }
        self.close_dialogs();
        self.refresh_all();
        if failures.is_empty() {
            self.report(self.tf("status.done", &[("count", &copied.to_string())]));
            CommandOutcome::Ok
        } else {
            let message = failures.join("; ");
            self.report_error(&message);
            CommandOutcome::Failed(message)
        }
    }

    /// Read one directory into entries, or explain why that failed.
    fn read_dir_entries(&self, path: &str) -> Result<Vec<Entry>, String> {
        let location = Location::parse(path).map_err(|e| e.to_string())?;
        let options = crate::model::default_read_options(&self.settings);
        rc_core::listing::Listing::read(&location, &options, SortSpec::default())
            .map(|listing| listing.entries().to_vec())
            .map_err(|e| e.to_string())
    }

    // ----- search ----------------------------------------------------

    /// Start a search in the background. The dialog keeps showing progress and
    /// its result list is filled as matches arrive.
    pub fn start_search(&mut self, state: &SearchState) -> bool {
        if let Some(job) = self.search_job.take() {
            job.cancel();
        }
        match crate::model::search::SearchJob::spawn(state) {
            Some(job) => {
                self.search_job = Some(job);
                if let Some(Dialog::Search(dialog)) = self.top_dialog_mut() {
                    dialog.running = true;
                    dialog.results.clear();
                }
                self.report(self.t("dialog.search.progress"));
                true
            }
            None => {
                self.report_error("invalid search request");
                false
            }
        }
    }

    /// Stop the running search.
    pub fn cancel_search(&mut self) {
        if let Some(job) = self.search_job.take() {
            job.cancel();
            let hits = job.wait().0;
            self.finish_search(hits, false);
        }
    }

    /// Advance the running search and copy new results into the dialog.
    /// Returns true while the search is still running.
    pub fn poll_search(&mut self) -> bool {
        let running = match self.search_job.as_mut() {
            Some(job) => job.poll(),
            None => return false,
        };
        let (hits, truncated) = match self.search_job.as_ref() {
            Some(job) => (job.hits.clone(), job.truncated),
            None => return false,
        };
        if let Some(Dialog::Search(dialog)) = self.top_dialog_mut() {
            dialog.results = hits.clone();
            dialog.truncated = truncated;
            dialog.running = running;
        }
        if !running {
            self.search_job = None;
            self.finish_search(hits, truncated);
        }
        running
    }

    fn finish_search(&mut self, hits: Vec<SearchHit>, truncated: bool) {
        if let Some(Dialog::Search(dialog)) = self.top_dialog_mut() {
            dialog.results = hits.clone();
            dialog.running = false;
            dialog.truncated = truncated;
        }
        if hits.is_empty() {
            self.report(self.t("dialog.search.not_found"));
        } else {
            self.report(self.tf(
                "dialog.search.results_count",
                &[("count", &hits.len().to_string())],
            ));
        }
    }

    /// Show the directory of a search result in the other panel and put the
    /// cursor on the file.
    pub fn goto_search_hit(&mut self, index: usize) -> CommandOutcome {
        let hit = match self.top_dialog() {
            Some(Dialog::Search(dialog)) => dialog.results.get(index).cloned(),
            _ => None,
        };
        let Some(hit) = hit else {
            return CommandOutcome::None;
        };
        let path = PathBuf::from(&hit.path);
        let Some(parent) = path.parent() else {
            return CommandOutcome::None;
        };
        let target = self.focus.other();
        let outcome = self.open_in(target, Location::local(parent.to_path_buf()));
        if let Err(e) = outcome {
            return CommandOutcome::Failed(e.to_string());
        }
        if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string())
            && let Some(index) = self.panel(target).listing.find(&name)
        {
            self.panel_mut(target).set_cursor(index);
        }
        CommandOutcome::Ok
    }

    /// Switch the UI language and reload the translator.
    ///
    /// Returns false (and reports the problem) when the language pack is not
    /// installed, so a typo in the settings file cannot leave the UI in a
    /// half-switched state.
    pub fn set_language(&mut self, code: &str) -> bool {
        let code = code.trim();
        if code.is_empty() {
            return false;
        }
        if self.strings.language().eq_ignore_ascii_case(code) {
            self.settings.language = code.to_string();
            return true;
        }
        if !self.strings.set_language(code) {
            let available = self.strings.language();
            self.report_error(format!(
                "language {code:?} is not installed (current: {available})"
            ));
            return false;
        }
        self.settings.language = code.to_string();
        self.mark_settings_dirty();
        self.report(self.t("status.ready"));
        true
    }
}

/// Trailing decimal number of a numbered command family (`cm_ActivateTab3` → 3).
fn trailing_number(command: &str) -> Option<usize> {
    let digits: String = command
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    let digits: String = digits.chars().rev().collect();
    digits.parse().ok()
}

/// Split a `cm_Left…`/`cm_Right…` identifier into the panel it targets and the
/// equivalent `cm_Src…` command.
fn panel_prefix(command: &str) -> Option<(Side, String)> {
    if let Some(rest) = command.strip_prefix("cm_left") {
        return Some((Side::Left, format!("cm_src{rest}")));
    }
    if let Some(rest) = command.strip_prefix("cm_right") {
        return Some((Side::Right, format!("cm_src{rest}")));
    }
    None
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

/// True when these bytes are not text: a NUL byte or invalid UTF-8 means "binary".
///
/// The rule is deliberately simple and checkable. A file that *is* UTF-8 text with a few control
/// characters (a tab, a form feed) is still text; a file with a NUL in the first kilobyte is not,
/// which is what every diff tool and editor on the planet agrees on.
pub fn looks_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(8192)];
    if head.contains(&0) {
        return true;
    }
    std::str::from_utf8(head).is_err() && !head.is_ascii()
}

impl Model {
    /// Save the edited buffer, refusing to pretend when it fails.
    ///
    /// Shared by the window's Save button, the `F2` key of the editor row and a headless caller:
    /// one implementation, one behaviour.
    pub fn editor_save(&mut self) -> CommandOutcome {
        let Some(mut state) = self.editor.clone() else {
            return CommandOutcome::None;
        };
        if state.binary {
            let message = self.t("editor.binary");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let path = std::path::PathBuf::from(&state.path);
        match std::fs::write(&path, state.text.as_bytes()) {
            Ok(()) => {
                state.modified = false;
                self.report(self.t("status.done"));
                let _ = self.on_file_saved(&path);
                self.editor = Some(state);
                CommandOutcome::Ok
            }
            Err(error) => {
                let message = self.tf(
                    "editor.save.failed",
                    &[("path", &state.path), ("error", &error.to_string())],
                );
                self.report_error(&message);
                // The buffer stays as it was, with the error visible: nothing was written.
                self.editor = Some(state);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// Save the buffer somewhere the user certainly can write.
    pub fn editor_save_copy(&mut self) -> CommandOutcome {
        let Some(mut state) = self.editor.clone() else {
            return CommandOutcome::None;
        };
        let target = self.suggest_copy_path(std::path::Path::new(&state.path));
        match std::fs::write(&target, state.text.as_bytes()) {
            Ok(()) => {
                let message = self.tf(
                    "editor.saved_copy",
                    &[("path", &target.display().to_string())],
                );
                self.report_done(message, None);
                state.path = target.to_string_lossy().to_string();
                state.modified = false;
                self.editor = Some(state);
                CommandOutcome::Ok
            }
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// Re-read the file from disk, discarding nothing silently: a modified buffer is kept.
    pub fn editor_reload(&mut self) -> CommandOutcome {
        let Some(state) = self.editor.clone() else {
            return CommandOutcome::None;
        };
        if state.modified {
            let message = self.t("editor.modified").to_string();
            self.report_error(message.clone());
            return CommandOutcome::Failed(message);
        }
        match self.open_editor_for(std::path::PathBuf::from(&state.path)) {
            Ok(()) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.placement = state.placement;
                    editor.chrome = state.chrome;
                }
                CommandOutcome::Ok
            }
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// Close the editor, keeping a modified buffer open until the user decides.
    pub fn editor_close(&mut self) -> CommandOutcome {
        if let Some(state) = self.editor.clone()
            && state.modified
        {
            let message = self.t("editor.modified").to_string();
            self.report_error(message.clone());
            return CommandOutcome::Failed(message);
        }
        self.editor = None;
        CommandOutcome::Ok
    }

    /// Switch the viewer's mode from the keyboard.
    pub fn viewer_set_mode(&mut self, command: &str) -> CommandOutcome {
        let Some(state) = self.viewer.as_mut() else {
            return CommandOutcome::None;
        };
        state.mode = match command {
            "cm_viewertext" => crate::model::ViewerMode::Text,
            "cm_viewerhex" => crate::model::ViewerMode::Hex,
            "cm_viewerimage" => crate::model::ViewerMode::Image,
            _ => return CommandOutcome::None,
        };
        CommandOutcome::Ok
    }

    /// Zoom the viewer in or out, within a range that stays useful.
    pub fn viewer_zoom(&mut self, closer: bool) -> CommandOutcome {
        let Some(state) = self.viewer.as_mut() else {
            return CommandOutcome::None;
        };
        let factor = if closer { 1.25 } else { 0.8 };
        state.zoom = (state.zoom * factor).clamp(0.1, 10.0);
        CommandOutcome::Ok
    }
}

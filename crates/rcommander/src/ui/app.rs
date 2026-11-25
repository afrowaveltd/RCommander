//! Root egui application: layout, menus, keyboard and window glue.

use crate::i18n::{Strings, TranslatorStrings};
use crate::keys;
use crate::model::{CommandOutcome, Dialog, Model, Side};
use crate::paths;
use crate::settings_writer::{SaveOutcome, SettingsWriter};
use crate::theme::{self, Palette};
use crate::ui::agent_panel::AgentUi;
use crate::ui::dialogs::DialogAction;
use crate::ui::editor_window::EditorUi;
use crate::ui::help_window::HelpUi;
use crate::ui::panel_view::{self, PanelAction};
use crate::ui::thumbs::ThumbnailCache;
use crate::ui::viewer_window::ViewerUi;
use egui::{Context, Key, Modifiers};
use rc_core::config::Settings;
use rc_core::keymap::Shortcut;
use rc_core::location::Location;

/// One entry of the function-key bar.
struct FunctionKey {
    label_key: &'static str,
    command: &'static str,
    shortcut: &'static str,
}

const FUNCTION_KEYS: [FunctionKey; 8] = [
    FunctionKey {
        label_key: "cmd.help",
        command: "cm_HelpIndex",
        shortcut: "F1",
    },
    FunctionKey {
        label_key: "cmd.refresh",
        command: "cm_RereadSource",
        shortcut: "F2",
    },
    FunctionKey {
        label_key: "cmd.view",
        command: "cm_List",
        shortcut: "F3",
    },
    FunctionKey {
        label_key: "cmd.edit",
        command: "cm_Edit",
        shortcut: "F4",
    },
    FunctionKey {
        label_key: "cmd.copy",
        command: "cm_Copy",
        shortcut: "F5",
    },
    FunctionKey {
        label_key: "cmd.move",
        command: "cm_RenMov",
        shortcut: "F6",
    },
    FunctionKey {
        label_key: "cmd.mkdir",
        command: "cm_MkDir",
        shortcut: "F7",
    },
    FunctionKey {
        label_key: "cmd.delete",
        command: "cm_Delete",
        shortcut: "F8",
    },
];

/// One entry of the button bar.
/// The eframe application.
pub struct RCommanderApp {
    pub model: Model,
    pub palette: Palette,
    pub viewer_ui: ViewerUi,
    pub editor_ui: EditorUi,
    pub help_ui: HelpUi,
    fonts_installed: bool,
    /// Command line editing buffer kept in sync with the model.
    pub command_line_focus: bool,
    /// Command requested by a widget during this frame.
    pending_command: Option<String>,
    /// Content of the quick-view panel (`Ctrl+Q`).
    quick_viewer: ViewerUi,
    quick_view_path: Option<std::path::PathBuf>,
    /// The MCP client panel state.
    agent_ui: AgentUi,
    /// Thumbnail textures for the thumbnail view mode.
    thumbnails: ThumbnailCache,
    /// Index of the search result the user picked.
    search_hit_selection: usize,
    /// Screen rectangles of the two panels, used to resolve drop targets.
    /// Screen rectangles of the two panels, used to resolve drop targets.
    pub panel_rects: [Option<egui::Rect>; 2],
    /// Theme name that is currently applied to the egui context.
    applied_theme: String,
    /// Persists the settings file whenever something changes.
    pub settings_writer: SettingsWriter,
}

/// Button text with an icon in the accent colour in front of the caption.
///
/// The icon is drawn slightly larger than the caption so it reads as a symbol and
/// not as a stray character; both scale with the configured font size, so the bar
/// stays proportionate when the user changes it.
fn icon_button_text(
    icon: crate::icons::Icon,
    caption: &str,
    font_size: f32,
    palette: &Palette,
) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    let mut icon_format = egui::TextFormat {
        font_id: egui::FontId::proportional((font_size + 3.0).max(12.0)),
        // The tint follows the kind of action: destructive, creating, moving,
        // navigating, searching or quiet.
        color: palette.role(icon.colour_role()),
        valign: egui::Align::Center,
        ..Default::default()
    };
    icon_format.extra_letter_spacing = 0.0;
    job.append(&icon.glyph().to_string(), 0.0, icon_format);
    job.append(
        caption,
        (font_size * 0.45).max(4.0),
        egui::TextFormat {
            font_id: egui::FontId::proportional(font_size),
            color: palette.foreground,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    egui::WidgetText::LayoutJob(std::sync::Arc::new(job))
}

impl RCommanderApp {
    /// Create the application from an already built model.
    pub fn new(model: Model) -> Self {
        let palette = Palette::for_model(&model);
        let settings_writer =
            SettingsWriter::with_current(crate::paths::settings_path(), &model.settings);
        let mut app = RCommanderApp {
            model,
            palette,
            viewer_ui: ViewerUi::default(),
            editor_ui: EditorUi::default(),
            help_ui: HelpUi::default(),
            fonts_installed: false,
            command_line_focus: false,
            pending_command: None,
            quick_viewer: ViewerUi::default(),
            quick_view_path: None,
            agent_ui: AgentUi::default(),
            thumbnails: ThumbnailCache::new(),
            search_hit_selection: 0,
            panel_rects: [None, None],
            applied_theme: String::new(),
            settings_writer,
        };
        app.help_ui.reload(&paths::docs_dir());
        app
    }

    /// Build a model from disk: settings, translator, help and two panels.
    pub fn boot(left: Option<Location>, right: Option<Location>) -> Self {
        let settings = Settings::load(&paths::settings_path()).unwrap_or_default();
        // Leave an editable file behind on the very first start.
        crate::settings_writer::ensure_settings_file(paths::settings_path(), &settings);
        let strings = Box::new(TranslatorStrings::discover(
            &paths::locales_dir(),
            &settings.language,
        ));
        let home = rc_core::drive::home_dir()
            .map(Location::local)
            .unwrap_or_else(|| Location::local("/"));
        let left = left.unwrap_or_else(|| home.clone());
        let right = right.unwrap_or(home);
        let model = Model::boot(settings, strings, left, right);
        RCommanderApp::new(model)
    }

    /// Apply the active theme to the whole window when it changed.
    ///
    /// The panels painted their own colours from the start, but the window chrome
    /// (menu bar, tool bar, dialogs, status line) kept the colours of the first
    /// frame, which made "dark mode" look half-applied.
    fn apply_theme(&mut self, ctx: &Context) {
        let theme_name = self.model.settings.colors.theme.clone();
        let font_size = self.model.settings.display.font_size;
        if self.fonts_installed && self.applied_theme == theme_name {
            return;
        }
        self.palette = Palette::for_model(&self.model);
        theme::install_icon_font(ctx);
        theme::install_text_styles(ctx, font_size);
        ctx.set_visuals(theme::visuals(&self.palette, theme_name == "dark"));
        theme::install_menu_style(ctx);
        self.applied_theme = theme_name;
        self.fonts_installed = true;
    }

    /// Name of the theme currently applied to the window.
    pub fn applied_theme(&self) -> &str {
        &self.applied_theme
    }

    /// Persist the settings if something changed (called once per frame).
    fn persist_settings(&mut self) {
        if !self.model.settings_dirty {
            return;
        }
        match self
            .settings_writer
            .maybe_save(&self.model.settings, std::time::Instant::now())
        {
            SaveOutcome::Saved | SaveOutcome::Unchanged => {
                self.model.settings_dirty = false;
            }
            SaveOutcome::Throttled => {}
            SaveOutcome::Failed(message) => {
                self.model.settings_dirty = false;
                self.model
                    .report_error(format!("settings not saved: {message}"));
            }
        }
    }

    /// Execute one internal command and handle what the view layer must do.
    pub fn run_command(&mut self, command: &str, ctx: &Context) {
        if command == "rc_TreeToSource" {
            if let Some(Dialog::Tree(state)) = self.model.top_dialog().cloned()
                && let Some(index) = state.selected
                && let Some(path) = state.entries.get(index)
                && let Ok(location) = rc_core::location::Location::parse(path)
            {
                let side = self.model.focus;
                if let Err(e) = self.model.open_in(side, location) {
                    self.model.report_error(e.to_string());
                }
            }
            self.model.close_dialogs();
            return;
        }
        if command == "rc_RunEncode" {
            if let Some(Dialog::Encode(state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_encode(&state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunDecode" {
            if let Some(Dialog::Decode(mut state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_decode(&mut state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunSaveSelection" {
            if let Some(Dialog::SaveSelection(state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_save_selection(&state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunLoadSelection" {
            if let Some(Dialog::LoadSelection(mut state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_load_selection(&mut state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunSplit" {
            if let Some(Dialog::Split(state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_split(&state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunCombine" {
            if let Some(Dialog::Combine(state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_combine(&state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunChecksum" {
            if let Some(Dialog::Checksum(state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_create_checksums(&state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunVerify" {
            if let Some(Dialog::VerifyChecksum(mut state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_verify_checksums(&mut state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunCompareFiles" {
            if let Some(Dialog::CompareFiles(mut state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_compare_files(&mut state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunDuplicates" {
            if let Some(Dialog::Duplicates(mut state)) = self.model.top_dialog().cloned()
                && let CommandOutcome::Failed(message) = self.model.run_duplicates(&mut state)
            {
                self.model.report_error(message);
            }
            return;
        }
        if command == "rc_RunSearch" {
            if let Some(Dialog::Search(state)) = self.model.top_dialog().cloned() {
                self.model.start_search(&state);
            }
            return;
        }
        if command == "rc_StopSearch" {
            self.model.cancel_search();
            return;
        }
        if command == "rc_RunSync" {
            if let Some(Dialog::Synchronize(mut state)) = self.model.top_dialog().cloned() {
                match self.model.sync_compare(&mut state) {
                    CommandOutcome::Failed(message) => self.model.report_error(message),
                    _ => {
                        if let Some(Dialog::Synchronize(dialog)) = self.model.top_dialog_mut() {
                            dialog.rows = state.rows;
                        }
                    }
                }
            }
            return;
        }
        if command == "rc_ExecuteSync" {
            let state = match self.model.top_dialog() {
                Some(Dialog::Synchronize(state)) => state.clone(),
                _ => return,
            };
            if let CommandOutcome::Failed(message) = self.model.sync_execute(&state) {
                self.model.report_error(message);
            }
            return;
        }
        if let Some(rest) = command.strip_prefix("rc_SearchHit:") {
            if let Ok(index) = rest.parse::<usize>() {
                self.search_hit_selection = index;
                if let CommandOutcome::Failed(message) = self.model.goto_search_hit(index) {
                    self.model.report_error(message);
                }
            }
            return;
        }
        if command == "rc_Quit" {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if command == "cm_SaveSettings" || command.eq_ignore_ascii_case("cm_configsavesettings") {
            let outcome = self
                .settings_writer
                .force_save(&self.model.settings, std::time::Instant::now());
            match outcome {
                SaveOutcome::Failed(message) => self
                    .model
                    .report_error(format!("settings not saved: {message}")),
                _ => {
                    self.model.settings_dirty = false;
                    self.model.report(self.model.t("status.done"));
                }
            }
            return;
        }
        match self.model.execute(command) {
            CommandOutcome::OpenFile(path) => self.open_viewer(path),
            CommandOutcome::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            CommandOutcome::Minimize => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
            }
            CommandOutcome::Failed(message) => self.model.report_error(message),
            _ => {}
        }
        self.sync_theme();
    }

    /// Show `path` in the F3 viewer.
    pub fn open_viewer(&mut self, path: std::path::PathBuf) {
        self.model.open_viewer_for(path.clone());
        if !self.viewer_ui.load(&path) {
            let error = self.viewer_ui.error.clone().unwrap_or_default();
            self.model.report_error(error);
        }
    }

    /// Show `path` in the editor.
    pub fn open_editor(&mut self, path: std::path::PathBuf) {
        if let Err(e) = self.model.open_editor_for(path) {
            self.model.report_error(e.to_string());
        }
    }

    fn sync_theme(&mut self) {
        self.palette = Palette::for_model(&self.model);
    }

    /// Files dragged from another application onto a panel are copied there.
    fn handle_dropped_files(&mut self, ctx: &Context) {
        let (dropped, pointer) =
            ctx.input(|i| (i.raw.dropped_files.clone(), i.pointer.hover_pos()));
        if dropped.is_empty() {
            return;
        }
        let paths: Vec<std::path::PathBuf> = dropped
            .iter()
            .map(|file| file.path().to_path_buf())
            .collect();
        if paths.is_empty() {
            return;
        }
        let side = resolve_drop_side(self.panel_rects, pointer, self.model.focus);
        if let CommandOutcome::Failed(message) = self.model.drop_files_into(side, paths) {
            self.model.report_error(message);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &Context) {
        if self.model.modal() || ctx.memory(|m| m.focused()).is_some() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if key == Key::Escape {
                        if self.model.viewer.is_some() {
                            self.model.viewer = None;
                        } else if self.model.editor.is_some() {
                            self.model.editor = None;
                        } else if self.model.help.is_some() {
                            self.model.help = None;
                        } else {
                            self.model.cancel_quick_search();
                            self.model.close_dialogs();
                        }
                        continue;
                    }
                    if let Some(shortcut) = keys::shortcut_from_egui(key, modifiers)
                        && self.model.handle_key(&shortcut)
                    {
                        self.after_key();
                        continue;
                    }
                    // Unhandled keys: cursor movement and quick search.
                    self.handle_navigation_key(key, modifiers);
                }
                egui::Event::Text(text) => {
                    if !modifiers_are_plain(ctx) {
                        continue;
                    }
                    for ch in text.chars() {
                        if ch.is_alphabetic() || ch.is_numeric() || "._- ".contains(ch) {
                            self.model.handle_char(ch);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_navigation_key(&mut self, key: Key, modifiers: Modifiers) {
        let page = 20;
        match key {
            Key::ArrowDown => self.model.active_mut().move_cursor(1, false),
            Key::ArrowUp => self.model.active_mut().move_cursor(-1, false),
            Key::Home => self.model.active_mut().cursor_home(),
            Key::End => self.model.active_mut().cursor_end(),
            Key::PageDown => self.model.active_mut().page(page, true),
            Key::PageUp => self.model.active_mut().page(page, false),
            Key::Enter => self.enter_pressed(),
            Key::Insert => self.model.active_mut().toggle_current(),
            Key::Space => self.model.active_mut().toggle_and_advance(),
            _ => {
                let _ = modifiers;
            }
        }
    }

    fn enter_pressed(&mut self) {
        match self.model.enter() {
            CommandOutcome::OpenFile(path) => self.open_viewer(path),
            CommandOutcome::Ok => {}
            CommandOutcome::Failed(message) => self.model.report_error(message),
            _ => {}
        }
    }

    fn after_key(&mut self) {
        self.sync_theme();
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            let mut command: Option<String> = None;
            ui.menu_button(self.model.t("menu.file"), |ui| {
                if ui.button(self.model.t("cmd.view")).clicked() {
                    command = Some("cm_List".into());
                }
                if ui.button(self.model.t("cmd.edit")).clicked() {
                    command = Some("cm_Edit".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.copy")).clicked() {
                    command = Some("cm_Copy".into());
                }
                if ui.button(self.model.t("cmd.move")).clicked() {
                    command = Some("cm_RenMov".into());
                }
                if ui.button(self.model.t("cmd.mkdir")).clicked() {
                    command = Some("cm_MkDir".into());
                }
                if ui.button(self.model.t("cmd.delete")).clicked() {
                    command = Some("cm_Delete".into());
                }
                if ui.button(self.model.t("cmd.rename")).clicked() {
                    command = Some("cm_RenameOnly".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.pack")).clicked() {
                    command = Some("cm_PackFiles".into());
                }
                if ui.button(self.model.t("cmd.unpack")).clicked() {
                    command = Some("cm_UnpackFiles".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.split_file")).clicked() {
                    command = Some("cm_Split".into());
                }
                if ui.button(self.model.t("cmd.combine_files")).clicked() {
                    command = Some("cm_Combine".into());
                }
                if ui.button(self.model.t("cmd.encode")).clicked() {
                    command = Some("cm_Encode".into());
                }
                if ui.button(self.model.t("cmd.decode")).clicked() {
                    command = Some("cm_Decode".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.checksums")).clicked() {
                    command = Some("cm_CRCcreate".into());
                }
                if ui.button(self.model.t("cmd.verify_checksums")).clicked() {
                    command = Some("cm_CRCcheck".into());
                }
                if ui.button(self.model.t("cmd.compare_by_content")).clicked() {
                    command = Some("cm_CompareFilesByContent".into());
                }
                if ui.button(self.model.t("cmd.duplicates")).clicked() {
                    command = Some("cm_FindDuplicates".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.attributes")).clicked() {
                    command = Some("cm_SetAttrib".into());
                }
                if ui.button(self.model.t("cmd.properties")).clicked() {
                    command = Some("cm_VersionInfo".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.quit")).clicked() {
                    command = Some("cm_Exit".into());
                }
            });
            ui.menu_button(self.model.t("menu.mark"), |ui| {
                if ui.button(self.model.t("cmd.select_all")).clicked() {
                    command = Some("cm_SelectAll".into());
                }
                if ui.button(self.model.t("cmd.deselect_all")).clicked() {
                    command = Some("cm_Clearall".into());
                }
                if ui.button(self.model.t("cmd.invert_selection")).clicked() {
                    command = Some("cm_ExchangeSelection".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.save_selection")).clicked() {
                    command = Some("cm_SaveSelectionToFile".into());
                }
                if ui.button(self.model.t("cmd.restore_selection")).clicked() {
                    command = Some("cm_LoadSelectionFromFile".into());
                }
            });
            ui.menu_button(self.model.t("menu.commands"), |ui| {
                if ui.button(self.model.t("cmd.search")).clicked() {
                    command = Some("cm_SearchFor".into());
                }
                if ui.button(self.model.t("cmd.sync_dirs")).clicked() {
                    command = Some("cm_SynchronizeDirs".into());
                }
                if ui.button(self.model.t("cmd.multi_rename")).clicked() {
                    command = Some("cm_MultiRename".into());
                }
                if ui.button(self.model.t("cmd.hotlist")).clicked() {
                    command = Some("cm_DirectoryHotlist".into());
                }
                if ui.button(self.model.t("dialog.copy.background")).clicked() {
                    self.model.show_transfer_manager = true;
                }
                if ui.button(self.model.t("cmd.duplicates")).clicked() {
                    command = Some("cm_GetFileSpace".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.calculate_size")).clicked() {
                    command = Some("cm_GetFileSpace".into());
                }
                if ui.button(self.model.t("cmd.new_tab")).clicked() {
                    command = Some("cm_NewTab".into());
                }
            });
            ui.menu_button(self.model.t("menu.show"), |ui| {
                if ui.button(self.model.t("col.name")).clicked() {
                    command = Some("cm_SrcByName".into());
                }
                if ui.button(self.model.t("col.size")).clicked() {
                    command = Some("cm_SrcBySize".into());
                }
                if ui.button(self.model.t("col.date")).clicked() {
                    command = Some("cm_SrcByDateTime".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.branch_view")).clicked() {
                    command = Some("cm_BranchView".into());
                }
                if ui.button(self.model.t("cmd.all_files")).clicked() {
                    command = Some("cm_SrcAllFiles".into());
                }
            });
            ui.menu_button(self.model.t("menu.configuration"), |ui| {
                if ui.button(self.model.t("cmd.config")).clicked() {
                    command = Some("cm_Config".into());
                }
                if ui.button(self.model.t("cmd.refresh")).clicked() {
                    command = Some("cm_RereadSource".into());
                }
                if ui.button(self.model.t("cmd.secrets")).clicked() {
                    self.model.open_dialog(Dialog::UnlockSecrets(
                        crate::model::dialogs::UnlockSecretsState::new(self.model.secrets_exist()),
                    ));
                }
                if ui.button(self.model.t("cmd.gitconfig")).clicked() {
                    command = Some("cm_GitConfig".into());
                }
                if ui.button(self.model.t("cmd.gitpanel")).clicked() {
                    command = Some("cm_GitPanel".into());
                }
                if ui.button(self.model.t("cmd.gitbranches")).clicked() {
                    command = Some("cm_GitBranches".into());
                }
                if ui.button(self.model.t("cmd.gitremote")).clicked() {
                    command = Some("cm_GitRemote".into());
                }
                if ui.button(self.model.t("cmd.gitcopyout")).clicked() {
                    command = Some("cm_GitCopyOut".into());
                }
                if ui.button(self.model.t("cmd.gitstage")).clicked() {
                    command = Some("cm_GitStage".into());
                }
                if ui.button(self.model.t("cmd.gitcommit")).clicked() {
                    command = Some("cm_GitCommit".into());
                }
                if ui.button(self.model.t("cmd.gitclone")).clicked() {
                    command = Some("cm_GitClone".into());
                }
                if ui.button(self.model.t("cmd.aimodel")).clicked() {
                    command = Some("cm_AiModel".into());
                }
                if ui.button("dark <-> normal").clicked() {
                    command = Some("cm_ThemeToggle".into());
                }
            });
            ui.menu_button(self.model.t("menu.net"), |ui| {
                if ui.button(self.model.t("ftp.connect")).clicked() {
                    command = Some("cm_FtpConnect".into());
                }
                if ui.button(self.model.t("ftp.disconnect")).clicked() {
                    command = Some("cm_FtpDisconnect".into());
                }
                ui.separator();
                if ui.button(self.model.t("agent.panel")).clicked() {
                    self.model.show_agent_panel = true;
                }
            });
            ui.menu_button(self.model.t("menu.help"), |ui| {
                if ui.button(self.model.t("cmd.help")).clicked() {
                    command = Some("cm_HelpIndex".into());
                }
                ui.separator();
                if ui.button(self.model.t("cmd.about")).clicked() {
                    command = Some("cm_About".into());
                }
            });
            if let Some(command) = command {
                ui.close();
                self.pending_command = Some(command);
            }
        });
    }

    fn tool_bar(&mut self, ui: &mut egui::Ui) {
        let mut command: Option<String> = None;
        let buttons = self.model.settings.button_bar.buttons.clone();
        let font_size = self.model.settings.display.font_size;
        let palette = self.palette.clone();
        let hint = self.model.t("cmd.button_bar");
        ui.horizontal_wrapped(|ui| {
            for button in buttons.iter() {
                if button.separator {
                    ui.separator();
                }
                let caption = button.label();
                let label = match crate::icons::for_command(&button.command) {
                    Some(icon) => icon_button_text(icon, &caption, font_size, &palette),
                    None => egui::WidgetText::from(egui::RichText::new(&caption).size(font_size)),
                };
                let response = ui
                    .add(egui::Button::new(label).min_size(egui::vec2(0.0, font_size + 10.0)))
                    .on_hover_text(&button.command);
                // Keep the accessible name free of the icon glyph, so screen
                // readers and the UI tests see the plain caption.
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &caption)
                });
                if response.clicked() {
                    command = Some(button.command.clone());
                }
            }
            if ui.small_button("…").on_hover_text(hint).clicked() {
                command = Some("cm_ButtonConfig".to_string());
            }
        });
        if let Some(command) = command {
            self.pending_command = Some(command);
        }
    }

    fn panels(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette.clone();
        let split = self.model.settings.layout.panel_split.clamp(0.2, 0.8);
        let rtl = self.model.direction().is_rtl();
        let available = ui.available_size();
        let left_width = (available.x * split).max(120.0);
        let right_width = (available.x - left_width - 4.0).max(120.0);
        let mut actions: Vec<(Side, PanelAction)> = Vec::new();
        if self.model.quick_view {
            let mut command: Option<String> = None;
            ui.horizontal(|ui| {
                let target_side = if rtl { Side::Left } else { Side::Right };
                ui.allocate_ui_with_layout(
                    egui::vec2(right_width, available.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.quick_view_panel(ui);
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(left_width, available.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        for action in panel_view::show(
                            ui,
                            &mut self.model,
                            target_side.other(),
                            &palette,
                            &mut self.thumbnails,
                        ) {
                            actions.push((target_side.other(), action));
                        }
                    },
                );
            });
            if let Some(command) = command.take() {
                self.pending_command = Some(command);
            }
            for (side, action) in actions {
                match action {
                    PanelAction::Command(command) => self.pending_command = Some(command),
                    PanelAction::OpenFile(path) => self.open_viewer(path),
                    PanelAction::Enter => self.enter_pressed(),
                    PanelAction::Volume(root) => {
                        self.model.open_volume(side, &root);
                    }
                    PanelAction::ContextMenu => {}
                }
            }
            return;
        }
        ui.horizontal(|ui| {
            let order: [Side; 2] = if rtl {
                [Side::Right, Side::Left]
            } else {
                [Side::Left, Side::Right]
            };
            let widths = if rtl {
                [right_width, left_width]
            } else {
                [left_width, right_width]
            };
            for (index, side) in order.iter().enumerate() {
                ui.allocate_ui_with_layout(
                    egui::vec2(widths[index], available.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let response = panel_view::show(
                            ui,
                            &mut self.model,
                            *side,
                            &palette,
                            &mut self.thumbnails,
                        );
                        self.panel_rects[*side as usize] = Some(ui.min_rect());
                        for action in response {
                            actions.push((*side, action));
                        }
                    },
                );
            }
        });
        for (side, action) in actions {
            match action {
                PanelAction::Command(command) => self.pending_command = Some(command),
                PanelAction::OpenFile(path) => self.open_viewer(path),
                PanelAction::Enter => self.enter_pressed(),
                PanelAction::Volume(root) => {
                    self.model.open_volume(side, &root);
                }
                PanelAction::ContextMenu => {}
            }
        }
    }

    /// Content preview of the file under the cursor, rendered in place of the
    /// inactive panel (`Ctrl+Q`).
    fn quick_view_panel(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette.clone();
        let target = self
            .model
            .active()
            .current()
            .filter(|e| !e.is_dir())
            .map(|e| e.path.clone());
        if target != self.quick_view_path {
            self.quick_view_path = target.clone();
            self.quick_viewer = ViewerUi::default();
            if let Some(path) = target {
                let _ = self.quick_viewer.load(&path);
            }
        }
        egui::Frame::new()
            .fill(palette.header_background)
            .stroke(egui::Stroke::new(1.0, palette.panel_border))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(self.model.t("cmd.quick_view"))
                        .small()
                        .color(palette.hidden),
                );
                self.quick_viewer.ensure_texture(ui.ctx());
                let Some(loaded) = self.quick_viewer.loaded.clone() else {
                    ui.label(
                        egui::RichText::new(self.model.t("panel.empty")).color(palette.hidden),
                    );
                    return;
                };
                match &loaded.content {
                    rc_viewer::ViewerContent::Image(_) => {
                        if let Some(texture) = &self.quick_viewer.image_texture {
                            let size = texture.size_vec2();
                            let available = ui.available_size();
                            let scale = (available.x / size.x).min(available.y / size.y).min(1.0);
                            egui::ScrollArea::both()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.image((texture.id(), size * scale));
                                });
                        }
                    }
                    rc_viewer::ViewerContent::Text(text) => {
                        let mut body = String::new();
                        for line in text.text.lines().take(48) {
                            body.push_str(line);
                            body.push('\n');
                        }
                        if text.is_rtl {
                            ui.label(egui::RichText::new("RTL").small().color(palette.directory));
                        }
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(body).monospace())
                                        .selectable(false),
                                );
                            });
                    }
                    rc_viewer::ViewerContent::Hex(doc) => {
                        let mut body = String::new();
                        for line in doc.lines.iter().take(32) {
                            body.push_str(&format!(
                                "{:08x}  {:<48}  |{}|\n",
                                line.offset, line.hex, line.ascii
                            ));
                        }
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(body).monospace())
                                        .selectable(false),
                                );
                            });
                    }
                    other => {
                        ui.label(
                            egui::RichText::new(rc_viewer::describe(&rc_viewer::LoadedViewer {
                                format: loaded.format.clone(),
                                content: other.clone(),
                            }))
                            .color(palette.hidden),
                        );
                    }
                }
            });
    }

    fn command_line(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(">");
            let hint = self.model.t("cmd.open_command_line");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.model.command_line)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY),
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                match self.model.run_command_line() {
                    CommandOutcome::Failed(message) => self.model.report_error(message),
                    CommandOutcome::Quit => {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    _ => {}
                }
            }
        });
    }

    fn function_key_bar(&mut self, ui: &mut egui::Ui) {
        let mut command: Option<String> = None;
        ui.horizontal(|ui| {
            for entry in FUNCTION_KEYS.iter() {
                if ui
                    .small_button(format!(
                        "{} {}",
                        entry.shortcut,
                        self.model.t(entry.label_key)
                    ))
                    .clicked()
                {
                    command = Some(entry.command.to_string());
                }
            }
        });
        if let Some(command) = command {
            self.pending_command = Some(command);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette.clone();
        if self.model.is_busy() {
            let progress = self.model.transfer_progress();
            ui.horizontal(|ui| {
                if let Some((fraction, current)) = progress {
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .desired_width(160.0)
                            .text(format!("{}%", (fraction * 100.0).round() as u32)),
                    );
                    if !current.is_empty() {
                        ui.label(egui::RichText::new(current).small().color(palette.hidden));
                    }
                }
                if ui.small_button(self.model.t("dialog.cancel")).clicked() {
                    self.model.cancel_transfers();
                }
            });
        }
        let text = self.model.status_text();
        let side = match self.model.focus {
            Side::Left => self.model.t("panel.source"),
            Side::Right => self.model.t("panel.target"),
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{side}: {text}")).color(palette.foreground));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(space) = self.model.free_space {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} free",
                            rc_core::format::format_size_unit(space.available)
                        ))
                        .small()
                        .color(palette.hidden),
                    );
                }
                if !self.model.status.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.model.status)
                            .small()
                            .color(palette.hidden),
                    );
                }
            });
        });
    }
}

/// Which panel a drop at `pointer` belongs to; unknown positions use the
/// focused panel, so a drop is never silently ignored.
pub fn resolve_drop_side(
    rects: [Option<egui::Rect>; 2],
    pointer: Option<egui::Pos2>,
    focus: Side,
) -> Side {
    let Some(pos) = pointer else { return focus };
    match (rects[0], rects[1]) {
        (Some(left), _) if left.contains(pos) => Side::Left,
        (_, Some(right)) if right.contains(pos) => Side::Right,
        _ => focus,
    }
}

fn modifiers_are_plain(ctx: &Context) -> bool {
    ctx.input(|i| !(i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command))
}

impl eframe::App for RCommanderApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        let ctx = &ctx;
        self.apply_theme(ctx);
        self.pending_command = None;
        self.persist_settings();
        // Text requested by the clipboard commands goes to the system clipboard.
        if !self.model.clipboard.is_empty() {
            let parts: Vec<String> = self.model.clipboard.drain(..).collect();
            ctx.copy_text(parts.join("\n"));
        }
        self.handle_dropped_files(ctx);
        if self.model.poll_ai_test() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        if self.model.poll_git_push() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        if self.model.poll_connect() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        if self.model.poll_transfers() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        if self.model.poll_search() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        self.handle_shortcuts(ctx);

        egui::Panel::top("menu").show(root_ui, |ui| self.menu_bar(ui));
        if self.model.settings.layout.show_button_bar {
            egui::Panel::top("toolbar").show(root_ui, |ui| self.tool_bar(ui));
        }
        egui::Panel::bottom("status").show(root_ui, |ui| self.status_bar(ui));
        if self.model.settings.layout.show_function_key_bar {
            egui::Panel::bottom("fkeys").show(root_ui, |ui| self.function_key_bar(ui));
        }
        if self.model.settings.layout.show_command_line {
            egui::Panel::bottom("cmdline").show(root_ui, |ui| self.command_line(ui));
        }
        egui::CentralPanel::default().show(root_ui, |ui| self.panels(ui));

        let action = crate::ui::dialogs::show(ctx, &mut self.model);
        if let DialogAction::Command(command) = action {
            self.run_command(&command, ctx)
        }

        let palette = self.palette.clone();
        if self.model.viewer.is_some() {
            let closed =
                crate::ui::viewer_window::show(ctx, &mut self.model, &mut self.viewer_ui, &palette);
            if closed {
                self.model.viewer = None;
            }
        }
        if self.model.editor.is_some() {
            crate::ui::editor_window::show(ctx, &mut self.model, &palette);
        }
        if self.model.help.is_some() {
            crate::ui::help_window::show(ctx, &mut self.model, &mut self.help_ui, &palette);
        }
        if self.model.show_transfer_manager {
            let palette = self.palette.clone();
            if crate::ui::transfer_window::show(ctx, &mut self.model, &palette) {
                self.model.show_transfer_manager = false;
            }
        }
        if self.model.show_agent_panel {
            let closed =
                crate::ui::agent_panel::show(ctx, &mut self.model, &mut self.agent_ui, &palette);
            if closed {
                self.model.show_agent_panel = false;
            }
        }
        if self.model.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(command) = self.pending_command.take() {
            self.run_command(&command, ctx);
        }
    }
}

// The pending command field is declared here to keep the impl blocks tidy.
impl RCommanderApp {
    /// Build the palette for a theme name (used by tests).
    pub fn palette_for(theme: &str) -> Palette {
        // Without a model there is no theme directory, so only the built-ins are
        // reachable; the running application uses `Palette::for_model`.
        Palette::by_name(theme)
    }

    /// Palette of a theme file, resolved through the model's theme set.
    pub fn palette_named(&self, name: &str) -> Palette {
        Palette::from_resolved(&self.model.resolved_theme_named(name))
    }

    /// Human readable title with both panel paths.
    pub fn title(&self) -> String {
        self.model.window_title()
    }

    /// Shortcut display text for a command (used by the menus in the future).
    pub fn shortcut_label(&self, command: &str) -> String {
        self.model
            .keymap
            .shortcut_for(command)
            .map(keys::display_shortcut)
            .unwrap_or_default()
    }

    /// Build a model with an explicit translator (tests).
    pub fn with_strings(
        settings: Settings,
        strings: Box<dyn Strings>,
        left: Location,
        right: Location,
    ) -> Self {
        RCommanderApp::new(Model::boot(settings, strings, left, right))
    }

    /// Convenience wrapper for tests: run a command and return the outcome.
    pub fn execute(&mut self, command: &str) -> CommandOutcome {
        let outcome = self.model.execute(command);
        self.sync_theme();
        outcome
    }

    /// A shortcut that is bound to a command, for the menu labels.
    pub fn shortcut_for(&self, command: &str) -> Option<String> {
        self.model
            .keymap
            .shortcut_for(command)
            .map(keys::display_shortcut)
    }

    pub fn is_modal(&self) -> bool {
        self.model.modal()
    }

    pub fn dialog(&self) -> Option<&Dialog> {
        self.model.top_dialog()
    }

    pub fn key_stub() -> Shortcut {
        Shortcut::new("F1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(100.0, 100.0))
    }

    #[test]
    fn a_drop_inside_a_panel_targets_that_panel() {
        let rects = [Some(rect(0.0)), Some(rect(200.0))];
        assert_eq!(
            resolve_drop_side(rects, Some(egui::pos2(50.0, 50.0)), Side::Right),
            Side::Left
        );
        assert_eq!(
            resolve_drop_side(rects, Some(egui::pos2(250.0, 50.0)), Side::Left),
            Side::Right
        );
    }

    #[test]
    fn a_drop_outside_both_panels_falls_back_to_the_focused_panel() {
        let rects = [Some(rect(0.0)), Some(rect(200.0))];
        assert_eq!(
            resolve_drop_side(rects, Some(egui::pos2(500.0, 500.0)), Side::Left),
            Side::Left
        );
        assert_eq!(
            resolve_drop_side(rects, Some(egui::pos2(500.0, 500.0)), Side::Right),
            Side::Right
        );
    }

    #[test]
    fn an_unknown_pointer_position_uses_the_focus() {
        assert_eq!(
            resolve_drop_side([None, None], None, Side::Right),
            Side::Right
        );
        assert_eq!(
            resolve_drop_side(
                [Some(rect(0.0)), None],
                Some(egui::pos2(50.0, 50.0)),
                Side::Right
            ),
            Side::Left
        );
    }
}

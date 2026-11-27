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

// The F-key row used to live here as one static table. It moved into the model
// (`model::surfaces`) when the keyboard became surface-aware: the chat, the editor and the viewer
// each publish their own row, and the headless frontend needs the same one.

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
    /// Scroll and focus state of the chat window.
    chat_ui: crate::ui::chat_window::ChatUi,
    /// Thumbnail textures for the thumbnail view mode.
    thumbnails: ThumbnailCache,
    /// Index of the search result the user picked.
    search_hit_selection: usize,
    /// Screen rectangles of the two panels, used to resolve drop targets.
    /// Screen rectangles of the two panels, used to resolve drop targets.
    pub panel_rects: [Option<egui::Rect>; 2],
    /// The rectangle the two panels occupy, captured every frame so a workspace window knows what
    /// "cover the panels" means — the panels, not the whole window including the menu bar.
    workspace_rect: egui::Rect,
    /// The alert being shown in the status line, with the moment it arrived.
    alert_until: Option<(crate::model::AlertKind, String, std::time::Instant)>,
    /// Theme name that is currently applied to the egui context.
    applied_theme: String,
    /// Font settings the context was built from, so a change reinstalls the fonts.
    font_signature: String,
    /// Persists the settings file whenever something changes.
    pub settings_writer: SettingsWriter,
    /// Requests from `rcommander mcp` waiting for the human (Phase 16.0e).
    bridge_queue: Option<std::sync::mpsc::Receiver<rc_agent::bridge::IncomingRequest>>,
    /// Kept alive so the endpoint stays published while the window runs.
    approval_server: Option<rc_agent::ApprovalServer>,
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
    text_font: egui::FontId,
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
            font_id: text_font,
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
            chat_ui: crate::ui::chat_window::ChatUi::default(),
            thumbnails: ThumbnailCache::new(),
            search_hit_selection: 0,
            panel_rects: [None, None],
            workspace_rect: egui::Rect::NOTHING,
            alert_until: None,
            applied_theme: String::new(),
            font_signature: String::new(),
            settings_writer,
            bridge_queue: None,
            approval_server: None,
        };
        app.help_ui.reload(&paths::docs_dir());
        app
    }

    /// Publish an approval endpoint so `rcommander mcp` can ask *this* window.
    ///
    /// Called by the binary at start-up, not by `new`, because a test harness has no business
    /// accepting socket connections. Returns false when no loopback socket could be opened; the
    /// terminal confirmer still works, so this is a degraded mode and not a failure.
    pub fn start_approval_bridge(&mut self) -> bool {
        let (sender, receiver) = std::sync::mpsc::channel();
        match rc_agent::ApprovalServer::start(sender) {
            Some(server) => {
                self.approval_server = Some(server);
                self.bridge_queue = Some(receiver);
                true
            }
            None => false,
        }
    }

    /// Take the waiting agent approvals and open the dialog for the first one.
    fn poll_approval_bridge(&mut self) {
        let Some(queue) = self.bridge_queue.as_ref() else {
            return;
        };
        // Drain everything that arrived; the dialog shows the oldest first, the rest wait.
        let mut incoming: Vec<rc_agent::bridge::IncomingRequest> = Vec::new();
        while let Ok(request) = queue.try_recv() {
            incoming.push(request);
        }
        for request in incoming {
            let tool = request.tool.clone();
            let args = request.args.clone();
            self.model.ask_bridge_approval(tool, args, request);
        }
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
        let mut model = Model::boot(settings, strings, left, right);
        // A manifest that does not parse is reported on the status line and skipped; the rest of
        // the application does not depend on any plugin being there.
        let summary = model.load_plugins();
        model.report(summary);
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
        let rtl = self.model.direction().is_rtl();
        let signature = format!("{}|rtl={rtl}", theme::font_signature(&self.model.settings.fonts, font_size));
        if self.fonts_installed
            && self.applied_theme == theme_name
            && self.font_signature == signature
        {
            return;
        }
        self.palette = Palette::for_model(&self.model);
        theme::install_icon_font(ctx);
        // Only scan the machine when a custom face was actually chosen: the default installation
        // must not pay for a directory walk on every start.
        let (installed, failures) = if self.model.settings.fonts.is_builtin() {
            (Vec::new(), Vec::new())
        } else {
            let discovered = self.model.font_choices();
            let (installed, failures) =
                theme::install_user_fonts(ctx, &self.model.settings.fonts, &discovered);
            // The icon font has to be re-registered for the built-in families as well.
            theme::install_icon_font(ctx);
            (installed, failures)
        };
        if rtl {
            let discovered = self.model.font_choices();
            theme::install_arabic_fallback(ctx, &discovered, &installed);
        }
        theme::install_text_styles(ctx, &self.model.settings.fonts, font_size, &installed);
        ctx.set_visuals(theme::visuals(&self.palette, theme_name == "dark"));
        theme::install_menu_style(ctx);
        self.applied_theme = theme_name;
        self.font_signature = signature;
        self.fonts_installed = true;
        self.model.font_problems = failures;
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
    /// The path the desktop actions act on: the cursor entry, or the panel's own directory.
    fn desktop_target(&self, want_directory: bool) -> Option<std::path::PathBuf> {
        let panel = self.model.active();
        let rc_core::location::Location::Local(directory) = panel.location() else {
            return None;
        };
        if want_directory {
            return Some(directory.clone());
        }
        panel.current().map(|entry| entry.path.clone())
    }

    /// Run one of the desktop actions the context menu offers.
    fn run_desktop_action(&mut self, action: rc_core::desktop::Action) {
        match crate::system_open::run(&action) {
            Ok(started) => {
                let key = if started.program {
                    "status.running"
                } else {
                    "status.opening"
                };
                let message = self.model.tf(key, &[("what", &started.what)]);
                self.model.report(message);
            }
            Err(error) => {
                let message = self
                    .model
                    .tf("status.open.failed", &[("path", ""), ("error", &error)]);
                self.model.report_error(message);
            }
        }
    }

    pub fn run_command(&mut self, command: &str, ctx: &Context) {
        // The desktop actions need the cursor and the panel rather than the command table, so they
        // are handled here, where both are in reach.
        match command {
            "cm_EditorClose" => {
                if matches!(self.model.editor_close(), CommandOutcome::Failed(_)) {
                    self.editor_ui.dirty_prompt = Some(self.model.t("editor.discard_prompt"));
                }
                return;
            }
            "cm_ViewerReload" => {
                if let Some(state) = &self.model.viewer {
                    let _ = self.viewer_ui.load(std::path::Path::new(&state.path));
                }
                return;
            }
            "cm_OpenFolderExternal" => {
                // Open the actual panel directory in the platform file manager.
                // Unlike a row action this is safe for the background menu too.
                if let Some(path) = self.desktop_target(true) {
                    self.run_desktop_action(rc_core::desktop::Action::Open(path));
                }
                return;
            }
            "cm_OpenExternal" => {
                if let Some(path) = self.desktop_target(false) {
                    self.run_desktop_action(rc_core::desktop::Action::Open(path));
                }
                return;
            }
            "cm_SendByEmail" => {
                if let Some(path) = self.desktop_target(false) {
                    self.run_desktop_action(rc_core::desktop::Action::MailTo(path));
                }
                return;
            }
            "cm_TerminalHere" => {
                if let Some(path) = self.desktop_target(true) {
                    self.run_desktop_action(rc_core::desktop::Action::TerminalAt(path));
                }
                return;
            }
            "cm_RunInTerminal" => {
                if let Some(path) = self.desktop_target(false) {
                    self.run_desktop_action(rc_core::desktop::Action::RunInTerminal(path));
                }
                return;
            }
            "cm_OpenWith" => {
                // Windows has a native chooser; elsewhere the dialog lists the desktop's own
                // applications, which is the closest thing to the system menu.
                if rc_core::desktop::Platform::current() == rc_core::desktop::Platform::Windows {
                    if let Some(path) = self.desktop_target(false) {
                        self.run_desktop_action(rc_core::desktop::Action::OpenWith {
                            path,
                            app: String::new(),
                        });
                    }
                } else if let Some(path) = self.desktop_target(false) {
                    self.model.open_dialog(Dialog::OpenWith(
                        crate::model::dialogs::OpenWithState::new(
                            &path,
                            rc_core::desktop::desktop_apps(),
                        ),
                    ));
                }
                return;
            }
            _ => {}
        }
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
            CommandOutcome::OpenFile(path) => {
                if matches!(
                    command.to_ascii_lowercase().as_str(),
                    "cm_edit" | "cm_editnewfile" | "cm_editnew"
                ) {
                    self.open_editor(path);
                } else {
                    self.open_viewer(path);
                }
            }
            CommandOutcome::OpenExternally(path) => self.open_externally(path),
            CommandOutcome::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            CommandOutcome::Minimize => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
            }
            CommandOutcome::Failed(message) => self.model.report_error(message),
            _ => {}
        }
        self.sync_theme();
    }

    /// The rectangle a maximized workspace window should fill.
    fn workspace_for_windows(&self, ctx: &Context) -> egui::Rect {
        if self.workspace_rect.is_positive() {
            self.workspace_rect
        } else {
            // The very first frame has not drawn the panels yet; the content rect is the honest
            // stand-in and is replaced one frame later.
            ctx.content_rect()
        }
    }

    /// Show `path` in the F3 viewer.
    pub fn open_viewer(&mut self, path: std::path::PathBuf) {
        self.model.open_viewer_for(path.clone());
        if !self.viewer_ui.load(&path) {
            let error = self.viewer_ui.error.clone().unwrap_or_default();
            self.model.report_error(error);
        }
    }

    /// Hand `path` to the platform's default action, which is what Enter and a double-click mean.
    ///
    /// The result is announced rather than swallowed: a file manager that appears to do nothing is
    /// the worst possible answer, and the message also says whether something was *run* or *opened*.
    pub fn open_externally(&mut self, path: std::path::PathBuf) {
        match crate::system_open::open(&path) {
            Ok(started) => {
                let key = if started.program {
                    "status.running"
                } else {
                    "status.opening"
                };
                let message = self.model.tf(key, &[("what", &started.what)]);
                self.model.report(message);
            }
            Err(error) => {
                let message = self.model.tf(
                    "status.open.failed",
                    &[("path", &path.display().to_string()), ("error", &error)],
                );
                self.model.report_error(message);
            }
        }
    }

    /// Show `path` in the editor.
    pub fn open_editor(&mut self, path: std::path::PathBuf) {
        match self.model.open_editor_for(path) {
            Ok(()) => self.editor_ui = EditorUi::default(),
            Err(e) => self.model.report_error(e.to_string()),
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
        if self.model.modal() {
            return;
        }
        let widget_focused = ctx.memory(|m| m.focused()).is_some();
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    // Let ordinary text input and editing shortcuts reach the widget.
                    // Only document function keys and Escape are handled while it has focus.
                    if widget_focused
                        && !(matches!(
                            self.model.surface(),
                            crate::model::Surface::Editor | crate::model::Surface::Viewer
                        ) && modifiers.is_none()
                            && matches!(
                                key,
                                Key::Escape
                                    | Key::F1
                                    | Key::F2
                                    | Key::F3
                                    | Key::F4
                                    | Key::F5
                                    | Key::F6
                                    | Key::F7
                                    | Key::F8
                                    | Key::F9
                                    | Key::F10
                                    | Key::F11
                                    | Key::F12
                            ))
                    {
                        continue;
                    }
                    if key == Key::Escape {
                        if self.model.surface() == crate::model::Surface::Viewer {
                            self.model.viewer = None;
                        } else if self.model.surface() == crate::model::Surface::Editor {
                            self.run_command("cm_EditorClose", ctx);
                        } else if self.model.help.is_some() {
                            self.model.help = None;
                        } else {
                            self.model.cancel_quick_search();
                            self.model.close_dialogs();
                        }
                        continue;
                    }
                    let slot = match key {
                        Key::F1 => 1,
                        Key::F2 => 2,
                        Key::F3 => 3,
                        Key::F4 => 4,
                        Key::F5 => 5,
                        Key::F6 => 6,
                        Key::F7 => 7,
                        Key::F8 => 8,
                        Key::F9 => 9,
                        Key::F10 => 10,
                        Key::F11 => 11,
                        Key::F12 => 12,
                        _ => 0,
                    };
                    if slot != 0
                        && modifiers.is_none()
                        && !matches!(self.model.surface(), crate::model::Surface::Panel(_))
                    {
                        if let Some(entry) = self
                            .model
                            .function_keys()
                            .into_iter()
                            .find(|entry| entry.slot == slot && !entry.is_empty())
                        {
                            self.run_command(entry.command, ctx);
                        }
                        continue;
                    }
                    if let Some(shortcut) = keys::shortcut_from_egui(key, modifiers)
                        && let Some(command) =
                            self.model.keymap.lookup(&shortcut).map(str::to_owned)
                    {
                        self.run_command(&command, ctx);
                        continue;
                    }
                    if !matches!(self.model.surface(), crate::model::Surface::Panel(_)) {
                        continue;
                    }
                    // Unhandled keys: cursor movement and quick search.
                    self.handle_navigation_key(key, modifiers);
                }
                egui::Event::Text(text) => {
                    if widget_focused
                        || !modifiers_are_plain(ctx)
                        || !matches!(self.model.surface(), crate::model::Surface::Panel(_))
                    {
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
            CommandOutcome::OpenExternally(path) => self.open_externally(path),
            CommandOutcome::Ok => {}
            CommandOutcome::Failed(message) => self.model.report_error(message),
            _ => {}
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            let mut command: Option<String> = None;
            for &(group, actions) in crate::model::actions::ACTION_GROUPS {
                ui.menu_button(self.model.t(group), |ui| {
                    for &(action, key) in actions {
                        if action == "cm_ChatFiles" && !self.model.any_panel_shows_chat() {
                            continue;
                        }
                        if matches!(
                            action,
                            "cm_ReviewWorkspace" | "cm_OpenWorkspace" | "cm_DiscardWorkspace"
                        ) && self.model.agent_workspace.is_none()
                        {
                            continue;
                        }
                        if ui.button(self.model.t(key)).clicked() {
                            command = Some(action.to_owned());
                            ui.close();
                        }
                    }
                });
            }
            if let Some(command) = command {
                ui.close();
                self.pending_command = Some(command);
            }
        });
    }

    fn tool_bar(&mut self, ui: &mut egui::Ui) {
        let mut command: Option<String> = None;
        let mut edit: Option<usize> = None;
        let mut remove: Option<usize> = None;
        let buttons = self.model.settings.button_bar.buttons.clone();
        let font_size = self.model.settings.display.font_size;
        let palette = self.palette.clone();
        let hint = self.model.t("cmd.button_bar");
        ui.horizontal_wrapped(|ui| {
            for (index, button) in buttons.iter().enumerate() {
                if button.separator {
                    ui.separator();
                }
                let caption = crate::model::actions::button_caption(&self.model, button);
                let icon = if button.command.starts_with("rc_OpenDirectory:") {
                    Some(crate::icons::Icon::Folder)
                } else {
                    crate::icons::for_command(&button.command)
                };
                let label = match icon {
                    Some(icon) => icon_button_text(icon, &caption, font_size, ui.style().text_styles[&egui::TextStyle::Button].clone(), &palette),
                    None => egui::WidgetText::from(egui::RichText::new(&caption).size(font_size)),
                };
                let response = ui
                    .add(egui::Button::new(label).min_size(egui::vec2(0.0, font_size + 10.0)))
                    .on_hover_text(&button.command);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &caption)
                });
                if response.clicked() {
                    command = Some(button.command.clone());
                }
                response.context_menu(|ui| {
                    if ui.button(self.model.t("cmd.edit")).clicked() {
                        edit = Some(index);
                        ui.close();
                    }
                    if ui.button(self.model.t("toolbar.remove")).clicked() {
                        remove = Some(index);
                        ui.close();
                    }
                });
            }
            ui.menu_button("+", |ui| {
                for (action, key) in [
                    ("cm_ButtonAdd", "toolbar.add_action"),
                    ("cm_ButtonAddDirectory", "toolbar.add_directory"),
                    ("cm_ButtonConfig", "cmd.button_bar"),
                ] {
                    if ui.button(self.model.t(key)).clicked() {
                        command = Some(action.to_owned());
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text(self.model.t("toolbar.add"));
            if ui.small_button("…").on_hover_text(hint).clicked() {
                command = Some("cm_ButtonConfig".to_string());
            }
        });
        if let Some(index) = remove {
            let mut state = crate::model::dialogs::ButtonBarState::new();
            state.selected = Some(index);
            let _ = self.model.remove_button_bar(&mut state);
        }
        if let Some(index) = edit
            && let Some(button) = buttons.get(index)
        {
            let mut state = crate::model::dialogs::ButtonBarState::new();
            state.edit(index, button);
            self.model.open_dialog(Dialog::ButtonBar(state));
        }
        if let Some(command) = command {
            self.pending_command = Some(command);
        }
    }

    fn panels(&mut self, ui: &mut egui::Ui) {
        // The area the panels are about to fill; a maximized or pinned window covers exactly this.
        self.workspace_rect = ui.max_rect();
        let palette = self.palette.clone();
        use crate::model::DocumentPlacement;
        if self
            .model
            .editor
            .as_ref()
            .is_some_and(|s| s.placement == DocumentPlacement::Workspace)
        {
            self.panel_rects = [None, None];
            crate::ui::editor_window::embedded(ui, &mut self.model, &mut self.editor_ui, &palette);
            return;
        }
        if self
            .model
            .viewer
            .as_ref()
            .is_some_and(|s| s.placement == DocumentPlacement::Workspace)
        {
            self.panel_rects = [None, None];
            crate::ui::viewer_window::embedded(ui, &mut self.model, &mut self.viewer_ui, &palette);
            return;
        }
        let split = self.model.settings.layout.panel_split.clamp(0.2, 0.8);
        let rtl = self.model.direction().is_rtl();
        let available = ui.available_size();
        let left_width = (available.x * split).max(120.0);
        let right_width = (available.x - left_width - 4.0).max(120.0);
        let mut actions: Vec<(Side, PanelAction)> = Vec::new();
        let docked = self
            .model
            .editor
            .as_ref()
            .is_some_and(|s| matches!(s.placement, DocumentPlacement::Panel(_)))
            || self
                .model
                .viewer
                .as_ref()
                .is_some_and(|s| matches!(s.placement, DocumentPlacement::Panel(_)));
        if self.model.quick_view && !docked {
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
                    PanelAction::Command(command) => {
                        self.model.focus = side;
                        self.pending_command = Some(command);
                    }
                    PanelAction::OpenFile(path) => self.open_viewer(path),
                    PanelAction::OpenExternal(path) => self.open_externally(path),
                    PanelAction::Enter => self.enter_pressed(),
                    PanelAction::Volume(root) => {
                        self.model.open_volume(side, &root);
                    }
                    PanelAction::SourceCommand(command) => {
                        if let CommandOutcome::Failed(message) =
                            self.model.run_source_command(side, &command)
                        {
                            self.model.report_error(message);
                        }
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
            // Docked documents contain independent scroll areas. Reserve a fixed
            // rectangle for each pane so a child's desired size cannot resize
            // its sibling or the parent on the next frame.
            let dock_width = (available.x - ui.spacing().item_spacing.x).max(2.0);
            let dock_widths = if rtl {
                [dock_width * (1.0 - split), dock_width * split]
            } else {
                [dock_width * split, dock_width * (1.0 - split)]
            };
            for (index, side) in order.iter().enumerate() {
                let mut draw = |ui: &mut egui::Ui| {
                    let placement = DocumentPlacement::Panel(*side);
                    let editor_here = self
                        .model
                        .editor
                        .as_ref()
                        .is_some_and(|s| s.placement == placement);
                    let viewer_here = self
                        .model
                        .viewer
                        .as_ref()
                        .is_some_and(|s| s.placement == placement);
                    if editor_here || viewer_here {
                        if ui.rect_contains_pointer(ui.max_rect())
                            && ui.input(|i| i.pointer.any_pressed())
                        {
                            self.model.focus = *side;
                        }
                        self.panel_rects[*side as usize] = None;
                        if editor_here {
                            crate::ui::editor_window::embedded(
                                ui,
                                &mut self.model,
                                &mut self.editor_ui,
                                &palette,
                            );
                        } else {
                            crate::ui::viewer_window::embedded(
                                ui,
                                &mut self.model,
                                &mut self.viewer_ui,
                                &palette,
                            );
                        }
                        return;
                    }
                    if self.model.panel_shows_chat(*side) {
                        let pane = crate::ui::chat_window::pane(
                            ui,
                            &mut self.model,
                            &mut self.chat_ui,
                            &palette,
                            *side,
                        );
                        if pane.show_files {
                            self.pending_command = Some("cm_ChatFiles".to_string());
                        }
                        self.panel_rects[*side as usize] = Some(ui.min_rect());
                        return;
                    }
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
                };
                if docked {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(dock_widths[index], available.y.max(1.0)),
                        egui::Sense::hover(),
                    );
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt(("docked_pane", *side))
                            .max_rect(rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    child.set_clip_rect(rect.intersect(ui.clip_rect()));
                    draw(&mut child);
                } else {
                    ui.allocate_ui_with_layout(
                        egui::vec2(widths[index], available.y),
                        egui::Layout::top_down(egui::Align::Min),
                        draw,
                    );
                }
            }
        });
        for (side, action) in actions {
            match action {
                PanelAction::Command(command) => {
                        self.model.focus = side;
                        self.pending_command = Some(command);
                    }
                PanelAction::OpenFile(path) => self.open_viewer(path),
                PanelAction::OpenExternal(path) => self.open_externally(path),
                PanelAction::Enter => self.enter_pressed(),
                PanelAction::Volume(root) => {
                    self.model.open_volume(side, &root);
                }
                PanelAction::SourceCommand(command) => {
                    if let CommandOutcome::Failed(message) =
                        self.model.run_source_command(side, &command)
                    {
                        self.model.report_error(message);
                    }
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
                if self.quick_viewer.markdown_preview(ui, &palette, &self.model, true)
                    || self.quick_viewer.data_preview(ui, &self.model, true)
                {
                    return;
                }
                let Some(loaded) = self.quick_viewer.loaded.as_ref() else {
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
                    _ => {
                        ui.label(egui::RichText::new(rc_viewer::describe(loaded)).color(palette.hidden));
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

    /// The F-key bar: whatever the focused surface says those keys mean, with their variants.
    ///
    /// Two rows, the way Total Commander has always drawn it: the plain function keys on top and
    /// the modifier variants under them. The row comes from the model, so the bar and the keys
    /// cannot disagree; an empty slot is not drawn at all, because a button that cannot work is
    /// worse than a missing one.
    fn function_key_bar(&mut self, ui: &mut egui::Ui) {
        let mut command: Option<String> = None;
        let keys: Vec<crate::model::FnKey> = self
            .model
            .function_keys()
            .into_iter()
            .filter(|entry| !entry.is_empty())
            .collect();
        let surface = self.model.surface();
        let palette = self.palette.clone();
        let surface_label = match surface {
            crate::model::Surface::Panel(_) => self.model.t("surface.files"),
            crate::model::Surface::Chat => self.model.t("surface.chat"),
            crate::model::Surface::Editor => self.model.t("surface.editor"),
            crate::model::Surface::Viewer => self.model.t("surface.viewer"),
        };
        // Which modifier is held right now. A held modifier turns the key's caption into the
        // function it would run, which is the whole point: no second row of buttons underneath and
        // no legend to read — hold Shift and F1 says what Shift+F1 does.
        let held = ui.input(|i| {
            if i.modifiers.shift {
                Some("shift")
            } else if i.modifiers.ctrl || i.modifiers.command {
                Some("ctrl")
            } else if i.modifiers.alt {
                Some("alt")
            } else {
                None
            }
        });
        let side = self.model.focus;
        // Collected before the closure: they borrow the model, the buttons do not.
        let rows: Vec<(
            String,
            String,
            String,
            bool,
            Option<crate::model::surfaces::KeyVariant>,
        )> = keys
            .iter()
            .map(|key| {
                let variant = held.and_then(|modifier| {
                    self.model
                        .key_variants(key.slot)
                        .into_iter()
                        .find(|variant| variant.modifier == modifier)
                });
                let (shortcut, label) = match &variant {
                    Some(variant) => {
                        let modifier = self.model.t(variant.modifier_key);
                        let caption = if variant.label_key.is_empty() {
                            variant.command.clone()
                        } else {
                            self.model.t(variant.label_key)
                        };
                        (format!("{modifier}+F{}", key.slot), caption)
                    }
                    None => (key.shortcut(), self.model.t(key.label_key)),
                };
                // The command this button would run, and whether the place allows it.
                let command = variant
                    .as_ref()
                    .map(|variant| variant.command.clone())
                    .unwrap_or_else(|| key.command.to_string());
                let allowed = self.model.allows(side, &command);
                (shortcut, label, command, allowed, variant)
            })
            .collect();
        let unavailable_hint = self.model.t("cmd.not_available_here");
        let rtl = self.model.direction().is_rtl();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&surface_label)
                        .small()
                        .color(palette.hidden),
                );
                ui.separator();
                for (shortcut, label, recipe, allowed, variant) in &rows {
                    // Isolate the Latin shortcut from the surrounding Arabic caption.
                    let mut text = if rtl {
                        format!("\u{2066}{shortcut}\u{2069} {label}")
                    } else {
                        format!("{shortcut} {label}")
                    };
                    if variant.is_some() {
                        // A caption that changed because of a held key says so, or it looks like
                        // the bar randomly renamed itself.
                        text.push('*');
                    }
                    let button = egui::Button::new(text).small();
                    let response = ui.add_enabled(*allowed, button);
                    if *allowed {
                        if response.clicked() {
                            command = Some(recipe.clone());
                        }
                    } else {
                        response.on_disabled_hover_text(unavailable_hint.replace("{cmd}", label));
                    }
                }
            });
        });
        if let Some(command) = command {
            self.pending_command = Some(command);
        }
    }

    /// What the application is doing *right now*, or `None` when it is idle.
    ///
    /// The rule for the status line is that it never goes silent while work is in progress: a
    /// connection that is authenticating, a transfer that is running and an agent that is thinking
    /// all say so. "Nothing happened" is the one message a file manager must never send.
    fn current_activity(&self) -> Option<String> {
        if let Some(stage) = self.model.connect_stage() {
            return Some(self.model.t(stage.key()));
        }
        if let Some(progress) = self.model.transfer_progress() {
            let (fraction, current) = progress;
            let percent = (fraction * 100.0).round() as u32;
            return Some(if current.is_empty() {
                format!("{}%", percent)
            } else {
                format!("{percent}% · {current}")
            });
        }
        if self.model.chat_is_running() {
            let waiting = self.model.chat_waiting();
            return Some(match waiting {
                Some(wait) if !wait.responding => {
                    let elapsed = format!("{}:{:02}", wait.elapsed / 60, wait.elapsed % 60);
                    format!("{} · {elapsed}", self.model.t("ai.wait.loading"))
                }
                _ => self.model.t("ai.chat.running"),
            });
        }
        if self.model.is_searching() {
            return Some(self.model.t("cmd.search"));
        }
        if self.model.is_connecting() {
            return Some(self.model.t("session.connecting.short"));
        }
        None
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette.clone();
        // An alert is played once and shown until something replaces it. The sound is fired here
        // because this is the one place that runs exactly once per alert.
        if let Some(alert) = self.model.take_alert() {
            if let Some(sound) = alert.sound
                && self.model.settings.sounds.allows(sound)
            {
                crate::system_open::play_sound(sound);
            }
            self.alert_until = Some((alert.kind, alert.text, alert.at));
        }
        if let Some((kind, text, at)) = self.alert_until.clone() {
            let age = at.elapsed().as_secs_f32();
            // An error stays for a while and then folds into the ordinary status; a "done" is a
            // moment, not a state.
            let lifetime = if kind == crate::model::AlertKind::Error {
                12.0
            } else {
                4.0
            };
            if age > lifetime {
                self.alert_until = None;
            } else {
                let colour = match kind {
                    crate::model::AlertKind::Error => palette.warning,
                    crate::model::AlertKind::Done => palette.archive,
                };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(match kind {
                            crate::model::AlertKind::Error => "!",
                            crate::model::AlertKind::Done => "✓",
                        })
                        .strong()
                        .color(colour),
                    );
                    ui.label(egui::RichText::new(text).color(colour));
                });
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
        if let Some(activity) = self.current_activity() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new(activity).small().color(palette.accent));
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(150));
            });
        }
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
        if self.model.poll_chat() {
            // A streaming answer has to be redrawn to look alive.
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
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
        self.poll_approval_bridge();
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
        if self
            .model
            .viewer
            .as_ref()
            .is_some_and(|s| s.placement == crate::model::DocumentPlacement::Floating)
        {
            let closed =
                crate::ui::viewer_window::show(ctx, &mut self.model, &mut self.viewer_ui, &palette);
            if closed {
                self.model.viewer = None;
            }
        }
        if self
            .model
            .editor
            .as_ref()
            .is_some_and(|s| s.placement == crate::model::DocumentPlacement::Floating)
        {
            let workspace = self.workspace_for_windows(ctx);
            crate::ui::editor_window::show(
                ctx,
                &mut self.model,
                &mut self.editor_ui,
                &palette,
                workspace,
            );
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
        if self.model.show_chat_panel {
            let palette = self.palette.clone();
            let workspace = self.workspace_for_windows(ctx);
            if crate::ui::chat_window::show(
                ctx,
                &mut self.model,
                &mut self.chat_ui,
                &palette,
                workspace,
            ) {
                self.model.show_chat_panel = false;
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

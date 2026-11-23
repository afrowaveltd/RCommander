//! Root egui application: layout, menus, keyboard and window glue.

use crate::i18n::{Strings, TranslatorStrings};
use crate::keys;
use crate::model::{CommandOutcome, Dialog, Model, Side};
use crate::paths;
use crate::theme::{self, Palette};
use crate::ui::agent_panel::AgentUi;
use crate::ui::dialogs::DialogAction;
use crate::ui::editor_window::EditorUi;
use crate::ui::help_window::HelpUi;
use crate::ui::panel_view::{self, PanelAction};
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
struct ToolButton {
    label_key: &'static str,
    command: &'static str,
}

const TOOL_BUTTONS: [ToolButton; 10] = [
    ToolButton {
        label_key: "panel.source",
        command: "cm_SrcShort",
    },
    ToolButton {
        label_key: "panel.target",
        command: "cm_SrcLong",
    },
    ToolButton {
        label_key: "cmd.search",
        command: "cm_SearchFor",
    },
    ToolButton {
        label_key: "cmd.pack",
        command: "cm_PackFiles",
    },
    ToolButton {
        label_key: "cmd.unpack",
        command: "cm_UnpackFiles",
    },
    ToolButton {
        label_key: "cmd.multi_rename",
        command: "cm_MultiRename",
    },
    ToolButton {
        label_key: "cmd.sync_dirs",
        command: "cm_SynchronizeDirs",
    },
    ToolButton {
        label_key: "cmd.hotlist",
        command: "cm_DirectoryHotlist",
    },
    ToolButton {
        label_key: "cmd.calculate_size",
        command: "cm_GetFileSpace",
    },
    ToolButton {
        label_key: "cmd.config",
        command: "cm_Config",
    },
];

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
}

impl RCommanderApp {
    /// Create the application from an already built model.
    pub fn new(model: Model) -> Self {
        let palette = Palette::by_name(&model.settings.colors.theme);
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
        };
        app.help_ui.reload(&paths::docs_dir());
        app
    }

    /// Build a model from disk: settings, translator, help and two panels.
    pub fn boot(left: Option<Location>, right: Option<Location>) -> Self {
        let settings = Settings::load(&paths::settings_path()).unwrap_or_default();
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
        if command == "rc_Quit" {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
        self.palette = Palette::by_name(&self.model.settings.colors.theme);
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
        ui.horizontal_wrapped(|ui| {
            for button in TOOL_BUTTONS.iter() {
                if ui.button(self.model.t(button.label_key)).clicked() {
                    command = Some(button.command.to_string());
                }
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
        let mut actions: Vec<PanelAction> = Vec::new();
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
                        actions.extend(panel_view::show(
                            ui,
                            &mut self.model,
                            target_side.other(),
                            &palette,
                        ));
                    },
                );
            });
            if let Some(command) = command.take() {
                self.pending_command = Some(command);
            }
            for action in actions {
                match action {
                    PanelAction::Command(command) => self.pending_command = Some(command),
                    PanelAction::OpenFile(path) => self.open_viewer(path),
                    PanelAction::Enter => self.enter_pressed(),
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
                        actions.extend(panel_view::show(ui, &mut self.model, *side, &palette));
                    },
                );
            }
        });
        for action in actions {
            match action {
                PanelAction::Command(command) => self.pending_command = Some(command),
                PanelAction::OpenFile(path) => self.open_viewer(path),
                PanelAction::Enter => self.enter_pressed(),
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

fn modifiers_are_plain(ctx: &Context) -> bool {
    ctx.input(|i| !(i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command))
}

impl eframe::App for RCommanderApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        let ctx = &ctx;
        if !self.fonts_installed {
            theme::install_text_styles(ctx, self.model.settings.display.font_size);
            ctx.set_visuals(theme::visuals(
                &self.palette,
                self.model.settings.colors.theme == "dark",
            ));
            self.fonts_installed = true;
        }
        self.pending_command = None;
        if self.model.poll_transfers() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
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
        Palette::by_name(theme)
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

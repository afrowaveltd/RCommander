//! Rendering and mouse handling of one file panel.

use crate::icons::Icon;
use crate::model::{Model, Side, ViewMode};
use crate::theme::{Palette, RowStyle};
use crate::ui::thumbs::ThumbnailCache;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, Ui, Vec2};
use rc_core::entry::Entry;

/// Something the panel wants the application to do.
#[derive(Clone, Debug, PartialEq)]
pub enum PanelAction {
    /// Run an internal command (e.g. after a header click).
    Command(String),
    /// Preview the file (F3 semantics).
    OpenFile(std::path::PathBuf),
    /// Open with the platform's default action (double-click, Enter).
    OpenExternal(std::path::PathBuf),
    /// Open the directory the user double-clicked.
    Enter,
    /// Show a context menu for the cursor row.
    ContextMenu,
    /// Open a volume root chosen in the selector.
    Volume(std::path::PathBuf),
}

/// Background of one file row.
///
/// The cursor and the marks win over the zebra pattern, and an inactive panel
/// only tints its cursor row so it stays visibly "not focused".
pub fn row_background(
    index: usize,
    is_cursor: bool,
    is_selected: bool,
    active: bool,
    palette: &Palette,
) -> Option<Color32> {
    if is_cursor && active {
        return Some(palette.cursor_background);
    }
    if is_selected {
        return Some(palette.selected_background);
    }
    if is_cursor {
        return Some(palette.header_background);
    }
    // Every second row gets a very light tint, like the classic commander.
    if index % 2 == 1 {
        Some(palette.row_alternate)
    } else {
        None
    }
}

/// Text of one configured column for an entry.
fn column_text(id: &str, entry: &Entry, display: &rc_core::config::Display) -> String {
    match id {
        "name" => format!(
            "{} {}",
            crate::icons::Icon::for_entry(entry.kind, &entry.name).glyph(),
            entry.name
        ),
        "ext" => entry.extension(),
        "size" => {
            if entry.is_dir() {
                "<DIR>".to_string()
            } else if display.show_size_in_bytes {
                rc_core::format::format_thousands(entry.size)
            } else {
                rc_core::format::format_size_bytes(entry.size)
            }
        }
        "date" => rc_core::format::format_datetime(entry.mtime),
        "attrs" => entry.attr_string(),
        "comment" => entry
            .link_target
            .as_ref()
            .map(|t| t.to_string_lossy().to_string())
            .unwrap_or_default(),
        "mode" => entry
            .attrs
            .unix_mode
            .map(rc_core::entry::unix_mode_string)
            .unwrap_or_default(),
        "link" => entry
            .link_target
            .as_ref()
            .map(|t| t.to_string_lossy().to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Draw the whole panel: tabs, path bar, column header and the file list.
pub fn show(
    ui: &mut Ui,
    model: &mut Model,
    side: Side,
    palette: &Palette,
    thumbnails: &mut ThumbnailCache,
) -> Vec<PanelAction> {
    let mut actions = Vec::new();
    let rtl = model.direction().is_rtl();
    let active = model.focus == side;

    let frame = egui::Frame::new()
        .fill(palette.background)
        .stroke(Stroke::new(
            if active { 2.0 } else { 1.0 },
            if active {
                palette.accent
            } else {
                palette.panel_border
            },
        ))
        .inner_margin(egui::Margin::same(2));
    frame.show(ui, |ui| {
        // A click anywhere in the panel body activates it, including the empty
        // space below the last row and the panel border. Registered before the
        // widgets so a click on a row still reaches the row first.
        let background = ui.interact(
            ui.max_rect(),
            ui.scope_id().with(("panel_background", side)),
            Sense::click(),
        );
        if background.clicked() {
            model.focus = side;
        }
        if background.secondary_clicked() {
            model.focus = side;
        }
        // Right-clicking the empty space below the rows (or the panel border) offers the
        // operations that are about the directory itself, not about one row.
        background.context_menu(|ui| {
            ui.set_min_width(230.0);
            for item in folder_menu_items(model) {
                if item.separator {
                    ui.separator();
                    continue;
                }
                if menu_entry(ui, item.icon, &item.label) {
                    actions.push(PanelAction::Command(item.command));
                    ui.close();
                }
            }
        });
        tabs_row(ui, model, side, palette, rtl, &mut actions);
        path_row(ui, model, side, palette, active, rtl, &mut actions);
        if model.panel(side).view_mode == ViewMode::Thumbnails {
            thumbnail_grid(ui, model, side, palette, rtl, thumbnails, &mut actions);
        } else {
            list_header(ui, model, side, palette, rtl, &mut actions);
            file_list(ui, model, side, palette, active, rtl, &mut actions);
        }
    });
    actions
}

fn font(model: &Model) -> FontId {
    FontId::proportional(model.settings.display.font_size)
}

fn tabs_row(
    ui: &mut Ui,
    model: &mut Model,
    side: Side,
    palette: &Palette,
    rtl: bool,
    actions: &mut Vec<PanelAction>,
) {
    if !model.settings.layout.show_tabs {
        return;
    }
    let labels: Vec<(usize, String, bool)> = model
        .panel(side)
        .tabs
        .tabs()
        .iter()
        .enumerate()
        .map(|(i, t)| (i, t.label(), i == model.panel(side).tabs.active_index()))
        .collect();
    let mut clicked: Option<usize> = None;
    let mut close: Option<usize> = None;
    let mut focused = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (index, label, is_active) in labels {
            let text = if is_active {
                format!("[{label}]")
            } else {
                label
            };
            let response = ui.add(egui::Button::new(text).frame(false).fill(if is_active {
                palette.header_background
            } else {
                palette.background
            }));
            if response.clicked() {
                focused = true;
                if !is_active {
                    clicked = Some(index);
                }
            }
            if response.middle_clicked() {
                close = Some(index);
            }
        }
        if rtl {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |_| {});
        }
        if ui.small_button("+").clicked() {
            focused = true;
            actions.push(PanelAction::Command("cm_NewTab".to_string()));
        }
    });
    if focused {
        model.focus = side;
    }
    if let Some(index) = clicked {
        let settings = model.settings.clone();
        let _ = model.panel_mut(side).activate_tab(index, &settings);
        // Clicking a panel activates it, whatever it was before. The old
        // condition (`if active`) made the focus impossible to move with the
        // mouse: the left panel stayed active forever.
        model.focus = side;
    }
    if let Some(index) = close
        && model.panel_mut(side).tabs.close(index)
    {
        let location = model.panel(side).tabs.active().location.clone();
        let settings = model.settings.clone();
        let _ = model.panel_mut(side).open(location, &settings);
    }
}

fn path_row(
    ui: &mut Ui,
    model: &mut Model,
    side: Side,
    palette: &Palette,
    active: bool,
    rtl: bool,
    actions: &mut Vec<PanelAction>,
) {
    let path = model.panel(side).location().display();
    let editing = model.panel(side).path_edit.is_some();
    let mut submitted = false;
    // Volume selector: `C: System` on Windows, `DATA (/mnt/data)` elsewhere.
    if !editing {
        let volumes = rc_core::drive::selector_entries();
        if !volumes.is_empty() {
            let current = model.current_volume(side);
            let current_label = current
                .as_ref()
                .map(rc_core::drive::selector_label)
                .unwrap_or_else(|| model.t("panel.volume"));
            let mut chosen: Option<std::path::PathBuf> = None;
            egui::ComboBox::from_id_salt(("panel_volume", side))
                .selected_text(current_label)
                .width(180.0)
                .show_ui(ui, |ui| {
                    for (volume, label) in &volumes {
                        let selected = current.as_ref().is_some_and(|c| c.root == volume.root);
                        if ui.selectable_label(selected, label).clicked() {
                            chosen = Some(volume.root.clone());
                        }
                    }
                });
            if let Some(root) = chosen {
                actions.push(PanelAction::Volume(root));
            }
        }
    }
    ui.horizontal(|ui| {
        // A volume selector belongs to each panel, not to the window globally.
        // Enumerate drives only while the popup is open: probing disconnected
        // mapped drives on Windows can otherwise slow every rendered frame.
        if model.settings.layout.show_drive_bar {
            let root = model
                .panel(side)
                .location()
                .as_path()
                .and_then(|p| p.ancestors().last())
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let drive_caption = if root.is_empty() {
                model.t("panel.drives")
            } else {
                format!("{} {root}", model.t("panel.drives"))
            };
            ui.menu_button(drive_caption, |ui| {
                let volumes = rc_core::drive::volumes();
                if volumes.is_empty() {
                    ui.label(model.t("panel.no_drives"));
                }
                for volume in volumes {
                    let current = model
                        .panel(side)
                        .location()
                        .as_path()
                        .is_some_and(|p| p.starts_with(&volume.root));
                    if ui
                        .selectable_label(current, volume.display_name())
                        .clicked()
                    {
                        let settings = model.settings.clone();
                        let location = rc_core::location::Location::local(&volume.root);
                        match model.panel_mut(side).open(location, &settings) {
                            Ok(()) => {
                                model.focus = side;
                                ui.close();
                            }
                            Err(e) => model.report_error(e.to_string()),
                        }
                    }
                }
            });
        }
        if editing {
            let mut text = model.panel(side).path_edit.clone().unwrap_or_default();
            let response = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text(model.t("panel.path"))
                    .desired_width(f32::INFINITY),
            );
            if let Some(panel) = Some(model.panel_mut(side)) {
                panel.path_edit = Some(text.clone());
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                submitted = true;
            }
            response.request_focus();
        } else {
            let label = if model.panel(side).git_label.is_empty() {
                path.clone()
            } else {
                // A Git panel shows which branch (or revision) it is looking at.
                format!("{path}  [{}]", model.panel(side).git_label)
            };
            let response =
                ui.add(
                    egui::Label::new(egui::RichText::new(&label).font(font(model)).color(
                        if active {
                            palette.foreground
                        } else {
                            palette.hidden
                        },
                    ))
                    .truncate()
                    .sense(Sense::click()),
                );
            if response.clicked() {
                model.panel_mut(side).path_edit = Some(path.clone());
                model.focus = side;
            }
        }
        if rtl {
            ui.allocate_space(Vec2::ZERO);
        }
        // Explicit navigation: going up must not require knowing a shortcut.
        if ui
            .button(
                egui::RichText::new(format!(
                    "{} {}",
                    crate::icons::Icon::Up.glyph(),
                    model.t("cmd.up")
                ))
                .size(model.settings.display.font_size),
            )
            .on_hover_text(model.t("cmd.go_up"))
            .clicked()
        {
            // The button sits in *this* panel's header, so it belongs to this panel: a click both
            // activates it and sends the command there. Anything else is the bug the user reported.
            model.focus = side;
            actions.push(PanelAction::Command("cm_GoToParent".to_string()));
        }
        if ui
            .small_button(model.t("cmd.go_root"))
            .on_hover_text(model.t("cmd.go_root"))
            .clicked()
        {
            let root = model
                .panel(side)
                .location()
                .as_path()
                .map(|path| path.ancestors().last().map(|r| r.to_path_buf()));
            if let Some(Some(root)) = root
                && let Ok(location) = rc_core::location::Location::parse(&root.to_string_lossy())
            {
                let settings = model.settings.clone();
                if let Err(e) = model.panel_mut(side).open(location, &settings) {
                    model.report_error(e.to_string());
                }
            }
        }
        if ui
            .small_button(model.t("cmd.refresh"))
            .on_hover_text(model.t("cmd.refresh"))
            .clicked()
        {
            model.focus = side;
            actions.push(PanelAction::Command("cm_RereadSource".to_string()));
        }
    });
    if submitted {
        let text = model.panel(side).path_edit.clone().unwrap_or_default();
        model.panel_mut(side).path_edit = None;
        if let Ok(location) = rc_core::location::Location::parse(&text) {
            let settings = model.settings.clone();
            let _ = model.panel_mut(side).open(location, &settings);
        }
    }
}

/// Width of the flexible (name) column, shared by the header and the rows so the
/// captions always sit over their content.
fn name_column_width(specs: &[rc_core::config::ColumnSpec], width: f32) -> f32 {
    const MARGIN: f32 = 4.0;
    let fixed: f32 = specs.iter().skip(1).map(|c| c.width.max(20.0)).sum();
    (width - fixed - 2.0 * MARGIN).max(40.0)
}

fn list_header(
    ui: &mut Ui,
    model: &mut Model,
    side: Side,
    palette: &Palette,
    _rtl: bool,
    actions: &mut Vec<PanelAction>,
) {
    if model.panel(side).view_mode == ViewMode::Brief {
        return;
    }
    let spec = model.panel(side).sort();
    let header_bg = palette.header_background;
    let _rect = ui
        .available_rect_before_wrap()
        .with_max_y(ui.cursor().min.y + 18.0);
    let (rect, _response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0, header_bg);
    let font_id = FontId::proportional(model.settings.display.font_size - 1.0);
    // Column headers follow `settings.display.columns`, so hiding, adding and
    // reordering columns also changes the header row.
    let specs = model.settings.display.columns.clone();
    let flex = name_column_width(&specs, rect.width());
    let columns: Vec<(
        String,
        f32,
        Option<rc_core::sorting::SortField>,
        &'static str,
    )> = specs
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            let width = if index == 0 {
                flex
            } else {
                spec.width.max(20.0)
            };
            let label = spec
                .label_key()
                .map(|key| model.t(key))
                .unwrap_or_else(|| spec.id.clone());
            let (field, command) = match spec.id.as_str() {
                "ext" => (Some(rc_core::sorting::SortField::Extension), "cm_SrcByExt"),
                "size" => (Some(rc_core::sorting::SortField::Size), "cm_SrcBySize"),
                "date" => (
                    Some(rc_core::sorting::SortField::ModifiedTime),
                    "cm_SrcByDateTime",
                ),
                _ => (None, "cm_SrcByName"),
            };
            (label, width, field, command)
        })
        .collect();
    let mut x = rect.left() + 4.0;
    for (index, (key, width, field, command)) in columns.iter().enumerate() {
        let column_width = *width;
        let mut label = key.clone();
        if let Some(field) = field
            && spec.field == *field
        {
            label.push_str(if spec.reversed { " v" } else { " ^" });
        }
        let column_rect = Rect::from_min_size(
            egui::pos2(x, rect.top()),
            Vec2::new(column_width, rect.height()),
        );
        painter.text(
            column_rect.left_top() + Vec2::new(2.0, 2.0),
            Align2::LEFT_TOP,
            &label,
            font_id.clone(),
            palette.foreground,
        );
        let hit = ui.interact(
            column_rect,
            ui.scope_id().with(("hdr", side, index)),
            Sense::click(),
        );
        // Column headers are buttons (clicking sorts), so they get a name.
        hit.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        if hit.clicked() {
            actions.push(PanelAction::Command(command.to_string()));
        }
        x += column_width;
    }
}

fn file_list(
    ui: &mut Ui,
    model: &mut Model,
    side: Side,
    palette: &Palette,
    active: bool,
    rtl: bool,
    actions: &mut Vec<PanelAction>,
) {
    let row_height = model.settings.display.row_height.max(12.0);
    let view_mode = model.panel(side).view_mode;
    let entries: Vec<Entry> = model.panel(side).entries().to_vec();
    let cursor = model.panel(side).cursor();
    let selected: Vec<bool> = entries
        .iter()
        .map(|e| model.panel(side).selection.contains(&e.name))
        .collect();
    // Git status: the row colour reports changed/ignored/staged files instead of the
    // file-type colour, which is what a developer wants to see first.
    let git_status = model.panel(side).git_status.clone();
    let error = model.panel(side).error.clone();
    let display = model.settings.display.clone();
    let font_id = font(model);
    let mut clicked: Option<(usize, ClickKind)> = None;
    let mut menu_command: Option<String> = None;

    let scroll = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt(("panel_scroll", side));
    scroll.show_rows(ui, row_height, entries.len(), |ui, range| {
        for index in range {
            let entry = &entries[index];
            let row_rect = ui
                .available_rect_before_wrap()
                .with_max_y(ui.cursor().min.y + row_height);
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::click());
            let _ = row_rect;
            // Expose every row to assistive technology (and to UI tests).
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &entry.name));
            let is_cursor = index == cursor;
            let is_selected = selected[index];
            if ui.is_rect_visible(rect) {
                let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                if let Some(background) =
                    row_background(index, is_cursor, is_selected, active, palette)
                {
                    painter.rect_filled(rect, 0, background);
                }
                let style = RowStyle::for_entry(entry);
                let git_colour = git_status
                    .get(&entry.name)
                    .copied()
                    .and_then(crate::git_bridge::status_role)
                    .map(|role| palette.role(role));
                let color: Color32 = if is_cursor && active {
                    palette.cursor_foreground
                } else if is_selected {
                    palette.selected_foreground
                } else {
                    git_colour.unwrap_or_else(|| style.color(palette))
                };
                draw_row(
                    painter, rect, entry, view_mode, &font_id, color, palette, rtl, &display,
                );
            }
            if response.clicked() {
                let with_ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                clicked = Some((
                    index,
                    if with_ctrl {
                        ClickKind::Toggle
                    } else {
                        ClickKind::Select
                    },
                ));
            }
            if response.double_clicked() {
                clicked = Some((index, ClickKind::Open));
            }
            if response.secondary_clicked() {
                clicked = Some((index, ClickKind::ContextMenu));
            }
            // Explorer-style menu on the row, with our own commands on top of the usual
            // file operations.
            response.context_menu(|ui| {
                ui.set_min_width(230.0);
                for item in row_menu_items(model) {
                    if item.separator {
                        ui.separator();
                        continue;
                    }
                    if menu_entry(ui, item.icon, &item.label) {
                        menu_command = Some(item.command);
                        ui.close();
                    }
                }
            });
        }
    });
    if let Some((index, kind)) = clicked {
        // Clicking a panel activates it, whatever it was before. The old
        // condition (`if active`) made the focus impossible to move with the
        // mouse: the left panel stayed active forever.
        model.focus = side;
        let name = entries.get(index).map(|e| e.name.clone());
        match kind {
            ClickKind::Select => model.panel_mut(side).set_cursor(index),
            ClickKind::Toggle => {
                model.panel_mut(side).set_cursor(index);
                model.panel_mut(side).toggle_current();
            }
            ClickKind::Open => {
                model.panel_mut(side).set_cursor(index);
                if model.focus != side {
                    model.focus = side;
                }
                match model.enter() {
                    crate::model::CommandOutcome::OpenFile(path) => {
                        actions.push(PanelAction::OpenFile(path))
                    }
                    crate::model::CommandOutcome::OpenExternally(path) => {
                        actions.push(PanelAction::OpenExternal(path))
                    }
                    crate::model::CommandOutcome::Ok => {}
                    _ => {}
                }
                let _ = name;
            }
            ClickKind::ContextMenu => {
                // Explorer selects the row you right-click; the menu then acts on it.
                model.panel_mut(side).set_cursor(index);
                actions.push(PanelAction::ContextMenu);
            }
        }
    }
    if let Some(command) = menu_command {
        actions.push(PanelAction::Command(command));
    }
    if let Some(error) = error {
        ui.separator();
        ui.colored_label(palette.readonly, error);
    }
}

/// The entries of the row context menu: `(label, internal command)`.
///
/// Shortcuts are shown because a user who knows the key does not need the menu, and a
/// user who does not learns it here.
/// Draw one menu entry with its icon and shortcut, and return true when it was clicked.
fn menu_entry(ui: &mut egui::Ui, icon: Option<Icon>, label: &str) -> bool {
    let (text, shortcut) = match label.split_once('\t') {
        Some((text, shortcut)) => (text, Some(shortcut)),
        None => (label, None),
    };
    let Some(icon) = icon else {
        return ui
            .add(egui::Button::new(label).min_size(egui::vec2(230.0, 0.0)))
            .clicked();
    };
    let size = ui.style().text_styles[&egui::TextStyle::Button].size;
    let colour = ui.visuals().widgets.inactive.fg_stroke.color;
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &icon.glyph().to_string(),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size + 2.0),
            color: colour,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.append(
        text,
        size * 0.7,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color: colour,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    if let Some(shortcut) = shortcut {
        job.append(
            shortcut,
            size * 1.8,
            egui::TextFormat {
                font_id: egui::FontId::proportional(size - 1.0),
                color: ui.visuals().weak_text_color(),
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
    }
    let response = ui.add(egui::Button::new(job).min_size(egui::vec2(230.0, 0.0)));
    // The accessible name stays the caption: no glyph, no shortcut.
    let caption = text.to_string();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &caption));
    response.clicked()
}

/// The entries of the folder (empty area) menu.
fn folder_menu_items(model: &Model) -> Vec<MenuItem> {
    let shortcut = |command: &str| {
        model
            .keymap
            .shortcut_for(command)
            .map(crate::keys::display_shortcut)
            .unwrap_or_default()
    };
    let labelled = |label: String, command: &str| {
        let keys = shortcut(command);
        if keys.is_empty() {
            label
        } else {
            format!("{label}\t{keys}")
        }
    };
    let mut items = Vec::new();
    let mut push = |label: String, command: &str, icon: Option<Icon>, separator: bool| {
        items.push(MenuItem {
            label,
            command: command.to_string(),
            icon,
            separator,
        })
    };
    push(
        model.t("cmd.mkdir"),
        "cm_MkDir",
        Some(Icon::FolderPlus),
        false,
    );
    push(
        labelled(model.t("cmd.refresh"), "cm_RereadSource"),
        "cm_RereadSource",
        Some(Icon::Refresh),
        false,
    );
    push(model.t("cmd.up"), "cm_GoToParent", Some(Icon::Up), false);
    push(
        model.t("cmd.select_all"),
        "cm_SelectAll",
        Some(Icon::Check),
        true,
    );
    push(model.t("cmd.secrets"), "cm_Secrets", Some(Icon::Lock), true);
    push(
        model.t("cmd.config"),
        "cm_Config",
        Some(Icon::Settings),
        false,
    );
    items
}

/// One entry of a context menu.
struct MenuItem {
    label: String,
    command: String,
    icon: Option<Icon>,
    /// Draw a divider above this entry.
    separator: bool,
}

fn row_menu_items(model: &Model) -> Vec<MenuItem> {
    let is_dir = model
        .active()
        .cursor_entry()
        .map(|entry| entry.is_dir())
        .unwrap_or(false);
    let shortcut = |command: &str| {
        model
            .keymap
            .shortcut_for(command)
            .map(crate::keys::display_shortcut)
            .unwrap_or_default()
    };
    let labelled = |label: String, command: &str| {
        let keys = shortcut(command);
        if keys.is_empty() {
            label
        } else {
            format!("{label}\t{keys}")
        }
    };
    let mut items: Vec<MenuItem> = Vec::new();
    let mut push = |label: String, command: &str, icon: Option<Icon>, separator: bool| {
        items.push(MenuItem {
            label,
            command: command.to_string(),
            icon,
            separator,
        })
    };
    if is_dir {
        push(
            model.t("cmd.open"),
            "cm_Return",
            Some(Icon::FolderOpen),
            false,
        );
    } else {
        push(
            labelled(model.t("cmd.view"), "cm_List"),
            "cm_List",
            Some(Icon::Eye),
            false,
        );
        push(
            labelled(model.t("cmd.edit"), "cm_Edit"),
            "cm_Edit",
            Some(Icon::Edit),
            false,
        );
    }
    push(
        labelled(model.t("cmd.copy"), "cm_Copy"),
        "cm_Copy",
        Some(Icon::Copy),
        true,
    );
    push(
        labelled(model.t("cmd.move"), "cm_RenMov"),
        "cm_RenMov",
        Some(Icon::Download),
        false,
    );
    push(
        labelled(model.t("cmd.rename"), "cm_Rename"),
        "cm_Rename",
        Some(Icon::Tags),
        false,
    );
    push(
        model.t("cmd.mkdir"),
        "cm_MkDir",
        Some(Icon::FolderPlus),
        false,
    );
    push(
        labelled(model.t("cmd.delete"), "cm_Delete"),
        "cm_Delete",
        Some(Icon::Delete),
        true,
    );
    push(
        model.t("cmd.attributes"),
        "cm_SetAttrib",
        Some(Icon::Settings),
        false,
    );
    // The system's own actions: what a file manager is expected to offer next to its own.
    push(
        model.t("cmd.open_with"),
        "cm_OpenWith",
        Some(Icon::Settings),
        true,
    );
    if !is_dir {
        push(
            model.t("cmd.open_default"),
            "cm_OpenExternal",
            Some(Icon::FolderOpen),
            false,
        );
    }
    push(
        model.t("cmd.send_email"),
        "cm_SendByEmail",
        Some(Icon::Download),
        false,
    );
    if is_dir {
        push(
            model.t("cmd.terminal_here"),
            "cm_TerminalHere",
            Some(Icon::Terminal),
            false,
        );
    } else {
        push(
            model.t("cmd.run_terminal"),
            "cm_RunInTerminal",
            Some(Icon::Terminal),
            false,
        );
        push(
            model.t("cmd.terminal_here"),
            "cm_TerminalHere",
            Some(Icon::Terminal),
            false,
        );
    }
    items
}

/// Thumbnail grid used by the `Ctrl+Shift+F1` view mode.
fn thumbnail_grid(
    ui: &mut Ui,
    model: &mut Model,
    side: Side,
    palette: &Palette,
    rtl: bool,
    thumbnails: &mut ThumbnailCache,
    actions: &mut Vec<PanelAction>,
) {
    let entries: Vec<Entry> = model.panel(side).entries().to_vec();
    let cursor = model.panel(side).cursor();
    let selected: Vec<bool> = entries
        .iter()
        .map(|e| model.panel(side).selection.contains(&e.name))
        .collect();
    let thumb = model.settings.display.thumbnail_size.clamp(48, 256) as f32;
    let cell_w = thumb + 24.0;
    let cell_h = thumb + 42.0;
    let columns = ((ui.available_width() / cell_w).floor() as usize).max(1);
    let rows = entries.len().div_ceil(columns);
    let caption_font = FontId::proportional(model.settings.display.font_size - 1.0);
    let icon_font = FontId::proportional(thumb * 0.45);
    let active = model.focus == side;
    let mut clicked: Option<(usize, ClickKind)> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt(("thumbs", side))
        .show_rows(ui, cell_h, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for column in 0..columns {
                        let index = row * columns + column;
                        let Some(entry) = entries.get(index) else {
                            break;
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(cell_w, cell_h), Sense::click());
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Label, true, &entry.name)
                        });
                        let is_cursor = index == cursor;
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }
                        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                        // Card background: cursor and marks win, otherwise the zebra
                        // pattern keeps a large grid readable.
                        let card =
                            row_background(index, is_cursor, selected[index], active, palette)
                                .unwrap_or(palette.background);
                        painter.rect_filled(rect, 3, card);
                        painter.rect_stroke(
                            rect,
                            3,
                            egui::Stroke::new(
                                1.0,
                                if is_cursor && active {
                                    palette.accent
                                } else {
                                    palette.panel_border
                                },
                            ),
                            egui::StrokeKind::Inside,
                        );

                        let image_rect = Rect::from_min_size(
                            rect.min + Vec2::new(12.0, 6.0),
                            Vec2::splat(thumb),
                        );
                        // One colour decision for the whole card, so the icon never
                        // disappears into the cursor or selection background.
                        let ink = if is_cursor && active {
                            palette.cursor_foreground
                        } else if selected[index] {
                            palette.selected_foreground
                        } else {
                            RowStyle::for_entry(entry).color(palette)
                        };
                        let icon = crate::icons::Icon::for_entry(entry.kind, &entry.name);
                        match thumbnails.thumbnail(ui.ctx(), &entry.path, thumb as u32) {
                            Some(texture) => {
                                let size = texture.size_vec2();
                                let scale = (thumb / size.x).min(thumb / size.y).min(1.0);
                                let draw =
                                    Rect::from_center_size(image_rect.center(), size * scale);
                                painter.image(
                                    texture.id(),
                                    draw,
                                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            }
                            None => {
                                // A faint well keeps the grid aligned and makes the
                                // type icon read as a placeholder instead of a hole.
                                painter.rect_stroke(
                                    image_rect,
                                    2,
                                    egui::Stroke::new(1.0, palette.panel_border),
                                    egui::StrokeKind::Inside,
                                );
                                painter.text(
                                    image_rect.center(),
                                    Align2::CENTER_CENTER,
                                    icon.glyph(),
                                    icon_font.clone(),
                                    ink,
                                );
                                let extension = entry.extension().to_uppercase();
                                if !entry.is_dir() && !extension.is_empty() {
                                    painter.text(
                                        egui::pos2(
                                            image_rect.center().x,
                                            image_rect.bottom() - 6.0,
                                        ),
                                        Align2::CENTER_BOTTOM,
                                        extension,
                                        FontId::proportional((thumb * 0.13).max(8.0)),
                                        palette.hidden,
                                    );
                                }
                            }
                        }

                        // Caption on one line, elided to the card width.
                        let label_rect = Rect::from_min_size(
                            egui::pos2(rect.left() + 4.0, image_rect.bottom() + 4.0),
                            Vec2::new(cell_w - 8.0, 16.0),
                        );
                        let align = if rtl {
                            Align2::RIGHT_TOP
                        } else {
                            Align2::LEFT_TOP
                        };
                        let anchor = if rtl {
                            egui::pos2(label_rect.right(), label_rect.top())
                        } else {
                            label_rect.left_top()
                        };
                        painter.text(
                            anchor,
                            align,
                            elide(&entry.name, cell_w - 8.0, caption_font.size),
                            caption_font.clone(),
                            ink,
                        );

                        if response.clicked() {
                            clicked = Some((index, ClickKind::Select));
                        }
                        if response.double_clicked() {
                            clicked = Some((index, ClickKind::Open));
                        }
                    }
                });
            }
        });

    if let Some((index, kind)) = clicked {
        // Clicking a panel activates it, whatever it was before. The old
        // condition (`if active`) made the focus impossible to move with the
        // mouse: the left panel stayed active forever.
        model.focus = side;
        model.panel_mut(side).set_cursor(index);
        if kind == ClickKind::Open
            && let crate::model::CommandOutcome::OpenFile(path) = model.enter()
        {
            actions.push(PanelAction::OpenFile(path));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClickKind {
    Select,
    Toggle,
    Open,
    ContextMenu,
}

#[allow(clippy::too_many_arguments)]
fn draw_row(
    painter: egui::Painter,
    rect: Rect,
    entry: &Entry,
    view_mode: ViewMode,
    font: &FontId,
    color: Color32,
    palette: &Palette,
    rtl: bool,
    display: &rc_core::config::Display,
) {
    let y = rect.center().y;
    let ctrl_y = rect.top() + rect.height() / 2.0 - 2.0;
    let _ = ctrl_y;

    if view_mode == ViewMode::Brief {
        let column_width = (rect.width() / 3.0).max(80.0);
        let columns = ((rect.width() / column_width).floor() as usize).max(1);
        let index = 0;
        let _ = index;
        let x = if rtl {
            rect.right() - 4.0
        } else {
            rect.left() + 4.0
        };
        let align = if rtl {
            Align2::RIGHT_CENTER
        } else {
            Align2::LEFT_CENTER
        };
        painter.text(
            egui::pos2(x, y),
            align,
            entry.name.as_str(),
            font.clone(),
            color,
        );
        let _ = columns;
        return;
    }

    let margin = 4.0;
    // The columns come from the configuration, so the full view is user-defined.
    let specs = &display.columns;
    let flex = name_column_width(specs, rect.width());
    let mut cells: Vec<(String, f32, Align2)> = Vec::with_capacity(specs.len());
    for (index, spec) in specs.iter().enumerate() {
        let width = if index == 0 {
            flex
        } else {
            spec.width.max(20.0)
        };
        let align = if spec.id == "size" {
            Align2::RIGHT_CENTER
        } else {
            Align2::LEFT_CENTER
        };
        cells.push((column_text(&spec.id, entry, display), width, align));
    }
    let count = cells.len();
    let order: Vec<usize> = if rtl {
        (0..count).rev().collect()
    } else {
        (0..count).collect()
    };
    let mut x = if rtl {
        rect.right() - margin
    } else {
        rect.left() + margin
    };
    for index in order {
        let (text, width, align) = &cells[index];
        let width = *width;
        let (anchor_x, text_align) = if rtl {
            let a = egui::pos2(x - if index == 0 { 0.0 } else { width }, y);
            x -= width;
            (a, flip_align(*align))
        } else {
            let a = egui::pos2(x, y);
            x += width;
            (a, *align)
        };
        let muted = specs.get(index).map(|s| s.id == "attrs").unwrap_or(false);
        let shown = elide(text, width, font.size);
        painter.text(
            anchor_x,
            text_align,
            shown,
            font.clone(),
            if muted { palette.hidden } else { color },
        );
    }
}

fn flip_align(align: Align2) -> Align2 {
    if align.x() == egui::Align::LEFT {
        Align2::RIGHT_CENTER
    } else if align.x() == egui::Align::RIGHT {
        Align2::LEFT_CENTER
    } else {
        align
    }
}

fn elide(text: &str, width: f32, font_size: f32) -> String {
    let approx = (width / (font_size * 0.55)).floor().max(3.0) as usize;
    if text.chars().count() <= approx {
        return text.to_string();
    }
    let keep = approx.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_selector_is_enabled_in_default_layout() {
        assert!(
            rc_core::config::Settings::default().layout.show_drive_bar,
            "each panel should offer volume selection by default"
        );
    }

    #[test]
    fn elide_keeps_short_names() {
        assert_eq!(elide("abc", 100.0, 14.0), "abc");
        assert_eq!(elide("", 100.0, 14.0), "");
    }

    #[test]
    fn elide_truncates_long_names_with_an_ellipsis() {
        let out = elide("a-very-long-file-name.txt", 40.0, 14.0);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() < "a-very-long-file-name.txt".chars().count());
    }

    #[test]
    fn elide_never_produces_an_empty_result_for_long_input() {
        let out = elide(&"x".repeat(200), 10.0, 14.0);
        assert!(!out.is_empty());
    }

    #[test]
    fn the_zebra_pattern_skips_the_cursor_and_marks() {
        let palette = Palette::light();
        assert_eq!(
            row_background(0, false, false, true, &palette),
            None,
            "even rows keep the panel background"
        );
        assert_eq!(
            row_background(1, false, false, true, &palette),
            Some(palette.row_alternate)
        );
        assert_eq!(row_background(2, false, false, true, &palette), None);
        assert_eq!(
            row_background(3, false, false, true, &palette),
            Some(palette.row_alternate)
        );
        // The cursor row and marked rows win over the pattern.
        assert_eq!(
            row_background(1, true, false, true, &palette),
            Some(palette.cursor_background)
        );
        assert_eq!(
            row_background(1, false, true, true, &palette),
            Some(palette.selected_background)
        );
        assert_eq!(
            row_background(2, true, false, false, &palette),
            Some(palette.header_background),
            "inactive cursor"
        );
        let dark = Palette::dark();
        assert_eq!(
            row_background(1, false, false, true, &dark),
            Some(dark.row_alternate)
        );
    }

    #[test]
    fn align_flipping_mirrors_horizontal_alignment() {
        assert_eq!(flip_align(Align2::LEFT_CENTER), Align2::RIGHT_CENTER);
        assert_eq!(flip_align(Align2::RIGHT_CENTER), Align2::LEFT_CENTER);
        assert_eq!(flip_align(Align2::CENTER_CENTER), Align2::CENTER_CENTER);
    }
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    #[test]
    fn the_folder_menu_maps_entries_to_real_commands() {
        let model = Model::for_test(
            rc_core::config::Settings::default(),
            rc_core::location::Location::local("/tmp"),
            rc_core::location::Location::local("/tmp"),
        );
        let items = folder_menu_items(&model);
        let commands: Vec<&str> = items.iter().map(|item| item.command.as_str()).collect();
        for expected in ["cm_MkDir", "cm_RereadSource", "cm_Config"] {
            assert!(commands.contains(&expected), "{expected} in {commands:?}");
        }
        assert!(items.iter().filter(|item| item.separator).count() >= 2);
        for item in &items {
            assert!(!item.label.trim().is_empty());
            assert!(item.icon.is_some());
        }
    }

    #[test]
    fn the_header_and_the_rows_share_one_column_layout() {
        let specs = rc_core::config::Settings::default().display.columns;
        // The flexible name column is the same for both, so captions sit over their data.
        let width = 600.0;
        let flex = name_column_width(&specs, width);
        let fixed: f32 = specs.iter().skip(1).map(|c| c.width.max(20.0)).sum();
        assert!((flex + fixed + 8.0 - width).abs() < 0.01, "flex {flex}");
        // A very narrow panel still leaves a usable name column.
        assert!(name_column_width(&specs, 120.0) >= 40.0);
    }

    #[test]
    fn the_row_menu_maps_entries_to_real_commands() {
        let settings = rc_core::config::Settings::default();
        let model = Model::for_test(
            settings,
            rc_core::location::Location::local("/tmp"),
            rc_core::location::Location::local("/tmp"),
        );
        let items = row_menu_items(&model);
        let commands: Vec<&str> = items.iter().map(|item| item.command.as_str()).collect();
        for expected in [
            "cm_Copy",
            "cm_RenMov",
            "cm_MkDir",
            "cm_Rename",
            "cm_Delete",
            "cm_SetAttrib",
        ] {
            assert!(commands.contains(&expected), "{expected} in {commands:?}");
        }
        // Every entry has a label and a command, and no label is empty.
        for item in &items {
            assert!(!item.label.trim().is_empty());
            assert!(!item.command.trim().is_empty());
            assert!(item.icon.is_some(), "every entry has an icon");
        }
        // Dividers group "look", "move" and "destroy".
        assert!(items.iter().filter(|item| item.separator).count() >= 2);
        // With no entry under the cursor the menu still works (it acts on the panel).
        assert!(!items.is_empty());
    }
}

//! Modal dialog rendering.

use crate::model::dialogs::*;
use crate::model::{CommandOutcome, Model};
use egui::{Context, RichText};

/// What the dialog layer wants the application to do next.
#[derive(Clone, Debug, PartialEq)]
pub enum DialogAction {
    None,
    Close,
    Run,
    Command(String),
}

/// Draw the topmost dialog, if any, and apply the user's decision.
pub fn show(ctx: &Context, model: &mut Model) -> DialogAction {
    let Some(mut dialog) = model.dialogs.last().cloned() else {
        return DialogAction::None;
    };
    let mut action = DialogAction::None;
    let mut open = true;
    let title = title_for(model, &dialog);
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            action = match &mut dialog {
                Dialog::Transfer(state) => transfer(ui, model, state),
                Dialog::Overwrite(state) => overwrite(ui, model, state),
                Dialog::MakeDir(state) => mkdir(ui, model, state),
                Dialog::Rename(state) => rename(ui, model, state),
                Dialog::Delete(state) => delete(ui, model, state),
                Dialog::Search(state) => search(ui, model, state),
                Dialog::Pack(state) => pack(ui, model, state),
                Dialog::Unpack(state) => unpack(ui, model, state),
                Dialog::MultiRename(state) => multi_rename(ui, model, state),
                Dialog::Attributes(state) => attributes(ui, model, state),
                Dialog::Comment(state) => comment(ui, model, state),
                Dialog::Hotlist(state) => hotlist(ui, model, state),
                Dialog::Options(state) => options(ui, model, state),
                Dialog::FtpConnect(state) => ftp(ui, model, state),
                Dialog::Synchronize(state) => sync(ui, model, state),
                Dialog::History(state) => history(ui, model, state),
                Dialog::MaskPrompt(state) => mask_prompt(ui, model, state),
                Dialog::Compare(state) => compare(ui, model, state),
                Dialog::Tree(state) => tree(ui, model, state),
                Dialog::ButtonBar(state) => button_bar(ui, model, state),
                Dialog::ViewModes(state) => view_modes(ui, model, state),
                Dialog::Split(state) => split(ui, model, state),
                Dialog::Combine(state) => combine(ui, model, state),
                Dialog::Checksum(state) => checksum(ui, model, state),
                Dialog::VerifyChecksum(state) => verify_checksum(ui, model, state),
                Dialog::CompareFiles(state) => compare_files(ui, model, state),
                Dialog::Duplicates(state) => duplicates(ui, model, state),
                Dialog::About => about(ui, model),
                Dialog::Confirm {
                    message,
                    action: confirm_action,
                    ..
                } => confirm(ui, model, message, *confirm_action),
                Dialog::Progress(state) => progress(ui, model, state),
            };
        });
    if let Some(target) = model.dialogs.pop() {
        let updated = replace_dialog(dialog, target);
        model.dialogs.push(updated);
    }
    match action {
        DialogAction::Close => {
            model.dialogs.pop();
            DialogAction::Close
        }
        other => other,
    }
    .apply(model, open)
}

impl DialogAction {
    fn apply(self, model: &mut Model, _open: bool) -> DialogAction {
        let _ = model;
        self
    }
}

fn replace_dialog(new: Dialog, old: Dialog) -> Dialog {
    // Keep the edited state, but preserve the variant identity of the old one.
    match (new, old) {
        (Dialog::Transfer(mut a), Dialog::Transfer(_)) => {
            a.sources.sort();
            Dialog::Transfer(a)
        }
        (a, _) => a,
    }
}

fn title_for(model: &Model, dialog: &Dialog) -> String {
    let key = match dialog {
        Dialog::Transfer(state) => state.title_key(),
        Dialog::Overwrite(_) => "dialog.overwrite.title",
        Dialog::MakeDir(_) => "dialog.mkdir.title",
        Dialog::Rename(_) => "dialog.rename.title",
        Dialog::Delete(_) => "dialog.delete.title",
        Dialog::Search(_) => "dialog.search.title",
        Dialog::Pack(_) => "cmd.pack",
        Dialog::Unpack(_) => "cmd.unpack",
        Dialog::MultiRename(_) => "dialog.multi_rename.title",
        Dialog::Attributes(_) => "cmd.attributes",
        Dialog::Comment(_) => "cmd.edit_comment",
        Dialog::Hotlist(_) => "cmd.hotlist",
        Dialog::Options(_) => "dialog.config.title",
        Dialog::History(_) => "cmd.history",
        Dialog::MaskPrompt(_) => "cmd.select_user_spec",
        Dialog::Compare(_) => "cmd.compare_dirs",
        Dialog::Tree(_) => "cmd.cd_tree",
        Dialog::ButtonBar(_) => "cmd.button_bar",
        Dialog::ViewModes(_) => "cmd.custom_column_view",
        Dialog::Split(_) => "cmd.split_file",
        Dialog::Combine(_) => "cmd.combine_files",
        Dialog::Checksum(_) => "cmd.checksums",
        Dialog::VerifyChecksum(_) => "cmd.verify_checksums",
        Dialog::CompareFiles(_) => "cmd.compare_by_content",
        Dialog::Duplicates(_) => "cmd.duplicates",
        Dialog::FtpConnect(_) => "ftp.connect",
        Dialog::Synchronize(_) => "cmd.sync_dirs",
        Dialog::About => "cmd.about",
        Dialog::Confirm { title_key, .. } => title_key.as_str(),
        Dialog::Progress(state) => state.title_key.as_str(),
    };
    model.t(key)
}

fn buttons(ui: &mut egui::Ui, model: &Model) -> (bool, bool) {
    let mut ok = false;
    let mut cancel = false;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.ok")).clicked() {
            ok = true;
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            cancel = true;
        }
    });
    (ok, cancel)
}

fn transfer(ui: &mut egui::Ui, model: &mut Model, state: &mut TransferDialogState) -> DialogAction {
    ui.label(model.t("dialog.copy.to"));
    ui.add(egui::TextEdit::singleline(&mut state.destination).desired_width(f32::INFINITY));
    ui.label(state.summary());
    ui.checkbox(&mut state.background, model.t("dialog.copy.background"));
    ui.checkbox(&mut state.verify, model.t("dialog.copy.verify"));
    ui.checkbox(&mut state.preserve_times, model.t("dialog.copy.options"));
    ui.horizontal(|ui| {
        ui.label(model.t("dialog.copy.overwrite_all"));
        ui.radio_value(
            &mut state.overwrite,
            rc_core::ops::OverwriteMode::Overwrite,
            "overwrite",
        );
        ui.radio_value(
            &mut state.overwrite,
            rc_core::ops::OverwriteMode::Skip,
            "skip",
        );
        ui.radio_value(
            &mut state.overwrite,
            rc_core::ops::OverwriteMode::Rename,
            "rename",
        );
    });
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        if let Err(message) = state.validate() {
            model.report_error(message);
            return DialogAction::None;
        }
        let state = state.clone();
        if state.background {
            // Run in the background queue; the status bar reports progress and
            // the panels stay usable while the data is transferred.
            model.start_transfer(&state);
        } else {
            let _ = model.run_transfer(&state);
        }
        return DialogAction::Close;
    }
    DialogAction::None
}

fn overwrite(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut OverwriteDialogState,
) -> DialogAction {
    ui.label(format!("{} → {}", state.source, state.destination));
    if state.size_differs() {
        ui.label(model.t("dialog.overwrite.size_differs"));
    }
    if state.source_is_newer() {
        ui.label(model.t("dialog.overwrite.newer"));
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button("Overwrite").clicked() {
            action = DialogAction::Run;
        }
        if ui.button("Skip").clicked() {
            action = DialogAction::Close;
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn mkdir(ui: &mut egui::Ui, model: &mut Model, state: &mut MakeDirState) -> DialogAction {
    ui.label(model.t("dialog.mkdir.title"));
    let response = ui.add(egui::TextEdit::singleline(&mut state.name).desired_width(f32::INFINITY));
    response.request_focus();
    ui.checkbox(&mut state.in_target_panel, model.t("panel.target"));
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        let state = state.clone();
        if let CommandOutcome::Failed(message) = model.run_mkdir(&state) {
            model.report_error(message)
        }
        return DialogAction::Close;
    }
    DialogAction::None
}

fn rename(ui: &mut egui::Ui, model: &mut Model, state: &mut RenameState) -> DialogAction {
    ui.label(&state.original);
    let response = ui.add(egui::TextEdit::singleline(&mut state.name).desired_width(f32::INFINITY));
    response.request_focus();
    ui.checkbox(&mut state.keep_extension, model.t("col.ext"));
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        let state = state.clone();
        if let CommandOutcome::Failed(message) = model.run_rename(&state) {
            model.report_error(message);
        }
        return DialogAction::Close;
    }
    DialogAction::None
}

fn delete(ui: &mut egui::Ui, model: &mut Model, state: &mut DeleteState) -> DialogAction {
    ui.label(model.tf(
        "dialog.delete.confirm",
        &[("count", &state.count().to_string())],
    ));
    ui.checkbox(&mut state.permanent, model.t("dialog.delete.permanent"));
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        let state = state.clone();
        if model.settings.copy.background {
            model.start_delete(&state);
        } else if let CommandOutcome::Failed(message) = model.run_delete(&state) {
            model.report_error(message);
        }
        return DialogAction::Close;
    }
    DialogAction::None
}

fn search(ui: &mut egui::Ui, model: &mut Model, state: &mut SearchState) -> DialogAction {
    egui::Grid::new("search_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(model.t("panel.path"));
            ui.add(egui::TextEdit::singleline(&mut state.root).desired_width(280.0));
            ui.end_row();
            ui.label(model.t("dialog.search.name"));
            ui.add(egui::TextEdit::singleline(&mut state.name_mask).desired_width(280.0));
            ui.end_row();
            ui.label(model.t("dialog.search.text"));
            ui.add(egui::TextEdit::singleline(&mut state.content).desired_width(280.0));
            ui.end_row();
        });
    ui.checkbox(
        &mut state.case_sensitive,
        model.t("dialog.search.case_sensitive"),
    );
    ui.checkbox(&mut state.regex, model.t("dialog.search.regex"));
    ui.checkbox(&mut state.subdirs, model.t("dialog.search.subdirs"));
    ui.checkbox(
        &mut state.search_archives,
        model.t("dialog.search.include_archives"),
    );
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        let running = state.running;
        let start_label = if running {
            model.t("dialog.search.stop")
        } else {
            model.t("dialog.search.start")
        };
        if ui.button(start_label).clicked() {
            if running {
                action = DialogAction::Command("rc_StopSearch".to_string());
            } else if let Err(message) = state.validate() {
                model.report_error(message);
            } else {
                action = DialogAction::Command("rc_RunSearch".to_string());
            }
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    if !state.results.is_empty() {
        ui.separator();
        ui.label(model.tf(
            "dialog.search.results_count",
            &[("count", &state.results.len().to_string())],
        ));
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for hit in state.results.iter().take(200) {
                    let label = match &hit.line {
                        Some(line) => format!("{} — {}", hit.path, line),
                        None => hit.path.clone(),
                    };
                    if ui.selectable_label(false, label).clicked()
                        && let Some(parent) = std::path::Path::new(&hit.path).parent()
                    {
                        let location = rc_core::location::Location::local(parent.to_path_buf());
                        let _ = model.navigate_other(location);
                    }
                }
            });
    }
    action
}

fn pack(ui: &mut egui::Ui, model: &mut Model, state: &mut PackState) -> DialogAction {
    ui.label(model.t("dialog.copy.to"));
    ui.add(egui::TextEdit::singleline(&mut state.archive_name).desired_width(f32::INFINITY));
    ui.horizontal(|ui| {
        ui.label("format");
        ui.radio_value(&mut state.format, "zip".to_string(), "zip");
        ui.radio_value(&mut state.format, "tar.gz".to_string(), "tar.gz");
    });
    ui.add(egui::Slider::new(&mut state.level, 0..=9).text("level"));
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        if let Err(message) = state.validate() {
            model.report_error(message);
            return DialogAction::None;
        }
        let destination = model.active().location().display();
        let name = state.archive_name.clone();
        let path = std::path::Path::new(&destination).join(name);
        let sources: Vec<std::path::PathBuf> =
            state.sources.iter().map(std::path::PathBuf::from).collect();
        let format = state.format.clone();
        let level = state.level;
        let outcome = crate::archive_bridge::create_archive(&path, &sources, &format, level);
        match outcome {
            Ok(added) => model.report(model.tf("status.done", &[("count", &added.to_string())])),
            Err(message) => model.report_error(message),
        }
        model.dialogs.clear();
        model.refresh_all();
        return DialogAction::Close;
    }
    DialogAction::None
}

fn unpack(ui: &mut egui::Ui, model: &mut Model, state: &mut UnpackState) -> DialogAction {
    ui.label(&state.archive);
    ui.label(model.t("dialog.copy.to"));
    ui.add(egui::TextEdit::singleline(&mut state.destination).desired_width(f32::INFINITY));
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        if let Err(message) = state.validate() {
            model.report_error(message);
            return DialogAction::None;
        }
        let archive = std::path::PathBuf::from(&state.archive);
        let destination = std::path::PathBuf::from(&state.destination);
        let cancel_token = rc_core::ops::CancelToken::new();
        let outcome = crate::archive_bridge::extract_archive(&archive, &destination, &cancel_token);
        match outcome {
            Ok(files) => model.report(model.tf("status.done", &[("count", &files.to_string())])),
            Err(message) => model.report_error(message),
        }
        model.dialogs.clear();
        model.refresh_all();
        return DialogAction::Close;
    }
    DialogAction::None
}

fn multi_rename(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut MultiRenameState,
) -> DialogAction {
    egui::Grid::new("mr_grid").num_columns(2).show(ui, |ui| {
        ui.label(model.t("dialog.rename_pattern"));
        ui.add(egui::TextEdit::singleline(&mut state.pattern).desired_width(260.0));
        ui.end_row();
        ui.label("extension");
        ui.add(egui::TextEdit::singleline(&mut state.extension_pattern).desired_width(260.0));
        ui.end_row();
        ui.label("search");
        ui.add(egui::TextEdit::singleline(&mut state.search_for).desired_width(260.0));
        ui.end_row();
        ui.label("replace");
        ui.add(egui::TextEdit::singleline(&mut state.replace_with).desired_width(260.0));
        ui.end_row();
        ui.label("counter");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut state.counter_start));
            ui.add(egui::DragValue::new(&mut state.counter_step));
            ui.add(egui::DragValue::new(&mut state.counter_digits).range(0..=8));
        });
        ui.end_row();
    });
    state.refresh_preview();
    ui.separator();
    ui.label(model.t("dialog.rename_preview"));
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            for (old, new) in &state.preview {
                ui.label(format!("{old} → {new}"));
            }
        });
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        let state = state.clone();
        if let CommandOutcome::Failed(message) = model.run_multi_rename(&state) {
            model.report_error(message);
        }
        return DialogAction::Close;
    }
    DialogAction::None
}

fn attributes(ui: &mut egui::Ui, model: &mut Model, state: &mut AttributesState) -> DialogAction {
    ui.label(model.tf(
        "panel.selected",
        &[("count", &state.paths.len().to_string())],
    ));
    let mut readonly = state.readonly.unwrap_or(false);
    let mut hidden = state.hidden.unwrap_or(false);
    let mut system = state.system.unwrap_or(false);
    let mut archive = state.archive.unwrap_or(false);
    let mut changed = false;
    if ui
        .checkbox(&mut readonly, model.t("cmd.attributes"))
        .changed()
    {
        state.readonly = Some(readonly);
        changed = true;
    }
    if ui.checkbox(&mut hidden, "hidden").changed() {
        state.hidden = Some(hidden);
        changed |= true;
    }
    if ui.checkbox(&mut system, "system").changed() {
        state.system = Some(system);
        changed |= true;
    }
    if ui.checkbox(&mut archive, "archive").changed() {
        state.archive = Some(archive);
        changed |= true;
    }
    #[cfg(unix)]
    {
        let mut mode = state.unix_mode.unwrap_or(0o644);
        if ui
            .add(egui::Slider::new(&mut mode, 0..=0o777).text("unix mode"))
            .changed()
        {
            state.unix_mode = Some(mode);
            changed |= true;
        }
        let _ = &mut mode;
    }
    let _ = changed;
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        let state = state.clone();
        if let CommandOutcome::Failed(message) = model.run_attributes(&state) {
            model.report_error(message);
        }
        return DialogAction::Close;
    }
    DialogAction::None
}

fn comment(ui: &mut egui::Ui, model: &mut Model, state: &mut CommentState) -> DialogAction {
    ui.label(&state.name);
    ui.add(
        egui::TextEdit::multiline(&mut state.text)
            .desired_rows(4)
            .desired_width(f32::INFINITY),
    );
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        let state = state.clone();
        if let CommandOutcome::Failed(message) = model.run_comment(&state) {
            model.report_error(message);
        }
        return DialogAction::Close;
    }
    DialogAction::None
}

fn hotlist(ui: &mut egui::Ui, model: &mut Model, state: &mut HotlistState) -> DialogAction {
    let path = rc_core::config::config_dir().join("hotlist.toml");
    let mut list = rc_core::hotlist::Hotlist::load(&path);
    let mut action = DialogAction::None;
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            for (index, item) in list.items.iter().enumerate() {
                if item.separator {
                    ui.separator();
                    continue;
                }
                let selected = state.selected == Some(index);
                if ui
                    .selectable_label(selected, format!("{} — {}", item.title, item.path))
                    .clicked()
                {
                    state.selected = Some(index);
                    let location = rc_core::location::Location::parse(&item.path);
                    if let Ok(location) = location {
                        let _ = model.navigate(location);
                        action = DialogAction::Close;
                    }
                }
            }
        });
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("+ current dir").clicked() {
            let current = model.active().location().display();
            list.add(current.clone(), current);
            let _ = list.save(&path);
        }
        if ui.button("-").clicked()
            && let Some(index) = state.selected
        {
            list.remove(index);
            let _ = list.save(&path);
        }
    });
    if ui.button(model.t("dialog.config.cancel")).clicked() {
        return DialogAction::Close;
    }
    action
}

fn options(ui: &mut egui::Ui, model: &mut Model, state: &mut OptionsState) -> DialogAction {
    ui.horizontal_wrapped(|ui| {
        for tab in OptionsTab::all() {
            if ui
                .selectable_label(state.tab == tab, model.t(tab.key()))
                .clicked()
            {
                state.tab = tab;
            }
        }
    });
    ui.separator();
    match state.tab {
        OptionsTab::Display => {
            ui.checkbox(&mut state.draft.display.show_hidden, "show hidden");
            ui.checkbox(&mut state.draft.display.show_parent_entry, "show ..");
            ui.checkbox(&mut state.draft.display.dirs_first, "dirs first");
            ui.checkbox(&mut state.draft.display.natural_sort, "natural sort");
            ui.add(
                egui::Slider::new(&mut state.draft.display.font_size, 8.0..=28.0).text("font size"),
            );
            ui.add(
                egui::Slider::new(&mut state.draft.display.row_height, 14.0..=40.0)
                    .text("row height"),
            );
        }
        OptionsTab::Language => {
            let mut codes: Vec<String> = vec!["en".to_string()];
            if let Ok(locales) = rc_i18n::Translator::load_dir(&crate::paths::locales_dir()) {
                for locale in &locales {
                    let code = locale.code().to_string();
                    if !codes.contains(&code) {
                        codes.push(code);
                    }
                }
            }
            codes.sort();
            egui::ComboBox::from_label(model.t("dialog.config.language"))
                .selected_text(state.language_choice.clone())
                .show_ui(ui, |ui| {
                    for code in &codes {
                        ui.selectable_value(&mut state.language_choice, code.clone(), code);
                    }
                });
        }
        OptionsTab::Colors => {
            egui::ComboBox::from_label("theme")
                .selected_text(state.draft.colors.theme.clone())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut state.draft.colors.theme,
                        "light".to_string(),
                        "light",
                    );
                    ui.selectable_value(&mut state.draft.colors.theme, "dark".to_string(), "dark");
                });
        }
        OptionsTab::Editor => {
            ui.add(
                egui::Slider::new(&mut state.draft.editor.font_size, 8.0..=28.0).text("font size"),
            );
            ui.checkbox(&mut state.draft.editor.word_wrap, "word wrap");
            ui.add(egui::Slider::new(&mut state.draft.editor.tab_width, 1..=8).text("tab width"));
        }
        OptionsTab::Packer => {
            egui::ComboBox::from_label("format")
                .selected_text(state.draft.packer.default_type.clone())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut state.draft.packer.default_type,
                        "zip".to_string(),
                        "zip",
                    );
                    ui.selectable_value(
                        &mut state.draft.packer.default_type,
                        "tar.gz".to_string(),
                        "tar.gz",
                    );
                });
            ui.add(egui::Slider::new(&mut state.draft.packer.level, 0..=9).text("level"));
        }
        OptionsTab::Ftp => {
            ui.checkbox(&mut state.draft.ftp.passive_mode, "passive mode");
            ui.checkbox(&mut state.draft.ftp.use_mlsd, "use MLSD");
            ui.add(
                egui::DragValue::new(&mut state.draft.ftp.timeout_seconds)
                    .range(5..=600)
                    .prefix("timeout "),
            );
        }
        OptionsTab::Agent => {
            ui.checkbox(
                &mut state.draft.agent.server_enabled,
                model.t("agent.server_enabled"),
            );
            ui.checkbox(
                &mut state.draft.agent.require_confirmation,
                model.t("agent.ask"),
            );
            ui.horizontal(|ui| {
                ui.label("transport");
                ui.radio_value(
                    &mut state.draft.agent.server_transport,
                    "stdio".to_string(),
                    "stdio",
                );
                ui.radio_value(
                    &mut state.draft.agent.server_transport,
                    "http".to_string(),
                    "http",
                );
            });
            ui.add(
                egui::DragValue::new(&mut state.draft.agent.server_port)
                    .range(0..=65535)
                    .prefix("port "),
            );
        }
        OptionsTab::Shortcuts => {
            ui.label(model.t("dialog.config.shortcuts"));
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .id_salt("shortcut_scroll")
                .show(ui, |ui| {
                    for (shortcut, command) in state.shortcuts_edit.iter_mut() {
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(shortcut).desired_width(140.0));
                            ui.add(egui::TextEdit::singleline(command).desired_width(220.0));
                        });
                    }
                });
        }
    }
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        state.commit_shortcuts();
        let language = state.language_choice.clone();
        model.settings = state.draft.clone();
        model.set_language(&language);
        model.keymap = {
            let mut km = rc_core::keymap::KeyMap::defaults();
            km.apply_overrides(&model.settings.shortcuts);
            km
        };
        model.dialogs.clear();
        return DialogAction::Close;
    }
    DialogAction::None
}

fn ftp(ui: &mut egui::Ui, model: &mut Model, state: &mut FtpConnectState) -> DialogAction {
    egui::Grid::new("ftp_grid").num_columns(2).show(ui, |ui| {
        ui.label(model.t("ftp.host"));
        ui.add(egui::TextEdit::singleline(&mut state.connection.host).desired_width(220.0));
        ui.end_row();
        ui.label(model.t("ftp.port"));
        ui.add(egui::DragValue::new(&mut state.connection.port).range(0..=65535));
        ui.end_row();
        ui.label(model.t("ftp.user"));
        ui.add(egui::TextEdit::singleline(&mut state.connection.user).desired_width(220.0));
        ui.end_row();
        ui.label(model.t("ftp.password"));
        ui.add(
            egui::TextEdit::singleline(&mut state.connection.password)
                .password(true)
                .desired_width(220.0),
        );
        ui.end_row();
        ui.label(model.t("ftp.remote_dir"));
        ui.add(egui::TextEdit::singleline(&mut state.connection.remote_dir).desired_width(220.0));
        ui.end_row();
    });
    ui.checkbox(&mut state.connection.passive, model.t("ftp.passive"));
    ui.label(state.summary());
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        if let Err(message) = state.validate() {
            model.report_error(message);
            return DialogAction::None;
        }
        model.report(format!(
            "connecting to {} is not wired yet",
            state.summary()
        ));
        return DialogAction::Close;
    }
    DialogAction::None
}

fn history(ui: &mut egui::Ui, model: &mut Model, state: &mut HistoryState) -> DialogAction {
    if state.is_empty() {
        ui.label(model.t("panel.empty"));
        if ui.button(model.t("dialog.config.ok")).clicked() {
            return DialogAction::Close;
        }
        return DialogAction::None;
    }
    let mut action = DialogAction::None;
    egui::ScrollArea::vertical()
        .max_height(260.0)
        .id_salt("history_scroll")
        .show(ui, |ui| {
            for (index, row) in state.entries.iter().enumerate() {
                let selected = state.selected == Some(index);
                if ui.selectable_label(selected, &row.path).clicked() {
                    state.selected = Some(index);
                    action = DialogAction::Run;
                }
            }
        });
    if action == DialogAction::Run {
        if let Some(index) = state.selected
            && let Some(row) = state.entries.get(index)
            && let Ok(location) = rc_core::location::Location::parse(&row.path)
            && let Err(e) = model.navigate(location)
        {
            model.report_error(e.to_string());
        }
        return DialogAction::Close;
    }
    if ui.button(model.t("dialog.config.cancel")).clicked() {
        return DialogAction::Close;
    }
    DialogAction::None
}

fn mask_prompt(ui: &mut egui::Ui, model: &mut Model, state: &mut MaskPromptState) -> DialogAction {
    ui.label(model.t("dialog.search.name"));
    let response = ui.add(egui::TextEdit::singleline(&mut state.mask).desired_width(f32::INFINITY));
    response.request_focus();
    ui.checkbox(&mut state.hide_non_matching, model.t("panel.all_files"));
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        let state = state.clone();
        if let crate::model::CommandOutcome::Failed(message) = model.apply_mask(&state) {
            model.report_error(message);
        }
        return DialogAction::Close;
    }
    DialogAction::None
}

fn compare(ui: &mut egui::Ui, model: &mut Model, state: &mut CompareState) -> DialogAction {
    ui.label(state.summary());
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .id_salt("compare_scroll")
        .show(ui, |ui| {
            egui::Grid::new("compare_grid")
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    ui.label(model.t("col.name"));
                    ui.label(model.t("panel.source"));
                    ui.label(model.t("panel.target"));
                    ui.label("");
                    ui.end_row();
                    for row in state.rows.iter().take(500) {
                        ui.label(&row.name);
                        ui.label(
                            row.left_size
                                .map(rc_core::format::format_size_bytes)
                                .unwrap_or_else(|| "-".to_string()),
                        );
                        ui.label(
                            row.right_size
                                .map(rc_core::format::format_size_bytes)
                                .unwrap_or_else(|| "-".to_string()),
                        );
                        ui.label(&row.status);
                        ui.end_row();
                    }
                });
        });
    if ui.button(model.t("dialog.config.ok")).clicked() {
        return DialogAction::Close;
    }
    DialogAction::None
}

fn tree(ui: &mut egui::Ui, model: &mut Model, state: &mut TreeState) -> DialogAction {
    ui.label(&state.root);
    let mut action = DialogAction::None;
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .id_salt("tree_scroll")
        .show(ui, |ui| {
            for (index, path) in state.entries.iter().enumerate() {
                let selected = state.selected == Some(index);
                if ui.selectable_label(selected, path).clicked() {
                    state.selected = Some(index);
                    action = DialogAction::Run;
                }
            }
        });
    if action == DialogAction::Run {
        if let Some(index) = state.selected
            && let Some(path) = state.entries.get(index)
            && let Ok(location) = rc_core::location::Location::parse(path)
        {
            let other = model.focus.other();
            if let Err(e) = model.open_in(other, location) {
                model.report_error(e.to_string());
            }
        }
        return DialogAction::Close;
    }
    ui.horizontal(|ui| {
        if ui.button(model.t("panel.source")).clicked() {
            action = DialogAction::Command("rc_TreeToSource".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn button_bar(ui: &mut egui::Ui, model: &mut Model, state: &mut ButtonBarState) -> DialogAction {
    let buttons = model.settings.button_bar.buttons.clone();
    ui.label(model.t("cmd.button_bar"));
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .id_salt("buttonbar_scroll")
        .show(ui, |ui| {
            for (index, button) in buttons.iter().enumerate() {
                let selected = state.selected == Some(index);
                if ui
                    .selectable_label(selected, format!("{} — {}", button.label(), button.command))
                    .clicked()
                {
                    state.edit(index, button);
                }
            }
        });
    ui.separator();
    egui::Grid::new("buttonbar_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("caption");
            ui.add(egui::TextEdit::singleline(&mut state.edit_caption).desired_width(260.0));
            ui.end_row();
            ui.label("command");
            ui.add(
                egui::TextEdit::singleline(&mut state.edit_command)
                    .desired_width(260.0)
                    .hint_text("cm_Copy"),
            );
            ui.end_row();
            ui.label("parameters");
            ui.add(
                egui::TextEdit::singleline(&mut state.edit_parameters)
                    .desired_width(260.0)
                    .hint_text("%P %N"),
            );
            ui.end_row();
        });
    ui.horizontal(|ui| {
        if ui.button("+").clicked() {
            state.start_add();
        }
        if ui.button("apply").clicked() {
            let mut snapshot = state.clone();
            if let crate::model::CommandOutcome::Failed(message) =
                model.apply_button_bar(&mut snapshot)
            {
                model.report_error(message);
            }
            *state = snapshot;
        }
        if ui.button("-").clicked() {
            let mut snapshot = state.clone();
            let _ = model.remove_button_bar(&mut snapshot);
            *state = snapshot;
        }
        if ui.button("up").clicked() {
            let mut snapshot = state.clone();
            let _ = model.move_button_bar(&mut snapshot, true);
            *state = snapshot;
        }
        if ui.button("down").clicked() {
            let mut snapshot = state.clone();
            let _ = model.move_button_bar(&mut snapshot, false);
            *state = snapshot;
        }
    });
    let (ok, cancel) = buttons_close(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        model
            .settings
            .save(&rc_core::config::default_config_path())
            .unwrap_or_else(|e| model.report_error(e));
        return DialogAction::Close;
    }
    DialogAction::None
}

fn view_modes(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut ViewModeDialogState,
) -> DialogAction {
    let mut action = DialogAction::None;
    ui.label(model.t("cmd.custom_column_view"));
    // --- the active column list -------------------------------------
    let columns = model.settings.display.columns.clone();
    ui.horizontal_wrapped(|ui| {
        for spec in &columns {
            let label = spec
                .label_key()
                .map(|k| model.t(k))
                .unwrap_or_else(|| spec.id.clone());
            if ui
                .button(format!("{label} ×"))
                .on_hover_text(format!("{} — {} pt", spec.id, spec.width))
                .clicked()
            {
                let _ = model.remove_column(&spec.id);
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        for id in rc_core::config::ColumnSpec::known_ids() {
            if columns.iter().any(|c| c.id == id) {
                continue;
            }
            if ui.small_button(format!("+ {id}")).clicked()
                && let crate::model::CommandOutcome::Failed(message) = model.add_column(id)
            {
                model.report_error(message);
            }
        }
    });
    ui.separator();
    // --- saved view modes -------------------------------------------
    let modes = model.settings.display.view_modes.clone();
    egui::ScrollArea::vertical()
        .max_height(140.0)
        .id_salt("viewmode_scroll")
        .show(ui, |ui| {
            for (index, mode) in modes.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(
                            state.selected_mode == Some(index),
                            format!("{} ({})", mode.name, mode.kind),
                        )
                        .clicked()
                    {
                        *state = ViewModeDialogState::from_mode(mode, index);
                    }
                    if ui.small_button("apply").clicked() {
                        let _ = model.apply_view_mode(&mode.name);
                    }
                    if ui.small_button("×").clicked() {
                        let _ = model.remove_view_mode(&mode.name);
                    }
                });
            }
            if modes.is_empty() {
                ui.label(model.t("panel.empty"));
            }
        });
    egui::Grid::new("viewmode_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("name");
            ui.add(egui::TextEdit::singleline(&mut state.name).desired_width(200.0));
            ui.end_row();
            ui.label("kind");
            egui::ComboBox::from_id_salt("viewmode_kind")
                .selected_text(state.kind.clone())
                .show_ui(ui, |ui| {
                    for kind in ["brief", "full", "comments", "thumbnails", "tree"] {
                        ui.selectable_value(&mut state.kind, kind.to_string(), kind);
                    }
                });
            ui.end_row();
            ui.label("mask");
            ui.add(egui::TextEdit::singleline(&mut state.mask).desired_width(200.0));
            ui.end_row();
            ui.label("sort");
            egui::ComboBox::from_id_salt("viewmode_sort")
                .selected_text(state.sort_field.clone())
                .show_ui(ui, |ui| {
                    for field in ["name", "ext", "size", "date", "unsorted"] {
                        ui.selectable_value(&mut state.sort_field, field.to_string(), field);
                    }
                });
            ui.end_row();
        });
    ui.checkbox(&mut state.sort_reversed, "reversed");
    if ui.button(model.t("dialog.config.apply")).clicked() {
        match model.save_view_mode(state) {
            crate::model::CommandOutcome::Failed(message) => model.report_error(message),
            _ => model.report(model.t("status.done")),
        }
    }
    if ui.button(model.t("dialog.config.ok")).clicked() {
        action = DialogAction::Close;
    }
    action
}

fn split(ui: &mut egui::Ui, model: &mut Model, state: &mut SplitState) -> DialogAction {
    ui.label(&state.source);
    ui.horizontal(|ui| {
        ui.label(model.t("col.size"));
        ui.add(
            egui::DragValue::new(&mut state.part_size)
                .range(1024..=u32::MAX as u64)
                .speed(1024.0),
        );
        ui.label(state.size_label());
    });
    ui.horizontal(|ui| {
        ui.label(model.t("dialog.copy.to"));
        ui.add(egui::TextEdit::singleline(&mut state.destination).desired_width(f32::INFINITY));
    });
    if state.parts_created > 0 {
        ui.label(model.tf(
            "status.done",
            &[("count", &state.parts_created.to_string())],
        ));
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.apply")).clicked() {
            action = DialogAction::Command("rc_RunSplit".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn combine(ui: &mut egui::Ui, model: &mut Model, state: &mut CombineState) -> DialogAction {
    ui.label(&state.first_part);
    if state.discovered > 0 {
        ui.label(format!("{} part(s) found", state.discovered));
    }
    ui.horizontal(|ui| {
        ui.label(model.t("dialog.copy.to"));
        ui.add(egui::TextEdit::singleline(&mut state.destination).desired_width(f32::INFINITY));
    });
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.apply")).clicked() {
            action = DialogAction::Command("rc_RunCombine".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn checksum(ui: &mut egui::Ui, model: &mut Model, state: &mut ChecksumState) -> DialogAction {
    ui.label(model.tf(
        "panel.selected",
        &[("count", &state.sources.len().to_string())],
    ));
    ui.horizontal(|ui| {
        ui.label("algorithm");
        egui::ComboBox::from_id_salt("checksum_algo")
            .selected_text(state.algorithm.clone())
            .show_ui(ui, |ui| {
                for id in ["md5", "sha1", "sha256", "crc32"] {
                    ui.selectable_value(&mut state.algorithm, id.to_string(), id);
                }
            });
        ui.add(egui::TextEdit::singleline(&mut state.file_name).desired_width(200.0));
    });
    ui.label(&state.base);
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.apply")).clicked() {
            action = DialogAction::Command("rc_RunChecksum".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn verify_checksum(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut VerifyChecksumState,
) -> DialogAction {
    ui.label(&state.checksum_file);
    ui.horizontal(|ui| {
        ui.label(model.t("panel.path"));
        ui.add(egui::TextEdit::singleline(&mut state.base).desired_width(f32::INFINITY));
    });
    if state.checked > 0 {
        ui.label(state.summary());
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .id_salt("verify_scroll")
            .show(ui, |ui| {
                for (name, problem) in &state.failures {
                    ui.label(
                        egui::RichText::new(format!("{name}: {problem}"))
                            .color(egui::Color32::from_rgb(0xa0, 0x00, 0x00)),
                    );
                }
            });
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.apply")).clicked() {
            action = DialogAction::Command("rc_RunVerify".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn compare_files(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut CompareFilesState,
) -> DialogAction {
    ui.horizontal(|ui| {
        ui.label(model.t("panel.source"));
        ui.add(egui::TextEdit::singleline(&mut state.left).desired_width(f32::INFINITY));
    });
    ui.horizontal(|ui| {
        ui.label(model.t("panel.target"));
        ui.add(egui::TextEdit::singleline(&mut state.right).desired_width(f32::INFINITY));
    });
    ui.label(state.result_text());
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.apply")).clicked() {
            action = DialogAction::Command("rc_RunCompareFiles".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn duplicates(ui: &mut egui::Ui, model: &mut Model, state: &mut DuplicatesState) -> DialogAction {
    ui.horizontal(|ui| {
        ui.label(model.t("panel.path"));
        ui.add(egui::TextEdit::singleline(&mut state.root).desired_width(f32::INFINITY));
    });
    ui.horizontal(|ui| {
        ui.label(model.t("col.size"));
        ui.add(egui::DragValue::new(&mut state.min_size).range(0..=u32::MAX as u64));
        ui.label("algorithm");
        egui::ComboBox::from_id_salt("dupe_algo")
            .selected_text(state.algorithm.clone())
            .show_ui(ui, |ui| {
                for id in ["md5", "sha1", "sha256", "crc32"] {
                    ui.selectable_value(&mut state.algorithm, id.to_string(), id);
                }
            });
        ui.checkbox(&mut state.include_hidden, model.t("panel.all_files"));
    });
    if state.scanned > 0 || !state.groups.is_empty() {
        ui.label(state.summary());
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .id_salt("dupe_scroll")
            .show(ui, |ui| {
                for group in &state.groups {
                    ui.label(
                        egui::RichText::new(rc_core::format::format_size_unit(group.size)).strong(),
                    );
                    for path in &group.paths {
                        ui.label(format!("  {}", path.display()));
                    }
                    ui.separator();
                }
            });
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.search.start")).clicked() {
            action = DialogAction::Command("rc_RunDuplicates".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn buttons_close(ui: &mut egui::Ui, model: &Model) -> (bool, bool) {
    let mut ok = false;
    let mut cancel = false;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.ok")).clicked() {
            ok = true;
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            cancel = true;
        }
    });
    (ok, cancel)
}

fn sync(ui: &mut egui::Ui, model: &mut Model, state: &mut SyncState) -> DialogAction {
    ui.horizontal(|ui| {
        ui.label(model.t("panel.source"));
        ui.add(egui::TextEdit::singleline(&mut state.left).desired_width(200.0));
    });
    ui.horizontal(|ui| {
        ui.label(model.t("panel.target"));
        ui.add(egui::TextEdit::singleline(&mut state.right).desired_width(200.0));
    });
    ui.checkbox(&mut state.ignore_dates, "ignore dates");
    ui.checkbox(&mut state.by_content, "compare by content");
    ui.checkbox(&mut state.dry_run, "dry run");
    let mut action = DialogAction::None;
    if ui.button(model.t("cmd.compare_dirs")).clicked() {
        if let Err(message) = state.validate() {
            model.report_error(message);
        } else {
            action = DialogAction::Command("rc_RunSync".to_string());
        }
    }
    if ui.button(model.t("cmd.sync_dirs")).clicked() {
        action = DialogAction::Command("rc_ExecuteSync".to_string());
    }
    if !state.rows.is_empty() {
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .id_salt("sync_rows")
            .show(ui, |ui| {
                for row in &state.rows {
                    let left = row
                        .left_size
                        .map(rc_core::format::format_size_bytes)
                        .unwrap_or_else(|| "-".to_string());
                    let right = row
                        .right_size
                        .map(rc_core::format::format_size_bytes)
                        .unwrap_or_else(|| "-".to_string());
                    ui.label(format!(
                        "{}  {left:>10}  {right:>10}  {}",
                        row.status, row.name
                    ));
                }
            });
    }
    if ui.button(model.t("dialog.config.cancel")).clicked() {
        return DialogAction::Close;
    }
    action
}

fn about(ui: &mut egui::Ui, model: &mut Model) -> DialogAction {
    ui.heading("RCommander");
    ui.label(model.tf("app.version", &[("version", crate::VERSION)]));
    ui.label("A cross-platform dual-pane file manager written in Rust.");
    ui.label("Independent implementation — no code or assets from other file managers.");
    ui.separator();
    ui.label("MIT licence");
    if ui.button(model.t("dialog.config.ok")).clicked() {
        return DialogAction::Close;
    }
    DialogAction::None
}

fn confirm(
    ui: &mut egui::Ui,
    model: &mut Model,
    message: &str,
    action: ConfirmAction,
) -> DialogAction {
    ui.label(message);
    let mut result = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.yes")).clicked() {
            result = match action {
                ConfirmAction::Quit => DialogAction::Command("rc_Quit".to_string()),
                ConfirmAction::DeletePermanent => DialogAction::Run,
                ConfirmAction::Overwrite => DialogAction::Run,
                ConfirmAction::CloseWithTransfers => DialogAction::Command("rc_Quit".to_string()),
                ConfirmAction::SaveSettings => DialogAction::Command("cm_SaveSettings".to_string()),
                ConfirmAction::DiscardEditorChanges => DialogAction::Run,
            };
        }
        if ui.button(model.t("dialog.no")).clicked() {
            result = DialogAction::Close;
        }
    });
    result
}

fn progress(ui: &mut egui::Ui, model: &mut Model, state: &mut ProgressState) -> DialogAction {
    let bar = egui::ProgressBar::new(state.fraction()).text(format!("{}%", state.percent()));
    ui.add(bar);
    ui.label(&state.current_file);
    if state.cancellable && ui.button(model.t("dialog.cancel")).clicked() {
        return DialogAction::Close;
    }
    DialogAction::None
}

/// Bold heading used by several dialogs.
pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong());
}

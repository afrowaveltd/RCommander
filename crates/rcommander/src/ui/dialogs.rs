//! Modal dialog rendering.

use crate::model::dialogs::*;
use crate::model::{CommandOutcome, Model, ResetScope};
use egui::{Context, RichText};
use rc_core::config::SavedConnection;

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
                Dialog::WorkspaceReview(state) => workspace_review(ui, model, state),
                Dialog::OpenWith(state) => open_with(ui, model, state),
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
                Dialog::UnlockSecrets(state) => unlock_secrets(ui, model, state),
                Dialog::GitConfig(state) => git_config(ui, model, state),
                Dialog::GitCommit(state) => git_commit(ui, model, state),
                Dialog::GitBranches(state) => git_branches(ui, model, state),
                Dialog::GitRemote(state) => git_remote(ui, model, state),
                Dialog::GitClone(state) => git_clone(ui, model, state),
                Dialog::AgentApproval(state) => agent_approval(ui, model, state),
                Dialog::AiModel(state) => ai_model(ui, model, state),
                Dialog::Encode(state) => encode_dialog(ui, model, state),
                Dialog::Decode(state) => decode_dialog(ui, model, state),
                Dialog::SaveSelection(state) => save_selection(ui, model, state),
                Dialog::LoadSelection(state) => load_selection(ui, model, state),
                Dialog::About => about(ui, model),
                Dialog::Confirm {
                    message,
                    action: confirm_action,
                    ..
                } => confirm(ui, model, message, *confirm_action),
                Dialog::Progress(state) => progress(ui, model, state),
            };
        });
    if !open {
        // The window's × was pressed: close this dialog instead of writing the
        // state back (the previous code ignored `open` and the × did nothing).
        model.dialogs.pop();
        return DialogAction::Close;
    }
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
}

/// The review dialog: the changed files on the left, their diff on the right.
///
/// The diff is the point. "This file differs" is not a review; `+ one line, - one line, here they
/// are` is. Files that did not change are listed greyed out and unticked, because a review that
/// hides them makes the user wonder what happened to them.
fn workspace_review(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut WorkspaceReviewState,
) -> DialogAction {
    use crate::model::workspacereview::diff_marker;
    use rc_core::diff::DiffLine;
    let palette = crate::theme::Palette::for_model(model);

    ui.label(
        egui::RichText::new(state.review.summary())
            .small()
            .color(palette.hidden),
    );
    if state.review.is_clean() {
        ui.label(egui::RichText::new(model.t("workspace.review.clean")).color(palette.archive));
    }
    ui.separator();
    let mut apply = false;
    ui.horizontal_top(|ui| {
        // The list: a tick per file, plus what changed about it.
        ui.vertical(|ui| {
            ui.set_min_width(300.0);
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .id_salt("workspace_review_list")
                .show(ui, |ui| {
                    for (index, item) in state.review.items.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let mut tick = item.approved;
                            let enabled = item.blocked.is_none() && item.changed();
                            if ui
                                .add_enabled(enabled, egui::Checkbox::without_text(&mut tick))
                                .changed()
                            {
                                item.approved = tick;
                            }
                            let label = if ui
                                .selectable_label(state.selected == index, &item.relative)
                                .clicked()
                            {
                                state.selected = index;
                                true
                            } else {
                                false
                            };
                            let _ = label;
                            let summary = if item.binary {
                                model.t("workspace.review.binary").to_string()
                            } else {
                                item.diff.summary()
                            };
                            let colour = if item.blocked.is_some() {
                                palette.warning
                            } else {
                                palette.hidden
                            };
                            ui.label(egui::RichText::new(summary).small().color(colour));
                        });
                    }
                });
        });
        ui.separator();
        // The diff of the selected file.
        // The current file is taken mutably: the ticks are the whole point of the view.
        let selected_index = state.selected;
        ui.vertical(|ui| match state.review.items.get_mut(selected_index) {
            Some(item) => {
                if let Some(reason) = &item.blocked {
                    ui.label(egui::RichText::new(reason).color(palette.warning));
                }
                if item.binary {
                    ui.label(
                        egui::RichText::new(model.t("workspace.review.binary.hint"))
                            .small()
                            .color(palette.hidden),
                    );
                } else {
                    // The syntax verdict sits above the diff: a parser catches what a reader would
                    // have to notice, and a file with a syntax error is not pre-ticked.
                    match &item.syntax {
                        rc_core::validate::Verdict::Valid => {
                            ui.label(
                                egui::RichText::new(model.t("workspace.review.syntax.ok"))
                                    .small()
                                    .color(palette.archive),
                            );
                        }
                        rc_core::validate::Verdict::Broken(message) => {
                            ui.label(egui::RichText::new(message).small().color(palette.warning));
                        }
                        rc_core::validate::Verdict::Unknown => {}
                    }
                    // The blocked state is read before the loop: the loop borrows the hunks
                    // mutably, and `item.blocked` would borrow all of `item`.
                    let blocked = item.blocked.is_some();
                    egui::ScrollArea::both()
                        .max_height(320.0)
                        .id_salt("workspace_review_diff")
                        .show(ui, |ui| {
                            for hunk_index in 0..item.hunks.len() {
                                // A tick per hunk: "this change yes, that one no" is a real answer.
                                let mut toggled = false;
                                let mut taken = item.hunks[hunk_index].taken;
                                let header = item.hunks[hunk_index].header.clone();
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            !blocked,
                                            egui::Checkbox::without_text(&mut taken),
                                        )
                                        .changed()
                                    {
                                        toggled = true;
                                    }
                                    ui.label(
                                        egui::RichText::new(&header)
                                            .monospace()
                                            .small()
                                            .color(palette.accent),
                                    );
                                });
                                if toggled {
                                    item.hunks[hunk_index].taken = taken;
                                    // The file is written only if at least one hunk is taken.
                                    item.approved = item.hunks.iter().any(|hunk| hunk.taken);
                                }
                                for line in &item.hunks[hunk_index].lines {
                                    let marker = diff_marker(line);
                                    let colour = match line {
                                        DiffLine::Added(_) => palette.archive,
                                        DiffLine::Removed(_) => palette.readonly,
                                        DiffLine::Skipped(_) => palette.hidden,
                                        DiffLine::Same(_) => palette.foreground,
                                    };
                                    let text = match line {
                                        DiffLine::Same(content)
                                        | DiffLine::Added(content)
                                        | DiffLine::Removed(content) => {
                                            format!("{marker} {content}")
                                        }
                                        DiffLine::Skipped(count) => {
                                            format!("… {count} line(s) unchanged")
                                        }
                                    };
                                    ui.label(egui::RichText::new(text).monospace().color(colour));
                                }
                            }
                        });
                }
            }
            None => {
                ui.label(egui::RichText::new(model.t("workspace.review.none")));
            }
        });
    });
    if !state.status.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new(&state.status).color(palette.warning));
    }
    ui.separator();
    let mut cancel = false;
    ui.horizontal(|ui| {
        let count = state.review.approved().len();
        let label = model.tf("workspace.review.apply", &[("count", &count.to_string())]);
        if ui
            .add_enabled(count > 0, egui::Button::new(label))
            .clicked()
        {
            apply = true;
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            cancel = true;
        }
    });
    if cancel {
        return DialogAction::Close;
    }
    if apply {
        match model.apply_workspace_approved() {
            crate::model::CommandOutcome::Ok => return DialogAction::Close,
            crate::model::CommandOutcome::Failed(message) => state.status = message,
            _ => {}
        }
    }
    DialogAction::None
}

/// The "open with" chooser: a searchable list of the desktop's applications.
fn open_with(ui: &mut egui::Ui, model: &mut Model, state: &mut OpenWithState) -> DialogAction {
    let palette = crate::theme::Palette::for_model(model);
    ui.label(
        egui::RichText::new(state.path.display().to_string())
            .monospace()
            .small(),
    );
    if state.apps.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new(model.t("dialog.openwith.none")).color(palette.warning));
        ui.label(
            egui::RichText::new(model.t("dialog.openwith.none.hint"))
                .small()
                .color(palette.hidden),
        );
        let (_, cancel) = buttons(ui, model);
        return if cancel {
            DialogAction::Close
        } else {
            DialogAction::None
        };
    }
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .desired_width(240.0)
                .hint_text(model.t("dialog.openwith.filter")),
        );
        ui.label(
            egui::RichText::new(model.tf(
                "prompt.count",
                &[("count", &state.visible().len().to_string())],
            ))
            .small()
            .color(palette.hidden),
        );
    });
    ui.separator();
    let mut chosen: Option<usize> = None;
    let names: Vec<String> = state.visible().iter().map(|app| app.name.clone()).collect();
    let selected = state.selected;
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .auto_shrink([false, true])
        .id_salt("open_with_list")
        .show(ui, |ui| {
            for (index, name) in names.iter().enumerate() {
                if ui.selectable_label(index == selected, name).clicked() {
                    chosen = Some(index);
                }
            }
        });
    if let Some(index) = chosen {
        state.selected = index;
    }
    let mut action = DialogAction::None;
    // A double click or Enter opens; so does the button, which is what a keyboard user reaches for.
    if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !chosen.is_some() {
        chosen = Some(state.selected);
    }
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.openwith.open")).clicked() {
            chosen = Some(state.selected);
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    if let Some(index) = chosen
        && let Some(app) = state.visible().get(index).copied()
    {
        let plan = app.command_for(&state.path);
        let action = rc_core::desktop::Action::OpenWith {
            path: state.path.clone(),
            app: plan.program.clone(),
        };
        // An application whose `Exec` had more than the program name is run as-is.
        let started = if plan.args.is_empty() {
            crate::system_open::run(&action)
        } else {
            crate::system_open::run_plan(&plan)
        };
        match started {
            Ok(started) => {
                let message = model.tf("status.opening", &[("what", &started.what)]);
                model.report(message);
                return DialogAction::Close;
            }
            Err(error) => {
                let message = model.tf("status.open.failed", &[("path", ""), ("error", &error)]);
                model.report_error(message);
            }
        }
    }
    action
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
        Dialog::WorkspaceReview(_) => "workspace.review.title",
        Dialog::OpenWith(_) => "dialog.openwith.title",
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
        Dialog::UnlockSecrets(_) => "dialog.secrets.title",
        Dialog::GitConfig(_) => "git.title",
        Dialog::GitCommit(_) => "git.commit.title",
        Dialog::GitBranches(_) => "git.branches.title",
        Dialog::GitRemote(_) => "git.remote.title",
        Dialog::GitClone(_) => "git.clone.title",
        Dialog::AgentApproval(_) => "agent.approval.title",
        Dialog::AiModel(_) => "ai.model.title",
        Dialog::Encode(_) => "cmd.encode",
        Dialog::Decode(_) => "cmd.decode",
        Dialog::SaveSelection(_) => "cmd.save_selection",
        Dialog::LoadSelection(_) => "cmd.restore_selection",
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
    // Enter confirms a dialog; Esc cancels (unless a text field has the focus,
    // in which case egui consumes the key itself).
    if ui.memory(|m| m.focused()).is_none() {
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            ok = true;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
    }
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

/// Localised confirmation: one target names the file, multiple targets show a count.
/// The former passed `count` to a `{name}` template, leaking the placeholder.
fn delete_confirmation_text(model: &Model, state: &DeleteState) -> String {
    if state.paths.len() == 1 {
        let name = std::path::Path::new(&state.paths[0])
            .file_name()
            .map(|part| part.to_string_lossy().into_owned())
            .unwrap_or_else(|| state.paths[0].clone());
        model.tf("dialog.delete.confirm", &[("name", &name)])
    } else {
        model.tf(
            "dialog.delete.confirm_many",
            &[("count", &state.count().to_string())],
        )
    }
}

fn delete(ui: &mut egui::Ui, model: &mut Model, state: &mut DeleteState) -> DialogAction {
    ui.label(delete_confirmation_text(model, state));
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

/// "Dangerous" page: factory resets, each behind an explicit confirmation.
///
/// The encrypted credential store cannot be recovered without its master
/// passphrase, so this page is the documented way out of a forgotten
/// passphrase: it drops the store and everything in it.
fn dangerous_page(ui: &mut egui::Ui, model: &mut Model, state: &mut OptionsState) {
    let palette = crate::theme::Palette::for_model(model);
    ui.label(
        egui::RichText::new(model.t("reset.warning"))
            .color(palette.warning)
            .strong(),
    );
    ui.add_space(4.0);

    if let Some(scope) = state.pending_reset {
        let label = model.t(scope.label_key());
        ui.label(
            egui::RichText::new(model.tf("reset.confirm", &[("action", &label)]))
                .color(palette.warning)
                .strong(),
        );
        ui.horizontal(|ui| {
            if ui.button(model.t("reset.confirm_yes")).clicked() {
                let report = model.reset(scope);
                if !state.applied_resets.contains(&scope) {
                    state.applied_resets.push(scope);
                }
                state.pending_reset = None;
                // A settings reset replaces the model settings, so the draft the
                // dialog is editing has to follow or OK would undo the reset.
                if matches!(scope, ResetScope::Settings | ResetScope::Everything) {
                    state.draft = model.settings.clone();
                    state.language_choice = model.settings.language.clone();
                    state.shortcuts_edit = state
                        .draft
                        .shortcuts
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    state.shortcuts_edit.sort();
                }
                let message = if report.is_empty() {
                    model.t("reset.nothing_to_do")
                } else {
                    let count = report.removed_files.to_string();
                    model.tf("reset.done", &[("count", &count)])
                };
                model.report(message);
            }
            if ui.button(model.t("reset.cancel")).clicked() {
                state.pending_reset = None;
            }
        });
        ui.separator();
    }

    egui::ScrollArea::vertical()
        .max_height(200.0)
        .id_salt("dangerous_actions")
        .show(ui, |ui| {
            for scope in ResetScope::all() {
                ui.horizontal(|ui| {
                    let label = model.t(scope.label_key());
                    let done = state.applied_resets.contains(&scope);
                    let text = if done {
                        format!("{} ({})", label, model.t("reset.done_mark"))
                    } else {
                        label
                    };
                    if ui
                        .button(
                            egui::RichText::new(text).color(if scope.destroys_credentials() {
                                palette.warning
                            } else {
                                palette.foreground
                            }),
                        )
                        .clicked()
                    {
                        state.pending_reset = Some(scope);
                    }
                    ui.label(egui::RichText::new(model.t(scope.hint_key())).color(palette.hidden));
                });
            }
        });
}

/// Colors page: pick a theme from the theme directory, edit every role and see a
/// live preview next to it.
///
/// The actions sit in the first row, above the scrolling parts, so they stay
/// reachable no matter how long the role list or the theme names get.
fn colors_page(ui: &mut egui::Ui, model: &mut Model, state: &mut OptionsState) {
    let current = state.draft.colors.theme.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label(model.t("theme.name"));
        egui::ComboBox::from_id_salt("theme_choice")
            .selected_text(model.theme_display_name(&current))
            .width(130.0)
            .show_ui(ui, |ui| {
                for name in model.theme_names() {
                    let label = model.theme_display_name(&name);
                    if ui
                        .selectable_label(name.eq_ignore_ascii_case(&current), label)
                        .clicked()
                    {
                        state.draft.colors.theme = name.clone();
                        state.theme_edit = model.active_theme_for_editing_named(&name);
                        state.theme_name = state.theme_edit.meta.name.clone();
                        state.theme_status.clear();
                    }
                }
            });
        if ui.button(model.t("theme.reload")).clicked() {
            model.load_themes();
            state.theme_status = model.t("theme.reloaded");
        }
        if ui.button(model.t("theme.save")).clicked() {
            let mut theme = state.theme_edit.clone();
            theme.meta.name = state.theme_name.trim().to_string();
            if theme.meta.name.is_empty() {
                state.theme_status = model.t("theme.name_required");
            } else {
                match model.save_theme(theme) {
                    Ok(name) => {
                        state.draft.colors.theme = name.clone();
                        state.theme_status = model.tf("theme.saved", &[("name", &name)]);
                    }
                    Err(message) => state.theme_status = message,
                }
            }
        }
        if ui.button(model.t("theme.delete")).clicked() {
            let name = state.theme_name.clone();
            match model.delete_theme(&name) {
                Ok(true) => {
                    state.theme_status = model.tf("theme.deleted", &[("name", &name)]);
                    state.draft.colors.theme = model.settings.colors.theme.clone();
                    state.theme_edit =
                        model.active_theme_for_editing_named(&state.draft.colors.theme);
                }
                Ok(false) => state.theme_status = model.t("theme.nothing_deleted"),
                Err(message) => state.theme_status = message,
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(model.t("theme.save_as"));
        ui.add(egui::TextEdit::singleline(&mut state.theme_name).desired_width(140.0));
        if !state.theme_status.is_empty() {
            ui.label(egui::RichText::new(&state.theme_status).small());
        }
    });

    ui.horizontal_top(|ui| {
        egui::ScrollArea::vertical()
            .max_height(170.0)
            .id_salt("theme_roles")
            .show(ui, |ui| {
                egui::Grid::new("theme_roles_grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for role in rc_core::theme::Role::ALL {
                            ui.label(model.t(&format!("theme.role.{}", role.name())));
                            let mut colour = parse_ui_color(
                                state
                                    .theme_edit
                                    .colors
                                    .get(role.name())
                                    .map(|s| s.as_str())
                                    .unwrap_or("#000000"),
                            );
                            if ui.color_edit_button_srgb(&mut colour).changed() {
                                state.theme_edit.colors.insert(
                                    role.name().to_string(),
                                    format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2]),
                                );
                            }
                            ui.end_row();
                        }
                    });
            });
        ui.separator();
        ui.vertical(|ui| {
            let resolved = preview_resolved(state);
            let preview = crate::theme::Palette::from_resolved(&resolved);
            theme_preview(ui, &preview);
            for (front, back, ratio) in resolved.contrast_warnings() {
                let front_name = model.t(&format!("theme.role.{}", front.name()));
                let back_name = model.t(&format!("theme.role.{}", back.name()));
                let ratio = format!("{ratio:.1}");
                let text = model.tf(
                    "theme.contrast_warning",
                    &[
                        ("front", &front_name),
                        ("back", &back_name),
                        ("ratio", &ratio),
                    ],
                );
                ui.label(egui::RichText::new(text).color(preview.warning).small());
            }
        });
    });

    for problem in model.theme_problems() {
        ui.label(
            egui::RichText::new(problem.clone()).color(egui::Color32::from_rgb(0xb0, 0x20, 0x20)),
        );
    }
}

/// Resolve the theme being edited against the built-in base it inherits from.
fn preview_resolved(state: &OptionsState) -> rc_core::theme::Resolved {
    let base_name = state.theme_edit.meta.base.clone();
    let base = rc_core::theme::Resolved::builtin(&base_name)
        .unwrap_or_else(rc_core::theme::Resolved::light);
    state.theme_edit.resolve(&base)
}

/// A small sample panel painted with the preview palette.
fn theme_preview(ui: &mut egui::Ui, palette: &crate::theme::Palette) {
    let rows: [(&str, egui::Color32, Option<egui::Color32>); 4] = [
        ("readme.txt", palette.foreground, None),
        ("Documents", palette.directory, Some(palette.row_alternate)),
        ("hidden.log", palette.hidden, None),
        (
            "readonly.txt",
            palette.readonly,
            Some(palette.row_alternate),
        ),
    ];
    let row_height = ui.spacing().interact_size.y.max(18.0);
    let width = ui.available_width().min(360.0);
    for (index, (text, colour, background)) in rows.iter().enumerate() {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, row_height), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 0, background.unwrap_or(palette.background));
        painter.rect_stroke(
            rect,
            0,
            egui::Stroke::new(1.0, palette.panel_border),
            egui::StrokeKind::Inside,
        );
        // The first row shows the cursor colours, the second a marked row.
        if index == 0 {
            painter.rect_filled(rect, 0, palette.cursor_background);
        } else if index == 4 {
            painter.rect_filled(rect, 0, palette.selected_background);
        }
        let text_colour = if index == 0 {
            palette.cursor_foreground
        } else {
            *colour
        };
        painter.text(
            rect.left_center() + egui::vec2(6.0, 0.0),
            egui::Align2::LEFT_CENTER,
            *text,
            egui::FontId::proportional(13.0),
            text_colour,
        );
        response.on_hover_text(*text);
    }
}

fn parse_ui_color(text: &str) -> [u8; 3] {
    match rc_core::theme::Rgb::parse(text) {
        Ok(rgb) => [rgb.r, rgb.g, rgb.b],
        Err(_) => [0, 0, 0],
    }
}

/// The font page: three roles, each with a family and a size, and a live sample.
///
/// The three roles exist because three kinds of text need different faces. Nothing here touches
/// the panels directly: the page edits the draft, and accepting the dialog reinstalls the fonts.
fn fonts_page(ui: &mut egui::Ui, model: &mut Model, state: &mut OptionsState) {
    let palette = crate::theme::Palette::for_model(model);
    ui.label(
        egui::RichText::new(model.t("fonts.help"))
            .small()
            .color(palette.hidden),
    );
    ui.separator();

    // The scan is done on first use and kept; a rescan is one click for a just-installed font.
    let body_size = state.draft.display.font_size;
    let mono_choices = model.font_choices_for("mono");
    let all_choices = model.font_choices_for("body");
    let mut rescan = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(
                model.tf("fonts.found", &[("count", &all_choices.len().to_string())]),
            )
            .small()
            .color(palette.hidden),
        );
        if ui.button(model.t("fonts.rescan")).clicked() {
            rescan = true;
        }
    });
    if rescan {
        let count = model.rescan_fonts();
        let text = model.tf("fonts.rescanned", &[("count", &count.to_string())]);
        model.report(text);
    }

    // One row per role: family picker, then the size that belongs to it.
    for (role, label_key) in [
        ("body", "fonts.body"),
        ("heading", "fonts.heading"),
        ("mono", "fonts.mono"),
    ] {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(model.t(label_key)).strong());
            let choices: &[rc_core::font::FontFile] = if role == "mono" {
                &mono_choices
            } else {
                &all_choices
            };
            let current = match role {
                "body" => state.draft.fonts.body_family.clone(),
                "heading" => state.draft.fonts.heading_family.clone(),
                _ => state.draft.fonts.mono_family.clone(),
            };
            let shown = if current.trim().is_empty() {
                model.t(match role {
                    "mono" => "fonts.default.mono",
                    "heading" => "fonts.default.heading",
                    _ => "fonts.default.body",
                })
            } else {
                current.clone()
            };
            egui::ComboBox::from_id_salt(("font", role))
                .selected_text(shown)
                .width(260.0)
                .show_ui(ui, |ui| {
                    let builtin = match role {
                        "mono" => model.t("fonts.default.mono"),
                        "heading" => model.t("fonts.default.heading"),
                        _ => model.t("fonts.default.body"),
                    };
                    if ui
                        .selectable_label(current.trim().is_empty(), builtin)
                        .clicked()
                    {
                        set_family(&mut state.draft.fonts, role, String::new());
                    }
                    for font in choices {
                        if ui
                            .selectable_label(current == font.family, font.label())
                            .clicked()
                        {
                            set_family(&mut state.draft.fonts, role, font.family.clone());
                        }
                    }
                });
            match role {
                "body" => {
                    ui.add(
                        egui::DragValue::new(&mut state.draft.display.font_size)
                            .range(8.0..=28.0)
                            .suffix(" pt"),
                    );
                }
                "heading" => {
                    ui.add(
                        egui::DragValue::new(&mut state.draft.fonts.heading_size)
                            .range(8.0..=48.0)
                            .suffix(" pt"),
                    );
                }
                _ => {
                    ui.add(
                        egui::DragValue::new(&mut state.draft.fonts.mono_size)
                            .range(8.0..=48.0)
                            .suffix(" pt"),
                    );
                }
            }
        });
    }

    ui.separator();
    ui.label(
        egui::RichText::new(model.t("fonts.sample"))
            .small()
            .color(palette.hidden),
    );
    // The sample uses the draft sizes, so the effect is visible before accepting.
    ui.label(
        egui::RichText::new(model.t("fonts.sample.heading"))
            .size(state.draft.fonts.heading_size.clamp(8.0, 48.0)),
    );
    ui.label(egui::RichText::new(model.t("fonts.sample.body")).size(body_size.clamp(8.0, 28.0)));
    ui.label(
        egui::RichText::new(model.t("fonts.sample.mono"))
            .monospace()
            .size(state.draft.fonts.mono_size.clamp(8.0, 48.0)),
    );
    if !model.font_problems.is_empty() {
        ui.separator();
        for problem in &model.font_problems {
            ui.label(egui::RichText::new(problem).color(palette.warning).small());
        }
    }
    ui.label(
        egui::RichText::new(model.t("fonts.note"))
            .small()
            .italics()
            .color(palette.hidden),
    );
}

fn set_family(fonts: &mut rc_core::config::Fonts, role: &str, family: String) {
    match role {
        "body" => fonts.body_family = family,
        "heading" => fonts.heading_family = family,
        _ => fonts.mono_family = family,
    }
}

/// The prompt page: the catalogue on the left, the editor on the right.
///
/// A built-in prompt cannot be edited in place — an upgrade would overwrite it — so editing one
/// starts by copying it into the user's own file under the same id, which makes the copy an
/// *override*. *Reset* deletes that override and the shipped text applies again. That is the whole
/// trick: one mechanism for "change a system prompt" and for "change it back".
fn prompts_page(ui: &mut egui::Ui, model: &mut Model) {
    use rc_core::prompt::Category;
    let palette = crate::theme::Palette::for_model(model);
    // The catalogue is read on first use, so a startup that never opens this page pays nothing.
    let _ = model.prompts();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("prompts_scroll")
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(model.t("prompt.page.help"))
                    .small()
                    .color(palette.hidden),
            );
            let problems: Vec<String> = model.prompts().problems().to_vec();
            for problem in problems {
                ui.label(egui::RichText::new(problem).color(palette.warning).small());
            }
            ui.separator();

            // ---- category tabs and the list -------------------------------------
            ui.horizontal_wrapped(|ui| {
                for category in Category::ALL {
                    let label = model.t(category.label_key());
                    let chosen = model.prompt_picker.category == Some(category);
                    if ui.selectable_label(chosen, label).clicked() {
                        model.prompt_picker.category = Some(category);
                    }
                }
                if ui
                    .selectable_label(
                        model.prompt_picker.category.is_none(),
                        model.t("prompt.all"),
                    )
                    .clicked()
                {
                    model.prompt_picker.category = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(model.t("prompt.new")).clicked() {
                        let category = model.prompt_picker.category.unwrap_or(Category::General);
                        model.new_prompt(category);
                    }
                });
            });

            let entries = model.visible_prompts();
            let mut selected: Option<String> = None;
            let mut copy: Option<String> = None;
            let mut reset: Option<String> = None;
            let mut delete: Option<String> = None;
            let current = model.prompt_editor.id.clone();
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .auto_shrink([false, true])
                .id_salt("prompt_editor_list")
                .show(ui, |ui| {
                    for entry in &entries {
                        let id = entry.prompt.id.clone();
                        let overridden = model.prompts().is_overridden(&id);
                        let mut label = entry.prompt.title.clone();
                        if !entry.builtin {
                            label.push_str(&format!("  ({})", model.t("prompt.badge.mine")));
                        } else if overridden {
                            label.push_str(&format!("  ({})", model.t("prompt.badge.edited")));
                        }
                        if ui
                            .selectable_label(current.as_deref() == Some(id.as_str()), label)
                            .clicked()
                        {
                            selected = Some(id.clone());
                        }
                        ui.horizontal(|ui| {
                            if entry.builtin {
                                if ui.small_button(model.t("prompt.copy")).clicked() {
                                    copy = Some(id.clone());
                                }
                            } else {
                                if ui.small_button(model.t("prompt.reset")).clicked() {
                                    reset = Some(id.clone());
                                }
                                if ui.small_button(model.t("prompt.delete")).clicked() {
                                    delete = Some(id.clone());
                                }
                            }
                        });
                    }
                });
            if let Some(id) = selected {
                model.edit_prompt(&id);
            }
            if let Some(id) = copy
                && let Err(error) = model.copy_prompt_to_mine(&id)
            {
                model.report_error(error);
            }
            if let Some(id) = reset
                && let Err(error) = model.reset_prompt(&id)
            {
                model.report_error(error);
            }
            if let Some(id) = delete
                && let Err(error) = model.delete_prompt(&id)
            {
                model.report_error(error);
            }

            ui.separator();
            // ---- the editor ------------------------------------------------------
            if model.prompt_editor.id.is_none() {
                ui.label(
                    egui::RichText::new(model.t("prompt.editor.none"))
                        .italics()
                        .color(palette.hidden),
                );
                return;
            }
            ui.label(
                egui::RichText::new(model.t("prompt.editor.title"))
                    .strong()
                    .color(palette.accent),
            );
            egui::Grid::new("prompt_editor_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(model.t("prompt.field.title"));
                    ui.add(
                        egui::TextEdit::singleline(&mut model.prompt_editor.title)
                            .desired_width(340.0),
                    );
                    ui.end_row();
                    ui.label(model.t("prompt.field.about"));
                    ui.add(
                        egui::TextEdit::singleline(&mut model.prompt_editor.about)
                            .desired_width(340.0),
                    );
                    ui.end_row();
                    if model.prompt_editor.creating {
                        ui.label(model.t("prompt.field.category"));
                        egui::ComboBox::from_id_salt("prompt_editor_category")
                            .selected_text(model.t(model.prompt_editor.category.label_key()))
                            .show_ui(ui, |ui| {
                                for category in Category::ALL {
                                    let label = model.t(category.label_key());
                                    ui.selectable_value(
                                        &mut model.prompt_editor.category,
                                        category,
                                        label,
                                    );
                                }
                            });
                        ui.end_row();
                    }
                });
            ui.label(model.t("prompt.field.text"));
            ui.add(
                egui::TextEdit::multiline(&mut model.prompt_editor.text)
                    .desired_rows(5)
                    .desired_width(f32::INFINITY),
            );
            // What the variables will be replaced with, so nobody has to guess.
            ui.label(
                egui::RichText::new(model.t("prompt.field.variables"))
                    .small()
                    .color(palette.hidden),
            );
            let variables = rc_core::prompt::variables(&model.prompt_editor.text);
            if variables.is_empty() {
                ui.label(egui::RichText::new(model.t("prompt.field.variables.none")).small());
            } else {
                let values = model.prompt_values();
                for name in variables {
                    let value = values
                        .get(&name)
                        .cloned()
                        .unwrap_or_else(|| model.t("prompt.field.variables.missing").to_string());
                    ui.label(
                        egui::RichText::new(format!("{{{{{name}}}}} → {value}"))
                            .small()
                            .monospace(),
                    );
                }
            }
            if let Some(target) = model.prompt_reset_target() {
                ui.separator();
                ui.label(
                    egui::RichText::new(model.t("prompt.editor.original"))
                        .small()
                        .color(palette.hidden),
                );
                ui.label(egui::RichText::new(&target.text).small().italics());
            }
            if !model.prompt_editor.status.is_empty() {
                ui.label(
                    egui::RichText::new(&model.prompt_editor.status)
                        .small()
                        .color(palette.accent),
                );
            }
            ui.horizontal(|ui| {
                if ui.button(model.t("prompt.save")).clicked() {
                    match model.save_edited_prompt() {
                        Ok(_) => {}
                        Err(error) => model.report_error(error),
                    }
                }
                if ui.button(model.t("dialog.config.cancel")).clicked() {
                    model.clear_prompt_editor();
                }
            });
            ui.label(
                egui::RichText::new(model.t("prompt.page.where"))
                    .small()
                    .italics()
                    .color(palette.hidden),
            );
        });
}

/// The security page: who may do what, what is pre-approved, and what the log says.
///
/// Everything on the page is a *policy* control, so it applies to the agent — not to the human
/// using the panels. The two live controls (revoking session approvals and the audit state) act on
/// the running model, because a safety button that waits for OK would be useless.
/// The plugin manager: what is installed, what each one may do, and which are on.
///
/// The page answers the three questions a user actually has, in that order: *what did I install*,
/// *what is it allowed to do*, and *is it running*. Switching one off is immediate — the source is
/// unregistered, so its panels and its capabilities disappear rather than waiting for a restart.
fn plugins_page(ui: &mut egui::Ui, model: &mut Model) {
    let palette = crate::theme::Palette::for_model(model);
    let mut toggle: Option<(String, bool)> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("plugins_scroll")
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(model.t("plugins.heading"))
                    .strong()
                    .color(palette.accent),
            );
            ui.label(
                egui::RichText::new(model.t("plugins.hint"))
                    .small()
                    .color(palette.hidden),
            );
            ui.add_space(6.0);
            if model.plugin_entries().is_empty() {
                ui.label(model.t("plugins.none"));
            }
            // Collected first: the list borrows the model that the toggle needs.
            let entries: Vec<(String, String, String, String, bool, bool, String)> = model
                .plugin_entries()
                .iter()
                .map(|plugin| {
                    (
                        plugin.id.clone(),
                        plugin.name.clone(),
                        plugin.version.clone(),
                        plugin.description.clone(),
                        plugin.enabled,
                        plugin.is_read_only(),
                        plugin.backend.to_string(),
                    )
                })
                .collect();
            for (id, name, version, description, enabled, read_only, backend) in entries {
                ui.separator();
                ui.horizontal(|ui| {
                    let mut on = enabled;
                    // A checkbox, because the question is yes/no and the answer takes effect now.
                    if ui.checkbox(&mut on, model.t("plugins.enabled")).changed() {
                        toggle = Some((id.clone(), on));
                    }
                    ui.label(egui::RichText::new(&name).strong());
                    ui.label(
                        egui::RichText::new(format!("{id}://"))
                            .small()
                            .color(palette.hidden),
                    );
                    if !version.trim().is_empty() {
                        ui.label(
                            egui::RichText::new(format!("v{version}"))
                                .small()
                                .color(palette.hidden),
                        );
                    }
                    ui.label(egui::RichText::new(backend).small().color(palette.hidden));
                });
                if !description.trim().is_empty() {
                    ui.label(egui::RichText::new(description).small());
                }
                // What the place allows, said in words before the switch is touched.
                let capabilities = if read_only {
                    model.t("plugins.capabilities.readonly")
                } else {
                    model.t("plugins.capabilities.writable")
                };
                ui.label(
                    egui::RichText::new(capabilities)
                        .small()
                        .color(if read_only {
                            palette.hidden
                        } else {
                            palette.readonly
                        }),
                );
            }
            // A manifest that did not load is worth showing here: this is the page where somebody
            // would come to look for the plugin that "does not appear".
            if !model.plugin_problems.is_empty() {
                ui.separator();
                ui.label(
                    egui::RichText::new(model.t("plugins.problems"))
                        .strong()
                        .color(palette.readonly),
                );
                for problem in &model.plugin_problems {
                    ui.label(
                        egui::RichText::new(problem.to_string())
                            .small()
                            .color(palette.readonly),
                    );
                }
            }
        });
    if let Some((id, enabled)) = toggle {
        let summary = model.set_plugin_enabled(&id, enabled);
        model.report(summary);
    }
}

fn security_page(ui: &mut egui::Ui, model: &mut Model, state: &mut OptionsState) {
    let palette = crate::theme::Palette::for_model(model);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("security_scroll")
        .show(ui, |ui| {
            // ---- the profile ----------------------------------------------------
            ui.label(
                egui::RichText::new(model.t("agent.security.profile"))
                    .strong()
                    .color(palette.accent),
            );
            ui.horizontal_wrapped(|ui| {
                for (name, key) in [
                    ("sandbox", "agent.profile.sandbox"),
                    ("user", "agent.profile.user"),
                    ("privileged", "agent.profile.privileged"),
                ] {
                    if ui
                        .selectable_label(state.draft.agent.profile == name, model.t(key))
                        .clicked()
                    {
                        state.draft.agent.profile = name.to_string();
                    }
                }
            });
            let profile_key = match state.draft.agent.profile.as_str() {
                "user" => "agent.security.profile.user.help",
                "privileged" => "agent.security.profile.privileged.help",
                _ => "agent.security.profile.sandbox.help",
            };
            ui.label(
                egui::RichText::new(model.t(profile_key))
                    .small()
                    .color(palette.hidden),
            );
            ui.separator();

            // ---- what needs an approval -----------------------------------------
            ui.label(
                egui::RichText::new(model.t("agent.security.approvals"))
                    .strong()
                    .color(palette.accent),
            );
            ui.label(
                egui::RichText::new(model.t("agent.security.approvals.help"))
                    .small()
                    .color(palette.hidden),
            );
            for operation in [
                rc_core::policy::Operation::Execute,
                rc_core::policy::Operation::Publish,
                rc_core::policy::Operation::Network,
                rc_core::policy::Operation::Privileged,
                rc_core::policy::Operation::Delete,
                rc_core::policy::Operation::Write,
            ] {
                let name = crate::model::policy::operation_name(operation);
                let mut checked = state
                    .draft
                    .agent
                    .require_approval
                    .iter()
                    .any(|entry| entry == name);
                if ui
                    .checkbox(&mut checked, model.t(operation.key()))
                    .changed()
                {
                    state
                        .draft
                        .agent
                        .require_approval
                        .retain(|entry| entry != name);
                    if checked {
                        state.draft.agent.require_approval.push(name.to_string());
                    }
                }
            }
            ui.separator();

            // ---- permanent pre-approvals ----------------------------------------
            ui.label(
                egui::RichText::new(model.t("agent.security.always"))
                    .strong()
                    .color(palette.accent),
            );
            ui.label(
                egui::RichText::new(model.t("agent.security.always.help"))
                    .small()
                    .color(palette.hidden),
            );
            if state.draft.agent.preapproved.is_empty() {
                ui.label(
                    egui::RichText::new(model.t("agent.security.always.none"))
                        .italics()
                        .color(palette.hidden),
                );
            }
            let mut revoke: Option<String> = None;
            for entry in &state.draft.agent.preapproved {
                ui.horizontal(|ui| {
                    let name = rc_core::policy::parse_operation(&entry.operation)
                        .map(|operation| model.t(operation.key()))
                        .unwrap_or_else(|| entry.operation.clone());
                    ui.label(egui::RichText::new(name).strong());
                    if !entry.label.is_empty() {
                        ui.label(
                            egui::RichText::new(&entry.label)
                                .small()
                                .color(palette.hidden),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(
                                egui::RichText::new(model.t("agent.security.always.revoke"))
                                    .color(palette.warning),
                            )
                            .clicked()
                        {
                            revoke = Some(entry.operation.clone());
                        }
                        if entry.created > 0 {
                            ui.label(
                                egui::RichText::new(date_text(entry.created))
                                    .small()
                                    .color(palette.hidden),
                            );
                        }
                    });
                });
            }
            if let Some(operation) = revoke {
                state
                    .draft
                    .agent
                    .preapproved
                    .retain(|entry| entry.operation != operation);
                model.settings.agent.preapproved = state.draft.agent.preapproved.clone();
                if let Some(parsed) = rc_core::policy::parse_operation(&operation) {
                    model.revoke_preapproval(parsed);
                }
            }
            ui.separator();

            // ---- the live controls ----------------------------------------------
            ui.label(
                egui::RichText::new(model.t("agent.security.session"))
                    .strong()
                    .color(palette.accent),
            );
            let session: Vec<String> = [
                rc_core::policy::Operation::Read,
                rc_core::policy::Operation::Write,
                rc_core::policy::Operation::Create,
                rc_core::policy::Operation::Delete,
                rc_core::policy::Operation::Rename,
                rc_core::policy::Operation::Execute,
                rc_core::policy::Operation::Publish,
                rc_core::policy::Operation::Network,
                rc_core::policy::Operation::Privileged,
            ]
            .into_iter()
            .filter(|operation| model.policy().is_approved_this_session(*operation))
            .map(|operation| model.t(operation.key()))
            .collect();
            if session.is_empty() {
                ui.label(
                    egui::RichText::new(model.t("agent.security.session.none"))
                        .italics()
                        .color(palette.hidden),
                );
            } else {
                ui.horizontal_wrapped(|ui| {
                    for name in &session {
                        ui.label(egui::RichText::new(name).strong());
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui
                    .button(
                        egui::RichText::new(model.t("agent.security.session.revoke"))
                            .color(palette.warning),
                    )
                    .clicked()
                {
                    let count = model.revoke_agent_grants();
                    let text = model.tf(
                        "agent.security.session.revoked",
                        &[("count", &count.to_string())],
                    );
                    model.report(text);
                }
                ui.checkbox(
                    &mut state.draft.agent.audit,
                    model.t("agent.security.audit"),
                );
            });
            ui.separator();

            // ---- the audit trail -------------------------------------------------
            ui.label(
                egui::RichText::new(model.t("agent.security.log"))
                    .strong()
                    .color(palette.accent),
            );
            let chain = model.audit_state();
            let events = model.audit().read();
            ui.label(
                egui::RichText::new(model.tf(
                    "agent.security.log.events",
                    &[("count", &events.len().to_string())],
                ))
                .small()
                .color(palette.hidden),
            );
            let (text, colour) = match chain {
                rc_core::audit::ChainState::Intact { events } => (
                    model.tf("agent.security.log.ok", &[("count", &events.to_string())]),
                    palette.foreground,
                ),
                rc_core::audit::ChainState::Broken { line } => (
                    model.tf("agent.security.log.broken", &[("line", &line.to_string())]),
                    palette.warning,
                ),
                rc_core::audit::ChainState::Empty => {
                    (model.t("agent.security.log.missing"), palette.hidden)
                }
            };
            ui.label(egui::RichText::new(text).color(colour));
            if let Some(event) = events.last() {
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {} · {} · {}",
                        date_text(event.time),
                        event.origin,
                        event.operation,
                        event.verdict.as_str()
                    ))
                    .small()
                    .monospace()
                    .color(palette.hidden),
                );
            }
        });
}

/// A short local date for the audit and pre-approval rows.
fn date_text(unix_seconds: i64) -> String {
    match chrono::DateTime::from_timestamp(unix_seconds, 0) {
        Some(utc) => utc
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        None => String::new(),
    }
}

fn options(ui: &mut egui::Ui, model: &mut Model, state: &mut OptionsState) -> DialogAction {
    let palette = crate::theme::Palette::for_model(model);
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
            ui.separator();
            ui.label(
                egui::RichText::new(model.t("sounds.title"))
                    .strong()
                    .color(palette.accent),
            );
            ui.label(
                egui::RichText::new(model.t("sounds.help"))
                    .small()
                    .color(palette.hidden),
            );
            ui.checkbox(&mut state.draft.sounds.enabled, model.t("sounds.enabled"));
            ui.add_enabled_ui(state.draft.sounds.enabled, |ui| {
                ui.checkbox(&mut state.draft.sounds.on_error, model.t("sounds.error"));
                ui.checkbox(
                    &mut state.draft.sounds.on_disconnect,
                    model.t("sounds.disconnect"),
                );
                ui.checkbox(
                    &mut state.draft.sounds.on_agent_done,
                    model.t("sounds.agent_done"),
                );
            });
            if state.draft.sounds.enabled && ui.button(model.t("sounds.test")).clicked() {
                // The test button is the only honest way to answer "will I hear it?": it plays
                // exactly what an error would play.
                crate::system_open::play_sound(rc_core::desktop::Sound::Problem);
            }
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
            colors_page(ui, model, state);
        }
        OptionsTab::Fonts => {
            fonts_page(ui, model, state);
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
        OptionsTab::Security => {
            security_page(ui, model, state);
        }
        OptionsTab::Prompts => {
            prompts_page(ui, model);
        }
        OptionsTab::Plugins => {
            plugins_page(ui, model);
        }
        OptionsTab::Dangerous => {
            dangerous_page(ui, model, state);
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
        model.settings.fonts.sanitise();
        model.dialogs.clear();
        // A changed profile or pre-approval list has to take effect now, not after a restart.
        model.refresh_policy();
        return DialogAction::Close;
    }
    DialogAction::None
}

fn ftp(ui: &mut egui::Ui, model: &mut Model, state: &mut FtpConnectState) -> DialogAction {
    // Switching the protocol moves the port to that protocol's default, but only
    // while the shown port is still a default itself.
    let mut protocol_choice: Option<&'static str> = None;
    ui.horizontal(|ui| {
        ui.label("protocol");
        if ui
            .selectable_label(state.connection.protocol == "ftp", "FTP")
            .clicked()
        {
            protocol_choice = Some("ftp");
        }
        if ui
            .selectable_label(state.connection.protocol == "sftp", "SFTP")
            .clicked()
        {
            protocol_choice = Some("sftp");
        }
        // Show what will be used, so nobody has to guess the default.
        ui.label(
            egui::RichText::new(format!(
                "{}:{}",
                state.connection.host,
                SavedConnection::default_port(&state.connection.protocol)
            ))
            .small(),
        );
    });
    if let Some(protocol) = protocol_choice {
        state.set_protocol(protocol);
    }
    sessions_section(ui, model, state);
    egui::Grid::new("ftp_grid").num_columns(2).show(ui, |ui| {
        ui.label(model.t("ftp.host"));
        ui.add(
            egui::TextEdit::singleline(&mut state.connection.host)
                .hint_text("localhost")
                .desired_width(220.0),
        );
        ui.end_row();
        ui.label(model.t("ftp.port"));
        ui.horizontal(|ui| {
            // `default_port` is pre-selected, so connecting to a local server
            // needs no port at all; the hint names the default for the protocol.
            let default_port = SavedConnection::default_port(&state.connection.protocol);
            ui.add(egui::DragValue::new(&mut state.connection.port).range(0..=65535))
                .on_hover_text(default_port.to_string());
            if ui
                .small_button(format!("{default_port}"))
                .on_hover_text(model.t("ftp.port_default"))
                .clicked()
            {
                state.connection.port = default_port;
            }
        });
        ui.end_row();
        ui.label(model.t("ftp.user"));
        ui.add(
            egui::TextEdit::singleline(&mut state.connection.user)
                .hint_text(model.t("ftp.user_hint"))
                .desired_width(220.0),
        );
        ui.end_row();
        ui.label(model.t("ftp.password"));
        ui.add(
            egui::TextEdit::singleline(&mut state.connection.password)
                .password(true)
                .hint_text(model.t("ftp.password_hint"))
                .desired_width(220.0),
        );
        ui.end_row();
        ui.label(model.t("ftp.remote_dir"));
        ui.add(
            egui::TextEdit::singleline(&mut state.connection.remote_dir)
                .hint_text("/")
                .desired_width(220.0),
        );
        ui.end_row();
        if state.is_ssh() {
            ui.label("authentication");
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut state.connection.auth_method,
                    "password".to_string(),
                    "password",
                );
                ui.selectable_value(
                    &mut state.connection.auth_method,
                    "key".to_string(),
                    "key file",
                );
                ui.selectable_value(
                    &mut state.connection.auth_method,
                    "agent".to_string(),
                    "ssh-agent",
                );
            });
            ui.end_row();
            if state.connection.auth_method == "key" {
                ui.label("key file");
                ui.add(
                    egui::TextEdit::singleline(&mut state.connection.key_path)
                        .desired_width(220.0)
                        .hint_text("~/.ssh/id_ed25519"),
                );
                ui.end_row();
            }
            ui.label("host key");
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut state.connection.host_key_policy,
                    "tofu".to_string(),
                    "trust on first use",
                );
                ui.selectable_value(
                    &mut state.connection.host_key_policy,
                    "strict".to_string(),
                    "strict",
                );
                ui.selectable_value(
                    &mut state.connection.host_key_policy,
                    "ignore".to_string(),
                    "ignore",
                );
            });
            ui.end_row();
        }
    });
    ui.checkbox(&mut state.connection.passive, model.t("ftp.passive"));
    ui.checkbox(&mut state.save_password, "remember the password");
    ui.label(state.summary());
    let (ok, cancel) = buttons(ui, model);
    if cancel {
        return DialogAction::Close;
    }
    if ok {
        if let Err(message) = state.validate() {
            state.status = message.clone();
            model.report_error(message);
            return DialogAction::None;
        }
        let connection = state.connection.clone();
        let name = if state.session_name.trim().is_empty() {
            connection.host.clone()
        } else {
            state.session_name.trim().to_string()
        };
        let session = rc_core::sessions::Session::from_connection(name, &connection);
        // Remembering the password is opt-in.
        let _ = state.save_password;
        match model.start_connect(&connection, &session) {
            crate::model::CommandOutcome::Ok => {
                // The attempt runs in the background: the dialog closes now and
                // the panel fills in when `poll_connect` reports the result.
                model.close_dialogs();
                return DialogAction::Close;
            }
            crate::model::CommandOutcome::Failed(message) => {
                // Keep the dialog open so the password can be corrected.
                state.status = message.clone();
                model.report_error(message);
                return DialogAction::None;
            }
            _ => return DialogAction::None,
        }
    }
    DialogAction::None
}

/// The saved-session list inside the connect dialog: load, save, delete and
/// import from an OpenSSH `ssh_config` file.
fn sessions_section(ui: &mut egui::Ui, model: &mut Model, state: &mut FtpConnectState) {
    let names = model.session_names();
    egui::CollapsingHeader::new(model.t("ftp.sessions"))
        .default_open(!names.is_empty())
        .id_salt("ftp_sessions")
        .show(ui, |ui| {
            if names.is_empty() {
                ui.label(egui::RichText::new(model.t("ftp.sessions.none")).small());
            } else {
                // A long list would push the buttons off the dialog, so it gets
                // its own scroller.
                egui::ScrollArea::vertical()
                    .max_height(110.0)
                    .id_salt("ftp_session_list")
                    .show(ui, |ui| {
                        for name in &names {
                            if let Some(session) = model.session(name) {
                                let selected = state.session_choice == *name;
                                let response = ui
                                    .selectable_label(selected, session.summary())
                                    .on_hover_text(session.address());
                                if response.clicked() {
                                    state.session_choice = name.clone();
                                    state.session_name = name.clone();
                                    state.connection = session.to_connection();
                                    state.status = String::new();
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button(model.t("ftp.sessions.load")).clicked()
                        && let Some(session) = model.session(&state.session_choice).cloned()
                    {
                        state.connection = session.to_connection();
                        state.session_name = session.name.clone();
                        state.session_status =
                            model.tf("ftp.sessions.loaded", &[("name", &session.name)]);
                    }
                    if ui
                        .add_enabled(
                            !state.session_choice.is_empty(),
                            egui::Button::new(model.t("ftp.sessions.delete")),
                        )
                        .clicked()
                    {
                        let name = state.session_choice.clone();
                        match model.delete_session(&name) {
                            Ok(true) => {
                                state.session_choice.clear();
                                state.session_status =
                                    model.tf("ftp.sessions.deleted", &[("name", &name)]);
                            }
                            Ok(false) => {}
                            Err(message) => state.session_status = message,
                        }
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(model.t("ftp.sessions.name"));
                ui.add(
                    egui::TextEdit::singleline(&mut state.session_name)
                        .hint_text(state.connection.host.clone())
                        .desired_width(180.0),
                );
                if ui.button(model.t("ftp.sessions.save")).clicked() {
                    if let Err(message) = state.validate() {
                        state.session_status = message;
                    } else {
                        let connection = state.connection.clone();
                        match model.save_current_as_session(&state.session_name, &connection) {
                            Ok(name) => {
                                state.session_choice = name.clone();
                                state.session_name = name.clone();
                                state.session_status =
                                    model.tf("ftp.sessions.saved", &[("name", &name)]);
                            }
                            Err(message) => state.session_status = message,
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.ssh_config_path)
                        .desired_width(220.0)
                        .hint_text("~/.ssh/config"),
                );
                if ui.button(model.t("ftp.sessions.import")).clicked() {
                    let path = std::path::PathBuf::from(state.ssh_config_path.trim());
                    match model.import_ssh_config(&path) {
                        Ok(count) => {
                            let count = count.to_string();
                            state.session_status =
                                model.tf("ftp.sessions.imported", &[("count", &count)]);
                        }
                        Err(message) => state.session_status = message,
                    }
                }
            });
            if !state.session_status.is_empty() {
                ui.label(egui::RichText::new(&state.session_status).small());
            }
        });
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
                    .selectable_label(selected, format!("{} — {}", crate::model::actions::button_caption(model, button), button.command))
                    .clicked()
                {
                    state.edit(index, button);
                }
            }
        });
    ui.separator();
    ui.menu_button(model.t("toolbar.choose_action"), |ui| {
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for &(group, actions) in crate::model::actions::ACTION_GROUPS {
                ui.collapsing(model.t(group), |ui| {
                    for &(command, key) in actions {
                        if ui.button(model.t(key)).clicked() {
                            state.edit_command = command.to_owned();
                            state.edit_caption.clear();
                            state.edit_parameters.clear();
                            ui.close();
                        }
                    }
                });
            }
        });
    });
    egui::Grid::new("buttonbar_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(model.t("toolbar.caption"));
            ui.add(egui::TextEdit::singleline(&mut state.edit_caption).desired_width(260.0));
            ui.end_row();
            ui.label(model.t("toolbar.command"));
            ui.add(
                egui::TextEdit::singleline(&mut state.edit_command)
                    .desired_width(260.0)
                    .hint_text("cm_Copy"),
            );
            ui.end_row();
            ui.label(model.t("toolbar.parameters"));
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
        if ui.button(model.t("toolbar.apply")).clicked() {
            let mut snapshot = state.clone();
            if let crate::model::CommandOutcome::Failed(message) =
                model.apply_button_bar(&mut snapshot)
            {
                model.report_error(message);
            }
            *state = snapshot;
        }
        if ui.button(model.t("toolbar.remove")).clicked() {
            let mut snapshot = state.clone();
            let _ = model.remove_button_bar(&mut snapshot);
            *state = snapshot;
        }
        if ui.button(model.t("toolbar.up")).clicked() {
            let mut snapshot = state.clone();
            let _ = model.move_button_bar(&mut snapshot, true);
            *state = snapshot;
        }
        if ui.button(model.t("toolbar.down")).clicked() {
            let mut snapshot = state.clone();
            let _ = model.move_button_bar(&mut snapshot, false);
            *state = snapshot;
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button(model.t("toolbar.save_custom")).clicked() {
            model.save_custom_button_bar();
        }
        if ui.button(model.t("toolbar.reset_default")).clicked() {
            model.reset_button_bar();
            *state = ButtonBarState::new();
        }
        let restore = model.t("toolbar.restore_custom");
        if ui.add_enabled(model.settings.button_bar_custom.is_some(), egui::Button::new(restore)).clicked() {
            model.restore_custom_button_bar();
            *state = ButtonBarState::new();
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

fn encode_dialog(ui: &mut egui::Ui, model: &mut Model, state: &mut EncodeState) -> DialogAction {
    ui.label(model.tf(
        "panel.selected",
        &[("count", &state.sources.len().to_string())],
    ));
    ui.horizontal(|ui| {
        ui.label("encoding");
        for id in ["MIME", "UUE", "XXE"] {
            ui.selectable_value(&mut state.encoding, id.to_string(), id);
        }
    });
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.apply")).clicked() {
            action = DialogAction::Command("rc_RunEncode".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn decode_dialog(ui: &mut egui::Ui, model: &mut Model, state: &mut DecodeState) -> DialogAction {
    ui.label(&state.source);
    ui.horizontal(|ui| {
        ui.label("target name");
        ui.add(
            egui::TextEdit::singleline(&mut state.target_name)
                .desired_width(280.0)
                .hint_text("from the envelope"),
        );
    });
    if state.decoded_bytes > 0 {
        ui.label(model.tf(
            "status.done",
            &[("count", &state.decoded_bytes.to_string())],
        ));
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.apply")).clicked() {
            action = DialogAction::Command("rc_RunDecode".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn save_selection(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut SaveSelectionState,
) -> DialogAction {
    ui.label(model.tf("panel.selected", &[("count", &state.count.to_string())]));
    ui.horizontal(|ui| {
        ui.label(model.t("col.name"));
        ui.add(egui::TextEdit::singleline(&mut state.file_name).desired_width(260.0));
    });
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.apply")).clicked() {
            action = DialogAction::Command("rc_RunSaveSelection".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn load_selection(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut LoadSelectionState,
) -> DialogAction {
    ui.horizontal(|ui| {
        ui.label(model.t("col.name"));
        ui.add(egui::TextEdit::singleline(&mut state.file_name).desired_width(260.0));
    });
    if state.loaded > 0 {
        ui.label(model.tf(
            "panel.selection_of",
            &[("count", &state.loaded.to_string())],
        ));
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.apply")).clicked() {
            action = DialogAction::Command("rc_RunLoadSelection".to_string());
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

fn unlock_secrets(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut UnlockSecretsState,
) -> DialogAction {
    ui.label(model.t(state.title()));
    if state.is_create() && state.existing {
        ui.label(egui::RichText::new(model.t("dialog.secrets.start_over_warning")).small());
    }

    // Passphrase. Enter in the first field moves to the confirmation field when
    // creating, otherwise it submits.
    ui.label(model.t("dialog.secrets.passphrase"));
    let first = ui.add(
        egui::TextEdit::singleline(&mut state.passphrase)
            .password(true)
            .desired_width(f32::INFINITY)
            .hint_text(model.t("dialog.secrets.passphrase")),
    );
    if first.gained_focus() || state.passphrase.is_empty() {
        first.request_focus();
    }

    let mut submit = false;
    if state.is_create() {
        ui.label(model.t("dialog.secrets.confirm"));
        let second = ui.add(
            egui::TextEdit::singleline(&mut state.confirm)
                .password(true)
                .desired_width(f32::INFINITY)
                .hint_text(model.t("dialog.secrets.confirm")),
        );
        if second.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            submit = true;
        }
        if first.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            // Move the focus to the confirmation field instead of submitting.
            second.request_focus();
        }
        ui.label(egui::RichText::new(model.t("dialog.secrets.warning")).small());
    } else if first.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        submit = true;
    }

    if let Some(error) = &state.error {
        ui.label(egui::RichText::new(error).color(egui::Color32::from_rgb(0xa0, 0x00, 0x00)));
    }

    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("dialog.config.ok")).clicked() {
            submit = true;
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
        if state.existing
            && !state.force_create
            && ui.button(model.t("dialog.secrets.start_over")).clicked()
        {
            state.start_over();
        }
    });
    if action == DialogAction::Close {
        return action;
    }
    if submit {
        if let Err(message) = state.validate() {
            state.error = Some(message);
            return DialogAction::None;
        }
        let passphrase = state.passphrase.clone();
        // "Start over" means: remove the old encrypted file first.
        if state.force_create && state.existing {
            model.reset_secret_store();
        }
        match model.unlock_secrets(&passphrase) {
            Ok(()) => {
                model.close_dialogs();
                return DialogAction::Close;
            }
            Err(message) => {
                model.report_error(&message);
                state.error = Some(message);
                return DialogAction::None;
            }
        }
    }
    DialogAction::None
}

/// Pick the model the agent talks to.
fn ai_model(ui: &mut egui::Ui, model: &mut Model, state: &mut AiModelState) -> DialogAction {
    let palette = crate::theme::Palette::for_model(model);
    let url = model.settings.agent.provider_url.clone();
    ui.horizontal(|ui| {
        ui.label(model.t("ai.provider"));
        ui.label(
            egui::RichText::new(format!("{} · {url}", model.settings.agent.provider_kind))
                .small()
                .color(palette.hidden),
        );
        if ui.button(model.t("ai.refresh")).clicked() {
            model.refresh_ai_models();
        }
    });

    let wait_label = model.t("ai.wait.interval");
    ui.horizontal(|ui| {
        ui.label(wait_label);
        if ui
            .add(
                egui::DragValue::new(&mut model.settings.agent.response_wait_seconds)
                    .range(1..=3600)
                    .suffix(" s"),
            )
            .changed()
        {
            model.mark_settings_dirty();
        }
    });
    let models = model.ai_models().to_vec();
    let current = model.ai_model().map(|name| name.to_string());
    if let Some(error) = model.ai_models.error.clone() {
        ui.label(egui::RichText::new(error).color(palette.warning));
    }
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .id_salt("ai_models")
        .show(ui, |ui| {
            if models.is_empty() {
                ui.label(model.t("ai.models.none"));
            }
            for (index, entry) in models.iter().enumerate() {
                let chosen = current.as_deref() == Some(entry.id.as_str());
                let marker = if chosen { "✓ " } else { "" };
                let label = format!("{marker}{}", entry.describe());
                if ui
                    .selectable_label(state.selected == Some(index), label)
                    .clicked()
                {
                    state.selected = Some(index);
                }
            }
        });
    if let Some(reply) = model.last_ai_reply.clone() {
        ui.separator();
        ui.label(
            egui::RichText::new(model.tf("ai.last_reply", &[("model", &reply.model)]))
                .small()
                .color(palette.hidden),
        );
        ui.label(
            egui::RichText::new(reply.content.trim())
                .italics()
                .color(palette.foreground),
        );
    }
    if !state.status.is_empty() {
        ui.label(egui::RichText::new(&state.status).color(palette.warning));
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        let picked = state
            .selected
            .and_then(|index| models.get(index))
            .map(|entry| entry.id.clone());
        if ui
            .add_enabled(picked.is_some(), egui::Button::new(model.t("ai.use")))
            .clicked()
            && let Some(name) = picked
        {
            if model.select_ai_model(&name) {
                state.status = model.tf("ai.selected", &[("model", &name)]);
            } else {
                state.status = model.tf("ai.model.unknown", &[("model", &name)]);
            }
        }
        if ui
            .add_enabled(
                model.ai_model().is_some() && !model.is_testing_model(),
                egui::Button::new(model.t("ai.test")),
            )
            .clicked()
        {
            model.test_ai_model();
            state.status = model.t("ai.testing_started");
        }
        if ui.button(model.t("dialog.config.ok")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

/// Ask the user to allow one side-effecting operation.
fn agent_approval(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut AgentApprovalState,
) -> DialogAction {
    let palette = crate::theme::Palette::for_model(model);
    let pending = state.pending.clone();
    ui.label(
        egui::RichText::new(model.tf("agent.approval.origin", &[("origin", &pending.origin)]))
            .strong(),
    );
    if state.bridge_id.is_some() {
        // A blocked agent is worth saying out loud: the user may not know anything is waiting.
        ui.label(
            egui::RichText::new(model.t("agent.bridge.waiting"))
                .small()
                .color(palette.accent),
        );
    }
    egui::Grid::new("agent_approval_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(model.t("agent.approval.operation"));
            ui.label(
                egui::RichText::new(model.t(operation_key(pending.operation)))
                    .color(palette.accent)
                    .strong(),
            );
            ui.end_row();
            ui.label(model.t("agent.approval.target"));
            ui.label(egui::RichText::new(&pending.target).monospace().small());
            ui.end_row();
            if !pending.destination.is_empty() {
                ui.label(model.t("agent.approval.destination"));
                ui.label(
                    egui::RichText::new(&pending.destination)
                        .monospace()
                        .small(),
                );
                ui.end_row();
            }
        });
    if !pending.explanation.is_empty() {
        ui.separator();
        ui.label(model.t("agent.approval.why"));
        ui.label(egui::RichText::new(&pending.explanation).italics());
    }

    // What saying yes actually permits, in plain words.
    ui.separator();
    ui.label(model.t("agent.approval.consequences"));
    for key in pending.consequence_keys() {
        let colour = if key == "agent.risk.irreversible" {
            palette.warning
        } else {
            palette.foreground
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("•").color(palette.hidden));
            ui.label(egui::RichText::new(model.t(key)).color(colour));
        });
    }

    if state.confirming_always {
        return always_confirmation(ui, model, state, &palette);
    }

    if !state.status.is_empty() {
        ui.label(egui::RichText::new(&state.status).color(palette.warning));
    }
    ui.separator();
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new(model.t("agent.approval.allow_once")).fill(palette.accent))
            .clicked()
        {
            model.approve_pending_with(crate::model::policy::ApprovalDecision::Once, "");
            action = DialogAction::Close;
        }
        if ui.button(model.t("agent.approval.allow_session")).clicked() {
            model.approve_pending_with(crate::model::policy::ApprovalDecision::Session, "");
            action = DialogAction::Close;
        }
        // The permanent answer is the red one: it is the only click that changes the settings.
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(model.t("agent.approval.allow_always"))
                        .color(palette.warning),
                )
                .fill(palette.status_background),
            )
            .clicked()
        {
            state.confirming_always = true;
            state.typed.clear();
        }
        if ui.button(model.t("agent.approval.refuse")).clicked() {
            model.refuse_pending();
            action = DialogAction::Close;
        }
    });
    action
}

/// The second window of "allow always": the exact scope, and a word the user has to type.
fn always_confirmation(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut AgentApprovalState,
    palette: &crate::theme::Palette,
) -> DialogAction {
    let pending = state.pending.clone();
    ui.separator();
    ui.label(
        egui::RichText::new(model.t("agent.always.title"))
            .strong()
            .color(palette.warning),
    );
    for key in [pending.always_scope_key(), "agent.always.persistence"] {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("•").color(palette.hidden));
            ui.label(model.t(key));
        });
    }
    ui.label(
        egui::RichText::new(model.t("agent.always.revocable"))
            .small()
            .color(palette.hidden),
    );
    ui.separator();
    ui.label(
        egui::RichText::new(model.tf(
            "agent.always.type_word",
            &[("word", crate::model::policy::CONFIRM_WORD)],
        ))
        .strong(),
    );
    let confirmed = crate::model::policy::confirms_always(&state.typed);
    let field = ui.add(
        egui::TextEdit::singleline(&mut state.typed)
            .desired_width(200.0)
            .hint_text(crate::model::policy::CONFIRM_WORD),
    );
    field.request_focus();
    if !state.status.is_empty() {
        ui.label(egui::RichText::new(&state.status).color(palette.warning));
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        let send = ui.add_enabled(
            confirmed,
            egui::Button::new(
                egui::RichText::new(model.t("agent.always.send")).color(palette.warning),
            ),
        );
        if send.clicked()
            || (confirmed && field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
        {
            model
                .approve_pending_with(crate::model::policy::ApprovalDecision::Always, &state.typed);
            action = DialogAction::Close;
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            state.confirming_always = false;
            state.typed.clear();
            state.status.clear();
        }
    });
    action
}

/// Localisation key of an operation name, for the dialog.
fn operation_key(operation: rc_core::policy::Operation) -> &'static str {
    operation.key()
}

/// Clone a remote into the panel's directory.
fn git_clone(ui: &mut egui::Ui, model: &mut Model, state: &mut GitCloneState) -> DialogAction {
    let palette = crate::theme::Palette::for_model(model);
    ui.label(model.t("git.clone.url"));
    ui.add(
        egui::TextEdit::singleline(&mut state.url)
            .desired_width(f32::INFINITY)
            .hint_text("https://github.com/owner/repo.git"),
    );
    ui.horizontal(|ui| {
        ui.label(model.t("git.clone.into"));
        ui.add(egui::TextEdit::singleline(&mut state.target).desired_width(220.0));
    });
    ui.horizontal(|ui| {
        ui.label(model.t("git.clone.folder"));
        ui.add(
            egui::TextEdit::singleline(&mut state.folder)
                .desired_width(180.0)
                .hint_text(crate::model::dialogs::folder_name(&state.url)),
        );
    });
    ui.label(
        egui::RichText::new(state.destination().display().to_string())
            .small()
            .color(palette.hidden),
    );
    if !state.status.is_empty() {
        ui.label(egui::RichText::new(&state.status).color(palette.warning));
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("git.clone.do")).clicked() {
            if state.url.trim().is_empty() {
                state.status = model.t("git.clone.url_required");
            } else {
                let destination = state.destination();
                match model.start_clone(&state.url, &destination) {
                    crate::model::CommandOutcome::Ok => action = DialogAction::Close,
                    crate::model::CommandOutcome::Failed(error) => state.status = error,
                    _ => {}
                }
            }
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

/// Fetch, pull, push and merge for the panel's repository.
fn git_remote(ui: &mut egui::Ui, model: &mut Model, state: &mut GitRemoteState) -> DialogAction {
    let palette = crate::theme::Palette::for_model(model);
    ui.horizontal(|ui| {
        ui.label(model.t("git.remote.branch"));
        ui.label(egui::RichText::new(&state.branch).strong());
        if !state.upstream.is_empty() {
            ui.label(
                egui::RichText::new(format!("→ {}", state.upstream))
                    .small()
                    .color(palette.hidden),
            );
        }
    });
    ui.label(
        egui::RichText::new(model.tf(
            "git.remote.tracking",
            &[
                ("ahead", &state.ahead.to_string()),
                ("behind", &state.behind.to_string()),
            ],
        ))
        .small(),
    );
    ui.separator();
    let mut action = DialogAction::None;
    ui.horizontal_wrapped(|ui| {
        if ui.button(model.t("git.remote.fetch")).clicked() {
            model.git_fetch();
            state.status = model.t("git.working");
        }
        if ui.button(model.t("git.remote.pull")).clicked() {
            model.git_pull();
            state.status = model.t("git.working");
        }
        if ui.button(model.t("git.remote.push")).clicked() {
            model.git_push();
            state.status = model.t("git.working");
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(model.t("git.remote.merge"));
        ui.add(egui::TextEdit::singleline(&mut state.merge_from).desired_width(140.0));
        if ui.button(model.t("git.remote.merge_do")).clicked() {
            let name = state.merge_from.trim().to_string();
            if name.is_empty() {
                state.status = model.t("git.branches.name_required");
            } else {
                match model.git_merge(&name) {
                    crate::model::CommandOutcome::Ok => {
                        state.status = model.status.clone();
                    }
                    crate::model::CommandOutcome::Failed(error) => state.status = error,
                    _ => {}
                }
            }
        }
    });
    if !state.status.is_empty() {
        ui.label(egui::RichText::new(&state.status).small());
    }
    action
}

/// Pick a branch of the panel's repository, or create a new one.
fn git_branches(
    ui: &mut egui::Ui,
    model: &mut Model,
    state: &mut GitBranchesState,
) -> DialogAction {
    let palette = crate::theme::Palette::for_model(model);
    ui.label(
        egui::RichText::new(state.root.display().to_string())
            .small()
            .color(palette.hidden),
    );
    if state.dirty {
        ui.label(
            egui::RichText::new(model.t("git.branches.dirty"))
                .color(palette.warning)
                .small(),
        );
    }
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .id_salt("git_branches")
        .show(ui, |ui| {
            for (index, name) in state.branches.iter().enumerate() {
                let current = *name == state.current;
                let label = if current {
                    format!("{name}  ✓")
                } else {
                    name.clone()
                };
                if ui
                    .selectable_label(state.selected == Some(index), label)
                    .clicked()
                {
                    state.selected = Some(index);
                }
            }
        });
    ui.separator();
    let mut created = false;
    ui.horizontal(|ui| {
        ui.label(model.t("git.branches.new"));
        ui.add(egui::TextEdit::singleline(&mut state.new_branch).desired_width(140.0));
        if ui.button(model.t("git.branches.create")).clicked() {
            let name = state.new_branch.trim().to_string();
            if name.is_empty() {
                state.status = model.t("git.branches.name_required");
            } else {
                match model.git_create_branch(&name) {
                    crate::model::CommandOutcome::Ok => created = true,
                    crate::model::CommandOutcome::Failed(error) => state.status = error,
                    _ => {}
                }
            }
        }
    });
    if created {
        model.close_dialogs();
        return DialogAction::Close;
    }
    if !state.status.is_empty() {
        ui.label(
            egui::RichText::new(&state.status)
                .color(palette.warning)
                .small(),
        );
    }
    let chosen = state.chosen().cloned();
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                chosen.is_some(),
                egui::Button::new(model.t("git.branches.switch")),
            )
            .clicked()
            && let Some(name) = chosen.clone()
        {
            let allow_dirty = state.allow_dirty;
            match model.git_checkout(&name, allow_dirty) {
                crate::model::CommandOutcome::Ok => action = DialogAction::Close,
                crate::model::CommandOutcome::Failed(error) => state.status = error,
                _ => {}
            }
        }
        if state.dirty {
            ui.checkbox(&mut state.allow_dirty, model.t("git.branches.force"));
        }
        if ui.button(model.t("dialog.config.cancel")).clicked() {
            action = DialogAction::Close;
        }
    });
    action
}

/// Offer to commit the file the editor just saved, and optionally push it.
fn git_commit(ui: &mut egui::Ui, model: &mut Model, state: &mut GitCommitState) -> DialogAction {
    let palette = crate::theme::Palette::for_model(model);
    ui.label(model.tf("git.commit.file", &[("name", &state.description())]));
    ui.label(
        egui::RichText::new(state.root.display().to_string())
            .small()
            .color(palette.hidden),
    );
    ui.label(model.t("git.commit.message"));
    ui.add(
        egui::TextEdit::singleline(&mut state.message)
            .desired_width(f32::INFINITY)
            .hint_text(model.t("git.commit.message")),
    );
    ui.checkbox(&mut state.push, model.t("git.commit.push"));
    ui.checkbox(&mut state.remember, model.t("git.commit.remember"));
    if !state.status.is_empty() {
        ui.label(egui::RichText::new(&state.status).color(palette.warning));
    }
    let mut action = DialogAction::None;
    ui.horizontal(|ui| {
        if ui.button(model.t("git.commit.do")).clicked() {
            if state.message.trim().is_empty() {
                state.status = model.t("git.commit.message_required");
            } else {
                if state.remember {
                    model.settings.git.commit_on_save = "always".to_string();
                    model.mark_settings_dirty();
                }
                let push = state.push;
                model.settings.git.push_after_commit = push;
                let root = state.root.clone();
                let paths = state.paths.clone();
                let message = state.message.clone();
                match model.commit_and_push(&root, &paths, &message) {
                    crate::model::CommandOutcome::Ok => action = DialogAction::Close,
                    crate::model::CommandOutcome::Failed(error) => state.status = error,
                    _ => {}
                }
            }
        }
        if ui.button(model.t("git.commit.skip")).clicked() {
            action = DialogAction::Close;
        }
        if ui.button(model.t("git.commit.never")).clicked() {
            model.settings.git.commit_on_save = "never".to_string();
            model.mark_settings_dirty();
            action = DialogAction::Close;
        }
    });
    action
}

/// Ask whether the machine's Git configuration may be read, and show exactly what
/// was found — without any password.
fn git_config(ui: &mut egui::Ui, model: &mut Model, state: &mut GitConfigState) -> DialogAction {
    let palette = crate::theme::Palette::for_model(model);
    let mut answer: Option<DialogAction> = None;
    if state.is_asking() {
        ui.label(model.t("git.ask"));
    } else {
        ui.label(egui::RichText::new(model.t("git.active")).color(palette.archive));
    }
    ui.add_space(2.0);
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .id_salt("git_config_lines")
        .show(ui, |ui| {
            let lines = state.lines();
            if lines.is_empty() {
                ui.label(model.t("git.nothing_found"));
            }
            for line in lines {
                let (key, value) = line.split_once('|').unwrap_or(("", ""));
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(model.t(key)).strong());
                    ui.label(
                        egui::RichText::new(value).color(if key.starts_with("git.problem") {
                            palette.warning
                        } else {
                            palette.foreground
                        }),
                    );
                });
            }
        });
    ui.label(egui::RichText::new(model.t("git.no_secrets")).small());
    if !state.status.is_empty() {
        ui.label(egui::RichText::new(&state.status).color(palette.warning));
    }
    ui.checkbox(&mut state.remember, model.t("git.remember"));
    ui.horizontal(|ui| {
        if ui.button(model.t("git.allow")).clicked() {
            let config = state.config.clone();
            let remember = state.remember;
            model.accept_git_config(config, remember);
            answer = Some(DialogAction::Close);
        }
        if ui.button(model.t("git.deny")).clicked() {
            let remember = state.remember;
            model.deny_git_config(remember);
            answer = Some(DialogAction::Close);
        }
    });
    if let Some(action) = answer {
        model.close_dialogs();
        return action;
    }
    if !state.is_asking() && ui.button(model.t("git.forget")).clicked() {
        model.forget_git_config();
        model.close_dialogs();
        return DialogAction::Close;
    }
    DialogAction::None
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

// Regression: a real Czech language pack must not show literal {name}/{count}.
#[cfg(test)]
mod delete_confirmation_tests {
    use super::*;
    use crate::i18n::TranslatorStrings;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    #[test]
    fn delete_question_uses_filename_or_selected_count() {
        let locales = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../locales");
        let mut model = Model::for_test(
            Settings::default(),
            Location::local("/"),
            Location::local("/"),
        );
        model.strings = Box::new(TranslatorStrings::discover(&locales, "cs"));

        let single = DeleteState::new(vec!["/tmp/žluťoučký.txt".into()], false);
        assert_eq!(
            delete_confirmation_text(&model, &single),
            "Smazat žluťoučký.txt?"
        );

        let multiple =
            DeleteState::new(vec!["/tmp/první.txt".into(), "/tmp/druhý.txt".into()], true);
        assert_eq!(
            delete_confirmation_text(&model, &multiple),
            "Smazat 2 vybraných položek?"
        );
    }
}

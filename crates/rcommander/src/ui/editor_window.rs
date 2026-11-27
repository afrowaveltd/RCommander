//! The F4 editor.
//!
//! Editing is the point of this window, so the whole of it is built around three facts:
//!
//! * **The text is the user's.** Typing goes straight into the buffer; there is no "preview mode"
//!   to discover.
//! * **Saving must not lose work.** A file that cannot be written is *never* reported as saved:
//!   the window stays open, the reason is named, and two honest ways out are offered — save a copy
//!   somewhere the user certainly can write (their documents folder), or ask the system for
//!   elevated rights. Neither is done silently.
//! * **Binary files are refused before the text turns into mojibake.** A file with NUL bytes or
//!   invalid UTF-8 is reported as binary and left alone; the F3 viewer is the place to look at it.
//!
//! The window chrome (minimize, maximize, pin, close) comes from
//! [`crate::ui::workspace_window`], so the editor behaves like the chat window and can cover both
//! panels while the user works.

use crate::model::Model;
use crate::theme::Palette;
use crate::ui::workspace_window::ChromeAction;
use egui::{Context, TextStyle};

/// Editor working state kept by the view layer.
#[derive(Default)]
pub struct EditorUi {
    pub dirty_prompt: Option<String>,
    /// Where the "save a copy" suggestion points.
    pub suggested_copy: Option<std::path::PathBuf>,
    /// Why the last save did not work, if it did not.
    pub save_error: Option<String>,
}

/// Draw the editor window; returns true when it should be closed.
pub fn show(
    ctx: &Context,
    model: &mut Model,
    ui_state: &mut EditorUi,
    palette: &Palette,
    workspace: egui::Rect,
) -> bool {
    render(ctx, None, model, ui_state, palette, workspace)
}

pub fn embedded(ui: &mut egui::Ui, model: &mut Model, ui_state: &mut EditorUi, palette: &Palette) {
    let ctx = ui.ctx().clone();
    let workspace = ui.max_rect();
    render(&ctx, Some(ui), model, ui_state, palette, workspace);
}

fn render(ctx: &Context, embedded: Option<&mut egui::Ui>, model: &mut Model, ui_state: &mut EditorUi, palette: &Palette, workspace: egui::Rect) -> bool {
    let Some(mut state) = model.editor.take() else {
        return true;
    };
    let title = format!("{} — {}", model.t("cmd.edit"), state.path);
    let mut save_requested = false;
    let mut close_requested = false;
    let mut save_copy = false;
    let mut elevate = false;

    let mut chrome = state.chrome;
    let occupied = model.viewer.as_ref().map(|s| s.placement);
    let mut discard_requested = false;
    let mut contents = |ui: &mut egui::Ui| {
            ui.label(&title);
            if crate::ui::workspace_window::document_controls(ui, model, &mut state.placement, occupied) {
                close_requested = true;
            }
            if let Some(message) = &ui_state.dirty_prompt {
                ui.label(egui::RichText::new(message).color(palette.warning));
                ui.horizontal(|ui| {
                    if ui.button(model.t("editor.discard_changes")).clicked() {
                        discard_requested = true;
                    }
                    if ui.button(model.t("dialog.config.cancel")).clicked() {
                        ui_state.dirty_prompt = None;
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button(model.t("prompt.save")).clicked() {
                    save_requested = true;
                }
                if ui.button(model.t("dialog.config.cancel")).clicked() {
                    close_requested = true;
                }
                if state.modified {
                    ui.label(
                        egui::RichText::new(model.t("editor.modified")).color(palette.readonly),
                    );
                }
                let writable = can_write(std::path::Path::new(&state.path));
                if !writable && !state.binary {
                    ui.label(
                        egui::RichText::new(model.t("editor.readonly"))
                            .small()
                            .color(palette.warning),
                    );
                }
                ui.label(
                    egui::RichText::new(model.tf(
                        "editor.size",
                        &[
                            ("chars", &state.text.chars().count().to_string()),
                            ("lines", &state.text.lines().count().to_string()),
                        ],
                    ))
                    .small()
                    .color(palette.hidden),
                );
            });
            if state.binary {
                ui.separator();
                ui.label(
                    egui::RichText::new(model.t("editor.binary"))
                        .color(palette.warning)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(model.t("editor.binary.hint"))
                        .small()
                        .color(palette.hidden),
                );
                return;
            }
            if let Some(error) = &ui_state.save_error {
                ui.separator();
                ui.label(egui::RichText::new(error).color(palette.warning));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(model.t("editor.save_copy")).clicked() {
                        save_copy = true;
                    }
                    if ui.button(model.t("editor.elevate")).clicked() {
                        elevate = true;
                    }
                    if let Some(path) = &ui_state.suggested_copy {
                        ui.label(
                            egui::RichText::new(path.display().to_string())
                                .small()
                                .monospace()
                                .color(palette.hidden),
                        );
                    }
                });
            }
            ui.separator();
            // The text fills what the window has left, so a maximized editor is a usable editor.
            let viewport = ui.available_size().max(egui::vec2(1.0, 1.0));
            egui::ScrollArea::vertical()
                .id_salt("document_editor")
                .max_width(viewport.x)
                .max_height(viewport.y)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut state.text)
                            .font(TextStyle::Monospace)
                            .desired_width((viewport.x - 20.0).max(1.0))
                            .desired_rows(1)
                            .lock_focus(true),
                    );
                    if response.changed() {
                        state.modified = true;
                        state.dirty_since_open = true;
                        ui_state.save_error = None;
                    }
                });
    };
    let action = if let Some(ui) = embedded {
        contents(ui);
        ChromeAction::None
    } else {
        crate::ui::workspace_window::show(
            ctx, "editor_window", &title, workspace, &mut chrome, palette, contents,
        )
    };
    if matches!(action, ChromeAction::Close) {
        close_requested = true;
    }
    state.chrome = chrome;

    let path = std::path::PathBuf::from(&state.path);
    if save_requested && !state.binary {
        match write_file(&path, &state.text) {
            Ok(()) => {
                state.modified = false;
                ui_state.dirty_prompt = None;
                ui_state.save_error = None;
                ui_state.suggested_copy = None;
                model.report(model.t("status.done"));
                // A saved file in a repository becomes a commit (and a push) — the question is
                // asked by default, never silently.
                let _ = model.on_file_saved(&path);
            }
            Err(error) => {
                // Nothing was written, so nothing is reported as saved.
                let message = model.tf(
                    "editor.save.failed",
                    &[("path", &state.path), ("error", &error)],
                );
                model.report_error(message.clone());
                ui_state.save_error = Some(message);
                ui_state.suggested_copy = Some(model.suggest_copy_path(&path));
            }
        }
    }
    if save_copy && let Some(target) = ui_state.suggested_copy.clone() {
        match write_file(&target, &state.text) {
            Ok(()) => {
                // The copy is a copy: the window says where the text went, so nobody loses track of
                // which file holds the truth.
                state.modified = false;
                ui_state.dirty_prompt = None;
                ui_state.save_error = None;
                let message = model.tf(
                    "editor.saved_copy",
                    &[("path", &target.display().to_string())],
                );
                model.report(message);
                state.path = target.to_string_lossy().to_string();
            }
            Err(error) => model.report_error(error),
        }
    }
    if elevate {
        // Elevation is Phase 20 (the pkexec/UAC helper). Until it exists, saying so is better than
        // a button that pretends to try.
        let elevated = rc_core::desktop::Action::Elevated(path.clone());
        match crate::system_open::run(&elevated) {
            Ok(started) => {
                let message = model.tf("editor.elevate.started", &[("what", &started.what)]);
                model.report(message);
            }
            Err(error) => {
                let message = model.tf("editor.elevate.failed", &[("error", &error)]);
                model.report_error(message);
            }
        }
    }

    if discard_requested || (close_requested && !state.modified) {
        ui_state.dirty_prompt = None;
        model.editor = None;
        return true;
    }
    if close_requested {
        ui_state.dirty_prompt = Some(model.t("editor.discard_prompt"));
        state.chrome.closed = false;
    }
    model.editor = Some(state);
    false
}

/// Write the buffer, refusing to pretend when it fails.
fn write_file(path: &std::path::Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text.as_bytes()).map_err(|error| error.to_string())
}

/// True when this file can probably be written.
///
/// The question is asked *before* the user types, so the warning appears on opening rather than
/// after a save attempt that goes nowhere. A file that does not exist yet is judged by its folder.
fn can_write(path: &std::path::Path) -> bool {
    if path.exists() {
        return std::fs::metadata(path)
            .map(|metadata| !metadata.permissions().readonly())
            .unwrap_or(false);
    }
    path.parent()
        .map(|parent| {
            std::fs::metadata(parent)
                .map(|metadata| !metadata.permissions().readonly())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_writable_file_is_written_and_a_missing_folder_is_refused_honestly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, b"old").unwrap();
        assert!(write_file(&path, "new").is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        // A path whose parent does not exist fails, and the error is the system's, not a lie.
        let missing = dir.path().join("no/such/dir/file.txt");
        assert!(write_file(&missing, "x").is_err());
    }

    #[test]
    fn writability_is_reported_before_the_user_types() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, b"x").unwrap();
        assert!(can_write(&file));
        // A file that does not exist yet is judged by its folder.
        assert!(can_write(&dir.path().join("new.txt")));
        assert!(!can_write(&dir.path().join("no-such-folder/new.txt")));
    }
}

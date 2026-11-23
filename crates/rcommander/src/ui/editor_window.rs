//! The F4 editor window.

use crate::model::Model;
use crate::theme::Palette;
use egui::{Context, TextStyle};

/// Editor working state kept by the view layer.
#[derive(Default)]
pub struct EditorUi {
    pub dirty_prompt: Option<String>,
}

/// Draw the editor window; returns true when it should be closed.
pub fn show(ctx: &Context, model: &mut Model, palette: &Palette) -> bool {
    let Some(mut state) = model.editor.clone() else {
        return true;
    };
    let mut open = true;
    let title = format!("{} — {}", model.t("cmd.edit"), state.path);
    let mut save_requested = false;
    let mut close_requested = false;
    egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .default_size([760.0, 560.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button(model.t("dialog.config.apply")).clicked() {
                    save_requested = true;
                }
                if ui.button(model.t("dialog.config.cancel")).clicked() {
                    close_requested = true;
                }
                if state.modified {
                    ui.label(egui::RichText::new("modified").color(palette.readonly));
                }
                ui.label(
                    egui::RichText::new(format!("{} chars", state.text.chars().count()))
                        .small()
                        .color(palette.hidden),
                );
            });
            ui.separator();
            let response = ui.add(
                egui::TextEdit::multiline(&mut state.text)
                    .font(TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(24)
                    .lock_focus(true),
            );
            if response.changed() {
                state.modified = true;
                state.dirty_since_open = true;
            }
        });
    if save_requested {
        let path = std::path::PathBuf::from(&state.path);
        match std::fs::write(&path, state.text.as_bytes()) {
            Ok(()) => {
                state.modified = false;
                model.report(model.t("status.done"));
            }
            Err(e) => model.report_error(e.to_string()),
        }
    }
    if !open || close_requested {
        model.editor = None;
        return true;
    }
    model.editor = Some(state);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_ui_defaults_are_clean() {
        let ui = EditorUi::default();
        assert!(ui.dirty_prompt.is_none());
    }
}

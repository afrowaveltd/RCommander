//! The agent chat window (Phase 16.1).
//!
//! A roomy conversation pane: the transcript scrolls, the answer streams in as the model writes it,
//! and *Stop* keeps what already arrived. Buttons are labelled through the i18n packs and coloured
//! from the active theme palette, so the window survives a theme switch like every other panel.

use crate::model::Model;
use crate::model::chat::Speaker;
use crate::theme::Palette;
use egui::{Context, RichText};

/// State the view keeps between frames (scroll position, focus).
#[derive(Default)]
pub struct ChatUi {
    /// Scroll to the newest line on the next frame.
    pub follow: bool,
    /// The transcript changed since the last frame.
    pub dirty: bool,
}

/// Render the chat window; returns true when the user closed it.
pub fn show(ctx: &Context, model: &mut Model, ui_state: &mut ChatUi, palette: &Palette) -> bool {
    let mut open = true;
    let running = model.chat_is_running();
    let model_name = model
        .ai_model()
        .map(|name| name.to_string())
        .unwrap_or_else(|| model.t("ai.model.none"));
    // Strings are resolved up front: the body borrows the model mutably for the input field.
    let title = model.t("ai.chat.title");
    let label_model = model.t("ai.chat.model");
    let label_clear = model.t("ai.chat.clear");
    let label_stop = model.t("ai.chat.stop");
    let label_send = model.t("ai.chat.send");
    let label_empty = model.t("ai.chat.empty");
    let label_placeholder = model.t("ai.chat.placeholder");
    let send_hint = if running {
        model.t("ai.chat.running")
    } else {
        model.t("ai.chat.hint")
    };
    let you = model.t("ai.chat.speaker.user");
    let assistant = model.t("ai.chat.speaker.model");
    let system = model.t("ai.chat.speaker.system");
    let mut send = false;
    let mut stop = false;
    let mut clear = false;

    egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .default_size([760.0, 560.0])
        .min_size([420.0, 260.0])
        .show(ctx, |ui| {
            // Header: which model answers, and the two transcript-wide actions.
            ui.horizontal(|ui| {
                ui.label(RichText::new(&label_model).small().color(palette.hidden));
                ui.label(RichText::new(&model_name).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(&label_clear).clicked() {
                        clear = true;
                    }
                    if running
                        && ui
                            .button(RichText::new(&label_stop).color(palette.warning))
                            .clicked()
                    {
                        stop = true;
                    }
                });
            });
            ui.separator();

            // The transcript. It takes everything but the input row.
            let input_height = ui.spacing().interact_size.y * 3.0;
            let available = ui.available_height();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height((available - input_height).max(80.0))
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let lines = &model.chat().lines;
                    for line in lines {
                        let speaker = match line.speaker {
                            Speaker::User => &you,
                            Speaker::Model => &assistant,
                            Speaker::System => &system,
                        };
                        transcript_line(ui, line, speaker, palette);
                    }
                    if lines.is_empty() {
                        ui.label(RichText::new(&label_empty).italics().color(palette.hidden));
                    }
                });
            ui.separator();

            // The input row. Enter sends, Shift+Enter inserts a newline.
            ui.horizontal(|ui| {
                let input = ui.add(
                    egui::TextEdit::multiline(&mut model.chat().input)
                        .desired_rows(2)
                        .desired_width(ui.available_width() - 90.0)
                        .hint_text(&label_placeholder),
                );
                if input.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let shift = ctx.input(|i| i.modifiers.shift);
                    if !shift {
                        send = true;
                    }
                }
                ui.vertical(|ui| {
                    let button = ui.add_enabled(
                        !running && !model.chat().input.trim().is_empty(),
                        egui::Button::new(&label_send),
                    );
                    if button.clicked() {
                        send = true;
                    }
                    ui.label(RichText::new(send_hint).small().color(palette.hidden));
                });
            });
        });

    if clear {
        model.chat_clear();
        ui_state.follow = true;
    }
    if stop {
        model.chat_stop();
    }
    if send {
        model.chat_send();
        ui_state.follow = true;
    }
    !open
}

/// One transcript entry: speaker name, then the text with an "in progress" marker.
fn transcript_line(
    ui: &mut egui::Ui,
    line: &crate::model::chat::ChatLine,
    speaker: &str,
    palette: &Palette,
) {
    ui.horizontal_wrapped(|ui| {
        let colour = match line.speaker {
            Speaker::User => palette.accent,
            Speaker::Model => palette.directory,
            Speaker::System => palette.hidden,
        };
        ui.label(RichText::new(speaker).strong().color(colour));
        if line.streaming {
            ui.label(RichText::new("…").color(palette.hidden));
        }
    });
    // A system note is an aside, so it is dimmed and indented.
    let mut text = RichText::new(&line.text);
    if line.speaker == Speaker::System {
        text = text.italics().color(palette.hidden);
    }
    ui.horizontal_wrapped(|ui| {
        ui.add_space(12.0);
        ui.label(text);
    });
    ui.add_space(6.0);
}

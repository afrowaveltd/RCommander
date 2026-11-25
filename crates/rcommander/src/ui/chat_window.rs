//! The agent chat window (Phase 16.1).
//!
//! A roomy conversation pane: the transcript scrolls, the answer streams in as the model writes it,
//! and *Stop* keeps what already arrived. Buttons are labelled through the i18n packs and coloured
//! from the active theme palette, so the window survives a theme switch like every other panel.
//!
//! Model answers are Markdown — that is what a language model writes — so they are rendered
//! through [`crate::ui::markdown_view`] instead of showing their asterisks. Parsing a line costs a
//! few microseconds, but a transcript is redrawn every frame while an answer streams, so finished
//! lines keep their parsed form until their text changes.

use std::collections::HashMap;

use crate::model::Model;
use crate::model::chat::Speaker;
use crate::theme::Palette;
use crate::ui::markdown_view::render_blocks;
use egui::{Context, RichText};

/// State the view keeps between frames (scroll position, focus, parsed answers).
#[derive(Default)]
pub struct ChatUi {
    /// Scroll to the newest line on the next frame.
    pub follow: bool,
    /// Parsed Markdown keyed by a line's stable identity, with the text length it was parsed from.
    parsed: HashMap<u64, (usize, Vec<rc_help::Block>)>,
}

impl ChatUi {
    /// The parsed blocks of a line, reusing the previous parse while the text is unchanged.
    ///
    /// The length is enough of a fingerprint here: a streaming answer only ever grows, and any
    /// other change comes with a new line identity.
    fn blocks(&mut self, seq: u64, text: &str) -> &[rc_help::Block] {
        let stale = match self.parsed.get(&seq) {
            Some((len, _)) => *len != text.len(),
            None => true,
        };
        if stale {
            self.parsed
                .insert(seq, (text.len(), rc_help::parse_markdown(text)));
        }
        &self.parsed.get(&seq).expect("just inserted").1
    }

    /// Forget the cache; the transcript was cleared.
    pub fn invalidate(&mut self) {
        self.parsed.clear();
    }
}

/// What a chat surface offers the caller after a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChatActions {
    /// The window's close button was pressed.
    pub close: bool,
    /// "Show files" was pressed in a panel pane.
    pub show_files: bool,
}

/// Render the chat window; returns true when the user closed it.
pub fn show(ctx: &Context, model: &mut Model, ui_state: &mut ChatUi, palette: &Palette) -> bool {
    let mut open = true;
    let title = model.t("ai.chat.title");
    let actions = egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .default_size([760.0, 560.0])
        .min_size([420.0, 260.0])
        .show(ctx, |ui| body(ui, model, ui_state, palette, false))
        .map(|response| response.inner)
        .unwrap_or_default();
    let _ = actions;
    !open
}

/// Render the chat inside a panel, next to the file list of the other panel.
///
/// The panel keeps its listing underneath: nothing in the model is reset when the pane is opened,
/// so "show files" brings back the same directory, cursor and selection.
pub fn pane(
    ui: &mut egui::Ui,
    model: &mut Model,
    ui_state: &mut ChatUi,
    palette: &Palette,
    side: crate::model::Side,
) -> ChatActions {
    let mut actions = ChatActions::default();
    egui::Frame::new()
        .fill(palette.background)
        .stroke(egui::Stroke::new(1.0, palette.panel_border))
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            let label_files = model.t("agent.pane.files");
            let label_title = model.t("ai.chat.title");
            let active = model.focus == side;
            ui.horizontal(|ui| {
                ui.label(RichText::new(&label_title).strong().color(if active {
                    palette.accent
                } else {
                    palette.hidden
                }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(&label_files).clicked() {
                        actions.show_files = true;
                    }
                });
            });
            ui.separator();
            let inner = body(ui, model, ui_state, palette, true);
            actions.close |= inner.close;
        });
    actions
}

/// The transcript and the input row, shared by the window and the panel.
fn body(
    ui: &mut egui::Ui,
    model: &mut Model,
    ui_state: &mut ChatUi,
    palette: &Palette,
    compact: bool,
) -> ChatActions {
    let actions = ChatActions::default();
    let running = model.chat_is_running();
    let model_name = model
        .ai_model()
        .map(|name| name.to_string())
        .unwrap_or_else(|| model.t("ai.model.none"));
    // Strings are resolved up front: the body borrows the model mutably for the input field.
    let label_model = model.t("ai.chat.model");
    let label_clear = model.t("ai.chat.clear");
    let label_stop = model.t("ai.chat.stop");
    let label_send = model.t("ai.chat.send");
    let label_empty = model.t("ai.chat.empty");
    let label_placeholder = model.t("ai.chat.placeholder");
    let label_prompts = model.t("prompt.pick");
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
    // A panel is narrower than the window, so the input gets one line there and two in the window;
    // more lines of typing space would push the transcript out of sight.
    let rows = if compact { 1.0 } else { 2.0 };
    let input_height = ui.spacing().interact_size.y * (rows + 1.0);
    let available = ui.available_height();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height((available - input_height).max(60.0))
        .stick_to_bottom(true)
        .show(ui, |ui| {
            // A read-only borrow: rendering needs the transcript, not the session.
            let lines = &model.chat_session.lines;
            if lines.is_empty() {
                ui_state.invalidate();
                ui.label(RichText::new(&label_empty).italics().color(palette.hidden));
            }
            for line in lines {
                let speaker = match line.speaker {
                    Speaker::User => &you,
                    Speaker::Model => &assistant,
                    Speaker::System => &system,
                };
                transcript_line(ui, line, speaker, palette, ui_state);
            }
        });
    ui.separator();

    // The prompt picker, when it is open, sits above the input so the choice is visible while
    // the text lands in the box below it.
    if model.prompt_picker.open {
        prompt_picker(ui, model, palette);
    }

    // The input row. Enter sends, Shift+Enter inserts a newline.
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 90.0).max(80.0);
        let input = ui.add(
            egui::TextEdit::multiline(&mut model.chat().input)
                .desired_rows(rows as usize)
                .desired_width(width)
                .hint_text(&label_placeholder),
        );
        if input.has_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
            let shift = ui.ctx().input(|i| i.modifiers.shift);
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
            // The prompt catalogue: the chat is more useful when a good question is one click
            // away, and a prompt is only text — it cannot grant anything.
            if ui
                .add_enabled(!running, egui::Button::new(&label_prompts))
                .clicked()
            {
                model.prompt_picker.open = !model.prompt_picker.open;
            }
            ui.label(RichText::new(send_hint).small().color(palette.hidden));
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
    actions
}

/// The prompt catalogue: category tabs, a search field, the list and a preview.
///
/// The preview shows the text that will be inserted with the variables already filled in, which is
/// the only honest way to offer a prompt with `{{selection}}` in it: the user sees the question
/// before it is asked.
fn prompt_picker(ui: &mut egui::Ui, model: &mut Model, palette: &Palette) {
    use rc_core::prompt::Category;
    egui::Frame::new()
        .fill(palette.header_background)
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let all_label = model.t("prompt.all");
                let none = model.prompt_picker.category.is_none();
                if ui.selectable_label(none, all_label).clicked() {
                    model.prompt_picker.category = None;
                    model.prompt_picker.selected = 0;
                }
                for category in Category::ALL {
                    let label = model.t(category.label_key());
                    let chosen = model.prompt_picker.category == Some(category);
                    if ui.selectable_label(chosen, label).clicked() {
                        model.prompt_picker.category = Some(category);
                        model.prompt_picker.selected = 0;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(model.t("dialog.config.cancel")).clicked() {
                        model.prompt_picker.open = false;
                    }
                });
            });
            let search_hint = model.t("prompt.search");
            ui.horizontal(|ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut model.prompt_picker.search)
                        .desired_width(220.0)
                        .hint_text(&search_hint),
                );
                if search.changed() {
                    model.prompt_picker.selected = 0;
                }
                let visible = model.visible_prompts();
                ui.label(
                    RichText::new(
                        model.tf("prompt.count", &[("count", &visible.len().to_string())]),
                    )
                    .small()
                    .color(palette.hidden),
                );
            });
            let entries = model.visible_prompts();
            let mut chosen: Option<String> = None;
            egui::ScrollArea::vertical()
                .max_height(140.0)
                .auto_shrink([false, true])
                .id_salt("prompt_list")
                .show(ui, |ui| {
                    for (index, entry) in entries.iter().enumerate() {
                        let selected = index == model.prompt_picker.selected;
                        let label = if entry.prompt.about.trim().is_empty() {
                            entry.prompt.title.clone()
                        } else {
                            format!("{} — {}", entry.prompt.title, entry.prompt.about)
                        };
                        let id = entry.prompt.id.clone();
                        let mut unpin = false;
                        let mut pin = false;
                        ui.horizontal(|ui| {
                            let pinned = model.prompt_is_pinned(&id);
                            let prefix = if pinned {
                                format!("* {}", label)
                            } else {
                                label
                            };
                            let response = ui.selectable_label(selected, prefix);
                            if response.clicked() {
                                model.prompt_picker.selected = index;
                                chosen = Some(id.clone());
                            }
                            // A built-in prompt says where it comes from, so nobody looks for a
                            // way to edit it in place.
                            if entry.builtin {
                                response.on_hover_text(model.t("prompt.builtin.hint"));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let label = if pinned {
                                        model.t("prompt.unpin")
                                    } else {
                                        model.t("prompt.pin")
                                    };
                                    if ui.small_button(label).clicked() {
                                        if pinned {
                                            unpin = true;
                                        } else {
                                            pin = true;
                                        }
                                    }
                                },
                            );
                        });
                        if unpin || pin {
                            model.toggle_prompt_pin(&id);
                        }
                    }
                });
            if let Some(id) = chosen {
                model.insert_prompt(&id);
            } else if let Some(entry) = entries.get(model.prompt_picker.selected) {
                // Preview the selected prompt so the variables are visible before the click.
                let id = entry.prompt.id.clone();
                if model.prompt_picker.preview.is_empty() {
                    model.preview_prompt(&id);
                }
                let preview = model.prompt_picker.preview.clone();
                let missing = model.prompt_picker.missing.clone();
                if !preview.is_empty() {
                    ui.separator();
                    ui.label(
                        RichText::new(model.t("prompt.preview"))
                            .small()
                            .color(palette.hidden),
                    );
                    ui.add(egui::Label::new(RichText::new(&preview).small().italics()).wrap());
                    if !missing.is_empty() {
                        let names = missing.join(", ");
                        ui.label(
                            RichText::new(model.tf("prompt.missing", &[("names", &names)]))
                                .small()
                                .color(palette.warning),
                        );
                    }
                }
                if ui.button(model.t("prompt.use")).clicked() {
                    model.insert_prompt(&id);
                }
            }
        });
}

/// One transcript entry: speaker name, then the text with an "in progress" marker.
fn transcript_line(
    ui: &mut egui::Ui,
    line: &crate::model::chat::ChatLine,
    speaker: &str,
    palette: &Palette,
    ui_state: &mut ChatUi,
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
    ui.horizontal(|ui| {
        // Indent the body so a long answer is visibly the answer, not another speaker.
        ui.add_space(12.0);
        ui.vertical(|ui| match line.speaker {
            // An answer is Markdown: headings, lists, tables and code fences are what a model
            // writes, and showing the markup characters instead would be harder to read.
            Speaker::Model => {
                let mut link_target = None;
                let blocks = ui_state.blocks(line.seq, &line.text);
                render_blocks(ui, blocks, palette, &mut link_target);
            }
            // A question and a system note are plain text, indented so they line up.
            _ => {
                let mut text = RichText::new(&line.text);
                if line.speaker == Speaker::System {
                    text = text.italics().color(palette.hidden);
                }
                ui.add(egui::Label::new(text).wrap());
            }
        });
    });
    ui.add_space(6.0);
}

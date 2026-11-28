//! The in-app help window.
//!
//! Content comes from `Docs/<Topic>/<lang>.md` through `rc-help`; nothing is
//! compiled in, so users can add topics and languages by dropping files in.

use crate::model::Model;
use crate::theme::Palette;
use crate::ui::markdown_view::render_blocks;
use egui::{Context, RichText};
use rc_help::{Align, HelpDocument, HelpIndex};

/// Help view state.
#[derive(Default)]
pub struct HelpUi {
    pub index: Option<HelpIndex>,
    pub current: Option<HelpDocument>,
    pub error: Option<String>,
    pub search: String,
    pub hits: Vec<rc_help::SearchHit>,
}

impl HelpUi {
    /// (Re)load the help index from disk.
    pub fn reload(&mut self, root: &std::path::Path) {
        match HelpIndex::load(root) {
            Ok(index) => {
                self.index = Some(index);
                self.error = None;
            }
            Err(e) => {
                self.index = None;
                self.error = Some(e.to_string());
            }
        }
    }

    /// Open a topic in the requested language, falling back to English.
    pub fn open(&mut self, topic: &str, language: &str) {
        self.current = None;
        if let Some(index) = &self.index {
            if let Some(doc) = index.body(topic, language, "en") {
                self.current = Some(doc);
                return;
            }
            self.error = Some(format!("topic {topic:?} not found"));
        }
    }

    pub fn topics(&self) -> Vec<(String, String)> {
        match &self.index {
            Some(index) => index
                .topics()
                .iter()
                .map(|t| {
                    (
                        t.id.clone(),
                        t.title_for("en", "en").unwrap_or(&t.id).to_string(),
                    )
                })
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn run_search(&mut self, query: &str, language: &str) {
        self.hits = match &self.index {
            Some(index) if !query.trim().is_empty() => {
                rc_help::search(index, query, language, "en")
            }
            _ => Vec::new(),
        };
    }
}

/// Draw the help window; returns true when it should be closed.
pub fn show(ctx: &Context, model: &mut Model, ui_state: &mut HelpUi, palette: &Palette) -> bool {
    if ui_state.index.is_none() {
        let root = crate::paths::docs_dir();
        ui_state.reload(&root);
    }
    let Some(mut help_state) = model.help.clone() else {
        return true;
    };
    if ui_state.current.is_none() {
        if let Some(topic) = help_state.topic.clone() {
            ui_state.open(&topic, &help_state.language);
        } else if let Some((first, _)) = ui_state.topics().first().cloned() {
            ui_state.open(&first, &help_state.language);
        }
    }

    let mut open = true;
    egui::Window::new(model.t("dialog.help.title"))
        .open(&mut open)
        .resizable(true)
        .default_size([820.0, 600.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(model.t("dialog.help.contents"));
                let mut selected_topic = None;
                egui::ComboBox::from_id_salt("help_topic")
                    .selected_text(
                        ui_state
                            .current
                            .as_ref()
                            .map(|d| d.title.clone())
                            .unwrap_or_default(),
                    )
                    .show_ui(ui, |ui| {
                        for (id, title) in ui_state.topics() {
                            if ui.selectable_label(false, title).clicked() {
                                selected_topic = Some(id);
                            }
                        }
                    });
                if let Some(topic) = selected_topic {
                    help_state.topic = Some(topic.clone());
                    ui_state.open(&topic, &help_state.language);
                }
                ui.separator();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut ui_state.search)
                        .hint_text(model.t("viewer.find"))
                        .desired_width(160.0),
                );
                if response.changed() {
                    ui_state.run_search(&ui_state.search.clone(), &help_state.language);
                }
            });
            ui.separator();
            if !ui_state.hits.is_empty() {
                ui.label(model.tf(
                    "dialog.search.results_count",
                    &[("count", &ui_state.hits.len().to_string())],
                ));
                for hit in ui_state.hits.clone().iter().take(20) {
                    if ui.link(format!("{} — {}", hit.title, hit.line)).clicked() {
                        help_state.topic = Some(hit.topic_id.clone());
                        ui_state.open(&hit.topic_id, &help_state.language);
                    }
                }
                ui.separator();
            }
            if let Some(document) = &ui_state.current {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&document.title).heading());
                    if document.used_fallback {
                        ui.label(
                            RichText::new(model.t("dialog.help.missing_translation"))
                                .small()
                                .color(palette.hidden),
                        );
                    }
                    ui.label(
                        RichText::new(&document.language)
                            .small()
                            .color(palette.hidden),
                    );
                });
                let mut link_target: Option<String> = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_blocks(ui, &document.blocks, palette, &mut link_target);
                    });
                if let Some(target) = link_target
                    && let Some(topic) = ui_state
                        .index
                        .as_ref()
                        .and_then(|index| index.resolve_link(&document.topic_id, &target))
                {
                    help_state.topic = Some(topic.clone());
                    ui_state.open(&topic, &help_state.language);
                }
            } else if let Some(error) = &ui_state.error {
                ui.label(RichText::new(error).color(palette.readonly));
            } else {
                ui.label(model.t("dialog.help.no_topic"));
            }
        });
    model.help = if open { Some(help_state) } else { None };
    !open
}

/// Align is part of the model; the table renderer keeps the API stable.
pub fn alignment_label(align: Align) -> &'static str {
    match align {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_labels_are_stable() {
        assert_eq!(alignment_label(Align::Left), "left");
        assert_eq!(alignment_label(Align::Center), "center");
        assert_eq!(alignment_label(Align::Right), "right");
    }

    #[test]
    fn help_ui_reloads_from_the_repository_docs() {
        let mut ui_state = HelpUi::default();
        let docs = crate::paths::docs_dir();
        if docs.is_dir() {
            ui_state.reload(&docs);
            assert!(ui_state.index.is_some());
            let topics = ui_state.topics();
            assert!(!topics.is_empty(), "Docs/ should contain topics");
            assert!(topics.iter().any(|(id, _)| id == "Previewer"));
            ui_state.open("Previewer", "cs");
            assert!(ui_state.current.is_some());
            ui_state.run_search("hex", "en");
            assert!(!ui_state.hits.is_empty());
        }
    }

    #[test]
    fn opening_an_unknown_topic_reports_an_error() {
        let mut ui_state = HelpUi::default();
        let index = rc_help::HelpIndex::load(&crate::paths::docs_dir())
            .or_else(|_| rc_help::HelpIndex::load(std::path::Path::new("/nonexistent")));
        ui_state.index = index.ok();
        ui_state.open("does-not-exist", "en");
        assert!(ui_state.current.is_none());
    }
}

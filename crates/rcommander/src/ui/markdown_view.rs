//! Rendering Markdown blocks into egui widgets.
//!
//! `rc-help` parses Markdown into [`Block`]s; this module turns them into widgets. It lives apart
//! from the help window because the agent chat needs exactly the same thing: a model answer is
//! Markdown, and showing its asterisks instead of bold text would be a poor answer to read.
//!
//! Nothing here decides *content*; unknown or partial markup has already degraded to text in the
//! parser (which is what makes rendering a half-received stream safe).

use crate::theme::Palette;
use egui::{RichText, TextStyle};
use rc_help::{Block, Span};

pub fn render_blocks(
    ui: &mut egui::Ui,
    blocks: &[Block],
    palette: &Palette,
    link_target: &mut Option<String>,
) {
    for block in blocks {
        match block {
            Block::Heading { level, spans } => {
                let text = plain(spans);
                let size = match level {
                    1 => 20.0,
                    2 => 17.0,
                    3 => 15.0,
                    _ => 13.0,
                };
                ui.add_space(6.0);
                ui.label(RichText::new(text).size(size).strong());
            }
            Block::Paragraph(spans) => {
                ui.horizontal_wrapped(|ui| render_spans(ui, spans, palette, link_target));
                ui.add_space(4.0);
            }
            Block::Quote(spans) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("│").color(palette.panel_border));
                    ui.horizontal_wrapped(|ui| render_spans(ui, spans, palette, link_target));
                });
            }
            Block::BulletList(items) => {
                for item in items {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("•");
                        render_spans(ui, item, palette, link_target);
                    });
                }
                ui.add_space(4.0);
            }
            Block::NumberedList { start, items } => {
                for (index, item) in items.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("{}.", start + index as u64));
                        render_spans(ui, item, palette, link_target);
                    });
                }
                ui.add_space(4.0);
            }
            Block::Table {
                header,
                align,
                rows,
            } => {
                egui::Grid::new(format!("help_table_{}", rows.len()))
                    .striped(true)
                    .show(ui, |ui| {
                        for cell in header {
                            let text = plain(cell);
                            ui.label(RichText::new(text).strong());
                        }
                        ui.end_row();
                        for row in rows {
                            for (index, cell) in row.iter().enumerate() {
                                let _ = align.get(index);
                                ui.label(plain(cell));
                            }
                            ui.end_row();
                        }
                    });
                ui.add_space(4.0);
            }
            Block::CodeBlock { language, lines } => {
                let mut text = String::new();
                if !language.is_empty() {
                    text.push_str(&format!("// {language}\n"));
                }
                for line in lines {
                    text.push_str(line);
                    text.push('\n');
                }
                egui::Frame::new()
                    .fill(palette.header_background)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(text).text_style(TextStyle::Monospace))
                                .selectable(true),
                        );
                    });
                ui.add_space(4.0);
            }
            Block::Rule => {
                ui.separator();
            }
        }
    }
}

pub fn render_spans(
    ui: &mut egui::Ui,
    spans: &[Span],
    palette: &Palette,
    link_target: &mut Option<String>,
) {
    for span in spans {
        match span {
            Span::Text(text) => {
                ui.label(RichText::new(text));
            }
            Span::Bold(inner) => {
                let text = plain(inner);
                ui.label(RichText::new(text).strong());
            }
            Span::Italic(inner) => {
                let text = plain(inner);
                ui.label(RichText::new(text).italics());
            }
            Span::Code(text) => {
                ui.label(
                    RichText::new(text)
                        .monospace()
                        .background_color(palette.header_background),
                );
            }
            Span::Link { text, target } => {
                let label = plain(text);
                if ui.link(label).clicked() {
                    *link_target = Some(target.clone());
                }
            }
            Span::LineBreak => {
                ui.label(" ");
            }
        }
    }
}

pub fn plain(spans: &[Span]) -> String {
    let mut out = String::new();
    for span in spans {
        match span {
            Span::Text(t) => out.push_str(t),
            Span::Bold(inner) | Span::Italic(inner) | Span::Link { text: inner, .. } => {
                out.push_str(&plain(inner))
            }
            Span::Code(t) => out.push_str(t),
            Span::LineBreak => out.push(' '),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    fn spans(text: &str) -> Vec<Span> {
        vec![Span::Text(text.to_string())]
    }

    #[test]
    fn plain_flattens_nested_spans() {
        let input = vec![
            Span::Text("a ".into()),
            Span::Bold(spans("bold")),
            Span::Italic(spans(" italic")),
            Span::Code(" code".into()),
            Span::Link {
                text: spans(" link"),
                target: "x".into(),
            },
        ];
        assert_eq!(plain(&input), "a bold italic code link");
    }

    #[test]
    fn plain_handles_line_breaks_and_empty_input() {
        assert_eq!(plain(&[Span::LineBreak]), " ");
        assert_eq!(plain(&[]), "");
    }

    /// Render `text` the way the chat does and return the harness for querying.
    fn render(text: &str) -> egui_kittest::Harness<'static> {
        let blocks = rc_help::parse_markdown(text);
        let palette = Palette::light();
        egui_kittest::Harness::new_ui(move |ui| {
            let mut link = None;
            render_blocks(ui, &blocks, &palette, &mut link);
        })
    }

    #[test]
    fn markup_becomes_text_instead_of_asterisks() {
        let mut harness = render("This is **bold** and `code`.");
        harness.run();
        // The words are there; the markup characters are not.
        harness.get_by_label("bold");
        assert!(
            harness.root().query_all_by_label_contains("**").count() == 0,
            "the asterisks are gone"
        );
    }

    #[test]
    fn a_heading_list_and_code_block_draw_their_content() {
        let mut harness = render("# Title\n\n- one\n- two\n\n```sh\nls -l\n```\n");
        harness.run();
        harness.get_by_label("Title");
        harness.get_by_label("one");
        harness.get_by_label("two");
        // The code block keeps the command, and the language is a comment above it.
        assert!(harness.root().query_all_by_label_contains("ls -l").count() > 0);
        assert!(
            harness.root().query_all_by_label_contains("sh").count() > 0,
            "the language label is shown"
        );
    }

    #[test]
    fn half_received_markup_still_renders() {
        // What a streaming answer looks like mid-fence: no closing fence yet.
        let mut harness = render("Run this:\n\n```sh\nls -l");
        harness.run();
        assert!(
            harness.root().query_all_by_label_contains("ls -l").count() > 0,
            "an unfinished fence does not lose the code"
        );
    }
}

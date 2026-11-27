//! Rendering Markdown blocks into egui widgets.
//!
//! `rc-help` parses Markdown into [`Block`]s; this module turns them into widgets. It lives apart
//! from the help window because the agent chat needs exactly the same thing: a model answer is
//! Markdown, and showing its asterisks instead of bold text would be a poor answer to read.
//!
//! Nothing here decides *content*; unknown or partial markup has already degraded to text in the
//! parser (which is what makes rendering a half-received stream safe).

use crate::theme::Palette;
use egui::{Color32, RichText};
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
                // Syntax highlighting. The colouring never changes the text, so a half-received
                // answer still reads exactly as it arrived.
                let mut job = code_layout(language, lines, palette);
                job.wrap.max_width = ui.available_width() - 12.0;
                egui::Frame::new()
                    .fill(palette.header_background)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(job).selectable(true));
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

/// Build the layout job for a fenced code block: monospaced, syntax coloured, selectable.
///
/// The language label is shown as a comment above the code, and the code keeps its own line breaks,
/// so copying a block out of an answer gives back something a person could paste into a file.
pub fn code_layout(language: &str, lines: &[String], palette: &Palette) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let body = |colour: Color32| egui::TextFormat {
        font_id: egui::FontId::monospace(13.0),
        color: colour,
        ..Default::default()
    };
    if !language.trim().is_empty() {
        let mut label = body(palette.hidden);
        label.italics = true;
        job.append(language.trim(), 0.0, label);
        job.append("\n", 0.0, body(palette.foreground));
    }
    // One highlighter for the whole block, so a block comment carries across lines.
    let mut highlighter = rc_core::syntax::Highlighter::new(language);
    for line in lines {
        for span in highlighter.line(line) {
            let colour = span
                .kind
                .role_name()
                .and_then(rc_core::theme::Role::from_name)
                .map(|role| palette.role(role))
                .unwrap_or(palette.foreground);
            job.append(&span.text, 0.0, body(colour));
        }
        job.append("\n", 0.0, body(palette.foreground));
    }
    job
}

/// Layout for an editable or read-only source buffer. Never insert a language label,
/// normalize line endings, or add a final newline: egui cursor positions must
/// refer to exactly the original text. The size limit keeps large logs cheap.
pub fn source_layout(
    path: &str, text: &str, palette: &Palette, font: egui::FontId, highlight: bool,
) -> egui::text::LayoutJob {
    let language = if path.to_ascii_lowercase().ends_with(".mdown")
        || path.to_ascii_lowercase().ends_with(".mkd")
        || path.to_ascii_lowercase().ends_with(".mkdn")
    { "markdown" } else { path };
    let mut highlighter = rc_core::syntax::Highlighter::new(language);
    let mut job = egui::text::LayoutJob::default();
    let format = |color| egui::TextFormat {
        font_id: font.clone(), color, ..Default::default()
    };
    if !highlight || text.len() > 256 * 1024
        || highlighter.language() == rc_core::syntax::Language::Plain
    {
        job.append(text, 0.0, format(palette.foreground));
        return job;
    }
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        for span in highlighter.line(body) {
            let color = span.kind.role_name()
                .and_then(rc_core::theme::Role::from_name)
                .map(|role| palette.role(role))
                .unwrap_or(palette.foreground);
            job.append(&span.text, 0.0, format(color));
        }
        if line.ends_with('\n') {
            job.append("\n", 0.0, format(palette.foreground));
        }
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    fn spans(text: &str) -> Vec<Span> {
        vec![Span::Text(text.to_string())]
    }

    #[test]
    fn source_layout_preserves_buffer_and_highlights_known_languages() {
        let palette = Palette::light();
        let text = "int main() {\r\n/* žluťoučký\r\ncomment */ return 42; }";
        let font = egui::FontId::monospace(13.0);
        let job = source_layout("main.c", text, &palette, font.clone(), true);
        assert_eq!(job.text, text);
        assert!(job.sections.iter().any(|s| s.format.color == palette.code_keyword));
        assert!(job.sections.iter().any(|s| s.format.color == palette.code_comment));
        assert!(job.sections.iter().any(|s| s.format.color == palette.code_number));
        let plain = source_layout("file.txt", text, &palette, font, true);
        assert_eq!(plain.text, text);
        assert_eq!(plain.sections.len(), 1);
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

#[cfg(test)]
mod code_tests {
    use super::*;

    fn text_of(job: &egui::text::LayoutJob) -> String {
        job.text.clone()
    }

    #[test]
    fn a_code_block_keeps_its_text_and_gains_colour() {
        let palette = Palette::light();
        let lines = vec!["let count: usize = 42;".to_string(), "// done".to_string()];
        let job = code_layout("rust", &lines, &palette);
        let text = text_of(&job);
        assert!(text.contains("let count: usize = 42;"), "{text}");
        assert!(text.contains("// done"), "{text}");
        assert!(
            text.starts_with("rust\n"),
            "the language is labelled: {text}"
        );
        // At least three different colours: the keyword, the number and the comment.
        let mut colours: Vec<Color32> = job.sections.iter().map(|s| s.format.color).collect();
        colours.sort_by_key(|c| (c.r(), c.g(), c.b()));
        colours.dedup();
        assert!(colours.len() >= 4, "the block is coloured: {colours:?}");
        assert!(colours.contains(&palette.code_keyword), "{colours:?}");
        assert!(colours.contains(&palette.code_number), "{colours:?}");
        assert!(colours.contains(&palette.code_comment), "{colours:?}");
    }

    #[test]
    fn a_block_without_a_language_is_still_copied_verbatim() {
        let palette = Palette::dark();
        let lines = vec!["plain text".to_string(), "second line".to_string()];
        let job = code_layout("", &lines, &palette);
        assert_eq!(job.text, "plain text\nsecond line\n");
        // Everything is the foreground colour, and nothing is invented.
        assert!(
            job.sections
                .iter()
                .all(|section| section.format.color == palette.foreground
                    || section.format.color == palette.hidden)
        );
    }

    #[test]
    fn every_highlighted_kind_maps_to_a_distinct_colour() {
        // The six roles must be distinguishable, or the colouring is decoration.
        let palette = Palette::light();
        let colours = [
            palette.code_keyword,
            palette.code_string,
            palette.code_comment,
            palette.code_number,
            palette.code_type,
            palette.code_function,
        ];
        for (index, colour) in colours.iter().enumerate() {
            for other in colours.iter().skip(index + 1) {
                assert_ne!(colour, other, "two code roles share a colour");
            }
        }
    }
}

//! Plain-text projection of the markdown block model.
//!
//! Used for full-text search, for the CLI/MCP output and for anything that
//! needs the readable text of a help page without any markdown syntax.

use crate::markdown::{Block, Span};

/// Render `blocks` as plain text.
///
/// Rules:
///
/// * Headings, paragraphs and quotes contribute their inline text only.
/// * Bullet items are prefixed with `• `, numbered items with `N) `; both use
///   one line per item.
/// * Table cells are separated by two spaces, one line per row.
/// * Code blocks contribute their lines verbatim.
/// * [`Block::Rule`] contributes nothing.
/// * Blocks are separated by a blank line; the result has no trailing newline.
pub fn to_plain_text(blocks: &[Block]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) => {
                parts.push(spans_plain(spans));
            }
            Block::BulletList(items) => {
                let rendered: Vec<String> = items
                    .iter()
                    .map(|item| format!("• {}", spans_plain(item)))
                    .collect();
                parts.push(rendered.join("\n"));
            }
            Block::NumberedList { start, items } => {
                let rendered: Vec<String> = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| format!("{}) {}", start + index as u64, spans_plain(item)))
                    .collect();
                parts.push(rendered.join("\n"));
            }
            Block::Table { header, rows, .. } => {
                let mut lines: Vec<String> = Vec::new();
                if !header.is_empty() {
                    lines.push(join_cells(header));
                }
                for row in rows {
                    lines.push(join_cells(row));
                }
                parts.push(lines.join("\n"));
            }
            Block::CodeBlock { lines, .. } => parts.push(lines.join("\n")),
            Block::Rule => {}
        }
    }
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<String>>()
        .join("\n\n")
        .trim_end()
        .to_string()
}

fn join_cells(cells: &[Vec<Span>]) -> String {
    cells
        .iter()
        .map(|cell| spans_plain(cell))
        .collect::<Vec<String>>()
        .join("  ")
}

/// Flatten inline spans to their text, dropping every markup marker.
pub(crate) fn spans_plain(spans: &[Span]) -> String {
    let mut out = String::new();
    for span in spans {
        match span {
            Span::Text(text) => out.push_str(text),
            Span::Bold(inner) | Span::Italic(inner) => out.push_str(&spans_plain(inner)),
            Span::Code(code) => out.push_str(code),
            Span::Link { text, .. } => out.push_str(&spans_plain(text)),
            Span::LineBreak => out.push('\n'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse_markdown;

    #[test]
    fn plain_text_of_empty_blocks_is_empty() {
        assert_eq!(to_plain_text(&[]), "");
        assert_eq!(to_plain_text(&[Block::Rule]), "");
    }

    #[test]
    fn code_and_links_lose_their_markup() {
        let blocks = parse_markdown("[label](../x.md) then `code`");
        assert_eq!(to_plain_text(&blocks), "label then code");
    }

    #[test]
    fn lists_and_tables_are_projected() {
        let blocks = parse_markdown("- one\n- two\n\n| a | b |\n| - | - |\n| 1 | 2 |");
        assert_eq!(to_plain_text(&blocks), "• one\n• two\n\na  b\n1  2");
        let numbered = parse_markdown("2. second\n3. third");
        assert_eq!(to_plain_text(&numbered), "2) second\n3) third");
    }

    #[test]
    fn quotes_and_code_blocks_keep_their_text() {
        let blocks = parse_markdown("> advise\n\n```\nlet x = 1;\n```");
        assert_eq!(to_plain_text(&blocks), "advise\n\nlet x = 1;");
    }
}

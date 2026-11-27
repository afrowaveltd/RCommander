//! Passive, bounded data previews. No document content is executed or resolved.
use crate::model::Model;
use egui::RichText;

const INPUT_LIMIT: usize = 256 * 1024;
const MAX_ROWS: usize = 128;
const MAX_COLS: usize = 24;
const MAX_NODES: usize = 1024;
const MAX_DEPTH: usize = 16;

#[derive(Debug)]
pub(super) enum DataPreview {
    Table { rows: Vec<Vec<String>>, limited: bool },
    Tree { root: Node, limited: bool },
    Unavailable,
}

#[derive(Debug)]
pub(super) struct Node {
    name: String,
    value: String,
    children: Vec<Node>,
}

impl DataPreview {
    /// Format hints come from the viewer's content classification, not raw extensions.
    pub(super) fn parse(hint: &str, mime: &str, text: &str, truncated: bool) -> Option<Self> {
        match hint {
            "csv" => {
                let mut end = text.len().min(INPUT_LIMIT);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let delimiter = if mime == "text/tab-separated-values" { '\t' } else { ',' };
                let partial = truncated || end < text.len();
                Some(match table(&text[..end], delimiter, partial) {
                    Ok((rows, limited)) => Self::Table { rows, limited },
                    Err(()) => Self::Unavailable,
                })
            }
            "json" | "toml" => {
                if truncated || text.len() > INPUT_LIMIT {
                    return Some(Self::Unavailable);
                }
                let value = if hint == "json" {
                    serde_json::from_str::<serde_json::Value>(text).ok()
                } else {
                    // TOML datetime values serialize as strings. Source mode preserves
                    // the exact spelling, comments, order and datetime type.
                    toml::from_str::<toml::Value>(text)
                        .ok()
                        .and_then(|value| serde_json::to_value(value).ok())
                };
                Some(match value {
                    Some(value) => {
                        let mut budget = MAX_NODES;
                        let mut limited = false;
                        let root = node("root".into(), &value, 0, &mut budget, &mut limited);
                        Self::Tree { root, limited }
                    }
                    None => Self::Unavailable,
                })
            }
            _ => None,
        }
    }

    pub(super) fn render(&self, ui: &mut egui::Ui, model: &Model, compact: bool) -> bool {
        match self {
            Self::Unavailable => {
                // Keep the ordinary source fallback available, rather than
                // inventing a partial tree for invalid or incomplete JSON/TOML.
                ui.label(model.t("viewer.data.unavailable"));
                false
            }
            Self::Table { rows, limited } => {
                let row_limit = if compact { 12 } else { MAX_ROWS };
                let col_limit = if compact { 8 } else { MAX_COLS };
                if *limited || rows.len() > row_limit || rows.iter().any(|r| r.len() > col_limit) {
                    ui.label(model.t("viewer.preview.limited"));
                }
                let viewport = ui.available_size().max(egui::vec2(1.0, 1.0));
                egui::ScrollArea::both()
                    .id_salt("data_table_scroll")
                    .max_width(viewport.x)
                    .max_height(viewport.y)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("data_table").striped(true).show(ui, |ui| {
                            // CSV need not have a header. Number every record and
                            // never discard the first one by assuming it does.
                            for (index, row) in rows.iter().take(row_limit).enumerate() {
                                ui.label(RichText::new((index + 1).to_string()).strong());
                                for cell in row.iter().take(col_limit) {
                                    ui.add(egui::Label::new(cell.as_str()).selectable(true));
                                }
                                ui.end_row();
                            }
                        });
                    });
                true
            }
            Self::Tree { root, limited } => {
                if *limited || compact {
                    ui.label(model.t("viewer.preview.limited"));
                }
                ui.label(model.t("viewer.data.normalized"));
                let viewport = ui.available_size().max(egui::vec2(1.0, 1.0));
                egui::ScrollArea::both()
                    .id_salt("data_tree_scroll")
                    .max_width(viewport.x)
                    .max_height(viewport.y)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        draw_node(ui, root, 0, compact);
                    });
                true
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Start,
    Unquoted,
    Quoted,
    Closed,
}

/// Comma/tab-separated records with doubled quotes and embedded newlines.
/// A truncated input never presents its last, possibly incomplete record.
fn table(text: &str, delimiter: char, partial: bool) -> Result<(Vec<Vec<String>>, bool), ()> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut field = Field::Start;
    let mut chars = text.chars().peekable();
    let mut limited = partial;
    let mut started = false;
    while let Some(ch) = chars.next() {
        started = true;
        if field == Field::Quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    field = Field::Closed;
                }
            } else {
                cell.push(ch);
            }
            continue;
        }
        if ch == delimiter || ch == '\n' || ch == '\r' {
            push_cell(&mut row, &mut cell, &mut limited);
            field = Field::Start;
            if ch != delimiter {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                rows.push(std::mem::take(&mut row));
                started = false;
                if rows.len() == MAX_ROWS {
                    limited |= chars.peek().is_some();
                    return Ok((rows, limited));
                }
            }
            continue;
        }
        match field {
            Field::Start if ch == '"' => field = Field::Quoted,
            Field::Start | Field::Unquoted if ch != '"' => {
                field = Field::Unquoted;
                cell.push(ch);
            }
            _ => return Err(()),
        }
    }
    if !partial {
        if field == Field::Quoted {
            return Err(());
        }
        if started {
            push_cell(&mut row, &mut cell, &mut limited);
            rows.push(row);
        }
    }
    Ok((rows, limited))
}

fn push_cell(row: &mut Vec<String>, cell: &mut String, limited: &mut bool) {
    if row.len() >= MAX_COLS {
        *limited = true;
        cell.clear();
        return;
    }
    let value = std::mem::take(cell);
    row.push(shorten(value, limited));
}

fn shorten(mut text: String, limited: &mut bool) -> String {
    if let Some((end, _)) = text.char_indices().nth(256) {
        text.truncate(end);
        text.push('…');
        *limited = true;
    }
    text
}

fn node(
    name: String,
    value: &serde_json::Value,
    depth: usize,
    budget: &mut usize,
    limited: &mut bool,
) -> Node {
    *budget -= 1;
    let name = shorten(name, limited);
    let mut children = Vec::new();
    let description = match value {
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                if depth >= MAX_DEPTH || *budget == 0 {
                    *limited = true;
                    break;
                }
                children.push(node(key.clone(), item, depth + 1, budget, limited));
            }
            format!("{{}} · {}", map.len())
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                if depth >= MAX_DEPTH || *budget == 0 {
                    *limited = true;
                    break;
                }
                children.push(node(format!("[{index}]"), item, depth + 1, budget, limited));
            }
            format!("[] · {}", items.len())
        }
        _ => shorten(value.to_string(), limited),
    };
    Node { name, value: description, children }
}

fn draw_node(ui: &mut egui::Ui, node: &Node, depth: usize, compact: bool) {
    let label = format!("{}: {}", node.name, node.value);
    if node.children.is_empty() {
        ui.add(egui::Label::new(label).selectable(true));
    } else {
        egui::CollapsingHeader::new(label)
            .default_open(depth == 0)
            .show(ui, |ui| {
                let limit = if compact { 8 } else { node.children.len() };
                if !compact || depth < 2 {
                    for (index, child) in node.children.iter().take(limit).enumerate() {
                        ui.push_id(index, |ui| draw_node(ui, child, depth + 1, compact));
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_preserves_first_record_quotes_unicode_and_multiline_fields() {
        let (rows, limited) = table("name,value\r\n\"žluťoučký, kůň\",\"a\"\"b\nc\"\r\nx,", ',', false).unwrap();
        assert!(!limited);
        assert_eq!(rows, vec![vec!["name", "value"], vec!["žluťoučký, kůň", "a\"b\nc"], vec!["x", ""]]);
    }

    #[test]
    fn tsv_and_incomplete_csv_do_not_invent_records() {
        let (rows, limited) = table("one\ttwo\nthree\tunfinished", '\t', true).unwrap();
        assert_eq!(rows, vec![vec!["one", "two"]]);
        assert!(limited);
        assert!(table("\"unclosed", ',', false).is_err());
        assert!(table("\"closed\"bad", ',', false).is_err());
        assert!(table("un\"quoted", ',', false).is_err());
    }

    #[test]
    fn table_and_tree_have_explicit_limits() {
        let text = "a,b\n".repeat(MAX_ROWS + 1);
        let (rows, limited) = table(&text, ',', false).unwrap();
        assert_eq!(rows.len(), MAX_ROWS);
        assert!(limited);
        let value = serde_json::Value::Array(vec![serde_json::Value::Null; MAX_NODES + 1]);
        let mut budget = MAX_NODES;
        let mut limited = false;
        let root = node("root".into(), &value, 0, &mut budget, &mut limited);
        assert_eq!(root.children.len(), MAX_NODES - 1);
        assert!(limited);
    }

    #[test]
    fn json_and_toml_use_parsers_and_invalid_data_falls_back() {
        for (hint, text) in [("json", "{\"key\":42}"), ("toml", "key = 42")] {
            let Some(DataPreview::Tree { root, limited }) = DataPreview::parse(hint, "", text, false) else { panic!("tree expected") };
            assert!(!limited);
            assert_eq!(root.children[0].name, "key");
            assert_eq!(root.children[0].value, "42");
        }
        assert!(matches!(DataPreview::parse("json", "", "{", false), Some(DataPreview::Unavailable)));
        assert!(matches!(DataPreview::parse("json", "", "{}", true), Some(DataPreview::Unavailable)));
        assert!(DataPreview::parse("html", "", "<html>", false).is_none());
    }
}

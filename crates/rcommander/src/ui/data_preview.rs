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
    Subtitles { rows: Vec<Vec<String>>, limited: bool },
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
            "srt" | "vtt" => {
                let mut end = text.len().min(INPUT_LIMIT);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let partial = truncated || end < text.len();
                Some(match subtitle_rows(&text[..end], hint == "vtt", partial) {
                    Ok((rows, limited)) => Self::Subtitles { rows, limited },
                    Err(()) => Self::Unavailable,
                })
            }
            "jsonl" => {
                let mut end = text.len().min(INPUT_LIMIT);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let partial = truncated || end < text.len();
                Some(match json_lines_tree(&text[..end], partial) {
                    Ok((root, limited)) => Self::Tree { root, limited },
                    Err(()) => Self::Unavailable,
                })
            }
            "ini" => {
                if truncated || text.len() > INPUT_LIMIT {
                    return Some(Self::Unavailable);
                }
                Some(match ini_tree(text) {
                    Ok((root, limited)) => Self::Tree { root, limited },
                    Err(()) => Self::Unavailable,
                })
            }
            "xml" => {
                if truncated || text.len() > INPUT_LIMIT {
                    return Some(Self::Unavailable);
                }
                Some(match xml_tree(text) {
                    Ok((root, limited)) => Self::Tree { root, limited },
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
            Self::Table { rows, limited } | Self::Subtitles { rows, limited } => {
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
                            if matches!(self, Self::Subtitles { .. }) {
                                ui.label("#");
                                for key in ["viewer.subtitle.id", "viewer.subtitle.start", "viewer.subtitle.end", "viewer.text"] {
                                    ui.label(RichText::new(model.t(key)).strong());
                                }
                                ui.end_row();
                            }
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

/// Display subtitle cues in source order, with identifier, start, end and
/// payload. Cue markup is literal text; styling and positioning are not run.
/// A truncated cue is omitted unless a complete blank separator precedes it.
fn subtitle_rows(text: &str, vtt: bool, partial: bool) -> Result<(Vec<Vec<String>>, bool), ()> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = if partial {
        text.rfind('\n').map_or("", |end| &text[..=end])
    } else {
        text
    };
    let mut lines = text.lines();
    if vtt {
        let header = lines.next().ok_or(())?;
        if header != "WEBVTT" && !header.starts_with("WEBVTT ") && !header.starts_with("WEBVTT\t") {
            return Err(());
        }
        // Header metadata is not cue text.
        for line in lines.by_ref() {
            if line.trim().is_empty() {
                break;
            }
            if line.contains("-->") {
                return Err(());
            }
        }
    }

    let mut rows = Vec::new();
    let mut block = Vec::new();
    let mut limited = partial;
    for line in lines.chain((!partial).then_some("")) {
        if !line.trim().is_empty() {
            block.push(line);
            continue;
        }
        if block.is_empty() {
            continue;
        }
        let first = block[0].trim();
        if vtt && (first == "NOTE" || first.starts_with("NOTE ") || first.starts_with("NOTE\t")
            || first == "STYLE" || first == "REGION")
        {
            block.clear();
            continue;
        }
        if rows.len() == MAX_ROWS {
            limited = true;
            break;
        }
        rows.push(subtitle_cue(&block, vtt, &mut limited)?);
        block.clear();
    }
    Ok((rows, limited))
}

fn subtitle_cue(block: &[&str], vtt: bool, limited: &mut bool) -> Result<Vec<String>, ()> {
    let first = block.first().ok_or(())?.trim();
    let (id, timing_index) = if vtt && first.contains("-->") {
        ("", 0)
    } else {
        if !vtt && (first.is_empty() || !first.bytes().all(|byte| byte.is_ascii_digit())) {
            return Err(());
        }
        (first, 1)
    };
    let timing = block.get(timing_index).ok_or(())?;
    let mut fields = timing.split_whitespace();
    let start = fields.next().ok_or(())?;
    if fields.next() != Some("-->") {
        return Err(());
    }
    let end = fields.next().ok_or(())?;
    // WebVTT has optional positioning settings. They remain in Source mode.
    // Nonstandard SRT timing extensions are not silently interpreted.
    if !vtt && fields.next().is_some() {
        return Err(());
    }
    if subtitle_timestamp(start, vtt).ok_or(())? >= subtitle_timestamp(end, vtt).ok_or(())? {
        return Err(());
    }
    let payload = block.get(timing_index + 1..).ok_or(())?.join("\n");
    Ok([id.to_owned(), start.to_owned(), end.to_owned(), payload]
        .into_iter()
        .map(|cell| shorten(cell, limited))
        .collect())
}

/// Validate the timestamp, using checked arithmetic for untrusted hour values.
/// WebVTT also permits MM:SS.mmm, whereas SRT requires HH:MM:SS,mmm.
fn subtitle_timestamp(text: &str, vtt: bool) -> Option<u64> {
    let (clock, millis) = text.split_once(if vtt { '.' } else { ',' })?;
    if millis.len() != 3 || !millis.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fields: Vec<&str> = clock.split(':').collect();
    let (hours, minutes, seconds) = match fields.as_slice() {
        [hours, minutes, seconds] if hours.len() >= 2 => (*hours, *minutes, *seconds),
        [minutes, seconds] if vtt => ("0", *minutes, *seconds),
        _ => return None,
    };
    if minutes.len() != 2 || seconds.len() != 2
        || ![hours, minutes, seconds].iter().all(|value| value.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let hours: u64 = hours.parse().ok()?;
    let minutes: u64 = minutes.parse().ok()?;
    let seconds: u64 = seconds.parse().ok()?;
    if minutes >= 60 || seconds >= 60 {
        return None;
    }
    hours.checked_mul(3_600_000)?
        .checked_add(minutes * 60_000 + seconds * 1_000)?
        .checked_add(millis.parse().ok()?)
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

/// Each complete nonblank line is an independent JSON value. Preserve its
/// physical line number, never interpret an incomplete tail as a record, and
/// share a single node budget across the entire preview.
fn json_lines_tree(text: &str, partial: bool) -> Result<(Node, bool), ()> {
    let mut root = Node {
        name: "records".into(),
        value: String::new(),
        children: Vec::new(),
    };
    let mut budget = MAX_NODES - 1;
    let mut limited = partial;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if partial && !line.ends_with('\n') {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        if root.children.len() == MAX_ROWS || budget == 0 {
            limited = true;
            break;
        }
        let value = serde_json::from_str::<serde_json::Value>(line).map_err(|_| ())?;
        root.children.push(node(
            format!("[{}]", index + 1),
            &value,
            1,
            &mut budget,
            &mut limited,
        ));
    }
    Ok((root, limited))
}

/// A conservative INI preview: sections and assignments in source order,
/// including duplicate names. No interpolation, includes or inline comment
/// stripping. Unsupported dialect syntax falls back to the original source.
fn ini_tree(text: &str) -> Result<(Node, bool), ()> {
    let mut budget = MAX_NODES;
    let mut limited = false;
    let mut root = xml_node("root".into(), String::new(), &mut budget, &mut limited)?;
    let mut section: Option<usize> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            let name = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')).ok_or(())?.trim();
            if name.is_empty() || name.contains(['[', ']']) {
                return Err(());
            }
            root.children.push(xml_node(format!("[{name}]"), String::new(), &mut budget, &mut limited)?);
            section = Some(root.children.len() - 1);
            continue;
        }
        let (key, value) = line.split_once('=').ok_or(())?;
        let key = key.trim();
        if key.is_empty() {
            return Err(());
        }
        let entry = xml_node(key.to_owned(), value.trim().to_owned(), &mut budget, &mut limited)?;
        if let Some(index) = section {
            root.children[index].children.push(entry);
        } else {
            root.children.push(entry);
        }
    }
    Ok((root, limited))
}

/// Build a display tree from XML events, preserving child order, qualified
/// names, attributes and mixed content. This is not an XML schema validator.
/// DTDs and custom entities are deliberately unsupported. No I/O resolver
/// exists here, so namespace URLs and processing instructions remain data.
fn xml_tree(text: &str) -> Result<(Node, bool), ()> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(text);
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    let mut budget = MAX_NODES;
    let mut limited = false;
    loop {
        match reader.read_event().map_err(|_| ())? {
            Event::Start(tag) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(());
                }
                let item = xml_element(&tag, &mut budget, &mut limited)?;
                stack.push(item);
            }
            Event::Empty(tag) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(());
                }
                let item = xml_element(&tag, &mut budget, &mut limited)?;
                xml_attach(&mut stack, &mut root, item)?;
            }
            Event::End(_) => {
                // Reader's default end-name check rejects mismatched tags.
                let item = stack.pop().ok_or(())?;
                xml_attach(&mut stack, &mut root, item)?;
            }
            Event::Text(text) => {
                let value = std::str::from_utf8(text.as_ref()).map_err(|_| ())?;
                if !value.trim().is_empty() {
                    xml_leaf(&mut stack, "#text", value.to_owned(), &mut budget, &mut limited)?;
                }
            }
            Event::CData(text) => {
                let value = std::str::from_utf8(text.as_ref()).map_err(|_| ())?;
                xml_leaf(&mut stack, "#CDATA", value.to_owned(), &mut budget, &mut limited)?;
            }
            Event::GeneralRef(reference) => {
                let name = std::str::from_utf8(reference.as_ref()).map_err(|_| ())?;
                // Only predefined XML entities and numeric references. Never
                // consult a DTD, custom resolver, filesystem or network.
                let escaped = format!("&{name};");
                let value = quick_xml::escape::unescape(&escaped).map_err(|_| ())?;
                xml_leaf(&mut stack, "#text", value.into_owned(), &mut budget, &mut limited)?;
            }
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) => {
                // Comments/declarations are available in the source; PIs
                // are not instructions to this application.
            }
            Event::DocType(_) => return Err(()),
            Event::Eof => {
                if !stack.is_empty() {
                    return Err(());
                }
                return root.map(|root| (root, limited)).ok_or(());
            }
        }
    }
}

fn xml_element(
    tag: &quick_xml::events::BytesStart<'_>,
    budget: &mut usize,
    limited: &mut bool,
) -> Result<Node, ()> {
    let name = std::str::from_utf8(tag.name().as_ref()).map_err(|_| ())?.to_owned();
    let mut item = xml_node(name, String::new(), budget, limited)?;
    for attr in tag.attributes() {
        let attr = attr.map_err(|_| ())?;
        let key = std::str::from_utf8(attr.key.as_ref()).map_err(|_| ())?;
        let raw = std::str::from_utf8(attr.value.as_ref()).map_err(|_| ())?;
        let value = quick_xml::escape::unescape(raw).map_err(|_| ())?;
        item.children.push(xml_node(format!("@{key}"), value.into_owned(), budget, limited)?);
    }
    Ok(item)
}

fn xml_node(
    name: String,
    value: String,
    budget: &mut usize,
    limited: &mut bool,
) -> Result<Node, ()> {
    if *budget == 0 {
        return Err(());
    }
    *budget -= 1;
    Ok(Node {
        name: shorten(name, limited),
        value: shorten(value, limited),
        children: Vec::new(),
    })
}

fn xml_attach(stack: &mut [Node], root: &mut Option<Node>, item: Node) -> Result<(), ()> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(item);
    } else if root.is_none() {
        *root = Some(item);
    } else {
        return Err(());
    }
    Ok(())
}

fn xml_leaf(
    stack: &mut [Node],
    name: &str,
    value: String,
    budget: &mut usize,
    limited: &mut bool,
) -> Result<(), ()> {
    let parent = stack.last_mut().ok_or(())?;
    parent.children.push(xml_node(name.into(), value, budget, limited)?);
    Ok(())
}

fn draw_node(ui: &mut egui::Ui, node: &Node, depth: usize, compact: bool) {
    let label = if node.value.is_empty() {
        node.name.clone()
    } else {
        format!("{}: {}", node.name, node.value)
    };
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
    fn srt_preview_preserves_identifiers_timestamps_and_multiline_text() {
        let input = "1\r\n00:00:01,000 --> 00:00:03,250\r\nPříliš žluťoučký\r\nkůň\r\n\r\n7\r\n00:00:04,000 --> 00:00:05,000\r\n<i>Hello</i>";
        let Some(DataPreview::Subtitles { rows, limited }) = DataPreview::parse("srt", "", input, false) else {
            panic!("subtitle table expected");
        };
        assert!(!limited);
        assert_eq!(rows, vec![
            vec!["1", "00:00:01,000", "00:00:03,250", "Příliš žluťoučký\nkůň"],
            vec!["7", "00:00:04,000", "00:00:05,000", "<i>Hello</i>"],
        ]);
    }

    #[test]
    fn webvtt_handles_named_cues_settings_notes_and_short_timestamps() {
        let input = "WEBVTT - Lesson\nLanguage: en\n\nNOTE teacher note\nignore me\n\nSTYLE\n::cue { color: red }\n\nintro\n00:01.000 --> 00:02.500 align:start\nHello\n\n00:03.000 --> 00:04.000\nWorld";
        let Some(DataPreview::Subtitles { rows, limited }) = DataPreview::parse("vtt", "", input, false) else {
            panic!("WebVTT table expected");
        };
        assert!(!limited);
        assert_eq!(rows, vec![
            vec!["intro", "00:01.000", "00:02.500", "Hello"],
            vec!["", "00:03.000", "00:04.000", "World"],
        ]);
    }

    #[test]
    fn subtitles_reject_invalid_timestamps_and_omit_truncated_cues() {
        for input in [
            "1\n00:00:02,000 --> 00:00:01,000\nWrong",
            "1\n00:00:60,000 --> 00:01:01,000\nWrong",
            "1\n00:00:01.000 --> 00:00:02,000\nWrong",
            "one\n00:00:01,000 --> 00:00:02,000\nWrong",
        ] {
            assert!(matches!(DataPreview::parse("srt", "", input, false), Some(DataPreview::Unavailable)));
        }
        assert!(matches!(DataPreview::parse("vtt", "", "not WEBVTT\n\n00:01.000 --> 00:02.000\nText", false), Some(DataPreview::Unavailable)));
        let input = "1\n00:00:01,000 --> 00:00:02,000\nComplete\n\n2\n00:00:03,000 --> 00:00:04,000\nPartial\n";
        let (rows, limited) = subtitle_rows(input, false, true).unwrap();
        assert!(limited);
        assert_eq!(rows.len(), 1);
        let (rows, limited) = subtitle_rows("1\n00:00:01,000 --> 00:00:02,000\nComplete\n\n", false, true).unwrap();
        assert!(limited);
        assert_eq!(rows.len(), 1);
        let input = "1\n00:00:01,000 --> 00:00:02,000\nText\n\n".repeat(MAX_ROWS + 1);
        let (rows, limited) = subtitle_rows(&input, false, false).unwrap();
        assert!(limited);
        assert_eq!(rows.len(), MAX_ROWS);
    }

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

    #[test]
    fn json_lines_preview_preserves_record_order_and_physical_line_numbers() {
        let input = "{\"event\":\"start\"}\r\n\n[1,2]\nnull\n\"žluťoučký\"";
        let Some(DataPreview::Tree { root, limited }) = DataPreview::parse("jsonl", "", input, false) else {
            panic!("JSON Lines tree expected");
        };
        assert!(!limited);
        assert_eq!(root.children.len(), 4);
        assert_eq!(root.children[0].name, "[1]");
        assert_eq!(root.children[0].children[0].name, "event");
        assert_eq!(root.children[1].name, "[3]");
        assert_eq!(root.children[2].value, "null");
        assert_eq!(root.children[3].value, "\"žluťoučký\"");
    }

    #[test]
    fn json_lines_drops_partial_tail_but_rejects_invalid_complete_records() {
        let (root, limited) = json_lines_tree("{}\n{\"unfinished", true).unwrap();
        assert!(limited);
        assert_eq!(root.children.len(), 1);
        assert!(json_lines_tree("{}\nnot json\n", true).is_err());
        assert!(json_lines_tree("{}\n{", false).is_err());
        assert!(matches!(DataPreview::parse("jsonl", "", "not json", false), Some(DataPreview::Unavailable)));
        let (root, limited) = json_lines_tree("{}\n", true).unwrap();
        assert!(limited);
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn json_lines_limits_records_nodes_and_utf8_input() {
        let input = "{}\n".repeat(MAX_ROWS + 1);
        let (root, limited) = json_lines_tree(&input, false).unwrap();
        assert!(limited);
        assert_eq!(root.children.len(), MAX_ROWS);
        let input = format!("[{}]\ntrue", "0,".repeat(MAX_NODES) + "0");
        let (root, limited) = json_lines_tree(&input, false).unwrap();
        assert!(limited);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].children.len(), MAX_NODES - 2);
        let input = format!("{{}}\n\"{}\"", "ě".repeat(INPUT_LIMIT));
        let Some(DataPreview::Tree { root, limited }) = DataPreview::parse("jsonl", "", &input, false) else {
            panic!("bounded JSON Lines tree expected");
        };
        assert!(limited);
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn ini_preview_preserves_order_duplicates_and_literal_values() {
        let input = "global = first\n; comment\n[server]\nkey = žluťoučký\nkey=second=part # literal\n[server]\nempty=\npath=${HOME}/file\n";
        let Some(DataPreview::Tree { root, limited }) = DataPreview::parse("ini", "", input, false) else {
            panic!("INI tree expected");
        };
        assert!(!limited);
        assert_eq!(root.children[0].name, "global");
        assert_eq!(root.children[0].value, "first");
        assert_eq!(root.children[1].name, "[server]");
        assert_eq!(root.children[1].children[0].value, "žluťoučký");
        assert_eq!(root.children[1].children[1].name, "key");
        assert_eq!(root.children[1].children[1].value, "second=part # literal");
        assert_eq!(root.children[2].name, "[server]");
        assert_eq!(root.children[2].children[0].value, "");
        assert_eq!(root.children[2].children[1].value, "${HOME}/file");
    }

    #[test]
    fn ini_preview_falls_back_for_unsupported_or_oversized_input() {
        for input in ["[bad", "[]", "=value", "include other.ini", "[a][b]", "key: value"] {
            assert!(matches!(DataPreview::parse("ini", "", input, false), Some(DataPreview::Unavailable)));
        }
        let wide = "key=value\n".repeat(MAX_NODES);
        assert!(matches!(DataPreview::parse("ini", "", &wide, false), Some(DataPreview::Unavailable)));
        let large = "x".repeat(INPUT_LIMIT + 1);
        assert!(matches!(DataPreview::parse("ini", "", &large, false), Some(DataPreview::Unavailable)));
        assert!(matches!(DataPreview::parse("ini", "", "key=value", true), Some(DataPreview::Unavailable)));
        let long = format!("key={}", "ě".repeat(300));
        let (root, limited) = ini_tree(&long).unwrap();
        assert!(limited);
        assert!(root.children[0].value.ends_with('…'));
    }

    #[test]
    fn xml_preview_preserves_attributes_order_and_text() {
        let (root, limited) = xml_tree(r#"<root xmlns:x="urn:x" id="A&amp;B"><x:item>žluťoučký</x:item><empty/><![CDATA[a<b]]>&lt;</root>"#).unwrap();
        assert!(!limited);
        assert_eq!(root.name, "root");
        assert_eq!(root.children[0].name, "@xmlns:x");
        assert_eq!(root.children[1].value, "A&B");
        assert_eq!(root.children[2].name, "x:item");
        assert_eq!(root.children[2].children[0].value, "žluťoučký");
        assert_eq!(root.children[3].name, "empty");
        assert_eq!(root.children[4].value, "a<b");
        assert_eq!(root.children[5].value, "<");
    }

    #[test]
    fn xml_preview_rejects_dtd_entities_and_invalid_structure() {
        for text in [
            r#"<!DOCTYPE root SYSTEM "file:///private"><root/>"#,
            r#"<!DOCTYPE root [<!ENTITY x "test">]><root>&x;</root>"#,
            "<root>&unknown;</root>",
            "<root a='&unknown;'/>",
            "<root><a></root>",
            "<root>",
            "<a/><b/>",
            "text<root/>",
            "<root a='1' a='2'/>",
        ] {
            assert!(xml_tree(text).is_err(), "{text}");
        }
        let deep = "<a>".repeat(MAX_DEPTH + 1) + &"</a>".repeat(MAX_DEPTH + 1);
        assert!(xml_tree(&deep).is_err());
        let wide = format!("<a>{}</a>", "<b/>".repeat(MAX_NODES));
        assert!(xml_tree(&wide).is_err());
        assert!(matches!(DataPreview::parse("xml", "", "<a/>", true), Some(DataPreview::Unavailable)));
    }
}

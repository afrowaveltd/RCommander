//! A deliberately small markdown parser that never fails.
//!
//! The block model is exactly what a rich egui renderer needs — headings,
//! paragraphs, lists, tables, fenced code, quotes and rules — and nothing else.
//! [`parse_markdown`] is total: malformed input is tolerated, never reported.
//!
//! # Supported subset
//!
//! * ATX headings `#` … `######` (optional closing `#` sequence).
//! * Paragraphs; a soft line break becomes a space, a hard break (two trailing
//!   spaces or a trailing backslash) becomes [`Span::LineBreak`].
//! * Bullet lists with `-`, `*` or `+`; ordered lists with `1.` or `1)`.
//! * Fenced code blocks opened with ``` or ~~~ (three or more characters) and
//!   an optional info string whose first word is the language.
//! * Inline code, `**bold**`/`__bold__`, `*italic*`/`_italic_`, `[text](target)`
//!   links, and `\`-escapes.
//! * Link destinations keep their spaces (`[x](../01 - Keyboard shortcuts/en.md)`);
//!   only a trailing `"title"`, `'title'` or `(title)` is dropped.
//! * `---`/`***`/`___` rules and `>` block quotes.
//! * GFM pipe tables with `:---`, `:---:` and `---:` alignment.
//!
//! # Tolerances
//!
//! * A UTF-8 BOM at the start of the document is ignored.
//! * CRLF and lone CR line endings are normalised to LF.
//! * Tabs count as whitespace for block and list detection; they are kept
//!   verbatim inside code blocks and inline text.
//! * An unterminated code fence swallows the rest of the document.
//! * Unclosed inline markers are emitted literally instead of being dropped.
//! * A table row with too many cells is truncated to the header width, a row
//!   with too few cells is padded with empty cells (never an error).
//! * A table (and any other block) that is not recognised at all degrades to a
//!   paragraph, so arbitrary plain text yields exactly one [`Block::Paragraph`].
//! * Indented (four-space) code blocks are *not* recognised; such lines are
//!   ordinary paragraphs.
//! * Inline nesting is depth-limited, so pathological input cannot overflow the
//!   stack.

/// Horizontal alignment of a table column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    /// Left aligned — also the default for columns without a `:` marker.
    Left,
    /// Centered (`:---:`).
    Center,
    /// Right aligned (`---:`).
    Right,
}

/// Inline markup inside a block.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Span {
    /// Literal text.
    Text(String),
    /// Strong emphasis: `**text**` or `__text__`.
    Bold(Vec<Span>),
    /// Emphasis: `*text*` or `_text_`.
    Italic(Vec<Span>),
    /// Inline code: `` `text` ``.
    Code(String),
    /// A link `[text](target)`; `target` is the raw destination.
    Link {
        /// Link text, possibly with nested inline markup.
        text: Vec<Span>,
        /// Raw link destination, e.g. `../Previewer/en.md`.
        target: String,
    },
    /// A hard line break inside a paragraph.
    LineBreak,
}

/// A block-level markdown element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// An ATX heading with a level between 1 and 6.
    Heading {
        /// Heading level, `1` for `#`.
        level: u8,
        /// Inline content of the heading.
        spans: Vec<Span>,
    },
    /// A paragraph of inline content.
    Paragraph(Vec<Span>),
    /// An unordered list; one span vector per item.
    BulletList(Vec<Vec<Span>>),
    /// An ordered list; only the first item's number is preserved.
    NumberedList {
        /// Number of the first item.
        start: u64,
        /// Inline content per item.
        items: Vec<Vec<Span>>,
    },
    /// A GFM pipe table.
    Table {
        /// One span vector per header cell.
        header: Vec<Vec<Span>>,
        /// One alignment per column; same width as `header`.
        align: Vec<Align>,
        /// Rows of cells; every row has exactly `header.len()` cells.
        rows: Vec<Vec<Vec<Span>>>,
    },
    /// A fenced code block.
    CodeBlock {
        /// Info-string language, empty when absent.
        language: String,
        /// Code lines, verbatim and without the fence.
        lines: Vec<String>,
    },
    /// A block quote; lines are joined with [`Span::LineBreak`].
    Quote(Vec<Span>),
    /// A horizontal rule.
    Rule,
}

/// Maximum nesting depth for inline markup; deeper markers stay literal.
const MAX_INLINE_DEPTH: u32 = 8;

/// Parse `text` into blocks. Never fails and never panics.
///
/// An empty (or whitespace-only) document yields an empty vector; anything else
/// yields at least one block.
pub fn parse_markdown(text: &str) -> Vec<Block> {
    let normalized = normalize(text);
    if normalized.trim().is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = normalized.split('\n').collect();
    // A trailing newline is not content; without this an unterminated fence at
    // the end of a file would gain a phantom empty code line.
    if lines.last().map(|line| line.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let text_line = lines[i];
        let trimmed = text_line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if let Some(fence) = Fence::open(trimmed) {
            let (block, next) = parse_fence(&lines, i, fence);
            blocks.push(block);
            i = next.max(i + 1);
            continue;
        }
        if let Some((level, rest)) = atx_heading(trimmed) {
            blocks.push(Block::Heading {
                level,
                spans: parse_inline(rest),
            });
            i += 1;
            continue;
        }
        if is_rule(trimmed) {
            blocks.push(Block::Rule);
            i += 1;
            continue;
        }
        if trimmed.starts_with('>') {
            let (block, next) = parse_quote(&lines, i);
            blocks.push(block);
            i = next.max(i + 1);
            continue;
        }
        if is_table_start(&lines, i) {
            let (block, next) = parse_table(&lines, i);
            blocks.push(block);
            i = next.max(i + 1);
            continue;
        }
        if is_bullet(trimmed) {
            let (block, next) = parse_list(&lines, i, ListKind::Bullet);
            blocks.push(block);
            i = next.max(i + 1);
            continue;
        }
        if is_numbered(trimmed) {
            let (block, next) = parse_list(&lines, i, ListKind::Numbered);
            blocks.push(block);
            i = next.max(i + 1);
            continue;
        }

        let (block, next) = parse_paragraph(&lines, i);
        blocks.push(block);
        i = next.max(i + 1);
    }
    blocks
}

/// Strip a BOM and normalise all line endings to `\n`.
pub(crate) fn normalize(text: &str) -> String {
    let stripped = text.strip_prefix('\u{feff}').unwrap_or(text);
    if !stripped.contains('\r') {
        return stripped.to_string();
    }
    stripped.replace("\r\n", "\n").replace('\r', "\n")
}

// ---------------------------------------------------------------- block level

/// A recognised code fence opener.
#[derive(Clone, Copy, Debug)]
struct Fence {
    marker: char,
    len: usize,
}

impl Fence {
    fn open(trimmed: &str) -> Option<Fence> {
        let marker = trimmed.chars().next()?;
        if marker != '`' && marker != '~' {
            return None;
        }
        let len = trimmed.chars().take_while(|&c| c == marker).count();
        if len < 3 {
            return None;
        }
        let info = &trimmed[len..];
        // A backtick fence may not have backticks in its info string; that is an
        // inline code span, not a fence.
        if marker == '`' && info.contains('`') {
            return None;
        }
        Some(Fence { marker, len })
    }

    fn closes(self, trimmed: &str) -> bool {
        let len = trimmed.chars().take_while(|&c| c == self.marker).count();
        len >= self.len && trimmed[len..].trim().is_empty()
    }
}

fn parse_fence(lines: &[&str], start: usize, fence: Fence) -> (Block, usize) {
    let opener = lines[start].trim();
    let info = &opener[fence.len..];
    let language = match info.split_whitespace().next() {
        Some(token) if !token.starts_with('{') => token.to_string(),
        _ => String::new(),
    };
    let mut body: Vec<String> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        if fence.closes(lines[i].trim()) {
            i += 1;
            break;
        }
        body.push(lines[i].to_string());
        i += 1;
    }
    (
        Block::CodeBlock {
            language,
            lines: body,
        },
        i,
    )
}

fn atx_heading(trimmed: &str) -> Option<(u8, &str)> {
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    let mut text = rest.trim();
    if text.ends_with('#') {
        let without = text.trim_end_matches('#');
        if without.is_empty() || without.ends_with(' ') || without.ends_with('\t') {
            text = without.trim_end();
        }
    }
    Some((level as u8, text))
}

fn is_rule(trimmed: &str) -> bool {
    if trimmed.chars().count() < 3 {
        return false;
    }
    let mut marker: Option<char> = None;
    let mut count = 0usize;
    for c in trimmed.chars() {
        match c {
            '-' | '*' | '_' => {
                if let Some(prev) = marker
                    && prev != c
                {
                    return false;
                }
                marker = Some(c);
                count += 1;
            }
            ' ' | '\t' => {}
            _ => return false,
        }
    }
    count >= 3
}

fn is_quote(trimmed: &str) -> bool {
    trimmed.starts_with('>')
}

fn parse_quote(lines: &[&str], start: usize) -> (Block, usize) {
    let mut spans: Vec<Span> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if !trimmed.starts_with('>') {
            break;
        }
        if i > start {
            spans.push(Span::LineBreak);
        }
        let content = &trimmed[1..];
        let content = content.strip_prefix(' ').unwrap_or(content);
        spans.extend(parse_inline(content));
        i += 1;
    }
    (Block::Quote(spans), i)
}

fn bullet_rest(trimmed: &str) -> Option<&str> {
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '-' | '*' | '+') {
        return None;
    }
    let rest = &trimmed[marker.len_utf8()..];
    if rest.is_empty() {
        return Some("");
    }
    if rest.starts_with(' ') || rest.starts_with('\t') {
        Some(rest.trim_start())
    } else {
        None
    }
}

fn numbered_rest(trimmed: &str) -> Option<(u64, &str)> {
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let rest = &trimmed[digits..];
    let marker = rest.chars().next()?;
    if marker != '.' && marker != ')' {
        return None;
    }
    let after = &rest[marker.len_utf8()..];
    let start: u64 = trimmed[..digits].parse().ok()?;
    if after.is_empty() {
        return Some((start, ""));
    }
    if after.starts_with(' ') || after.starts_with('\t') {
        Some((start, after.trim_start()))
    } else {
        None
    }
}

fn is_bullet(trimmed: &str) -> bool {
    bullet_rest(trimmed).is_some()
}

fn is_numbered(trimmed: &str) -> bool {
    numbered_rest(trimmed).is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Numbered,
}

/// Parse a bullet or ordered list starting at `start`.
///
/// Continuation lines that are not themselves list items or other block starts
/// are appended to the current item. Blank lines inside a list are tolerated
/// when the next non-blank line continues the list.
fn parse_list(lines: &[&str], start: usize, kind: ListKind) -> (Block, usize) {
    let mut items: Vec<Vec<Span>> = Vec::new();
    let mut first_number: u64 = 1;
    let mut i = start;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            let mut j = i;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            let continues = j < lines.len()
                && match kind {
                    ListKind::Bullet => is_bullet(lines[j].trim()),
                    ListKind::Numbered => is_numbered(lines[j].trim()),
                };
            if continues {
                i = j;
                continue;
            }
            break;
        }
        let mut content: Vec<&str> = Vec::new();
        match kind {
            ListKind::Bullet => match bullet_rest(trimmed) {
                Some(rest) => content.push(rest),
                None => break,
            },
            ListKind::Numbered => match numbered_rest(trimmed) {
                Some((number, rest)) => {
                    if items.is_empty() {
                        first_number = number;
                    }
                    content.push(rest);
                }
                None => break,
            },
        }
        i += 1;
        while i < lines.len() {
            let cont = lines[i].trim();
            if cont.is_empty() {
                break;
            }
            if is_bullet(cont) || is_numbered(cont) {
                break;
            }
            if starts_block(lines, i) {
                break;
            }
            content.push(lines[i]);
            i += 1;
        }
        items.push(paragraph_spans(&content));
    }
    let block = match kind {
        ListKind::Bullet => Block::BulletList(items),
        ListKind::Numbered => Block::NumberedList {
            start: first_number,
            items,
        },
    };
    (block, i)
}

fn parse_paragraph(lines: &[&str], start: usize) -> (Block, usize) {
    let mut content: Vec<&str> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            break;
        }
        if i > start && starts_block(lines, i) {
            break;
        }
        content.push(lines[i]);
        i += 1;
    }
    if content.is_empty() {
        // Should be unreachable (callers never pass a block start), but a
        // paragraph must still be produced rather than panicking on a slice.
        content.push(lines.get(start).copied().unwrap_or(""));
        i = start + 1;
    }
    (Block::Paragraph(paragraph_spans(&content)), i)
}

/// True when the line at `index` begins a block other than a paragraph.
fn starts_block(lines: &[&str], index: usize) -> bool {
    let trimmed = lines[index].trim();
    if trimmed.is_empty() {
        return true;
    }
    Fence::open(trimmed).is_some()
        || atx_heading(trimmed).is_some()
        || is_rule(trimmed)
        || is_quote(trimmed)
        || is_bullet(trimmed)
        || is_numbered(trimmed)
        || is_table_start(lines, index)
}

/// Build paragraph spans from raw lines: soft breaks become a space, hard
/// breaks become [`Span::LineBreak`].
fn paragraph_spans(lines: &[&str]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let total = lines.len();
    for (index, raw) in lines.iter().enumerate() {
        let trailing_whitespace = raw
            .chars()
            .rev()
            .take_while(|&c| c == ' ' || c == '\t')
            .count();
        let trimmed = raw.trim();
        let ends_with_backslash = trimmed.ends_with('\\');
        let hard_break = trailing_whitespace >= 2 || ends_with_backslash;
        let text = if ends_with_backslash {
            trimmed.trim_end_matches('\\').trim_end()
        } else {
            trimmed
        };
        spans.extend(parse_inline(text));
        if hard_break {
            spans.push(Span::LineBreak);
        } else if index + 1 < total {
            spans.push(Span::Text(" ".to_string()));
        }
    }
    merge_text(spans)
}

// ---------------------------------------------------------------------- table

/// Split a pipe-table row into trimmed-capable raw cell slices, honouring
/// `\|` escapes and optional leading/trailing pipes.
fn split_cells(line: &str) -> Vec<&str> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let body = strip_trailing_pipe(body);
    let bytes = body.as_bytes();
    let mut cells: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'|' => {
                cells.push(&body[start..i]);
                start = i + 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    cells.push(&body[start..]);
    cells
}

fn strip_trailing_pipe(s: &str) -> &str {
    if !s.ends_with('|') {
        return s;
    }
    let without = &s[..s.len() - 1];
    let backslashes = without.chars().rev().take_while(|&c| c == '\\').count();
    if backslashes % 2 == 0 { without } else { s }
}

fn is_delimiter_cell(cell: &str) -> bool {
    let cell = cell.trim();
    if cell.is_empty() {
        return false;
    }
    let core = cell.strip_prefix(':').unwrap_or(cell);
    let core = core.strip_suffix(':').unwrap_or(core);
    !core.is_empty() && core.chars().all(|c| c == '-')
}

fn cell_align(cell: &str) -> Align {
    let cell = cell.trim();
    let left = cell.starts_with(':');
    let right = cell.ends_with(':') && cell.chars().count() > 1;
    match (left, right) {
        (true, true) => Align::Center,
        (false, true) => Align::Right,
        _ => Align::Left,
    }
}

fn is_table_start(lines: &[&str], index: usize) -> bool {
    if index + 1 >= lines.len() {
        return false;
    }
    let head = lines[index].trim();
    if !head.contains('|') {
        return false;
    }
    let delimiter = lines[index + 1].trim();
    if !delimiter.contains('-') {
        return false;
    }
    let head_cells = split_cells(head);
    let delimiter_cells = split_cells(delimiter);
    // GFM requires the delimiter row to have exactly as many cells as the
    // header row; without that rule a paragraph followed by a `---` rule would
    // be mistaken for a one-column table.
    !delimiter_cells.is_empty()
        && head_cells.len() == delimiter_cells.len()
        && delimiter_cells.iter().all(|c| is_delimiter_cell(c))
}

fn parse_table(lines: &[&str], start: usize) -> (Block, usize) {
    let mut header: Vec<Vec<Span>> = split_cells(lines[start])
        .iter()
        .map(|c| parse_inline(c.trim()))
        .collect();
    let mut align: Vec<Align> = split_cells(lines[start + 1])
        .iter()
        .map(|c| cell_align(c))
        .collect();
    let columns = header.len().max(align.len()).max(1);
    header.resize(columns, Vec::new());
    align.resize(columns, Align::Left);
    let mut rows: Vec<Vec<Vec<Span>>> = Vec::new();
    let mut i = start + 2;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() || !trimmed.contains('|') {
            break;
        }
        if Fence::open(trimmed).is_some() || atx_heading(trimmed).is_some() || is_rule(trimmed) {
            break;
        }
        let mut cells: Vec<Vec<Span>> = split_cells(lines[i])
            .iter()
            .map(|c| parse_inline(c.trim()))
            .collect();
        // Tolerate misaligned rows: extra cells are ignored, missing cells are
        // padded with empty cells so every row has the header width.
        cells.resize(columns, Vec::new());
        rows.push(cells);
        i += 1;
    }
    (
        Block::Table {
            header,
            align,
            rows,
        },
        i,
    )
}

// --------------------------------------------------------------------- inline

fn parse_inline(text: &str) -> Vec<Span> {
    merge_text(parse_inline_at(text, 0))
}

fn parse_inline_at(text: &str, depth: u32) -> Vec<Span> {
    if text.is_empty() {
        return Vec::new();
    }
    if depth > MAX_INLINE_DEPTH {
        return vec![Span::Text(text.to_string())];
    }
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut out: Vec<Span> = Vec::new();
    let mut buf = String::new();
    let mut i = 0usize;
    while i < len {
        match chars[i] {
            '\\' => {
                if i + 1 < len && !chars[i + 1].is_alphanumeric() {
                    buf.push(chars[i + 1]);
                    i += 2;
                } else {
                    buf.push('\\');
                    i += 1;
                }
            }
            '`' => {
                let run = run_len(&chars, i, '`');
                match find_closer_run(&chars, i + run, '`', run) {
                    Some(j) => {
                        flush_text(&mut out, &mut buf);
                        let raw: String = chars[i + run..j].iter().collect();
                        let code = strip_code_padding(&raw);
                        out.push(Span::Code(code));
                        i = j + run;
                    }
                    None => {
                        buf.push('`');
                        i += 1;
                    }
                }
            }
            '$' if i + 1 < len
                && !chars[i + 1].is_whitespace()
                && chars[i + 1] != '$'
                && (i == 0 || chars[i - 1] != '$') =>
            {
                // Complete supported expressions only. Code, currency and an
                // unfinished streaming formula must remain literal.
                let end = (i + 1..len.min(i + 257)).find(|&j| chars[j] == '$');
                if let Some(j) = end.filter(|&j| j + 1 == len || chars[j + 1] != '$') {
                    let raw: String = chars[i + 1..j].iter().collect();
                    if let Some(rendered) = simple_inline_math(&raw) {
                        buf.push_str(&rendered);
                        i = j + 1;
                        continue;
                    }
                }
                buf.push('$');
                i += 1;
            }
            marker @ ('*' | '_') => {
                let run = run_len(&chars, i, marker);
                let need = if run >= 3 { 3 } else { run };
                let opens = marker != '_' || i == 0 || !chars[i - 1].is_alphanumeric();
                let closer = if opens {
                    find_emphasis_closer(&chars, i + need, marker, need)
                } else {
                    None
                };
                match closer {
                    Some(j) => {
                        flush_text(&mut out, &mut buf);
                        let inner: String = chars[i + need..j].iter().collect();
                        let inner = parse_inline_at(&inner, depth + 1);
                        out.push(match need {
                            3 => Span::Bold(vec![Span::Italic(inner)]),
                            2 => Span::Bold(inner),
                            _ => Span::Italic(inner),
                        });
                        i = j + need;
                    }
                    None => {
                        for _ in 0..need {
                            buf.push(marker);
                        }
                        i += need;
                    }
                }
            }
            '[' => match try_link(&chars, i, depth) {
                Some((text_spans, target, next)) => {
                    flush_text(&mut out, &mut buf);
                    out.push(Span::Link {
                        text: text_spans,
                        target,
                    });
                    i = next;
                }
                None => {
                    buf.push('[');
                    i += 1;
                }
            },
            other => {
                buf.push(other);
                i += 1;
            }
        }
    }
    flush_text(&mut out, &mut buf);
    merge_text(out)
}

/// Small dependency-free display subset for numeric powers, e.g. O(n^2).
/// Unsupported syntax keeps its original delimiters and content.
fn simple_inline_math(raw: &str) -> Option<String> {
    if !raw.contains('^')
        || !raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " ()+-=*/.,^{}".contains(c))
        || raw.ends_with(' ')
    {
        return None;
    }
    let mut chars = raw.chars().peekable();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c != '^' {
            if matches!(c, '{' | '}') {
                return None;
            }
            out.push(c);
            continue;
        }
        if !out
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == ')')
        {
            return None;
        }
        let braced = chars.peek() == Some(&'{');
        if braced {
            chars.next();
        }
        let mut digits = 0;
        if chars.peek() == Some(&'-') {
            chars.next();
            out.push('⁻');
        }
        while let Some(c) = chars.peek().copied() {
            let sup = match c {
                '0' => '⁰',
                '1' => '¹',
                '2' => '²',
                '3' => '³',
                '4' => '⁴',
                '5' => '⁵',
                '6' => '⁶',
                '7' => '⁷',
                '8' => '⁸',
                '9' => '⁹',
                _ => break,
            };
            chars.next();
            out.push(sup);
            digits += 1;
        }
        if digits == 0 || (braced && chars.next() != Some('}')) {
            return None;
        }
    }
    Some(out)
}

#[cfg(test)]
mod inline_math_tests {
    use super::*;

    #[test]
    fn inline_math_numeric_powers_render_without_dollars() {
        assert_eq!(
            parse_inline("Time: $O(n^2)$"),
            vec![Span::Text("Time: O(n²)".into())]
        );
        assert_eq!(parse_inline("$x^{-10}$"), vec![Span::Text("x⁻¹⁰".into())]);
    }

    #[test]
    fn inline_math_preserves_currency_code_incomplete_and_unsupported() {
        for input in ["$5 and $10", "$O(n^2)", "$x^y$", "$$x^2$$"] {
            assert_eq!(parse_inline(input), vec![Span::Text(input.into())]);
        }
        assert_eq!(parse_inline(r"\$x^2$"), vec![Span::Text("$x^2$".into())]);
        assert_eq!(
            parse_inline("\x60$O(n^2)$\x60"),
            vec![Span::Code("$O(n^2)$".into())]
        );
    }
}

fn strip_code_padding(raw: &str) -> String {
    if raw.len() >= 2 && raw.starts_with(' ') && raw.ends_with(' ') && !raw.trim().is_empty() {
        raw.trim().to_string()
    } else {
        raw.to_string()
    }
}

fn try_link(chars: &[char], at: usize, depth: u32) -> Option<(Vec<Span>, String, usize)> {
    let len = chars.len();
    let mut nesting = 0usize;
    let mut close = None;
    let mut j = at + 1;
    while j < len {
        match chars[j] {
            '\\' => {
                j += 2;
                continue;
            }
            '[' => nesting += 1,
            ']' => {
                if nesting == 0 {
                    close = Some(j);
                    break;
                }
                nesting -= 1;
            }
            _ => {}
        }
        j += 1;
    }
    let close = close?;
    if close + 1 >= len || chars[close + 1] != '(' {
        return None;
    }
    let mut parens = 0usize;
    let mut end = None;
    let mut k = close + 2;
    while k < len {
        match chars[k] {
            '\\' => {
                k += 2;
                continue;
            }
            '(' => parens += 1,
            ')' => {
                if parens == 0 {
                    end = Some(k);
                    break;
                }
                parens -= 1;
            }
            _ => {}
        }
        k += 1;
    }
    let end = end?;
    let text_source: String = chars[at + 1..close].iter().collect();
    let target_source: String = chars[close + 2..end].iter().collect();
    let target = normalize_target(&target_source)?;
    Some((parse_inline_at(&text_source, depth + 1), target, end + 1))
}

fn normalize_target(source: &str) -> Option<String> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    let source = if source.len() >= 2 && source.starts_with('<') && source.ends_with('>') {
        &source[1..source.len() - 1]
    } else {
        source
    };
    // Drop an optional trailing link title — `dest "title"`, `dest 'title'` or
    // `dest (title)`. Anything else, spaces included, is part of the target:
    // help files do legitimately link to folders such as `01 - Topic/en.md`.
    let target = strip_link_title(source).trim();
    if target.is_empty() {
        None
    } else {
        Some(target.to_string())
    }
}

fn strip_link_title(source: &str) -> &str {
    let mut previous_was_space = false;
    for (index, character) in source.char_indices() {
        if previous_was_space {
            let rest = source[index..].trim_start();
            let quoted = (rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2)
                || (rest.starts_with('\'') && rest.ends_with('\'') && rest.len() >= 2)
                || (rest.starts_with('(') && rest.ends_with(')') && rest.len() >= 2);
            if quoted {
                return source[..index].trim_end();
            }
        }
        previous_was_space = character == ' ' || character == '\t';
    }
    source
}

fn run_len(chars: &[char], at: usize, marker: char) -> usize {
    let mut count = 0usize;
    let mut i = at;
    while i < chars.len() && chars[i] == marker {
        count += 1;
        i += 1;
    }
    count
}

fn find_closer_run(chars: &[char], from: usize, marker: char, need: usize) -> Option<usize> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == marker && run_len(chars, i, marker) >= need {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_emphasis_closer(chars: &[char], from: usize, marker: char, need: usize) -> Option<usize> {
    let len = chars.len();
    let mut i = from;
    while i < len {
        match chars[i] {
            '\\' => i += 2,
            '`' => {
                let run = run_len(chars, i, '`');
                match find_closer_run(chars, i + run, '`', run) {
                    Some(j) => i = j + run,
                    None => i += run,
                }
            }
            c if c == marker => {
                if run_len(chars, i, marker) >= need {
                    let after = i + need;
                    let boundary_ok =
                        marker != '_' || after >= len || !chars[after].is_alphanumeric();
                    if boundary_ok {
                        return Some(i);
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn flush_text(out: &mut Vec<Span>, buf: &mut String) {
    if !buf.is_empty() {
        out.push(Span::Text(std::mem::take(buf)));
    }
}

/// Merge adjacent [`Span::Text`] values and drop empty ones.
fn merge_text(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        match span {
            Span::Text(text) => {
                if text.is_empty() {
                    continue;
                }
                match out.last_mut() {
                    Some(Span::Text(previous)) => previous.push_str(&text),
                    _ => out.push(Span::Text(text)),
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Span {
        Span::Text(value.to_string())
    }

    fn plain(blocks: &[Block]) -> String {
        crate::to_plain_text(blocks)
    }

    #[test]
    fn empty_and_whitespace_only_yield_no_blocks() {
        assert!(parse_markdown("").is_empty());
        assert!(parse_markdown("   \n\t\n  ").is_empty());
    }

    #[test]
    fn plain_text_becomes_one_paragraph() {
        let blocks = parse_markdown("Just some text without markup.");
        assert_eq!(
            blocks,
            vec![Block::Paragraph(vec![text(
                "Just some text without markup."
            )])]
        );

        let multi = parse_markdown("first line\nsecond line");
        assert_eq!(multi.len(), 1);
        assert_eq!(
            multi[0],
            Block::Paragraph(vec![text("first line second line")])
        );

        let weird = parse_markdown("C:\\Users\\x {json: true} 100% <=> |");
        assert_eq!(weird.len(), 1);
        assert!(matches!(weird[0], Block::Paragraph(_)));
    }

    #[test]
    fn headings_levels_and_closing_hashes() {
        let blocks = parse_markdown("# One\n###### Six\n####### seven\n# C#\n## Title ##\n");
        assert_eq!(
            blocks[0],
            Block::Heading {
                level: 1,
                spans: vec![text("One")]
            }
        );
        assert_eq!(
            blocks[1],
            Block::Heading {
                level: 6,
                spans: vec![text("Six")]
            }
        );
        // seven hashes is not a heading
        assert!(matches!(blocks[2], Block::Paragraph(_)));
        assert_eq!(
            blocks[3],
            Block::Heading {
                level: 1,
                spans: vec![text("C#")]
            }
        );
        assert_eq!(
            blocks[4],
            Block::Heading {
                level: 2,
                spans: vec![text("Title")]
            }
        );
    }

    #[test]
    fn headings_without_space_are_not_headings() {
        let blocks = parse_markdown("#NoSpace");
        assert!(matches!(blocks[0], Block::Paragraph(_)));
    }

    #[test]
    fn paragraphs_split_on_blank_lines() {
        let blocks = parse_markdown("first\n\nsecond\n\n\nthird");
        assert_eq!(blocks.len(), 3);
        assert_eq!(plain(&blocks), "first\n\nsecond\n\nthird");
    }

    #[test]
    fn hard_break_and_soft_break() {
        let blocks = parse_markdown("a  \nb\nc");
        assert_eq!(
            blocks[0],
            Block::Paragraph(vec![text("a"), Span::LineBreak, text("b c")])
        );
        let backslash = parse_markdown("a\\\nb");
        assert_eq!(
            backslash[0],
            Block::Paragraph(vec![text("a"), Span::LineBreak, text("b")])
        );
    }

    #[test]
    fn bullet_lists_all_markers_and_continuations() {
        let blocks = parse_markdown("- one\n* two\n+ three\n  continued");
        assert_eq!(
            blocks[0],
            Block::BulletList(vec![
                vec![text("one")],
                vec![text("two")],
                vec![text("three continued")],
            ])
        );
    }

    #[test]
    fn bullet_list_with_blank_line_between_items() {
        let blocks = parse_markdown("- one\n\n- two\n");
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0],
            Block::BulletList(vec![vec![text("one")], vec![text("two")]])
        );
    }

    #[test]
    fn numbered_lists_keep_start_number() {
        let blocks = parse_markdown("3. third\n4. fourth");
        assert_eq!(
            blocks[0],
            Block::NumberedList {
                start: 3,
                items: vec![vec![text("third")], vec![text("fourth")]]
            }
        );
        let alternate = parse_markdown("1) alt");
        assert_eq!(
            alternate[0],
            Block::NumberedList {
                start: 1,
                items: vec![vec![text("alt")]]
            }
        );
    }

    #[test]
    fn loose_numbered_lists_stay_one_list() {
        // A blank line between items keeps the list together and the number of
        // the first item.
        let blocks = parse_markdown("3. third\n\n4. fourth");
        assert_eq!(
            blocks[0],
            Block::NumberedList {
                start: 3,
                items: vec![vec![text("third")], vec![text("fourth")]]
            }
        );
    }

    #[test]
    fn a_decimal_number_is_not_an_ordered_list() {
        let blocks = parse_markdown("1.5 metres is short");
        assert!(matches!(blocks[0], Block::Paragraph(_)));
    }

    #[test]
    fn rules() {
        for source in ["---", "***", "___", "- - -", "  ----  "] {
            let blocks = parse_markdown(source);
            assert_eq!(blocks, vec![Block::Rule], "source: {source:?}");
        }
    }

    #[test]
    fn quotes_join_lines() {
        let blocks = parse_markdown("> first\n> second\n\nafter");
        assert_eq!(
            blocks[0],
            Block::Quote(vec![text("first"), Span::LineBreak, text("second")])
        );
        assert_eq!(blocks[1], Block::Paragraph(vec![text("after")]));
        assert_eq!(parse_markdown(">")[0], Block::Quote(vec![]));
    }

    #[test]
    fn fenced_code_blocks() {
        let blocks = parse_markdown("```rust\nfn main() {}\n\n  indented\n```\n\nafter");
        assert_eq!(
            blocks[0],
            Block::CodeBlock {
                language: "rust".to_string(),
                lines: vec![
                    "fn main() {}".to_string(),
                    String::new(),
                    "  indented".to_string()
                ],
            }
        );
        assert_eq!(blocks[1], Block::Paragraph(vec![text("after")]));

        let tilde = parse_markdown("~~~\nplain\n~~~");
        assert_eq!(
            tilde[0],
            Block::CodeBlock {
                language: String::new(),
                lines: vec!["plain".to_string()]
            }
        );
    }

    #[test]
    fn heading_inside_fence_is_code() {
        let blocks = parse_markdown("```\n# not a heading\n```");
        assert!(matches!(blocks[0], Block::CodeBlock { .. }));
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn unterminated_fence_swallows_the_rest() {
        let blocks = parse_markdown("```txt\nline one\n# still code\n");
        assert_eq!(
            blocks,
            vec![Block::CodeBlock {
                language: "txt".to_string(),
                lines: vec!["line one".to_string(), "# still code".to_string()],
            }]
        );
    }

    #[test]
    fn inline_code_and_escapes() {
        let blocks = parse_markdown("Use `cargo test -p rc-help` now");
        assert_eq!(
            blocks[0],
            Block::Paragraph(vec![
                text("Use "),
                Span::Code("cargo test -p rc-help".to_string()),
                text(" now")
            ])
        );
        let escaped = parse_markdown("literal \\*star\\* and \\`tick\\`");
        assert_eq!(
            escaped[0],
            Block::Paragraph(vec![text("literal *star* and `tick`")])
        );
        // unclosed backtick stays literal
        let unclosed = parse_markdown("a ` b");
        assert_eq!(unclosed[0], Block::Paragraph(vec![text("a ` b")]));
        // padded code span is trimmed like CommonMark
        let padded = parse_markdown("`` ` ``");
        assert_eq!(
            padded[0],
            Block::Paragraph(vec![Span::Code("`".to_string())])
        );
    }

    #[test]
    fn bold_italic_and_nesting() {
        let blocks = parse_markdown("**bold** and *italic* and _also italic_");
        assert_eq!(
            blocks[0],
            Block::Paragraph(vec![
                Span::Bold(vec![text("bold")]),
                text(" and "),
                Span::Italic(vec![text("italic")]),
                text(" and "),
                Span::Italic(vec![text("also italic")]),
            ])
        );
        let nested = parse_markdown("**outer *inner* outer**");
        assert_eq!(
            nested[0],
            Block::Paragraph(vec![Span::Bold(vec![
                text("outer "),
                Span::Italic(vec![text("inner")]),
                text(" outer"),
            ])])
        );
        let triple = parse_markdown("***both***");
        assert_eq!(
            triple[0],
            Block::Paragraph(vec![Span::Bold(vec![Span::Italic(vec![text("both")])])])
        );
        let underscore = parse_markdown("__strong__");
        assert_eq!(
            underscore[0],
            Block::Paragraph(vec![Span::Bold(vec![text("strong")])])
        );
    }

    #[test]
    fn intraword_underscores_are_literal() {
        let blocks = parse_markdown("snake_case_name stays");
        assert_eq!(
            blocks[0],
            Block::Paragraph(vec![text("snake_case_name stays")])
        );
    }

    #[test]
    fn pathological_nesting_does_not_panic() {
        for source in [
            "**bold *italic***",
            "***",
            "****",
            "_",
            "__",
            "*a**b***c",
            "[a](b",
            "[a](",
            "[]()",
            "[**x**](y) _[z](w)_",
            "**".repeat(200).as_str(),
            "*".repeat(500).as_str(),
            "`".repeat(500).as_str(),
            "[".repeat(200).as_str(),
            "> ".repeat(100).as_str(),
            "#".repeat(200).as_str(),
        ] {
            let blocks = parse_markdown(source);
            let _ = plain(&blocks);
        }
        // the nested-blocks case must also stay sane
        let mixed = parse_markdown("**bold *italic* bold** rest");
        assert!(matches!(mixed[0], Block::Paragraph(_)));
    }

    #[test]
    fn links() {
        let blocks =
            parse_markdown("See [the previewer](../Previewer/en.md) and [home](Previewer).");
        assert_eq!(
            blocks[0],
            Block::Paragraph(vec![
                text("See "),
                Span::Link {
                    text: vec![text("the previewer")],
                    target: "../Previewer/en.md".to_string(),
                },
                text(" and "),
                Span::Link {
                    text: vec![text("home")],
                    target: "Previewer".to_string()
                },
                text("."),
            ])
        );
        let titled = parse_markdown("[x](page.md \"title\")");
        assert_eq!(
            titled[0],
            Block::Paragraph(vec![Span::Link {
                text: vec![text("x")],
                target: "page.md".to_string(),
            }])
        );
        // Angle brackets are dropped, spaces inside the target are kept.
        let angle = parse_markdown("[x](<a b.md>)");
        assert!(
            matches!(&angle[0], Block::Paragraph(spans) if matches!(&spans[0], Span::Link { target, .. } if target == "a b.md"))
        );
        // A trailing title is dropped instead.
        let parenthesised = parse_markdown("[x](page.md (a title))");
        assert!(
            matches!(&parenthesised[0], Block::Paragraph(spans) if matches!(&spans[0], Span::Link { target, .. } if target == "page.md"))
        );
        // A help tree link whose folder name contains spaces survives intact.
        let spaced = parse_markdown("[x](../01 - Keyboard shortcuts/en.md)");
        assert!(
            matches!(&spaced[0], Block::Paragraph(spans) if matches!(&spans[0], Span::Link { target, .. } if target == "../01 - Keyboard shortcuts/en.md"))
        );
        let broken = parse_markdown("[not a link");
        assert_eq!(broken[0], Block::Paragraph(vec![text("[not a link")]));
    }

    #[test]
    fn tables_with_alignment() {
        let source = "| Name | Age | City |\n| :--- | :---: | ---: |\n| Ann | 31 | Brno |\n| Bob | 7 | Ostrava |\n";
        let blocks = parse_markdown(source);
        assert_eq!(
            blocks[0],
            Block::Table {
                header: vec![vec![text("Name")], vec![text("Age")], vec![text("City")]],
                align: vec![Align::Left, Align::Center, Align::Right],
                rows: vec![
                    vec![vec![text("Ann")], vec![text("31")], vec![text("Brno")]],
                    vec![vec![text("Bob")], vec![text("7")], vec![text("Ostrava")]],
                ],
            }
        );
    }

    #[test]
    fn table_without_outer_pipes() {
        let blocks = parse_markdown("a | b\n--- | ---\n1 | 2");
        assert_eq!(
            blocks[0],
            Block::Table {
                header: vec![vec![text("a")], vec![text("b")]],
                align: vec![Align::Left, Align::Left],
                rows: vec![vec![vec![text("1")], vec![text("2")]]],
            }
        );
    }

    #[test]
    fn table_with_misaligned_rows_is_tolerated() {
        let blocks = parse_markdown("| a | b |\n| --- | --- |\n| 1 | 2 | 3 |\n| 4 |\n");
        let Block::Table {
            header,
            align,
            rows,
        } = &blocks[0]
        else {
            panic!("expected table, got {:?}", blocks);
        };
        assert_eq!(header.len(), 2);
        assert_eq!(align.len(), 2);
        assert_eq!(rows.len(), 2);
        // extra cell dropped
        assert_eq!(rows[0], vec![vec![text("1")], vec![text("2")]]);
        // missing cell padded with an empty cell
        assert_eq!(rows[1], vec![vec![text("4")], Vec::<Span>::new()]);
    }

    #[test]
    fn escaped_pipe_inside_table_cell() {
        let blocks = parse_markdown("| a | b |\n| --- | --- |\n| x \\| y | z |\n");
        let Block::Table { rows, .. } = &blocks[0] else {
            panic!("expected table, got {:?}", blocks);
        };
        assert_eq!(rows[0], vec![vec![text("x | y")], vec![text("z")]]);
    }

    #[test]
    fn pipes_in_prose_are_not_a_table() {
        let blocks = parse_markdown("a | b\n\nnot a table | just text");
        assert!(blocks.iter().all(|b| matches!(b, Block::Paragraph(_))));
    }

    #[test]
    fn crlf_bom_and_tabs() {
        let source = "\u{feff}# Title\r\n\r\n- one\r\n\t- nested tab\r\n\r\nText\twith\ttabs\r\n";
        let blocks = parse_markdown(source);
        assert_eq!(
            blocks[0],
            Block::Heading {
                level: 1,
                spans: vec![text("Title")]
            }
        );
        assert_eq!(
            blocks[1],
            Block::BulletList(vec![vec![text("one")], vec![text("nested tab")]])
        );
        assert_eq!(blocks[2], Block::Paragraph(vec![text("Text\twith\ttabs")]));
    }

    #[test]
    fn lone_carriage_returns_are_line_endings() {
        let blocks = parse_markdown("# a\r- b\r# c");
        assert_eq!(blocks.len(), 3);
        assert!(matches!(blocks[0], Block::Heading { level: 1, .. }));
        assert!(matches!(blocks[1], Block::BulletList(_)));
        assert!(matches!(blocks[2], Block::Heading { level: 1, .. }));
    }

    #[test]
    fn a_rich_document_parses_completely() {
        let source = "\
# Previewer

Open files with **F3** and use `Esc` to close.

- fast
- read only

1. first
2. second

> quoted advice

| Key | Action |
| :-- | :----: |
| F3 | view |

```rust
fn main() {}
```

---

[Settings](../Settings/en.md) for more.
";
        let blocks = parse_markdown(source);
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, Block::Heading { level: 1, .. }))
        );
        assert!(blocks.iter().any(|b| matches!(b, Block::Table { .. })));
        assert!(blocks.iter().any(|b| matches!(b, Block::CodeBlock { .. })));
        assert!(blocks.iter().any(|b| matches!(b, Block::Rule)));
        assert!(blocks.iter().any(|b| matches!(b, Block::Quote(_))));
        assert!(blocks.iter().any(|b| matches!(b, Block::BulletList(_))));
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, Block::NumberedList { .. }))
        );
        let text = plain(&blocks);
        assert!(text.contains("Open files with F3 and use Esc to close."));
        assert!(!text.contains("**"));
        assert!(!text.contains('`'));
        assert!(!text.contains('#'));
    }
}

//! Topic discovery, the help index and document loading.
//!
//! # On-disk layout
//!
//! Help content is a plain directory tree, one folder per topic and one
//! markdown file per language inside it:
//!
//! ```text
//! Docs/
//!   Previewer/
//!     en.md
//!     cs.md
//!   FTP connections/
//!     en.md
//!   Advanced/
//!     Packing/
//!       en.md
//!   Keyboard shortcuts/
//!     en.md
//! ```
//!
//! Topic folders may be nested to any depth (bounded only by
//! [`MAX_WALK_DEPTH`]). The folder name is the topic id; an optional
//! `NN - ` prefix supplies the sort order. Nothing is compiled in: dropping a
//! new folder or a new `<lang>.md` file into the tree is all it takes to add a
//! topic or a translation.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{HelpError, Result};
use crate::markdown::{Block, Span, parse_markdown};
use crate::plain::spans_plain;

/// Maximum directory depth walked below the help root.
pub const MAX_WALK_DEPTH: usize = 32;

/// Separator used in [`HelpTopic::relative_path`] on every platform.
pub const PATH_SEPARATOR: char = '/';

/// One help topic: a single folder in the help tree.
///
/// The struct is plain data; it is produced by [`HelpIndex::load`] and only
/// read by the GUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpTopic {
    /// Topic id — the folder name with an optional `NN - ` ordering prefix
    /// removed, e.g. `"FTP connections"`.
    pub id: String,
    /// Path of the topic below the help root using `/` separators, e.g.
    /// `"FTP connections"` or `"Advanced/Packing"`.
    pub relative_path: String,
    /// Language code → title, taken from the first markdown heading of that
    /// language's file. Keys are lower-case language codes.
    pub title: BTreeMap<String, String>,
    /// Sort order parsed from an `NN - Topic` folder prefix, or [`u32::MAX`]
    /// when the folder has no prefix.
    pub order: u32,
    /// Languages this topic exists in, sorted, lower-case, e.g. `["cs", "en"]`.
    pub languages: Vec<String>,
}

impl HelpTopic {
    /// Title for `lang`, falling back to `fallback`.
    ///
    /// Resolution order: the requested language, the fallback language, and
    /// finally any language the topic happens to have (alphabetically first).
    /// Returns `None` only when the topic has no title at all, which cannot
    /// happen for a topic found on disk.
    pub fn title_for(&self, lang: &str, fallback: &str) -> Option<&str> {
        let requested = normalize_language(lang);
        if let Some(title) = self.title.get(&requested) {
            return Some(title.as_str());
        }
        let fallback = normalize_language(fallback);
        if let Some(title) = self.title.get(&fallback) {
            return Some(title.as_str());
        }
        self.title.values().next().map(|title| title.as_str())
    }
}

/// A parsed help document, ready for rendering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpDocument {
    /// Id of the topic the document belongs to.
    pub topic_id: String,
    /// Language that was actually used (after fallback).
    pub language: String,
    /// Language the caller asked for.
    pub requested_language: String,
    /// True when a language other than the requested one was used.
    pub used_fallback: bool,
    /// Title taken from the first heading, falling back to the topic id.
    pub title: String,
    /// File the content was read from.
    pub source_path: PathBuf,
    /// Parsed markdown blocks.
    pub blocks: Vec<Block>,
    /// Link destinations found in the document, in document order, deduplicated.
    pub link_targets: Vec<String>,
}

/// Index of every help topic below a root directory.
///
/// Build it with [`HelpIndex::load`]. A missing root is not an error: the
/// resulting index is simply empty.
#[derive(Clone, Debug)]
pub struct HelpIndex {
    root: PathBuf,
    topics: Vec<HelpTopic>,
    /// topic id → language → markdown file on disk.
    files: BTreeMap<String, BTreeMap<String, PathBuf>>,
    /// lower-case alias (id, relative path or raw folder name) → topic id.
    alias: BTreeMap<String, String>,
}

/// A topic as found while walking the tree.
struct Found {
    topic: HelpTopic,
    raw_dir_name: String,
    files: BTreeMap<String, PathBuf>,
}

/// Walking state: visited canonical directories and everything found so far.
struct WalkState {
    visited: BTreeSet<PathBuf>,
    found: Vec<Found>,
}

impl HelpIndex {
    /// Scan `root` recursively for topic folders.
    ///
    /// A directory becomes a topic when it directly contains at least one
    /// `<lang>.md` file; `.md` files anywhere else are ignored, as are hidden
    /// files and folders. The root directory itself never becomes a topic, but
    /// its sub-folders do.
    ///
    /// # Errors
    ///
    /// A missing root yields an empty index, not an error. Only a genuine,
    /// non-`NotFound` I/O failure while inspecting the root is reported.
    pub fn load(root: &Path) -> Result<HelpIndex> {
        let mut state = WalkState {
            visited: BTreeSet::new(),
            found: Vec::new(),
        };
        match fs::metadata(root) {
            Ok(meta) if meta.is_dir() => walk(root, "", 0, &mut state),
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(HelpError::Io {
                    path: root.to_path_buf(),
                    source: err,
                });
            }
        }

        let mut found = state.found;
        // Deterministic de-duplication: two folders can map to the same id
        // ("01 - Previewer" and "Previewer"); keep the lexicographically first
        // relative path so the result never depends on directory order.
        found.sort_by(|a, b| {
            a.topic
                .id
                .cmp(&b.topic.id)
                .then_with(|| a.topic.relative_path.cmp(&b.topic.relative_path))
        });
        let mut seen: BTreeSet<String> = BTreeSet::new();
        found.retain(|entry| seen.insert(entry.topic.id.clone()));

        let mut topics: Vec<HelpTopic> = Vec::with_capacity(found.len());
        let mut files: BTreeMap<String, BTreeMap<String, PathBuf>> = BTreeMap::new();
        let mut alias: BTreeMap<String, String> = BTreeMap::new();
        for entry in found {
            alias
                .entry(entry.topic.id.to_lowercase())
                .or_insert_with(|| entry.topic.id.clone());
            alias
                .entry(entry.topic.relative_path.to_lowercase())
                .or_insert_with(|| entry.topic.id.clone());
            alias
                .entry(entry.raw_dir_name.to_lowercase())
                .or_insert_with(|| entry.topic.id.clone());
            files.insert(entry.topic.id.clone(), entry.files);
            topics.push(entry.topic);
        }
        topics.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
        Ok(HelpIndex {
            root: root.to_path_buf(),
            topics,
            files,
            alias,
        })
    }

    /// Root directory the index was built from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// All topics, sorted by `(order, id)`.
    pub fn topics(&self) -> &[HelpTopic] {
        &self.topics
    }

    /// Union of all topic languages, sorted.
    pub fn languages(&self) -> Vec<String> {
        self.topics
            .iter()
            .flat_map(|topic| topic.languages.iter().cloned())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect()
    }

    /// Look up a topic by id.
    ///
    /// The lookup is forgiving: it accepts the id, the full relative path, the
    /// raw folder name (with an ordering prefix) and the last component of a
    /// path, all case-insensitively.
    pub fn topic(&self, id: &str) -> Option<&HelpTopic> {
        let needle = id.trim();
        if needle.is_empty() {
            return None;
        }
        if let Some(topic) = self.topics.iter().find(|topic| topic.id == needle) {
            return Some(topic);
        }
        if let Some(topic) = self.by_alias(needle) {
            return Some(topic);
        }
        let last = needle
            .rsplit([PATH_SEPARATOR, '\\'])
            .next()
            .unwrap_or(needle)
            .trim();
        if !last.is_empty()
            && last != needle
            && let Some(topic) = self.by_alias(last)
        {
            return Some(topic);
        }
        None
    }

    fn by_alias(&self, key: &str) -> Option<&HelpTopic> {
        let key = key.trim().to_lowercase();
        let id = self.alias.get(&key)?;
        self.topics.iter().find(|topic| &topic.id == id)
    }

    /// Load the body of topic `id`, falling back to `fallback_lang`.
    ///
    /// Returns `None` for an unknown topic or when the file cannot be read.
    ///
    /// Fallback rules: the requested language is used when the topic has it;
    /// otherwise `fallback_lang` is used with [`HelpDocument::used_fallback`]
    /// set; otherwise the topic's alphabetically first language is used, again
    /// flagged as a fallback, so a page always shows something rather than
    /// nothing.
    pub fn body(&self, id: &str, lang: &str, fallback_lang: &str) -> Option<HelpDocument> {
        let topic = self.topic(id)?;
        let topic_files = self.files.get(&topic.id)?;
        let requested = normalize_language(lang);
        let fallback = normalize_language(fallback_lang);
        let (used, used_fallback) = pick_language(&topic.languages, &requested, &fallback)?;
        let path = topic_files.get(&used)?.clone();
        let bytes = fs::read(&path).ok()?;
        let text = decode_text(&bytes);
        let blocks = parse_markdown(&text);
        let title = topic
            .title
            .get(&used)
            .cloned()
            .or_else(|| first_heading(&blocks))
            .unwrap_or_else(|| topic.id.clone());
        let link_targets = collect_link_targets(&blocks);
        Some(HelpDocument {
            topic_id: topic.id.clone(),
            language: used,
            requested_language: requested,
            used_fallback,
            title,
            source_path: path,
            blocks,
            link_targets,
        })
    }

    /// Resolve a markdown link target found in `from_topic` to another topic id.
    ///
    /// Accepted forms are `./Previewer/en.md`, `Previewer`, `../Settings/en.md`,
    /// `/Docs/Settings/en.md`, `Advanced/Packing` and the raw folder name. Links
    /// to the topic itself resolve to that topic; external links (`http://…`,
    /// `mailto:…`), bare in-page anchors (`#section`) and anything that does not
    /// name a known topic yield `None`.
    ///
    /// Segments are resolved relative to the topic's parent folder; `..`
    /// beyond the help root is clamped to the root instead of failing.
    pub fn resolve_link(&self, from_topic: &str, link: &str) -> Option<String> {
        let raw = link.trim();
        let target = strip_angle_brackets(raw).trim();
        if target.is_empty() || target.starts_with('#') {
            return None;
        }
        let lowered = target.to_ascii_lowercase();
        if lowered.contains("://")
            || lowered.starts_with("mailto:")
            || lowered.starts_with("ftp:")
            || lowered.starts_with("tel:")
        {
            return None;
        }
        let without_anchor = match target.find('#') {
            Some(position) => &target[..position],
            None => target,
        };
        let path = without_anchor.trim();
        if path.is_empty() {
            // "#anchor" only: a link inside the current page.
            return None;
        }
        let unified = path.replace('\\', "/");
        let absolute = unified.starts_with(PATH_SEPARATOR);
        let mut segments: Vec<String> = unified
            .split(PATH_SEPARATOR)
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect();
        if segments
            .first()
            .map(|segment| segment.eq_ignore_ascii_case("docs"))
            .unwrap_or(false)
        {
            segments.remove(0);
        }
        // A trailing `<lang>.md` names the file inside the topic folder; other
        // `.md` names are treated as a folder name so links to a folder file
        // still have a chance to resolve.
        if let Some(last) = segments.last().cloned()
            && let Some(stem) = markdown_stem(&last)
        {
            segments.pop();
            if !is_language_tag(&normalize_language(stem)) {
                let (_, name) = split_order_prefix(stem);
                if !name.is_empty() {
                    segments.push(name);
                }
            }
        }

        let mut resolved: Vec<String> = Vec::new();
        if !absolute && let Some(from) = self.topic(from_topic) {
            let mut parent: Vec<&str> = from.relative_path.split(PATH_SEPARATOR).collect();
            parent.pop();
            resolved.extend(parent.into_iter().map(|segment| segment.to_string()));
        }
        for segment in segments {
            match segment.as_str() {
                "." => {}
                ".." => {
                    resolved.pop();
                }
                other => resolved.push(other.to_string()),
            }
        }

        if resolved.is_empty() {
            // "./en.md" style link: the page links to itself.
            return self.topic(from_topic).map(|topic| topic.id.clone());
        }
        let candidate = resolved.join("/");
        if let Some(topic) = self.find_topic_by_path(&candidate) {
            return Some(topic.id.clone());
        }
        if let Some(stripped) = strip_prefix_of_last(&candidate)
            && let Some(topic) = self.find_topic_by_path(&stripped)
        {
            return Some(topic.id.clone());
        }
        let last = resolved.last()?.clone();
        if let Some(topic) = self.find_topic_by_path(&last) {
            return Some(topic.id.clone());
        }
        let (_, name) = split_order_prefix(&last);
        if name != last
            && let Some(topic) = self.find_topic_by_path(&name)
        {
            return Some(topic.id.clone());
        }
        None
    }

    fn find_topic_by_path(&self, candidate: &str) -> Option<&HelpTopic> {
        let key = candidate.trim().trim_matches(PATH_SEPARATOR).to_lowercase();
        if key.is_empty() {
            return None;
        }
        if let Some(topic) = self
            .topics
            .iter()
            .find(|topic| topic.relative_path.to_lowercase() == key)
        {
            return Some(topic);
        }
        self.by_alias(&key)
    }

    /// Report which of the `expected` languages a topic is missing.
    ///
    /// Returns `(topic id, missing languages)` in index order, only for topics
    /// that miss at least one expected language. Empty and duplicate entries in
    /// `expected` are ignored.
    pub fn missing_translations(&self, expected: &[String]) -> Vec<(String, Vec<String>)> {
        let mut wanted: Vec<String> = Vec::new();
        for language in expected {
            let language = normalize_language(language);
            if language.is_empty() || wanted.contains(&language) {
                continue;
            }
            wanted.push(language);
        }
        if wanted.is_empty() {
            return Vec::new();
        }
        let mut report: Vec<(String, Vec<String>)> = Vec::new();
        for topic in &self.topics {
            let missing: Vec<String> = wanted
                .iter()
                .filter(|language| !topic.languages.iter().any(|have| have == *language))
                .cloned()
                .collect();
            if !missing.is_empty() {
                report.push((topic.id.clone(), missing));
            }
        }
        report
    }
}

// ------------------------------------------------------------- walking the tree

fn walk(dir: &Path, relative_parent: &str, depth: usize, state: &mut WalkState) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    // Canonicalising every directory makes symlink cycles harmless: a folder
    // that resolves to an already visited directory is skipped.
    let canonical = match fs::canonicalize(dir) {
        Ok(canonical) => canonical,
        Err(_) => return,
    };
    if !state.visited.insert(canonical) {
        return;
    }
    let reader = match fs::read_dir(dir) {
        Ok(reader) => reader,
        Err(_) => return,
    };

    let mut subdirectories: Vec<(String, PathBuf)> = Vec::new();
    let mut markdown_files: Vec<(String, PathBuf)> = Vec::new();
    for entry in reader.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        let (is_dir, is_file) = if file_type.is_symlink() {
            // Follow links once; `visited` stops cycles further down.
            match fs::metadata(&path) {
                Ok(metadata) => (metadata.is_dir(), metadata.is_file()),
                Err(_) => continue,
            }
        } else {
            (file_type.is_dir(), file_type.is_file())
        };
        if is_dir {
            subdirectories.push((name, path));
        } else if is_file && let Some(language) = language_of_file(&path) {
            markdown_files.push((language, path));
        }
    }

    subdirectories.sort_by(|a, b| a.0.cmp(&b.0));
    markdown_files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    // Relative path of *this* directory below the root; the root itself is "".
    let raw_dir_name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string_lossy().into_owned());
    let (order, dir_id) = if depth == 0 {
        (u32::MAX, String::new())
    } else {
        let (order, name) = split_order_prefix(&raw_dir_name);
        let name = if name.is_empty() {
            raw_dir_name.trim().to_string()
        } else {
            name
        };
        (order, name)
    };
    let own_relative = if dir_id.is_empty() {
        String::new()
    } else if relative_parent.is_empty() {
        dir_id.clone()
    } else {
        format!("{relative_parent}{PATH_SEPARATOR}{dir_id}")
    };

    if depth > 0 && !markdown_files.is_empty() {
        let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut titles: BTreeMap<String, String> = BTreeMap::new();
        for (language, path) in markdown_files {
            titles.insert(language.clone(), title_of(&path).unwrap_or_default());
            files.entry(language).or_insert(path);
        }
        let id = dir_id.clone();
        let relative_path = own_relative.clone();
        let languages: Vec<String> = files.keys().cloned().collect();
        let title: BTreeMap<String, String> = languages
            .iter()
            .map(|language| {
                let title = titles
                    .get(language)
                    .filter(|title| !title.is_empty())
                    .cloned()
                    .unwrap_or_else(|| id.clone());
                (language.clone(), title)
            })
            .collect();
        state.found.push(Found {
            topic: HelpTopic {
                id,
                relative_path,
                title,
                order,
                languages,
            },
            raw_dir_name,
            files,
        });
    }

    for (_name, path) in subdirectories {
        // The child appends its own folder name, so it only needs this
        // directory's relative path as its parent.
        walk(&path, &own_relative, depth + 1, state);
    }
}

/// Language code of a `<lang>.md` file, or `None` for any other file.
fn language_of_file(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_string_lossy();
    if !extension.eq_ignore_ascii_case("md") {
        return None;
    }
    let stem = path.file_stem()?.to_string_lossy();
    let language = normalize_language(&stem);
    if is_language_tag(&language) {
        Some(language)
    } else {
        None
    }
}

/// True for a plausible language tag: ASCII letters/digits/`-`, up to 16 chars.
fn is_language_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.chars().count() <= 16
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Normalise a language code: trimmed, lower-case, `_` replaced by `-`.
pub(crate) fn normalize_language(language: &str) -> String {
    language.trim().to_lowercase().replace('_', "-")
}

/// Split an `NN - Topic` folder name into its order and its display name.
fn split_order_prefix(name: &str) -> (u32, String) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return (u32::MAX, String::new());
    }
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 10 {
        return (u32::MAX, trimmed.to_string());
    }
    let rest = &trimmed[digits.len()..];
    let mut body = rest;
    let had_whitespace = body.len() != body.trim_start().len();
    body = body.trim_start();
    let separator = body.chars().next();
    if matches!(separator, Some('-' | '.' | ')' | ':' | '_')) {
        let skipped = separator.map(|c| c.len_utf8()).unwrap_or(0);
        body = body[skipped..].trim_start();
    } else if !had_whitespace {
        // "3D viewer" is a name, not an ordering prefix.
        return (u32::MAX, trimmed.to_string());
    }
    let body = body.trim();
    if body.is_empty() {
        return (u32::MAX, trimmed.to_string());
    }
    let order = digits.parse::<u32>().unwrap_or(u32::MAX);
    (order, body.to_string())
}

fn strip_prefix_of_last(candidate: &str) -> Option<String> {
    let mut parts: Vec<&str> = candidate.split(PATH_SEPARATOR).collect();
    let last = parts.pop()?;
    let (_, name) = split_order_prefix(last);
    if name == last || name.is_empty() {
        return None;
    }
    parts.push(&name);
    Some(parts.join("/"))
}

fn markdown_stem(name: &str) -> Option<&str> {
    let (stem, extension) = name.rsplit_once('.')?;
    if extension.eq_ignore_ascii_case("md") && !stem.is_empty() {
        Some(stem)
    } else {
        None
    }
}

fn strip_angle_brackets(link: &str) -> &str {
    if link.len() >= 2 && link.starts_with('<') && link.ends_with('>') {
        &link[1..link.len() - 1]
    } else {
        link
    }
}

// ---------------------------------------------------------------- document bits

fn pick_language(languages: &[String], requested: &str, fallback: &str) -> Option<(String, bool)> {
    if !requested.is_empty() && languages.iter().any(|language| language == requested) {
        return Some((requested.to_string(), false));
    }
    if !fallback.is_empty() && languages.iter().any(|language| language == fallback) {
        return Some((fallback.to_string(), true));
    }
    languages.first().cloned().map(|language| (language, true))
}

/// Read the first heading of a markdown file; never fails.
fn title_of(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let text = decode_text(&bytes);
    first_heading(&parse_markdown(&text))
}

/// Decode a help file, tolerating anything the user may have saved.
///
/// UTF-16 files with a BOM (the "Unicode" option of Windows Notepad) are
/// decoded properly; every other byte sequence is decoded lossily, so a stray
/// invalid byte turns into U+FFFD instead of failing the whole page.
fn decode_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 {
        let (skip, little_endian) = match (bytes[0], bytes[1]) {
            (0xFF, 0xFE) => (2, true),
            (0xFE, 0xFF) => (2, false),
            _ => (0, true),
        };
        if skip == 2 {
            let units: Vec<u16> = bytes[skip..]
                .chunks_exact(2)
                .map(|pair| {
                    if little_endian {
                        u16::from_le_bytes([pair[0], pair[1]])
                    } else {
                        u16::from_be_bytes([pair[0], pair[1]])
                    }
                })
                .collect();
            if !units.is_empty() {
                return char::decode_utf16(units)
                    .map(|unit| unit.unwrap_or('\u{fffd}'))
                    .collect();
            }
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn first_heading(blocks: &[Block]) -> Option<String> {
    for block in blocks {
        if let Block::Heading { spans, .. } = block {
            let text = spans_plain(spans).trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// Collect every link destination in document order, without duplicates.
fn collect_link_targets(blocks: &[Block]) -> Vec<String> {
    fn walk_spans(spans: &[Span], out: &mut Vec<String>) {
        for span in spans {
            match span {
                Span::Link { text, target } => {
                    if !target.is_empty() && !out.iter().any(|seen| seen == target) {
                        out.push(target.clone());
                    }
                    walk_spans(text, out);
                }
                Span::Bold(inner) | Span::Italic(inner) => walk_spans(inner, out),
                Span::Text(_) | Span::Code(_) | Span::LineBreak => {}
            }
        }
    }

    let mut out: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) => {
                walk_spans(spans, &mut out);
            }
            Block::BulletList(items) => {
                for item in items {
                    walk_spans(item, &mut out);
                }
            }
            Block::NumberedList { items, .. } => {
                for item in items {
                    walk_spans(item, &mut out);
                }
            }
            Block::Table { header, rows, .. } => {
                for cell in header {
                    walk_spans(cell, &mut out);
                }
                for row in rows {
                    for cell in row {
                        walk_spans(cell, &mut out);
                    }
                }
            }
            Block::CodeBlock { .. } | Block::Rule => {}
        }
    }
    out
}

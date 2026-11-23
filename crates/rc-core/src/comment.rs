//! File comments stored in `descript.ion` (and the compatible `files.bbs`).
//!
//! Format: one line per file, `name comment`, where the name is quoted when it
//! contains spaces and backslashes/newlines inside the comment are escaped.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// File name of the comment database.
pub const COMMENT_FILE: &str = "descript.ion";
/// Alternative comment database name.
pub const COMMENT_FILE_ALT: &str = "files.bbs";

/// The comments of one directory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comments {
    entries: BTreeMap<String, String>,
}

impl Comments {
    pub fn new() -> Self {
        Comments::default()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Comment of `name`, if any.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(|s| s.as_str())
    }

    /// Set (or clear, with an empty string) the comment of `name`.
    pub fn set(&mut self, name: impl Into<String>, comment: impl Into<String>) {
        let name = name.into();
        let comment = comment.into();
        if comment.is_empty() {
            self.entries.remove(&name);
        } else {
            self.entries.insert(name, comment);
        }
    }

    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.entries.remove(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.entries.iter()
    }

    /// Parse `descript.ion` content.
    pub fn parse(text: &str) -> Self {
        let mut c = Comments::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() {
                continue;
            }
            if let Some((name, rest)) = split_name(line) {
                let comment = unescape(&rest).trim().to_string();
                if !name.is_empty() && !comment.is_empty() {
                    c.entries.insert(name, comment);
                }
            }
        }
        c
    }

    /// Render to `descript.ion` content, sorted by name.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (name, comment) in &self.entries {
            let name = if needs_quotes(name) {
                format!("\"{name}\"")
            } else {
                name.clone()
            };
            out.push_str(&name);
            out.push(' ');
            out.push_str(&escape(comment));
            out.push('\n');
        }
        out
    }

    /// Read the comment file of a directory (missing file → empty).
    pub fn load_dir(dir: &Path) -> Self {
        for file in [COMMENT_FILE, COMMENT_FILE_ALT] {
            let path = dir.join(file);
            if let Ok(text) = std::fs::read_to_string(&path) {
                return Comments::parse(&text);
            }
        }
        Comments::new()
    }

    /// Write the comment file of a directory (empty set deletes the file).
    pub fn save_dir(&self, dir: &Path) -> std::io::Result<()> {
        let path: PathBuf = dir.join(COMMENT_FILE);
        if self.entries.is_empty() {
            if path.exists() {
                std::fs::remove_file(path)?;
            }
            return Ok(());
        }
        std::fs::write(path, self.to_text())
    }
}

fn needs_quotes(name: &str) -> bool {
    name.is_empty() || name.chars().any(|c| c.is_whitespace() || c == '"')
}

fn split_name(line: &str) -> Option<(String, String)> {
    if let Some(rest) = line.strip_prefix('"') {
        let end = rest.find('"')?;
        let name = rest[..end].to_string();
        Some((name, rest[end + 1..].to_string()))
    } else {
        match line.find(char::is_whitespace) {
            Some(i) => Some((line[..i].to_string(), line[i..].to_string())),
            None => Some((line.to_string(), String::new())),
        }
    }
}

fn escape(comment: &str) -> String {
    let mut out = String::with_capacity(comment.len());
    for c in comment.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_lines() {
        let c = Comments::parse("a.txt first file\nb.txt second\n");
        assert_eq!(c.get("a.txt"), Some("first file"));
        assert_eq!(c.get("b.txt"), Some("second"));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn parse_quoted_names_and_escapes() {
        let c = Comments::parse("\"my file.txt\" a long comment\nc.txt line1\\nline2\n");
        assert_eq!(c.get("my file.txt"), Some("a long comment"));
        assert_eq!(c.get("c.txt"), Some("line1\nline2"));
    }

    #[test]
    fn round_trip_through_text() {
        let mut c = Comments::new();
        c.set("plain.txt", "hello");
        c.set("with space.txt", "two\\words");
        c.set("multi.txt", "line1\nline2");
        let text = c.to_text();
        let back = Comments::parse(&text);
        assert_eq!(back, c);
    }

    #[test]
    fn empty_comment_removes_the_entry() {
        let mut c = Comments::new();
        c.set("a", "x");
        assert_eq!(c.len(), 1);
        c.set("a", "");
        assert!(c.is_empty());
        assert_eq!(c.get("a"), None);
        assert_eq!(c.remove("a"), None);
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let c = Comments::parse("\n\n   \n\"unterminated comment\n");
        assert!(c.is_empty());
    }

    #[test]
    fn load_and_save_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Comments::new();
        c.set("x.txt", "note");
        c.save_dir(dir.path()).unwrap();
        assert!(dir.path().join(COMMENT_FILE).exists());
        assert_eq!(Comments::load_dir(dir.path()).get("x.txt"), Some("note"));
        c.set("x.txt", "");
        c.save_dir(dir.path()).unwrap();
        assert!(!dir.path().join(COMMENT_FILE).exists());
        assert!(Comments::load_dir(dir.path()).is_empty());
    }

    #[test]
    fn falls_back_to_files_bbs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(COMMENT_FILE_ALT), "y.txt alt comment\n").unwrap();
        assert_eq!(
            Comments::load_dir(dir.path()).get("y.txt"),
            Some("alt comment")
        );
    }

    #[test]
    fn iteration_is_sorted_by_name() {
        let mut c = Comments::new();
        c.set("b", "2");
        c.set("a", "1");
        let names: Vec<&String> = c.iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["a", "b"]);
    }
}

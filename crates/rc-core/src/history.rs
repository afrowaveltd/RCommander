//! Per-panel directory history (the `Alt+F8` / `Alt+Down` lists).

use std::path::PathBuf;

/// One remembered directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub path: PathBuf,
    /// Entry name the cursor was on, so returning restores the position.
    pub cursor: Option<String>,
    /// Unix timestamp of the visit.
    pub visited: i64,
}

/// A most-recently-used list of visited directories.
#[derive(Clone, Debug)]
pub struct DirHistory {
    entries: Vec<HistoryEntry>,
    capacity: usize,
    /// Remove older entries that end with the same directory name.
    thinning: bool,
}

impl Default for DirHistory {
    fn default() -> Self {
        DirHistory::new(30)
    }
}

impl DirHistory {
    pub fn new(capacity: usize) -> Self {
        DirHistory {
            entries: Vec::new(),
            capacity: capacity.max(1),
            thinning: true,
        }
    }

    pub fn with_thinning(mut self, thinning: bool) -> Self {
        self.thinning = thinning;
        self
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
        self.entries.truncate(self.capacity);
    }

    /// Record a visit. Re-visiting a directory moves it to the front.
    pub fn record(&mut self, path: impl Into<PathBuf>, cursor: Option<String>, now: i64) {
        let path = path.into();
        self.entries.retain(|e| e.path != path);
        if self.thinning
            && let Some(name) = path.file_name()
        {
            self.entries.retain(|e| e.path.file_name() != Some(name));
        }
        self.entries.insert(
            0,
            HistoryEntry {
                path,
                cursor,
                visited: now,
            },
        );
        self.entries.truncate(self.capacity);
    }

    /// The entry to go *back* to, given the current directory.
    pub fn previous(&self, current: &std::path::Path) -> Option<&HistoryEntry> {
        let pos = self.entries.iter().position(|e| e.path == current);
        match pos {
            Some(i) => self.entries.get(i + 1),
            None => self.entries.first(),
        }
    }

    /// The entry to go *forward* to, given the current directory.
    pub fn next(&self, current: &std::path::Path) -> Option<&HistoryEntry> {
        let pos = self.entries.iter().position(|e| e.path == current)?;
        if pos == 0 {
            None
        } else {
            self.entries.get(pos - 1)
        }
    }

    pub fn contains(&self, path: &std::path::Path) -> bool {
        self.entries.iter().any(|e| e.path == path)
    }

    pub fn remove(&mut self, path: &std::path::Path) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.path != path);
        self.entries.len() != before
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Paths only, most recent first.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.entries.iter().map(|e| e.path.clone()).collect()
    }

    /// Serialise to a newline-separated list of `path` (one per line) for the
    /// portable session file.
    pub fn to_lines(&self) -> String {
        self.entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Restore from [`DirHistory::to_lines`] output.
    pub fn from_lines(text: &str, capacity: usize) -> Self {
        let mut h = DirHistory::new(capacity);
        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() {
                h.entries.push(HistoryEntry {
                    path: PathBuf::from(line),
                    cursor: None,
                    visited: 0,
                });
            }
        }
        h.entries.truncate(h.capacity);
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn records_most_recent_first() {
        let mut h = DirHistory::new(10);
        h.record("/a", None, 1);
        h.record("/b", None, 2);
        assert_eq!(h.paths(), [PathBuf::from("/b"), PathBuf::from("/a")]);
        assert_eq!(h.len(), 2);
        assert!(!h.is_empty());
    }

    #[test]
    fn revisiting_moves_to_front_without_duplicates() {
        let mut h = DirHistory::new(10);
        h.record("/a", None, 1);
        h.record("/b", None, 2);
        h.record("/a", Some("x".into()), 3);
        assert_eq!(h.paths(), [PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(h.entries()[0].cursor.as_deref(), Some("x"));
    }

    #[test]
    fn thinning_removes_older_entries_with_same_last_component() {
        let mut h = DirHistory::new(10).with_thinning(true);
        h.record("/one/project", None, 1);
        h.record("/two/project", None, 2);
        assert_eq!(h.paths(), [PathBuf::from("/two/project")]);
        let mut plain = DirHistory::new(10).with_thinning(false);
        plain.record("/one/project", None, 1);
        plain.record("/two/project", None, 2);
        assert_eq!(plain.len(), 2);
    }

    #[test]
    fn capacity_is_respected() {
        let mut h = DirHistory::new(3);
        for i in 0..6 {
            h.record(format!("/d{i}"), None, i);
        }
        assert_eq!(h.len(), 3);
        assert_eq!(h.entries()[0].path, PathBuf::from("/d5"));
        h.set_capacity(1);
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn previous_and_next_walk_the_list() {
        let mut h = DirHistory::new(10);
        h.record("/a", None, 1);
        h.record("/b", None, 2);
        h.record("/c", None, 3); // c, b, a
        assert_eq!(
            h.previous(Path::new("/c")).unwrap().path,
            PathBuf::from("/b")
        );
        assert_eq!(
            h.previous(Path::new("/b")).unwrap().path,
            PathBuf::from("/a")
        );
        assert!(h.previous(Path::new("/a")).is_none());
        assert_eq!(h.next(Path::new("/b")).unwrap().path, PathBuf::from("/c"));
        assert!(h.next(Path::new("/c")).is_none());
        assert_eq!(
            h.previous(Path::new("/unknown")).unwrap().path,
            PathBuf::from("/c")
        );
    }

    #[test]
    fn contains_remove_and_clear_work() {
        let mut h = DirHistory::new(5);
        h.record("/a", None, 1);
        assert!(h.contains(Path::new("/a")));
        assert!(h.remove(Path::new("/a")));
        assert!(!h.remove(Path::new("/a")));
        h.record("/b", None, 1);
        h.clear();
        assert!(h.is_empty());
    }

    #[test]
    fn line_format_round_trips() {
        let mut h = DirHistory::new(5);
        h.record("/a", None, 1);
        h.record("/b", None, 2);
        let text = h.to_lines();
        assert_eq!(text, "/b\n/a");
        let back = DirHistory::from_lines(&text, 5);
        assert_eq!(back.paths(), [PathBuf::from("/b"), PathBuf::from("/a")]);
        assert_eq!(DirHistory::from_lines("", 5).len(), 0);
    }
}

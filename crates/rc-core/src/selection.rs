//! Selection (marking) model.

use crate::entry::Entry;
use crate::mask::MaskSet;
use std::collections::BTreeSet;

/// The set of marked entries of one panel, keyed by entry name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    names: BTreeSet<String>,
}

impl Selection {
    pub fn new() -> Self {
        Selection::default()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    pub fn insert(&mut self, name: impl Into<String>) -> bool {
        self.names.insert(name.into())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.names.remove(name)
    }

    pub fn clear(&mut self) {
        self.names.clear();
    }

    /// Replace the selection with exactly `names`.
    pub fn set<I, S>(&mut self, names: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.names = names.into_iter().map(Into::into).collect();
    }

    /// Mark or unmark a single name.
    pub fn toggle(&mut self, name: &str) {
        if !self.names.remove(name) {
            self.names.insert(name.to_string());
        }
    }

    /// Names in stable (alphabetical) order.
    pub fn names(&self) -> Vec<String> {
        self.names.iter().cloned().collect()
    }

    /// Drop names that no longer exist in `entries` (after re-reading a dir).
    pub fn retain_existing(&mut self, entries: &[Entry]) {
        let existing: BTreeSet<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        self.names.retain(|n| existing.contains(n.as_str()));
    }

    /// Select every selectable entry (the `..` pseudo entry is never marked).
    pub fn select_all(&mut self, entries: &[Entry], include_dirs: bool) -> usize {
        let mut n = 0;
        for e in entries.iter().filter(|e| e.name != "..") {
            if e.is_dir() && !include_dirs {
                continue;
            }
            self.names.insert(e.name.clone());
            n += 1;
        }
        n
    }

    /// Clear all marks.
    pub fn deselect_all(&mut self) -> usize {
        let n = self.names.len();
        self.names.clear();
        n
    }

    /// Invert the marks of every entry, optionally leaving directories alone.
    pub fn invert(&mut self, entries: &[Entry], include_dirs: bool) -> usize {
        let mut n = 0;
        for e in entries.iter().filter(|e| e.name != "..") {
            if e.is_dir() && !include_dirs {
                continue;
            }
            if !self.names.remove(&e.name) {
                self.names.insert(e.name.clone());
            }
            n += 1;
        }
        n
    }

    /// Expand the selection with every entry matching `mask` (the classic
    /// "Num +" dialog). Returns the number of newly marked entries.
    pub fn select_by_mask(
        &mut self,
        entries: &[Entry],
        mask: &MaskSet,
        include_dirs: bool,
    ) -> usize {
        let mut n = 0;
        for e in entries.iter().filter(|e| e.name != "..") {
            if e.is_dir() && !include_dirs {
                continue;
            }
            let matches = if mask.is_empty() {
                true
            } else {
                mask.is_match(&e.name)
            };
            if matches && self.names.insert(e.name.clone()) {
                n += 1;
            }
        }
        n
    }

    /// Remove marks from every entry matching `mask` (the classic "Num -").
    pub fn deselect_by_mask(
        &mut self,
        entries: &[Entry],
        mask: &MaskSet,
        include_dirs: bool,
    ) -> usize {
        let mut n = 0;
        for e in entries.iter().filter(|e| e.name != "..") {
            if e.is_dir() && !include_dirs {
                continue;
            }
            let matches = if mask.is_empty() {
                true
            } else {
                mask.is_match(&e.name)
            };
            if matches && self.names.remove(&e.name) {
                n += 1;
            }
        }
        n
    }

    /// Mark every entry sharing the extension of `name` ("Alt Num +").
    pub fn select_same_extension(
        &mut self,
        entries: &[Entry],
        name: &str,
        include_dirs: bool,
    ) -> usize {
        let ext = match entries.iter().find(|e| e.name == name) {
            Some(e) => e.extension(),
            None => return 0,
        };
        let mut n = 0;
        for e in entries.iter().filter(|e| e.name != "..") {
            if e.extension() != ext {
                continue;
            }
            if e.is_dir() && !include_dirs {
                continue;
            }
            if self.names.insert(e.name.clone()) {
                n += 1;
            }
        }
        n
    }

    /// Unmark every entry sharing the extension of `name` ("Alt Num -").
    pub fn deselect_same_extension(
        &mut self,
        entries: &[Entry],
        name: &str,
        include_dirs: bool,
    ) -> usize {
        let ext = match entries.iter().find(|e| e.name == name) {
            Some(e) => e.extension(),
            None => return 0,
        };
        let mut n = 0;
        for e in entries.iter().filter(|e| e.name != "..") {
            if e.extension() != ext {
                continue;
            }
            if e.is_dir() && !include_dirs {
                continue;
            }
            if self.names.remove(&e.name) {
                n += 1;
            }
        }
        n
    }

    /// Entries of `entries` that are marked, in listing order.
    pub fn selected_entries<'a>(&self, entries: &'a [Entry]) -> Vec<&'a Entry> {
        entries
            .iter()
            .filter(|e| self.names.contains(&e.name))
            .collect()
    }

    /// Number of marked files and their total size.
    pub fn statistics(&self, entries: &[Entry]) -> (usize, u64) {
        let mut count = 0;
        let mut bytes = 0;
        for e in entries {
            if self.names.contains(&e.name) && e.name != ".." {
                count += 1;
                if !e.is_dir() {
                    bytes += e.size;
                }
            }
        }
        (count, bytes)
    }

    /// Persistable representation, one name per line.
    pub fn to_file_format(&self) -> String {
        let mut s = self.names();
        s.sort();
        s.join("\n")
    }

    /// Restore a selection written by [`Selection::to_file_format`].
    pub fn from_file_format(text: &str) -> Self {
        let names: BTreeSet<String> = text
            .lines()
            .map(|l| l.trim_end_matches(['\r', '\n']))
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        Selection { names }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryKind;
    use std::path::PathBuf;

    fn entries() -> Vec<Entry> {
        let mut a = Entry::new("a.txt", PathBuf::from("/a.txt"), EntryKind::File, 10);
        a.size = 10;
        let mut b = Entry::new("b.txt", PathBuf::from("/b.txt"), EntryKind::File, 20);
        b.size = 20;
        let mut c = Entry::new("c.zip", PathBuf::from("/c.zip"), EntryKind::File, 30);
        c.size = 30;
        let d = Entry::new("sub", PathBuf::from("/sub"), EntryKind::Dir, 0);
        let up = Entry::parent_entry(PathBuf::from("/.."));
        vec![up, d, a, b, c]
    }

    #[test]
    fn insert_remove_and_toggle_behave_like_a_set() {
        let mut s = Selection::new();
        assert!(s.insert("a"));
        assert!(!s.insert("a"));
        assert!(s.contains("a"));
        s.toggle("a");
        assert!(!s.contains("a"));
        s.toggle("a");
        assert!(s.contains("a"));
        assert!(s.remove("a"));
        assert!(s.is_empty());
    }

    #[test]
    fn select_all_skips_parent_entry_and_dirs_when_asked() {
        let e = entries();
        let mut s = Selection::new();
        assert_eq!(s.select_all(&e, false), 3);
        assert_eq!(s.len(), 3);
        assert!(!s.contains(".."));
        assert!(!s.contains("sub"));
        let mut s2 = Selection::new();
        assert_eq!(s2.select_all(&e, true), 4);
        assert!(s2.contains("sub"));
    }

    #[test]
    fn invert_toggles_only_eligible_entries() {
        let e = entries();
        let mut s = Selection::new();
        s.set(["a.txt"]);
        s.invert(&e, false);
        assert!(!s.contains("a.txt"));
        assert!(s.contains("b.txt"));
        assert!(s.contains("c.zip"));
        assert!(!s.contains("sub"));
        assert!(!s.contains(".."));
    }

    #[test]
    fn mask_selection_expands_and_shrinks() {
        let e = entries();
        let mut s = Selection::new();
        let mask = MaskSet::parse("*.txt").unwrap();
        assert_eq!(s.select_by_mask(&e, &mask, true), 2);
        assert_eq!(s.len(), 2);
        assert_eq!(s.deselect_by_mask(&e, &mask, true), 2);
        assert!(s.is_empty());
    }

    #[test]
    fn same_extension_selection_matches_the_whole_group() {
        let e = entries();
        let mut s = Selection::new();
        assert_eq!(s.select_same_extension(&e, "b.txt", false), 2);
        assert!(s.contains("a.txt") && s.contains("b.txt"));
        assert_eq!(s.deselect_same_extension(&e, "a.txt", false), 2);
        assert!(s.is_empty());
        assert_eq!(s.select_same_extension(&e, "missing.txt", false), 0);
    }

    #[test]
    fn statistics_count_files_and_bytes_only() {
        let e = entries();
        let mut s = Selection::new();
        s.set(["a.txt", "b.txt", "sub"]);
        let (count, bytes) = s.statistics(&e);
        assert_eq!(count, 3);
        assert_eq!(bytes, 30, "directory contributes no bytes");
    }

    #[test]
    fn stale_names_are_dropped_on_reread() {
        let e = entries();
        let mut s = Selection::new();
        s.set(["a.txt", "gone.txt"]);
        s.retain_existing(&e);
        assert_eq!(s.names(), vec!["a.txt".to_string()]);
    }

    #[test]
    fn file_round_trip_keeps_names() {
        let mut s = Selection::new();
        s.set(["b.txt", "a.txt"]);
        let text = s.to_file_format();
        assert_eq!(text, "a.txt\nb.txt");
        let back = Selection::from_file_format(&text);
        assert_eq!(back, s);
        assert_eq!(Selection::from_file_format("").len(), 0);
    }

    #[test]
    fn selected_entries_preserve_listing_order() {
        let e = entries();
        let mut s = Selection::new();
        s.set(["c.zip", "a.txt"]);
        let names: Vec<String> = s
            .selected_entries(&e)
            .iter()
            .map(|x| x.name.clone())
            .collect();
        assert_eq!(names, ["a.txt", "c.zip"]);
    }
}

//! Directory sorting.

use crate::entry::Entry;
use std::cmp::Ordering;

/// Column a panel is sorted by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortField {
    Name,
    Extension,
    Size,
    ModifiedTime,
    /// File system order, as returned by the OS.
    Unsorted,
}

impl SortField {
    /// Short label used in column headers.
    pub fn label(self) -> &'static str {
        match self {
            SortField::Name => "Name",
            SortField::Extension => "Ext",
            SortField::Size => "Size",
            SortField::ModifiedTime => "Date",
            SortField::Unsorted => "Unsorted",
        }
    }
}

/// Full sort configuration of a panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortSpec {
    pub field: SortField,
    /// Reverse the comparison result (the `+`/`-` order indicator).
    pub reversed: bool,
    /// Keep directories before files.
    pub dirs_first: bool,
    /// Sort embedded numbers by value (`file2` before `file10`).
    pub natural: bool,
    pub case_sensitive: bool,
}

impl Default for SortSpec {
    fn default() -> Self {
        SortSpec {
            field: SortField::Name,
            reversed: false,
            dirs_first: true,
            natural: true,
            case_sensitive: false,
        }
    }
}

impl SortSpec {
    pub fn by(field: SortField) -> Self {
        SortSpec {
            field,
            ..SortSpec::default()
        }
    }

    pub fn reversed(mut self) -> Self {
        self.reversed = !self.reversed;
        self
    }

    pub fn without_dirs_first(mut self) -> Self {
        self.dirs_first = false;
        self
    }

    /// Direction marker shown in the column header.
    pub fn indicator(&self) -> &'static str {
        if self.reversed { "v" } else { "^" }
    }
}

/// Compare two names using natural ("human") ordering: digit runs compare by
/// numeric value, everything else case-insensitively unless requested
/// otherwise.
pub fn natural_cmp(a: &str, b: &str, case_sensitive: bool) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let na = take_number(&mut ai);
                    let nb = take_number(&mut bi);
                    match na.cmp(&nb) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                } else {
                    ai.next();
                    bi.next();
                    let (la, lb) = if case_sensitive {
                        (ca, cb)
                    } else {
                        (
                            ca.to_lowercase().next().unwrap_or(ca),
                            cb.to_lowercase().next().unwrap_or(cb),
                        )
                    };
                    match la.cmp(&lb) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
            }
        }
    }
}

fn take_number(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u128 {
    let mut n: u128 = 0;
    while let Some(c) = it.peek().copied() {
        if let Some(d) = c.to_digit(10) {
            n = n.saturating_mul(10).saturating_add(d as u128);
            it.next();
        } else {
            break;
        }
    }
    n
}

fn cmp_names(a: &str, b: &str, spec: &SortSpec) -> Ordering {
    if spec.natural {
        natural_cmp(a, b, spec.case_sensitive)
    } else if spec.case_sensitive {
        a.cmp(b)
    } else {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    }
}

fn compare(a: &Entry, b: &Entry, spec: &SortSpec) -> Ordering {
    if spec.dirs_first {
        let da = a.is_dir();
        let db = b.is_dir();
        if da != db {
            return if da {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
    }
    let primary = match spec.field {
        SortField::Name => cmp_names(&a.name, &b.name, spec),
        SortField::Extension => {
            let ea = a.extension();
            let eb = b.extension();
            cmp_names(&ea, &eb, spec).then_with(|| cmp_names(&a.name, &b.name, spec))
        }
        SortField::Size => a
            .display_size()
            .cmp(&b.display_size())
            .then_with(|| cmp_names(&a.name, &b.name, spec)),
        SortField::ModifiedTime => a
            .mtime
            .unwrap_or(0)
            .cmp(&b.mtime.unwrap_or(0))
            .then_with(|| cmp_names(&a.name, &b.name, spec)),
        SortField::Unsorted => Ordering::Equal,
    };
    if spec.reversed {
        primary.reverse()
    } else {
        primary
    }
}

/// Sort `entries` in place according to `spec`.
///
/// With [`SortField::Unsorted`] the entries keep their original relative order
/// (only the directory/file grouping is applied when `dirs_first` is set).
pub fn sort_entries(entries: &mut [Entry], spec: &SortSpec) {
    if spec.field == SortField::Unsorted {
        if spec.dirs_first {
            entries.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => Ordering::Equal,
            });
        }
        return;
    }
    entries.sort_by(|a, b| compare(a, b, spec));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryKind;
    use std::path::PathBuf;

    fn file(name: &str) -> Entry {
        Entry::new(name, PathBuf::from("/").join(name), EntryKind::File, 0)
    }
    fn dir(name: &str) -> Entry {
        Entry::new(name, PathBuf::from("/").join(name), EntryKind::Dir, 0)
    }
    fn names(v: &[Entry]) -> Vec<String> {
        v.iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn natural_order_puts_two_before_ten() {
        assert_eq!(natural_cmp("file2", "file10", false), Ordering::Less);
        assert_eq!(natural_cmp("file10", "file2", false), Ordering::Greater);
        assert_eq!(natural_cmp("file2", "file2", false), Ordering::Equal);
        assert_eq!(natural_cmp("a", "b", false), Ordering::Less);
        assert_eq!(natural_cmp("", "a", false), Ordering::Less);
        assert_eq!(natural_cmp("a", "", false), Ordering::Greater);
        assert_eq!(natural_cmp("x9y", "x9z", false), Ordering::Less);
        assert_eq!(natural_cmp("img0012", "img13", false), Ordering::Less);
    }

    #[test]
    fn natural_compare_is_case_insensitive_by_default() {
        assert_eq!(natural_cmp("Apple", "apple", false), Ordering::Equal);
        assert_ne!(natural_cmp("Apple", "apple", true), Ordering::Equal);
    }

    #[test]
    fn name_sort_is_case_insensitive_and_natural() {
        let mut v = vec![file("file10.txt"), file("file2.txt"), file("File1.txt")];
        sort_entries(&mut v, &SortSpec::default());
        assert_eq!(names(&v), ["File1.txt", "file2.txt", "file10.txt"]);
    }

    #[test]
    fn directories_come_first_by_default() {
        let mut v = vec![file("a.txt"), dir("zdir"), file("b.txt")];
        sort_entries(&mut v, &SortSpec::default());
        assert_eq!(names(&v), ["zdir", "a.txt", "b.txt"]);
        let mut w = vec![file("a.txt"), dir("zdir")];
        sort_entries(&mut w, &SortSpec::default().without_dirs_first());
        assert_eq!(names(&w), ["a.txt", "zdir"]);
    }

    #[test]
    fn reversed_order_flips_every_comparison() {
        let mut v = vec![file("a"), file("b"), file("c")];
        sort_entries(&mut v, &SortSpec::default().reversed());
        assert_eq!(names(&v), ["c", "b", "a"]);
        assert_eq!(SortSpec::default().reversed().indicator(), "v");
        assert_eq!(SortSpec::default().indicator(), "^");
    }

    #[test]
    fn extension_sort_groups_by_extension_then_name() {
        let mut v = vec![file("b.zip"), file("a.txt"), file("a.zip")];
        sort_entries(&mut v, &SortSpec::by(SortField::Extension));
        assert_eq!(names(&v), ["a.txt", "a.zip", "b.zip"]);
    }

    #[test]
    fn size_sort_uses_numeric_size() {
        let mut a = file("a");
        a.size = 100;
        let mut b = file("b");
        b.size = 20;
        let mut v = vec![a, b];
        sort_entries(&mut v, &SortSpec::by(SortField::Size));
        assert_eq!(names(&v), ["b", "a"]);
    }

    #[test]
    fn time_sort_uses_modification_time() {
        let mut a = file("old");
        a.mtime = Some(1000);
        let mut b = file("new");
        b.mtime = Some(2000);
        let mut v = vec![b, a];
        sort_entries(&mut v, &SortSpec::by(SortField::ModifiedTime));
        assert_eq!(names(&v), ["old", "new"]);
    }

    #[test]
    fn unsorted_keeps_original_order_but_groups_dirs() {
        let mut v = vec![file("z"), dir("d1"), file("a"), dir("d2")];
        sort_entries(&mut v, &SortSpec::by(SortField::Unsorted));
        assert_eq!(names(&v), ["d1", "d2", "z", "a"]);
        let mut w = vec![file("z"), dir("d1"), file("a")];
        let spec = SortSpec {
            dirs_first: false,
            ..SortSpec::by(SortField::Unsorted)
        };
        sort_entries(&mut w, &spec);
        assert_eq!(names(&w), ["z", "d1", "a"]);
    }
}

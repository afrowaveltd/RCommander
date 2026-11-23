//! A panel's directory listing: reading, filtering, sorting and cursor
//! movement.

use crate::entry::{Attrs, Entry, EntryKind};
use crate::error::{CoreError, CoreResult};
use crate::location::Location;
use crate::mask::MaskSet;
use crate::selection::Selection;
use crate::sorting::{SortSpec, sort_entries};
use std::fs;
use std::path::Path;

/// Options controlling how a directory is read and displayed.
#[derive(Clone, Debug, Default)]
pub struct ReadOptions {
    /// Include hidden (dot) entries.
    pub show_hidden: bool,
    /// Include the `..` pseudo entry at the top of the list.
    pub show_parent: bool,
    /// Restrict *files* to the matching masks; directories always stay visible.
    pub mask: MaskSet,
}

/// One panel's listing.
#[derive(Clone, Debug)]
pub struct Listing {
    location: Location,
    /// Everything read from the file system, before filtering/sorting.
    raw: Vec<Entry>,
    /// What the panel currently displays.
    entries: Vec<Entry>,
    spec: SortSpec,
    options: ReadOptions,
    cursor: usize,
}

impl Listing {
    /// Read a local directory.
    pub fn read(location: &Location, options: &ReadOptions, spec: SortSpec) -> CoreResult<Self> {
        let path = match location {
            Location::Local(p) => p.clone(),
            other => {
                return Err(CoreError::Unsupported(format!(
                    "read of non-local location {}",
                    other.display()
                )));
            }
        };
        let raw = read_dir_entries(&path, options)?;
        let mut listing = Listing {
            location: location.clone(),
            raw,
            entries: Vec::new(),
            spec,
            options: options.clone(),
            cursor: 0,
        };
        listing.rebuild();
        Ok(listing)
    }

    /// Build a listing from entries provided by a backend (archive, remote, …).
    pub fn from_entries(
        location: Location,
        raw: Vec<Entry>,
        options: &ReadOptions,
        spec: SortSpec,
    ) -> Self {
        let mut listing = Listing {
            location,
            raw,
            entries: Vec::new(),
            spec,
            options: options.clone(),
            cursor: 0,
        };
        listing.rebuild();
        listing
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }

    pub fn options(&self) -> &ReadOptions {
        &self.options
    }

    pub fn sort_spec(&self) -> SortSpec {
        self.spec
    }

    /// Visible entries, including the optional `..` row.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Entries excluding the `..` pseudo row.
    pub fn real_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.name != "..")
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of entries that were hidden by the hidden/mask filters.
    pub fn hidden_count(&self) -> usize {
        self.raw
            .len()
            .saturating_sub(self.entries.iter().filter(|e| e.name != "..").count())
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.cursor = 0;
        } else {
            self.cursor = index.min(self.entries.len() - 1);
        }
    }

    pub fn cursor_entry(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    /// Move the cursor by `delta`, optionally wrapping around the list ends.
    pub fn move_cursor(&mut self, delta: isize, wrap: bool) {
        if self.entries.is_empty() {
            self.cursor = 0;
            return;
        }
        let len = self.entries.len() as isize;
        let mut next = self.cursor as isize + delta;
        if wrap {
            next = ((next % len) + len) % len;
        } else {
            next = next.clamp(0, len - 1);
        }
        self.cursor = next as usize;
    }

    pub fn move_cursor_to(&mut self, index: usize) {
        self.set_cursor(index);
    }

    /// Index of the entry called `name`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.name == name)
    }

    /// Names in display order, for quick search.
    pub fn name_list(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.name.clone()).collect()
    }

    /// Re-read the directory from disk, keeping the cursor on the same name.
    pub fn reread(&mut self) -> CoreResult<()> {
        let keep = self.cursor_entry().map(|e| e.name.clone());
        let path = match &self.location {
            Location::Local(p) => p.clone(),
            other => {
                return Err(CoreError::Unsupported(format!(
                    "read of non-local location {}",
                    other.display()
                )));
            }
        };
        self.raw = read_dir_entries(&path, &self.options)?;
        self.rebuild();
        if let Some(name) = keep
            && let Some(i) = self.find(&name)
        {
            self.cursor = i;
        }
        Ok(())
    }

    /// Change the sort order and re-sort in place.
    pub fn resort(&mut self, spec: SortSpec) {
        self.spec = spec;
        self.rebuild();
    }

    /// Replace the filter mask.
    pub fn set_mask(&mut self, mask: MaskSet) {
        self.options.mask = mask;
        self.rebuild();
    }

    /// Show or hide hidden entries.
    pub fn set_show_hidden(&mut self, show: bool) {
        self.options.show_hidden = show;
        self.rebuild();
    }

    /// Statistics for the status line: (files, dirs, bytes of all entries).
    pub fn statistics(&self) -> (usize, usize, u64) {
        let mut files = 0;
        let mut dirs = 0;
        let mut bytes = 0;
        for e in self.real_entries() {
            if e.is_dir() {
                dirs += 1;
            } else {
                files += 1;
                bytes += e.size;
            }
        }
        (files, dirs, bytes)
    }

    /// Statistics of the marked entries: (count, bytes).
    pub fn selection_statistics(&self, selection: &Selection) -> (usize, u64) {
        selection.statistics(&self.entries)
    }

    /// Marks of entries that still exist, in display order.
    pub fn selected_entries(&self, selection: &Selection) -> Vec<&Entry> {
        selection.selected_entries(&self.entries)
    }

    fn rebuild(&mut self) {
        let mut visible: Vec<Entry> = self
            .raw
            .iter()
            .filter(|e| self.options.show_hidden || !e.is_hidden())
            .filter(|e| {
                e.is_dir() || self.options.mask.is_empty() || self.options.mask.is_match(&e.name)
            })
            .cloned()
            .collect();
        sort_entries(&mut visible, &self.spec);
        if self.options.show_parent {
            if let Some(parent) = self.location.parent() {
                visible.insert(
                    0,
                    Entry::parent_entry(
                        parent
                            .as_path()
                            .map(|p| p.to_path_buf())
                            .unwrap_or_default(),
                    ),
                );
            } else {
                visible.insert(0, Entry::parent_entry(std::path::PathBuf::new()));
            }
        }
        self.entries = visible;
        if self.cursor >= self.entries.len() {
            self.cursor = self.entries.len().saturating_sub(1);
        }
    }
}

/// Read all entries of `path` without filtering or sorting.
pub fn read_dir_entries(path: &Path, options: &ReadOptions) -> CoreResult<Vec<Entry>> {
    let rd = fs::read_dir(path).map_err(|e| CoreError::io(path, e))?;
    let mut out = Vec::new();
    for item in rd {
        let item = match item {
            Ok(i) => i,
            Err(_) => continue,
        };
        let name = item.file_name().to_string_lossy().to_string();
        if name.is_empty() {
            continue;
        }
        let full = item.path();
        let md = match fs::symlink_metadata(&full) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let ft = md.file_type();
        let (kind, size, target) = if ft.is_symlink() {
            match fs::metadata(&full) {
                Ok(t) => {
                    let kind = if t.is_dir() {
                        EntryKind::SymlinkToDir
                    } else {
                        EntryKind::SymlinkToFile
                    };
                    (kind, t.len(), fs::read_link(&full).ok())
                }
                Err(_) => (EntryKind::SymlinkBroken, 0, fs::read_link(&full).ok()),
            }
        } else if ft.is_dir() {
            (EntryKind::Dir, 0, None)
        } else if ft.is_file() {
            (EntryKind::File, md.len(), None)
        } else {
            (EntryKind::Special, md.len(), None)
        };
        let mut attrs = Attrs {
            readonly: md.permissions().readonly(),
            hidden: name.starts_with('.'),
            ..Attrs::default()
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            attrs.unix_mode = Some(md.mode() & 0o7777);
        }
        let _ = options;
        out.push(Entry {
            name,
            path: full,
            kind,
            size,
            mtime: mtime_of(&md),
            ctime: ctime_of(&md),
            attrs,
            link_target: target,
        });
    }
    Ok(out)
}

fn mtime_of(md: &fs::Metadata) -> Option<i64> {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

fn ctime_of(md: &fs::Metadata) -> Option<i64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(md.ctime())
    }
    #[cfg(not(unix))]
    {
        md.created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sorting::SortField;
    use std::fs::File;
    use std::io::Write;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        fs::create_dir(p.join("beta")).unwrap();
        fs::create_dir(p.join("Alpha")).unwrap();
        fs::create_dir(p.join(".hiddendir")).unwrap();
        for (name, size) in [
            ("file10.txt", 10usize),
            ("file2.txt", 2),
            ("zeta.md", 5),
            (".hidden.txt", 1),
        ] {
            let mut f = File::create(p.join(name)).unwrap();
            f.write_all(&vec![b'x'; size]).unwrap();
        }
        dir
    }

    #[test]
    fn reads_entries_hiding_dotfiles_by_default() {
        let dir = fixture();
        let listing = Listing::read(
            &Location::local(dir.path()),
            &ReadOptions::default(),
            SortSpec::default(),
        )
        .unwrap();
        let names: Vec<&str> = listing.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["Alpha", "beta", "file2.txt", "file10.txt", "zeta.md"]
        );
        assert_eq!(listing.hidden_count(), 2);
    }

    #[test]
    fn hidden_entries_can_be_shown() {
        let dir = fixture();
        let opts = ReadOptions {
            show_hidden: true,
            ..Default::default()
        };
        let listing =
            Listing::read(&Location::local(dir.path()), &opts, SortSpec::default()).unwrap();
        let names: Vec<&str> = listing.entries().iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&".hidden.txt"));
        assert!(names.contains(&".hiddendir"));
        assert_eq!(names.len(), 7);
    }

    #[test]
    fn parent_entry_is_prepended_when_requested() {
        let dir = fixture();
        let opts = ReadOptions {
            show_parent: true,
            ..Default::default()
        };
        let listing =
            Listing::read(&Location::local(dir.path()), &opts, SortSpec::default()).unwrap();
        assert_eq!(listing.entries()[0].name, "..");
        assert_eq!(listing.cursor(), 0);
        assert_eq!(listing.real_entries().count(), 5);
    }

    #[test]
    fn mask_filters_files_but_keeps_directories() {
        let dir = fixture();
        let opts = ReadOptions {
            mask: MaskSet::parse("*.txt").unwrap(),
            ..Default::default()
        };
        let listing =
            Listing::read(&Location::local(dir.path()), &opts, SortSpec::default()).unwrap();
        let names: Vec<&str> = listing.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta", "file2.txt", "file10.txt"]);
    }

    #[test]
    fn sorting_can_be_changed_after_reading() {
        let dir = fixture();
        let mut listing = Listing::read(
            &Location::local(dir.path()),
            &ReadOptions::default(),
            SortSpec::default(),
        )
        .unwrap();
        listing.resort(SortSpec {
            field: SortField::Name,
            reversed: true,
            ..SortSpec::default()
        });
        let names: Vec<&str> = listing.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["beta", "Alpha", "zeta.md", "file10.txt", "file2.txt"]
        );
    }

    #[test]
    fn cursor_moves_and_clamps() {
        let dir = fixture();
        let mut listing = Listing::read(
            &Location::local(dir.path()),
            &ReadOptions::default(),
            SortSpec::default(),
        )
        .unwrap();
        assert_eq!(listing.cursor_entry().unwrap().name, "Alpha");
        listing.move_cursor(2, false);
        assert_eq!(listing.cursor_entry().unwrap().name, "file2.txt");
        listing.move_cursor(100, false);
        assert_eq!(listing.cursor(), listing.len() - 1);
        listing.move_cursor(1, true);
        assert_eq!(listing.cursor(), 0);
        listing.move_cursor(-1, true);
        assert_eq!(listing.cursor(), listing.len() - 1);
        listing.set_cursor(0);
        assert_eq!(listing.cursor(), 0);
    }

    #[test]
    fn statistics_split_files_and_dirs() {
        let dir = fixture();
        let listing = Listing::read(
            &Location::local(dir.path()),
            &ReadOptions::default(),
            SortSpec::default(),
        )
        .unwrap();
        let (files, dirs, bytes) = listing.statistics();
        assert_eq!(files, 3);
        assert_eq!(dirs, 2);
        assert_eq!(bytes, 17);
    }

    #[test]
    fn selection_statistics_use_marked_entries() {
        let dir = fixture();
        let listing = Listing::read(
            &Location::local(dir.path()),
            &ReadOptions::default(),
            SortSpec::default(),
        )
        .unwrap();
        let mut sel = Selection::new();
        sel.set(["file10.txt", "zeta.md", "beta"]);
        assert_eq!(listing.selection_statistics(&sel), (3, 15));
    }

    #[test]
    fn reread_keeps_cursor_on_the_same_name() {
        let dir = fixture();
        let mut listing = Listing::read(
            &Location::local(dir.path()),
            &ReadOptions::default(),
            SortSpec::default(),
        )
        .unwrap();
        listing.set_cursor(listing.find("file2.txt").unwrap());
        File::create(dir.path().join("aaa.txt")).unwrap();
        listing.reread().unwrap();
        assert_eq!(listing.cursor_entry().unwrap().name, "file2.txt");
        assert!(listing.find("aaa.txt").is_some());
    }

    #[test]
    fn missing_directory_reports_io_error() {
        let err = Listing::read(
            &Location::local("/definitely/not/here"),
            &ReadOptions::default(),
            SortSpec::default(),
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::Io { .. }));
    }

    #[test]
    fn non_local_locations_are_rejected() {
        let err = Listing::read(
            &Location::archive("/a.zip", ""),
            &ReadOptions::default(),
            SortSpec::default(),
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::Unsupported(_)));
    }

    #[test]
    fn from_entries_supports_backend_listings() {
        let raw = vec![
            Entry::new("b.txt", "/x/b.txt".into(), EntryKind::File, 1),
            Entry::new("a.txt", "/x/a.txt".into(), EntryKind::File, 1),
        ];
        let listing = Listing::from_entries(
            Location::archive("/x.zip", ""),
            raw,
            &ReadOptions::default(),
            SortSpec::default(),
        );
        assert_eq!(listing.entries()[0].name, "a.txt");
    }
}

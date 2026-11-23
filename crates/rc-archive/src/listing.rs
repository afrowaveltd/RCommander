//! The directory model of an archive: the view the file panel renders.

use std::collections::BTreeMap;
use std::path::Path;

use rc_core::entry::{Entry, EntryKind};

use crate::error::Result;
use crate::member;
use crate::open;
use crate::read::ArchiveRead;

/// Directory listing model for one directory *inside* the archive.
///
/// [`list_dir`] is the function the panel calls; this struct is the same thing
/// in an owned form, for callers that prefer to keep the listing around
/// together with the path it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveDir {
    /// The directory inside the archive (`""` for the archive root).
    pub path: String,
    /// Direct children, directories first, then by name.
    pub entries: Vec<Entry>,
}

impl ArchiveDir {
    /// Build the listing of `inner` (`""` = archive root) — see [`list_dir`].
    pub fn list(archive: &Path, inner: &str) -> Result<ArchiveDir> {
        Ok(ArchiveDir {
            path: member::normalize_inner(inner),
            entries: list_dir(archive, inner)?,
        })
    }

    /// True when the directory holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The `..` entry pointing at the parent directory, or `None` at the root.
    pub fn parent_entry(&self, archive: &Path) -> Option<Entry> {
        if self.path.is_empty() {
            return None;
        }
        let parent = self
            .path
            .rsplit_once('/')
            .map(|(p, _)| p.to_string())
            .unwrap_or_default();
        Some(Entry::parent_entry(member::display_path(
            archive, &parent, "",
        )))
    }
}

/// One direct child, merged from every member that mentions it.
#[derive(Clone, Debug)]
struct Child {
    is_dir: bool,
    size: u64,
    mtime: Option<i64>,
}

/// Build the listing of `inner` (`""` = archive root) for the panel.
///
/// Only *direct* children are returned; intermediate path components are
/// synthesized as directories, so a member `a/b/c.txt` yields `a` at the root
/// and `b` inside `a`, even when the archive contains no explicit directory
/// entries. Members whose names would escape the archive (`../x`, `/x`,
/// `C:\x`) are never listed.
///
/// A path that does not exist in the archive is **not** an error: the result is
/// an empty list, which is what the panel wants for a stale view.
pub fn list_dir(path: &Path, inner: &str) -> Result<Vec<Entry>> {
    let archive = open(path)?;
    build_listing(archive.as_ref(), path, inner)
}

/// [`list_dir`] on an already open archive (used internally and by tests).
pub(crate) fn build_listing(
    archive: &dyn ArchiveRead,
    archive_path: &Path,
    inner: &str,
) -> Result<Vec<Entry>> {
    let base = member::normalize_inner(inner);
    let prefix = if base.is_empty() {
        String::new()
    } else {
        format!("{base}/")
    };

    let mut children: BTreeMap<String, Child> = BTreeMap::new();
    for entry in archive.entries() {
        if entry.path.is_empty() {
            continue;
        }
        // The raw name decides: `/abs.txt` normalises to the safe-looking
        // `abs.txt`, but it must never appear in a listing.
        let raw = archive.raw_name(&entry.path).unwrap_or(entry.path.as_str());
        if member::unsafe_reason(raw).is_some() {
            continue;
        }
        let rest = match entry.path.strip_prefix(&prefix) {
            Some(rest) => rest,
            None => continue,
        };
        if rest.is_empty() {
            continue;
        }
        let (name, is_dir, size, mtime) = match rest.split_once('/') {
            Some((name, _)) => (name.to_string(), true, 0, entry.mtime),
            None => (rest.to_string(), entry.is_dir, entry.size, entry.mtime),
        };
        if name.is_empty() {
            continue;
        }
        children
            .entry(name)
            .and_modify(|child| {
                // A directory entry always wins over a file entry of the same
                // name, and the larger size is the more useful one.
                child.is_dir |= is_dir;
                if !is_dir && !child.is_dir {
                    child.size = child.size.max(size);
                }
                if child.mtime.is_none() {
                    child.mtime = mtime;
                }
            })
            .or_insert(Child {
                is_dir,
                size: if is_dir { 0 } else { size },
                mtime,
            });
    }

    let mut out: Vec<Entry> = Vec::with_capacity(children.len());
    for (name, child) in children {
        let kind = if child.is_dir {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        let size = if child.is_dir { 0 } else { child.size };
        let mut entry = Entry::new(
            name.clone(),
            member::display_path(archive_path, &base, &name),
            kind,
            size,
        );
        entry.mtime = child.mtime;
        out.push(entry);
    }
    // Directories first, then case-insensitive name order: a stable default that
    // the panel is free to re-sort.
    out.sort_by(|a, b| {
        b.is_dir()
            .cmp(&a.is_dir())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(out)
}

/// True when `inner` names a directory that exists (or is a synthesized parent).
///
/// The archive root (`""`) is always a directory. A member path that is not in
/// the archive at all is not a directory.
pub fn is_dir(path: &Path, inner: &str) -> Result<bool> {
    let base = member::normalize_inner(inner);
    if base.is_empty() {
        return Ok(true);
    }
    let archive = open(path)?;
    let prefix = format!("{base}/");
    for entry in archive.entries() {
        // A file entry may shadow a directory of the same name (hand-built
        // archives do that), so both cases are checked against every member.
        if (entry.path == base && entry.is_dir) || entry.path.starts_with(&prefix) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::{PackOptions, create};
    use std::io::Write;

    struct Fixture {
        _dir: tempfile::TempDir,
        path: std::path::PathBuf,
    }

    /// A zip with an explicit directory, a nested member, a unicode name and an
    /// empty file.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("panel.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.add_directory("empty_dir", options).unwrap();
        write_member(&mut writer, "a/b/c.txt", b"deep", options);
        write_member(&mut writer, "a/top.txt", b"top", options);
        write_member(&mut writer, "žluťoučký.txt", "kůň".as_bytes(), options);
        write_member(&mut writer, "root.txt", b"", options);
        writer.finish().unwrap();
        Fixture { _dir: dir, path }
    }

    fn write_member<W: Write + std::io::Seek>(
        writer: &mut zip::ZipWriter<W>,
        name: &str,
        data: &[u8],
        options: zip::write::SimpleFileOptions,
    ) {
        writer.start_file(name, options).unwrap();
        writer.write_all(data).unwrap();
    }

    fn names(entries: &[Entry]) -> Vec<String> {
        entries.iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn root_lists_synthesized_and_explicit_children() {
        let fixture = fixture();
        let entries = list_dir(&fixture.path, "").unwrap();
        assert_eq!(
            names(&entries),
            ["a", "empty_dir", "root.txt", "žluťoučký.txt"]
        );

        let a = entries.iter().find(|e| e.name == "a").unwrap();
        assert!(a.is_dir(), "a is a synthesized parent directory");
        assert_eq!(a.size, 0);
        let root_txt = entries.iter().find(|e| e.name == "root.txt").unwrap();
        assert!(root_txt.is_file());
        assert_eq!(root_txt.size, 0, "empty file");
        let unicode = entries.iter().find(|e| e.name == "žluťoučký.txt").unwrap();
        assert_eq!(unicode.size, "kůň".len() as u64);
    }

    #[test]
    fn nested_directories_list_their_direct_children() {
        let fixture = fixture();
        let entries = list_dir(&fixture.path, "a").unwrap();
        assert_eq!(names(&entries), ["b", "top.txt"]);
        assert!(entries[0].is_dir());
        assert_eq!(entries[1].size, 3);

        // Trailing slashes and leading slashes in `inner` are tolerated.
        assert_eq!(
            names(&list_dir(&fixture.path, "a/").unwrap()),
            ["b", "top.txt"]
        );
        assert_eq!(
            names(&list_dir(&fixture.path, "/a").unwrap()),
            ["b", "top.txt"]
        );

        let deep = list_dir(&fixture.path, "a/b").unwrap();
        assert_eq!(names(&deep), ["c.txt"]);
        assert_eq!(deep[0].size, 4);
    }

    #[test]
    fn empty_explicit_directory_lists_nothing() {
        let fixture = fixture();
        assert!(list_dir(&fixture.path, "empty_dir").unwrap().is_empty());
    }

    #[test]
    fn nonexistent_inner_path_is_an_empty_list() {
        let fixture = fixture();
        assert!(list_dir(&fixture.path, "nope").unwrap().is_empty());
        assert!(list_dir(&fixture.path, "nope/deeper").unwrap().is_empty());
        assert!(list_dir(&fixture.path, "a/nope").unwrap().is_empty());
    }

    #[test]
    fn file_paths_have_the_archive_prefix() {
        let fixture = fixture();
        let entries = list_dir(&fixture.path, "a").unwrap();
        let top = entries.iter().find(|e| e.name == "top.txt").unwrap();
        assert_eq!(top.path, fixture.path.join("a").join("top.txt"));
    }

    #[test]
    fn is_dir_knows_explicit_and_synthesized_directories() {
        let fixture = fixture();
        assert!(
            is_dir(&fixture.path, "").unwrap(),
            "the root is always a directory"
        );
        assert!(is_dir(&fixture.path, "a").unwrap(), "synthesized parent");
        assert!(
            is_dir(&fixture.path, "a/b").unwrap(),
            "deep synthesized parent"
        );
        assert!(
            is_dir(&fixture.path, "empty_dir").unwrap(),
            "explicit directory entry"
        );
        assert!(!is_dir(&fixture.path, "root.txt").unwrap());
        assert!(!is_dir(&fixture.path, "nope").unwrap());
        assert!(!is_dir(&fixture.path, "a/top.txt").unwrap());
    }

    #[test]
    fn unsafe_members_are_not_listed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hostile.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        write_member(&mut writer, "../evil.txt", b"bad", options);
        write_member(&mut writer, "/abs.txt", b"bad", options);
        write_member(&mut writer, "ok.txt", b"fine", options);
        writer.finish().unwrap();

        let entries = list_dir(&path, "").unwrap();
        assert_eq!(
            names(&entries),
            ["ok.txt"],
            "hostile members never reach the panel"
        );
    }

    #[test]
    fn backslash_separated_members_are_split() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backslash.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        write_member(&mut writer, "dir\\sub\\x.txt", b"x", options);
        writer.finish().unwrap();

        assert_eq!(names(&list_dir(&path, "").unwrap()), ["dir"]);
        assert_eq!(names(&list_dir(&path, "dir").unwrap()), ["sub"]);
        assert_eq!(names(&list_dir(&path, "dir/sub").unwrap()), ["x.txt"]);
    }

    #[test]
    fn tar_gz_listing_works_too() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("tree");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("one.txt"), b"1").unwrap();
        std::fs::write(src.join("sub").join("two.txt"), b"22").unwrap();
        let archive = dir.path().join("tree.tar.gz");
        create(&archive, &[src], &PackOptions::default()).unwrap();

        let root = list_dir(&archive, "").unwrap();
        assert_eq!(names(&root), ["tree"]);
        let tree = list_dir(&archive, "tree").unwrap();
        assert_eq!(names(&tree), ["sub", "one.txt"]);
        assert!(is_dir(&archive, "tree/sub").unwrap());
        assert_eq!(names(&list_dir(&archive, "tree/sub").unwrap()), ["two.txt"]);
    }

    #[test]
    fn archive_dir_keeps_the_normalised_path_and_parent_entry() {
        let fixture = fixture();
        let listing = ArchiveDir::list(&fixture.path, "a/b/").unwrap();
        assert_eq!(listing.path, "a/b");
        assert_eq!(names(&listing.entries), ["c.txt"]);
        assert!(!listing.is_empty());

        let parent = listing.parent_entry(&fixture.path).unwrap();
        assert_eq!(parent.name, "..");
        assert!(parent.is_dir());
        assert_eq!(parent.path, fixture.path.join("a"));

        let root = ArchiveDir::list(&fixture.path, "").unwrap();
        assert!(
            root.parent_entry(&fixture.path).is_none(),
            "the root has no parent entry"
        );
    }

    #[test]
    fn duplicate_entries_collapse_with_directory_winning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dup.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        // A file entry called `x`, then a directory entry called `x/`.
        write_member(&mut writer, "x", b"data", options);
        writer.add_directory("x", options).unwrap();
        write_member(&mut writer, "x/inner.txt", b"i", options);
        writer.finish().unwrap();

        let entries = list_dir(&path, "").unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_dir(), "directory wins");
        assert!(is_dir(&path, "x").unwrap());
    }

    #[test]
    fn single_file_compressed_archives_list_one_member() {
        let dir = tempfile::tempdir().unwrap();
        let gz = dir.path().join("notes.txt.gz");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
        encoder.write_all(b"hello").unwrap();
        std::fs::write(&gz, encoder.finish().unwrap()).unwrap();

        let entries = list_dir(&gz, "").unwrap();
        assert_eq!(names(&entries), ["notes.txt"]);
        assert_eq!(entries[0].size, 5);
        assert!(
            list_dir(&gz, "notes.txt").unwrap().is_empty(),
            "a file has no children"
        );
        assert!(!is_dir(&gz, "notes.txt").unwrap());
    }

    #[test]
    fn listing_a_non_archive_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("bogus.zip");
        std::fs::write(&bogus, b"definitely not a zip").unwrap();
        assert!(list_dir(&bogus, "").is_err());
        assert!(is_dir(&bogus, "anything").is_err());
    }
}

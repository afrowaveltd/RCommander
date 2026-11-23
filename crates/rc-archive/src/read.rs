//! The uniform archive reader: one member description type and one trait.
//!
//! Every container is reduced to a flat list of [`ArchiveEntry`] values plus a
//! way to stream a member's bytes. That is enough for the file panel to present
//! an archive as a directory, and for the copy engine to move members out of it.

use std::io::Write;

use crate::error::{ArchiveError, Result};
use crate::kind::ArchiveKind;
use crate::member;

/// One member of an archive, as the panel needs to display it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// `/`-separated path inside the archive, no leading slash.
    ///
    /// Members created on Windows have their backslashes converted, and `.`
    /// components are removed. Hostile names (`../x`, `/x`) keep their `..`
    /// components so that extraction can refuse them; use
    /// [`ArchiveRead::raw_name`] to obtain the name exactly as stored.
    pub path: String,
    /// Last component of [`ArchiveEntry::path`]; also the listing key.
    pub name: String,
    /// True for directory members (explicit entries and synthesized parents).
    pub is_dir: bool,
    /// Uncompressed size in bytes; always `0` for directories.
    pub size: u64,
    /// Compressed (stored) size, when the container records it per member.
    pub compressed_size: Option<u64>,
    /// Modification time as unix seconds, when the container records one.
    pub mtime: Option<i64>,
}

impl ArchiveEntry {
    /// Build an entry from a raw member name, normalising the path.
    ///
    /// Directory entries always report a size of `0`; timestamps and compressed
    /// sizes are filled in by the backend afterwards.
    pub fn new(path: impl AsRef<str>, is_dir: bool, size: u64) -> Self {
        let path = member::normalize(path.as_ref());
        let name = member::file_name(&path).to_string();
        ArchiveEntry {
            path,
            name,
            is_dir,
            size: if is_dir { 0 } else { size },
            compressed_size: None,
            mtime: None,
        }
    }

    /// Number of path components, i.e. how deep the member sits.
    pub fn depth(&self) -> usize {
        self.path.split('/').filter(|c| !c.is_empty()).count()
    }

    /// The containing directory inside the archive (`""` for the root).
    pub fn parent(&self) -> String {
        match self.path.rsplit_once('/') {
            Some((parent, _)) => parent.to_string(),
            None => String::new(),
        }
    }
}

/// Read access to one archive.
///
/// The trait is object safe: [`crate::open`] returns `Box<dyn ArchiveRead>` and
/// the panel/copy engine only ever sees this interface. Implementations list
/// their members eagerly (once, when the archive is opened) and stream member
/// bytes on demand.
pub trait ArchiveRead {
    /// Which container this is.
    fn kind(&self) -> ArchiveKind;

    /// Every member, in archive order, directories included.
    fn entries(&self) -> &[ArchiveEntry];

    /// Read one member fully into memory (empty for directories).
    ///
    /// This is the convenient API for the viewer and for small members; it
    /// allocates the member's full uncompressed size. Use
    /// [`ArchiveRead::copy_to`] for anything that may be large.
    fn read(&mut self, path: &str) -> Result<Vec<u8>>;

    /// Stream one member into `writer` and return the number of bytes written.
    ///
    /// This never buffers the member: data is pumped through a fixed-size chunk
    /// buffer, so a 1 GiB member needs no more memory than a 1 KiB one. The only
    /// exception is the 7z backend, whose decoder needs its solid block's
    /// dictionary in memory.
    fn copy_to(&mut self, path: &str, writer: &mut dyn Write) -> Result<u64>;

    /// The member name exactly as it is stored inside the archive.
    ///
    /// This differs from [`ArchiveEntry::path`] for hostile or
    /// foreign-separator names (`/abs.txt`, `..\\evil`, `C:\\x`), which is what
    /// extraction needs in order to refuse them. The default implementation
    /// reports the normalised path.
    fn raw_name(&self, path: &str) -> Option<&str> {
        let want = member::normalize(path);
        self.entries()
            .iter()
            .find(|e| e.path == want)
            .map(|e| e.path.as_str())
    }

    /// Look one member up by its archive path (normalised before comparing).
    fn entry(&self, path: &str) -> Option<&ArchiveEntry> {
        let want = member::normalize(path);
        self.entries().iter().find(|e| e.path == want)
    }

    /// True when `path` names a directory member.
    fn is_dir(&self, path: &str) -> bool {
        match self.entry(path) {
            Some(e) => e.is_dir,
            None => {
                let want = member::normalize(path);
                let prefix = format!("{want}/");
                !want.is_empty() && self.entries().iter().any(|e| e.path.starts_with(&prefix))
            }
        }
    }

    /// Number of non-directory members.
    fn file_count(&self) -> usize {
        self.entries().iter().filter(|e| !e.is_dir).count()
    }

    /// Sum of the recorded uncompressed sizes of all non-directory members.
    fn uncompressed_size(&self) -> u64 {
        self.entries()
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.size)
            .sum()
    }
}

/// The error used when a requested member is not in the archive.
pub(crate) fn member_not_found(archive: &std::path::Path, member: &str) -> ArchiveError {
    ArchiveError::Malformed(format!(
        "no member named {member:?} in {}",
        archive.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_normalise_their_path() {
        let e = ArchiveEntry::new("/a\\b/c.txt", false, 12);
        assert_eq!(e.path, "a/b/c.txt");
        assert_eq!(e.name, "c.txt");
        assert_eq!(e.size, 12);
        assert_eq!(e.depth(), 3);
        assert_eq!(e.parent(), "a/b");
    }

    #[test]
    fn directory_entries_report_zero_size() {
        let e = ArchiveEntry::new("dir/", true, 4096);
        assert!(e.is_dir);
        assert_eq!(e.size, 0);
        assert_eq!(e.path, "dir");
        assert_eq!(e.parent(), "");
    }

    #[test]
    fn hostile_names_keep_their_parent_components() {
        let e = ArchiveEntry::new("../evil.txt", false, 3);
        assert_eq!(e.path, "../evil.txt");
        assert_eq!(e.name, "evil.txt");
    }

    /// A tiny in-memory archive used to exercise the trait's default methods.
    struct Fake {
        entries: Vec<ArchiveEntry>,
        data: Vec<u8>,
    }

    impl ArchiveRead for Fake {
        fn kind(&self) -> ArchiveKind {
            ArchiveKind::Zip
        }
        fn entries(&self) -> &[ArchiveEntry] {
            &self.entries
        }
        fn read(&mut self, _path: &str) -> Result<Vec<u8>> {
            Ok(self.data.clone())
        }
        fn copy_to(&mut self, _path: &str, writer: &mut dyn Write) -> Result<u64> {
            writer
                .write_all(&self.data)
                .map_err(|e| ArchiveError::io("fake", e))?;
            Ok(self.data.len() as u64)
        }
    }

    fn fake() -> Fake {
        Fake {
            entries: vec![
                ArchiveEntry::new("dir", true, 0),
                ArchiveEntry::new("dir/a.txt", false, 3),
                ArchiveEntry::new("b.txt", false, 0),
            ],
            data: b"abc".to_vec(),
        }
    }

    #[test]
    fn default_lookups_normalise_the_query() {
        let mut f = fake();
        assert_eq!(f.kind(), ArchiveKind::Zip);
        assert!(f.entry("dir/a.txt").is_some());
        assert!(
            f.entry("/dir/a.txt").is_some(),
            "leading slashes are tolerated"
        );
        assert!(f.entry("dir\\a.txt").is_some(), "backslashes are tolerated");
        assert!(f.entry("missing").is_none());
        assert!(f.is_dir("dir"));
        assert!(!f.is_dir("b.txt"));
        assert!(!f.is_dir("nope"));
        assert_eq!(f.file_count(), 2);
        assert_eq!(f.uncompressed_size(), 3);
        assert_eq!(f.raw_name("dir/a.txt"), Some("dir/a.txt"));
        assert_eq!(f.read("dir/a.txt").unwrap(), b"abc");
        let mut out = Vec::new();
        assert_eq!(f.copy_to("dir/a.txt", &mut out).unwrap(), 3);
        assert_eq!(out, b"abc");
    }

    #[test]
    fn not_found_error_names_the_member() {
        let err = member_not_found(std::path::Path::new("/tmp/a.zip"), "ghost.txt");
        let text = err.to_string();
        assert!(
            text.contains("ghost.txt") && text.contains("a.zip"),
            "{text}"
        );
    }
}

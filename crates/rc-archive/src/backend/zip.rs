//! Zip container support: reading members, and the rewrite primitives used by
//! [`crate::add_to_zip`] and [`crate::remove_from_zip`].

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Seek, Write};
use std::path::{Path, PathBuf};

use zip::result::ZipError;
use zip::{ZipArchive, ZipWriter};

use crate::error::{ArchiveError, Result};
use crate::kind::ArchiveKind;
use crate::member;
use crate::read::{ArchiveEntry, ArchiveRead, member_not_found};
use crate::stream;
use crate::timeutil;

/// Map a [`ZipError`] to an [`ArchiveError`], naming the member when there is
/// one.
pub(crate) fn zip_error_for(path: &Path, member: Option<&str>, error: ZipError) -> ArchiveError {
    let archive = path.display().to_string();
    match error {
        ZipError::Io(io) => ArchiveError::io(path, io),
        ZipError::FileNotFound => match member {
            Some(name) => member_not_found(path, name),
            None => ArchiveError::malformed(format!("{archive}: member not found")),
        },
        ZipError::UnsupportedArchive(reason) => ArchiveError::unsupported_member(
            archive,
            "zip",
            match member {
                Some(name) => format!("{name:?}: {reason}"),
                None => reason.to_string(),
            },
        ),
        ZipError::CompressionMethodNotSupported(method) => ArchiveError::unsupported_member(
            archive,
            "zip",
            match member {
                Some(name) => format!("{name:?}: compression method {method}"),
                None => format!("compression method {method}"),
            },
        ),
        ZipError::InvalidPassword => ArchiveError::PasswordRequired,
        other => ArchiveError::malformed(format!("{archive}: {other}")),
    }
}

/// Reading side of a zip archive.
///
/// The central directory is parsed once when the archive is opened; member data
/// is streamed from the underlying file on demand.
pub(crate) struct ZipBackend {
    path: PathBuf,
    archive: ZipArchive<BufReader<File>>,
    entries: Vec<ArchiveEntry>,
    raw: Vec<String>,
    /// Index passed to [`ZipArchive::by_index`] for each entry.
    index: Vec<usize>,
    /// Normalised path -> position in `entries`.
    by_path: HashMap<String, usize>,
    encrypted: Vec<bool>,
    crc: Vec<u32>,
}

impl ZipBackend {
    /// Open `path` and build the member list.
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| ArchiveError::io(path, e))?;
        let archive: ZipArchive<BufReader<File>> =
            ZipArchive::new(BufReader::new(file)).map_err(|e| zip_error_for(path, None, e))?;

        let count = archive.len();
        let mut entries = Vec::with_capacity(count);
        let mut raw = Vec::with_capacity(count);
        let mut index = Vec::with_capacity(count);
        let mut by_path: HashMap<String, usize> = HashMap::with_capacity(count);
        let mut encrypted = Vec::with_capacity(count);
        let mut crc = Vec::with_capacity(count);

        for i in 0..count {
            let name = match archive.name_for_index(i) {
                None => continue,
                Some(Ok(name)) => name.into_owned(),
                Some(Err(e)) => return Err(zip_error_for(path, None, e)),
            };
            let meta = archive
                .by_index_data(i)
                .map_err(|e| zip_error_for(path, Some(&name), e))?;
            let is_dir = meta.is_dir();
            let mut entry = ArchiveEntry::new(&name, is_dir, if is_dir { 0 } else { meta.size() });
            entry.compressed_size = Some(meta.compressed_size());
            entry.mtime = meta.last_modified().and_then(timeutil::dos_to_unix);

            let position = entries.len();
            by_path.entry(entry.path.clone()).or_insert(position);
            entries.push(entry);
            raw.push(name);
            index.push(i);
            encrypted.push(meta.encrypted());
            crc.push(meta.crc32());
        }

        Ok(ZipBackend {
            path: path.to_path_buf(),
            archive,
            entries,
            raw,
            index,
            by_path,
            encrypted,
            crc,
        })
    }

    /// Position of `path` in the member table.
    fn slot(&self, path: &str) -> Result<usize> {
        super::lookup(&self.by_path, path).ok_or_else(|| member_not_found(&self.path, path))
    }

    /// True when the entry at `slot` is encrypted and therefore unreadable
    /// without a password.
    fn needs_password(&self, slot: usize) -> bool {
        self.encrypted.get(slot).copied().unwrap_or(false)
    }

    /// Re-compress nothing: copy every existing member except the ones whose
    /// normalised path is in `skip` into `writer`.
    ///
    /// Used by the atomic rewrite paths; entries keep their original
    /// compression, so the operation is cheap.
    pub(crate) fn copy_entries_into<W: Write + Seek>(
        &mut self,
        writer: &mut ZipWriter<W>,
        skip: &HashSet<String>,
    ) -> Result<u64> {
        let mut copied = 0u64;
        for i in 0..self.archive.len() {
            let name = match self.archive.name_for_index(i) {
                None => continue,
                Some(Ok(name)) => name.into_owned(),
                Some(Err(e)) => return Err(zip_error_for(&self.path, None, e)),
            };
            let normalised = member::normalize(&name);
            if skip.contains(&normalised) || skip.iter().any(|s| under(&normalised, s)) {
                continue;
            }
            let file = self
                .archive
                .by_index(i)
                .map_err(|e| zip_error_for(&self.path, Some(&name), e))?;
            writer
                .raw_copy_file(file)
                .map_err(|e| zip_error_for(&self.path, Some(&name), e))?;
            copied += 1;
        }
        Ok(copied)
    }
}

/// True when `path` is `parent` itself or lives below it.
pub(crate) fn under(path: &str, parent: &str) -> bool {
    path == parent || path.starts_with(parent) && path.as_bytes().get(parent.len()) == Some(&b'/')
}

impl ArchiveRead for ZipBackend {
    fn kind(&self) -> ArchiveKind {
        ArchiveKind::Zip
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let slot = self.slot(path)?;
        if self.entries[slot].is_dir {
            return Ok(Vec::new());
        }
        if self.needs_password(slot) {
            return Err(ArchiveError::PasswordRequired);
        }
        let archive_path = self.path.clone();
        let entry_path = self.entries[slot].path.clone();
        let expected_crc = self.crc[slot];
        let index = self.index[slot];
        let mut file = self
            .archive
            .by_index(index)
            .map_err(|e| zip_error_for(&archive_path, Some(&entry_path), e))?;
        stream::pump_vec(
            &mut file,
            None,
            Some(expected_crc),
            &archive_path,
            &entry_path,
        )
    }

    fn copy_to(&mut self, path: &str, writer: &mut dyn Write) -> Result<u64> {
        let slot = self.slot(path)?;
        if self.entries[slot].is_dir {
            return Ok(0);
        }
        if self.needs_password(slot) {
            return Err(ArchiveError::PasswordRequired);
        }
        let archive_path = self.path.clone();
        let entry_path = self.entries[slot].path.clone();
        let expected_crc = self.crc[slot];
        let index = self.index[slot];
        let mut file = self
            .archive
            .by_index(index)
            .map_err(|e| zip_error_for(&archive_path, Some(&entry_path), e))?;
        stream::pump(
            &mut file,
            writer,
            None,
            Some(expected_crc),
            &archive_path,
            &entry_path,
        )
    }

    fn raw_name(&self, path: &str) -> Option<&str> {
        super::lookup(&self.by_path, path).and_then(|slot| self.raw.get(slot).map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::ArchiveKind as Kind;

    struct TempZip {
        _dir: tempfile::TempDir,
        path: PathBuf,
    }

    impl TempZip {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("t.zip");
            TempZip { _dir: dir, path }
        }
    }

    /// Build a zip by hand so hostile names can be stored verbatim.
    fn write_zip(path: &Path, members: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in members {
            if name.ends_with('/') {
                writer
                    .add_directory(name.trim_end_matches('/'), options)
                    .unwrap();
            } else {
                writer.start_file(*name, options).unwrap();
                writer.write_all(data).unwrap();
            }
        }
        writer.finish().unwrap();
    }

    #[test]
    fn reads_members_and_metadata() {
        let zip = TempZip::new();
        write_zip(&zip.path, &[("a.txt", b"hello"), ("dir/b.txt", b"nested")]);

        let mut backend = ZipBackend::open(&zip.path).unwrap();
        assert_eq!(backend.entries().len(), 2);
        assert_eq!(backend.kind(), Kind::Zip);
        let a = backend.entry("a.txt").unwrap();
        assert_eq!(a.size, 5);
        assert!(a.compressed_size.is_some());
        assert!(!a.is_dir);

        assert_eq!(backend.read("a.txt").unwrap(), b"hello");
        assert_eq!(backend.read("dir/b.txt").unwrap(), b"nested");

        let mut out = Vec::new();
        assert_eq!(backend.copy_to("dir/b.txt", &mut out).unwrap(), 6);
        assert_eq!(out, b"nested");

        // Directory members read as empty.
        assert!(
            backend.read("dir").is_err(),
            "dir was not stored as an explicit entry"
        );
        assert!(backend.read("nope").is_err());
    }

    #[test]
    fn backslash_names_are_normalised_but_kept_raw() {
        let zip = TempZip::new();
        write_zip(&zip.path, &[("dir\\sub\\c.txt", b"x")]);
        let mut backend = ZipBackend::open(&zip.path).unwrap();
        assert_eq!(backend.entries()[0].path, "dir/sub/c.txt");
        assert_eq!(backend.raw_name("dir/sub/c.txt"), Some("dir\\sub\\c.txt"));
        assert_eq!(backend.read("dir/sub/c.txt").unwrap(), b"x");
        assert_eq!(
            backend.entry("dir").map(|e| e.is_dir),
            None,
            "no explicit directory entry"
        );
        assert!(backend.is_dir("dir"), "synthesized parent is a directory");
    }

    #[test]
    fn hostile_names_keep_their_raw_form() {
        let zip = TempZip::new();
        write_zip(&zip.path, &[("../evil.txt", b"bad"), ("/abs.txt", b"bad2")]);
        let backend = ZipBackend::open(&zip.path).unwrap();
        assert_eq!(backend.entries()[0].path, "../evil.txt");
        assert_eq!(backend.raw_name("../evil.txt"), Some("../evil.txt"));
        assert_eq!(backend.raw_name("/abs.txt"), Some("/abs.txt"));
        assert_eq!(backend.entries()[1].path, "abs.txt");
        // The normalised path is safe to look up.
        assert!(backend.entry("abs.txt").is_some());
    }

    #[test]
    fn explicit_directory_entries_are_listed() {
        let zip = TempZip::new();
        write_zip(&zip.path, &[("dir/", b""), ("dir/a.txt", b"1")]);
        let backend = ZipBackend::open(&zip.path).unwrap();
        let dir = backend.entry("dir").unwrap();
        assert!(dir.is_dir);
        assert_eq!(dir.size, 0);
        assert_eq!(backend.file_count(), 1);
    }

    #[test]
    fn unicode_names_round_trip() {
        let zip = TempZip::new();
        write_zip(&zip.path, &[("žluťoučký.txt", "kůň".as_bytes())]);
        let mut backend = ZipBackend::open(&zip.path).unwrap();
        assert_eq!(backend.entries()[0].name, "žluťoučký.txt");
        assert_eq!(backend.read("žluťoučký.txt").unwrap(), "kůň".as_bytes());
    }

    #[test]
    fn corrupt_member_data_is_reported_not_panicked() {
        let zip = TempZip::new();
        // Stored (not deflated) so the member data is a contiguous run of bytes.
        let file = File::create(&zip.path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("a.txt", options).unwrap();
        writer.write_all(&vec![b'x'; 4096]).unwrap();
        writer.finish().unwrap();

        let mut bytes = std::fs::read(&zip.path).unwrap();
        let run = bytes
            .windows(64)
            .position(|w| w == vec![b'x'; 64].as_slice())
            .expect("member data must be findable");
        bytes[run + 32] ^= 0xFF;
        std::fs::write(&zip.path, bytes).unwrap();

        match ZipBackend::open(&zip.path) {
            Err(e) => assert!(!e.to_string().is_empty()),
            Ok(mut backend) => {
                // Either the decoder or the CRC check catches the damage.
                assert!(backend.read("a.txt").is_err());
            }
        }
    }

    #[test]
    fn copy_entries_into_skips_selected_members() {
        let source = TempZip::new();
        write_zip(
            &source.path,
            &[
                ("keep.txt", b"k"),
                ("drop.txt", b"d"),
                ("dir/inner.txt", b"i"),
            ],
        );

        let mut backend = ZipBackend::open(&source.path).unwrap();
        let target = TempZip::new();
        let mut writer = ZipWriter::new(File::create(&target.path).unwrap());
        let mut skip = HashSet::new();
        skip.insert("drop.txt".to_string());
        skip.insert("dir".to_string());
        assert_eq!(backend.copy_entries_into(&mut writer, &skip).unwrap(), 1);
        writer.finish().unwrap();

        let copied = ZipBackend::open(&target.path).unwrap();
        assert_eq!(copied.entries().len(), 1);
        assert_eq!(copied.entries()[0].path, "keep.txt");
    }

    #[test]
    fn under_matches_only_whole_components() {
        assert!(under("a/b", "a/b"));
        assert!(under("a/b/c", "a/b"));
        assert!(!under("a/bc", "a/b"));
        assert!(!under("a", "a/b"));
    }

    #[test]
    fn truncated_zip_is_an_error() {
        let zip = TempZip::new();
        write_zip(&zip.path, &[("a.txt", &vec![b'a'; 2048])]);
        let bytes = std::fs::read(&zip.path).unwrap();
        std::fs::write(&zip.path, &bytes[..bytes.len() / 2]).unwrap();
        assert!(ZipBackend::open(&zip.path).is_err());
        // Also on a genuinely empty file.
        std::fs::write(&zip.path, b"").unwrap();
        assert!(ZipBackend::open(&zip.path).is_err());
    }
}

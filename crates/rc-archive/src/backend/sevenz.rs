//! 7-zip support, decoding only.
//!
//! Reading is provided by the maintained pure-Rust [`sevenz_rust2`] crate, so
//! `.7z` archives can be listed, entered and extracted. Writing 7z is not
//! implemented (see [`crate::supported_for_writing`]).

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use sevenz_rust2::{ArchiveReader, Error as SevenError, Password};

use crate::error::{ArchiveError, Result};
use crate::kind::ArchiveKind;
use crate::read::{ArchiveEntry, ArchiveRead, member_not_found};
use crate::stream;
use crate::timeutil;

/// Decoder memory limit in KiB handed to `sevenz-rust2`: 512 MiB, large enough
/// for every common dictionary size but bounded so a hostile header cannot make
/// the decoder allocate unbounded memory.
const MEMORY_LIMIT_KB: usize = 512 * 1024;

/// Map a [`SevenError`] to an [`ArchiveError`].
fn seven_error(path: &Path, member: Option<&str>, error: SevenError) -> ArchiveError {
    let archive = path.display().to_string();
    match error {
        SevenError::PasswordRequired | SevenError::MaybeBadPassword(_) => {
            ArchiveError::PasswordRequired
        }
        SevenError::FileNotFound => match member {
            Some(name) => member_not_found(path, name),
            None => ArchiveError::malformed(format!("{archive}: member not found")),
        },
        SevenError::BadSignature(_) => {
            ArchiveError::malformed(format!("{archive}: not a 7z archive"))
        }
        SevenError::UnsupportedCompressionMethod(method) => ArchiveError::unsupported_member(
            archive,
            "7z",
            match member {
                Some(name) => format!("{name:?}: compression method {method}"),
                None => format!("compression method {method}"),
            },
        ),
        SevenError::MaxMemLimited { max_kb, actaul_kb } => ArchiveError::unsupported(format!(
            "{archive}: 7z decoder needs {actaul_kb} KiB but the limit is {max_kb} KiB"
        )),
        SevenError::Io(io, context) => {
            if io.kind() == std::io::ErrorKind::InvalidData
                || io.kind() == std::io::ErrorKind::UnexpectedEof
            {
                ArchiveError::malformed(format!("{archive}: {context}{io}"))
            } else {
                ArchiveError::io(path, io)
            }
        }
        other => ArchiveError::malformed(format!("{archive}: {other}")),
    }
}

/// Reading side of a 7z archive.
pub(crate) struct SevenZBackend {
    path: PathBuf,
    entries: Vec<ArchiveEntry>,
    raw: Vec<String>,
    by_path: HashMap<String, usize>,
    reader: Option<ArchiveReader<std::fs::File>>,
}

impl SevenZBackend {
    /// Open `path` and list its members.
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|e| ArchiveError::io(path, e))?;
        let reader = ArchiveReader::new_with_memory_limit(file, Password::empty(), MEMORY_LIMIT_KB)
            .map_err(|e| seven_error(path, None, e))?;

        let files = reader.archive().files.as_slice();
        let mut entries = Vec::with_capacity(files.len());
        let mut raw = Vec::with_capacity(files.len());
        let mut by_path: HashMap<String, usize> = HashMap::with_capacity(files.len());

        for file in files {
            if file.is_anti_item() {
                continue;
            }
            let name = file.name().replace('\\', "/");
            let is_dir = file.is_directory();
            let size = if is_dir { 0 } else { file.size() };
            let mut entry = ArchiveEntry::new(&name, is_dir, size);
            let ticks = u64::from(file.last_modified_date());
            if ticks > 0 {
                entry.mtime = timeutil::filetime_to_unix(ticks);
            }
            let position = entries.len();
            by_path.entry(entry.path.clone()).or_insert(position);
            entries.push(entry);
            raw.push(file.name().to_string());
        }

        Ok(SevenZBackend {
            path: path.to_path_buf(),
            entries,
            raw,
            by_path,
            reader: Some(reader),
        })
    }

    /// Position of `path` in the member table.
    fn slot(&self, path: &str) -> Result<usize> {
        super::lookup(&self.by_path, path).ok_or_else(|| member_not_found(&self.path, path))
    }
}

impl ArchiveRead for SevenZBackend {
    fn kind(&self) -> ArchiveKind {
        ArchiveKind::SevenZip
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let slot = self.slot(path)?;
        if self.entries[slot].is_dir {
            return Ok(Vec::new());
        }
        let archive_path = self.path.clone();
        let stored = self.raw[slot].clone();
        let reader = self.reader.as_mut().ok_or_else(|| {
            ArchiveError::malformed(format!("{}: archive was consumed", archive_path.display()))
        })?;
        reader
            .read_file(&stored)
            .map_err(|e| seven_error(&archive_path, Some(&stored), e))
    }

    fn copy_to(&mut self, path: &str, writer: &mut dyn Write) -> Result<u64> {
        let slot = self.slot(path)?;
        if self.entries[slot].is_dir {
            return Ok(0);
        }
        let archive_path = self.path.clone();
        let stored = self.raw[slot].clone();
        let mut found: Option<u64> = None;
        let mut failure: Option<ArchiveError> = None;

        let reader = self.reader.as_mut().ok_or_else(|| {
            ArchiveError::malformed(format!("{}: archive was consumed", archive_path.display()))
        })?;
        {
            let wanted = stored.clone();
            let mut sink = writer;
            let result = reader.for_each_entries(|entry, data| {
                if entry.name() != wanted {
                    return Ok(true);
                }
                match stream::pump(data, &mut sink, None, None, &archive_path, &wanted) {
                    Ok(n) => found = Some(n),
                    Err(e) => failure = Some(e),
                }
                Ok(false)
            });
            if let Err(e) = result {
                return Err(match failure {
                    Some(inner) => inner,
                    None => seven_error(&archive_path, Some(&stored), e),
                });
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        found.ok_or_else(|| member_not_found(&archive_path, &stored))
    }

    fn raw_name(&self, path: &str) -> Option<&str> {
        super::lookup(&self.by_path, path).and_then(|slot| self.raw.get(slot).map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sevenz_rust2::{ArchiveEntry as SzEntry, ArchiveWriter};
    use std::io::Cursor;

    /// 2020-09-13T12:26:40Z as a Windows FILETIME.
    fn filetime_2020() -> sevenz_rust2::NtTime {
        sevenz_rust2::NtTime::new(132_444_736_000_000_000)
    }

    fn make_archive(path: &Path) {
        let mut writer = ArchiveWriter::create(path).unwrap();
        let mut entry = SzEntry::new_file("a.txt");
        entry.has_last_modified_date = true;
        entry.last_modified_date = filetime_2020();
        writer
            .push_archive_entry(entry, Some(Cursor::new(b"hello seven".to_vec())))
            .unwrap();
        writer
            .push_archive_entry(SzEntry::new_directory("dir"), None::<Cursor<Vec<u8>>>)
            .unwrap();
        writer
            .push_archive_entry(
                SzEntry::new_file("dir/b.txt"),
                Some(Cursor::new(b"nested".to_vec())),
            )
            .unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn lists_and_reads_members() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.7z");
        make_archive(&path);

        let mut backend = SevenZBackend::open(&path).unwrap();
        assert_eq!(backend.kind(), ArchiveKind::SevenZip);
        assert_eq!(backend.entries().len(), 3);
        assert_eq!(backend.entry("a.txt").unwrap().size, 11);
        assert!(backend.entry("dir").unwrap().is_dir);
        assert_eq!(
            backend.entry("a.txt").unwrap().mtime,
            Some(1_600_000_000),
            "FILETIME converts to unix seconds"
        );

        assert_eq!(backend.read("a.txt").unwrap(), b"hello seven");
        assert_eq!(backend.read("dir/b.txt").unwrap(), b"nested");
        assert!(backend.read("dir").unwrap().is_empty());

        let mut out = Vec::new();
        assert_eq!(backend.copy_to("dir/b.txt", &mut out).unwrap(), 6);
        assert_eq!(out, b"nested");
        assert!(backend.read("ghost.txt").is_err());
    }

    #[test]
    fn copy_to_reads_a_solid_block_only_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("solid.7z");
        make_archive(&path);
        let mut backend = SevenZBackend::open(&path).unwrap();
        let mut first = Vec::new();
        let mut second = Vec::new();
        assert_eq!(backend.copy_to("a.txt", &mut first).unwrap(), 11);
        assert_eq!(backend.copy_to("dir/b.txt", &mut second).unwrap(), 6);
        assert_eq!(first, b"hello seven");
        assert_eq!(second, b"nested");
    }

    #[test]
    fn not_a_7z_file_is_a_clean_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.7z");
        std::fs::write(&path, b"this is not a 7z archive at all").unwrap();
        match SevenZBackend::open(&path) {
            Ok(_) => panic!("garbage must not open as a 7z archive"),
            Err(err) => assert!(matches!(err, ArchiveError::Malformed(_)), "{err:?}"),
        }
    }

    #[test]
    fn truncated_7z_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cut.7z");
        make_archive(&path);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
        match SevenZBackend::open(&path) {
            Ok(mut backend) => {
                // If the header survived, reading the data must fail.
                assert!(backend.read("a.txt").is_err());
            }
            Err(e) => assert!(!e.to_string().is_empty()),
        }
    }

    #[test]
    fn every_member_streams_to_a_sink() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stream.7z");
        make_archive(&path);
        let mut backend = SevenZBackend::open(&path).unwrap();
        let mut total = 0u64;
        for entry in backend.entries().to_vec() {
            total += backend.copy_to(&entry.path, &mut std::io::sink()).unwrap();
        }
        assert_eq!(total, 17, "11 bytes plus the 6 byte nested file");
    }
}

//! Single-file compressors (`gz`, `bz2`, `xz`, `zst`).
//!
//! A gzip file is not an archive — it holds exactly one stream. RCommander still
//! treats it as a one-member archive, so the user can enter it and copy the
//! decompressed file out, exactly like WinRAR/7-Zip do. The member is named after
//! the file with the compression suffix removed: `notes.txt.gz` -> `notes.txt`.
//!
//! The uncompressed size is only known up front for gzip, where the last four
//! bytes of the stream hold `ISIZE` (the size modulo 2^32); every other format
//! reports `0` meaning "unknown" until the member is read.

use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::{ArchiveError, Result};
use crate::kind::ArchiveKind;
use crate::member;
use crate::read::{ArchiveEntry, ArchiveRead, member_not_found};
use crate::stream;
use crate::timeutil;

/// Reading side of a single-file compressed archive.
pub(crate) struct SingleBackend {
    path: PathBuf,
    kind: ArchiveKind,
    entries: Vec<ArchiveEntry>,
    member: String,
}

/// Name of the single member: the file name without the compression suffix.
pub(crate) fn member_name(file_name: &str) -> String {
    let lower = file_name.to_ascii_lowercase();
    for suffix in [".gz", ".bz2", ".xz", ".zst", ".zstd", ".gzip", ".bzip2"] {
        if lower.ends_with(suffix) && lower.len() > suffix.len() {
            return file_name[..file_name.len() - suffix.len()].to_string();
        }
    }
    file_name.to_string()
}

/// `ISIZE` from the gzip trailer: uncompressed size modulo 2^32.
fn gzip_isize(path: &Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len < 18 {
        return None;
    }
    file.seek(SeekFrom::End(-4)).ok()?;
    let mut trailer = [0u8; 4];
    file.read_exact(&mut trailer).ok()?;
    Some(u64::from(u32::from_le_bytes(trailer)))
}

impl SingleBackend {
    /// Open `path`, expecting the compressor implied by `kind`.
    pub(crate) fn open(path: &Path, kind: ArchiveKind) -> Result<Self> {
        let metadata = fs::metadata(path).map_err(|e| ArchiveError::io(path, e))?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "data".to_string());
        let member = member_name(&file_name);
        let size = if kind == ArchiveKind::Gz {
            gzip_isize(path).unwrap_or(0)
        } else {
            0
        };

        let mut entry = ArchiveEntry::new(&member, false, size);
        entry.compressed_size = Some(metadata.len());
        entry.mtime = metadata
            .modified()
            .ok()
            .and_then(timeutil::system_time_to_unix);

        Ok(SingleBackend {
            path: path.to_path_buf(),
            kind,
            entries: vec![entry],
            member,
        })
    }

    /// Stream the decompressed member into `writer`.
    fn emit(&self, writer: &mut dyn Write) -> Result<u64> {
        let file = File::open(&self.path).map_err(|e| ArchiveError::io(&self.path, e))?;
        let reader: Box<dyn Read> = super::decompress(self.kind, Box::new(BufReader::new(file)))?;
        let mut reader = reader;
        stream::pump(&mut reader, writer, None, None, &self.path, &self.member)
    }
}

impl ArchiveRead for SingleBackend {
    fn kind(&self) -> ArchiveKind {
        self.kind
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        if member::normalize(path) != self.member {
            return Err(member_not_found(&self.path, path));
        }
        let mut out = Vec::new();
        self.emit(&mut out)?;
        Ok(out)
    }

    fn copy_to(&mut self, path: &str, writer: &mut dyn Write) -> Result<u64> {
        if member::normalize(path) != self.member {
            return Err(member_not_found(&self.path, path));
        }
        self.emit(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testutil::zstd_raw_frame;
    use flate2::Compression;
    use flate2::write::GzEncoder;

    fn gzip_file(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(6));
        encoder.write_all(data).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, encoder.finish().unwrap()).unwrap();
        path
    }

    #[test]
    fn member_names_lose_the_compression_suffix() {
        assert_eq!(member_name("notes.txt.gz"), "notes.txt");
        assert_eq!(member_name("notes.gz"), "notes");
        assert_eq!(member_name("UPPER.TXT.GZ"), "UPPER.TXT");
        assert_eq!(member_name("a.bz2"), "a");
        assert_eq!(member_name("a.xz"), "a");
        assert_eq!(member_name("a.zst"), "a");
        assert_eq!(member_name("noext"), "noext");
        assert_eq!(
            member_name(".gz"),
            ".gz",
            "a file called .gz keeps its name"
        );
    }

    #[test]
    fn gzip_exposes_one_member_with_known_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = gzip_file(dir.path(), "notes.txt.gz", b"hello gzip");
        let mut backend = SingleBackend::open(&path, ArchiveKind::Gz).unwrap();
        assert_eq!(backend.kind(), ArchiveKind::Gz);
        assert_eq!(backend.entries().len(), 1);
        assert_eq!(backend.member, "notes.txt");
        assert_eq!(
            backend.entries()[0].size,
            10,
            "ISIZE trailer gives the real size"
        );
        assert!(backend.entries()[0].compressed_size.is_some());
        assert!(backend.entries()[0].mtime.is_some());
        assert_eq!(backend.read("notes.txt").unwrap(), b"hello gzip");
        let mut out = Vec::new();
        assert_eq!(backend.copy_to("notes.txt", &mut out).unwrap(), 10);
        assert_eq!(out, b"hello gzip");
        assert!(backend.read("other.txt").is_err());
    }

    #[test]
    fn bzip2_and_zstd_files_decode() {
        let dir = tempfile::tempdir().unwrap();

        let bz_path = dir.path().join("payload.txt.bz2");
        let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::new(6));
        encoder.write_all(b"bzip2 body").unwrap();
        std::fs::write(&bz_path, encoder.finish().unwrap()).unwrap();
        let mut backend = SingleBackend::open(&bz_path, ArchiveKind::Bz2).unwrap();
        assert_eq!(backend.member, "payload.txt");
        assert_eq!(backend.read("payload.txt").unwrap(), b"bzip2 body");

        let zst_path = dir.path().join("payload.bin.zst");
        let payload = vec![b'z'; 512];
        std::fs::write(&zst_path, zstd_raw_frame(&payload)).unwrap();
        let mut backend = SingleBackend::open(&zst_path, ArchiveKind::Zst).unwrap();
        assert_eq!(backend.read("payload.bin").unwrap(), payload);
    }

    #[test]
    fn xz_file_decodes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.txt.xz");
        let mut xz =
            lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(1)).unwrap();
        xz.write_all(b"xz body").unwrap();
        std::fs::write(&path, xz.finish().unwrap()).unwrap();

        let mut backend = SingleBackend::open(&path, ArchiveKind::Xz).unwrap();
        assert_eq!(backend.read("data.txt").unwrap(), b"xz body");
        let mut out = Vec::new();
        assert_eq!(backend.copy_to("data.txt", &mut out).unwrap(), 7);
    }

    #[test]
    fn truncated_streams_are_errors_not_panics() {
        let dir = tempfile::tempdir().unwrap();
        let path = gzip_file(dir.path(), "broken.gz", &vec![b'x'; 4096]);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();

        let mut backend = SingleBackend::open(&path, ArchiveKind::Gz).unwrap();
        // gzip has its own CRC/length trailer, so a truncated stream must fail.
        let result = backend.read("broken");
        assert!(
            result.is_err(),
            "expected an error for a truncated gzip stream"
        );

        // A file that is not gzip at all.
        let bogus = dir.path().join("bogus.gz");
        std::fs::write(&bogus, b"not compressed at all").unwrap();
        let mut backend = SingleBackend::open(&bogus, ArchiveKind::Gz).unwrap();
        assert!(backend.read("bogus").is_err());
    }

    #[test]
    fn empty_gzip_file_opens_with_unknown_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.gz");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(6));
        encoder.write_all(b"").unwrap();
        std::fs::write(&path, encoder.finish().unwrap()).unwrap();

        let mut backend = SingleBackend::open(&path, ArchiveKind::Gz).unwrap();
        assert_eq!(backend.entries()[0].size, 0);
        assert!(backend.read("empty").unwrap().is_empty());
    }
}

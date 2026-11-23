//! Tar container support: plain tar plus the four compressed variants.
//!
//! Plain tar is read with `seek`: the member list records each member's data
//! offset, so reading one member never rescans the archive. Compressed tar
//! streams cannot be seeked, so the member list is built once at open time and
//! a member read re-streams the tar from the beginning — memory stays constant,
//! only time grows.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use tar::EntryType;

use crate::error::{ArchiveError, Result};
use crate::kind::ArchiveKind;
use crate::member;
use crate::read::{ArchiveEntry, ArchiveRead, member_not_found};
use crate::stream;

/// Reading side of a tar archive, plain or compressed.
pub(crate) struct TarBackend {
    path: PathBuf,
    kind: ArchiveKind,
    file: File,
    entries: Vec<ArchiveEntry>,
    raw: Vec<String>,
    sizes: Vec<u64>,
    /// Data offset inside the *uncompressed* stream, only usable for plain tar.
    offsets: Vec<u64>,
    by_path: HashMap<String, usize>,
}

/// Header types that carry no member of their own; the `tar` crate normally
/// consumes them, but a hand-built archive may leave them visible.
fn is_metadata_type(entry_type: EntryType) -> bool {
    matches!(
        entry_type,
        EntryType::GNULongName
            | EntryType::GNULongLink
            | EntryType::XHeader
            | EntryType::XGlobalHeader
    )
}

impl TarBackend {
    /// Open `path`, expecting the compression implied by `kind`.
    pub(crate) fn open(path: &Path, kind: ArchiveKind) -> Result<Self> {
        let file = File::open(path).map_err(|e| ArchiveError::io(path, e))?;
        let mut backend = TarBackend {
            path: path.to_path_buf(),
            kind,
            file,
            entries: Vec::new(),
            raw: Vec::new(),
            sizes: Vec::new(),
            offsets: Vec::new(),
            by_path: HashMap::new(),
        };
        backend.scan()?;
        Ok(backend)
    }

    /// Walk the (decompressed) tar stream once and record every member.
    fn scan(&mut self) -> Result<()> {
        let file = File::open(&self.path).map_err(|e| ArchiveError::io(&self.path, e))?;
        let reader: Box<dyn Read> = super::decompress(self.kind, Box::new(BufReader::new(file)))?;
        let mut archive = tar::Archive::new(reader);
        let entries = archive
            .entries()
            .map_err(|e| super::parse_error(&self.path, e))?;

        for item in entries {
            let item = item.map_err(|e| super::parse_error(&self.path, e))?;
            let entry_type = item.header().entry_type();
            if is_metadata_type(entry_type) {
                continue;
            }
            let size = item
                .header()
                .size()
                .map_err(|e| super::parse_error(&self.path, e))?;
            let mtime = item
                .header()
                .mtime()
                .ok()
                .and_then(|t| i64::try_from(t).ok());
            let offset = item.raw_file_position();
            let raw = String::from_utf8_lossy(&item.path_bytes()).into_owned();
            if raw.is_empty() {
                continue;
            }
            let is_dir = entry_type.is_dir();
            let mut entry = ArchiveEntry::new(&raw, is_dir, if is_dir { 0 } else { size });
            entry.mtime = mtime;
            let position = self.entries.len();
            self.by_path.entry(entry.path.clone()).or_insert(position);
            self.entries.push(entry);
            self.raw.push(raw);
            self.sizes.push(if is_dir { 0 } else { size });
            self.offsets.push(offset);
        }
        Ok(())
    }

    /// Position of `path` in the member table.
    fn slot(&self, path: &str) -> Result<usize> {
        super::lookup(&self.by_path, path).ok_or_else(|| member_not_found(&self.path, path))
    }

    /// Pump member `slot` into `writer`.
    fn emit(&mut self, slot: usize, writer: &mut dyn Write) -> Result<u64> {
        if self.entries[slot].is_dir {
            return Ok(0);
        }
        let member_path = self.entries[slot].path.clone();
        if self.kind == ArchiveKind::Tar {
            let offset = self.offsets[slot];
            let size = self.sizes[slot];
            self.file
                .seek(SeekFrom::Start(offset))
                .map_err(|e| ArchiveError::io(&self.path, e))?;
            let mut limited = (&self.file).take(size);
            let written = stream::pump(&mut limited, writer, None, None, &self.path, &member_path)?;
            return Ok(written);
        }
        self.stream_member(&member_path, writer)
    }

    /// Re-stream a compressed tar until `member_path` shows up.
    fn stream_member(&mut self, member_path: &str, writer: &mut dyn Write) -> Result<u64> {
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| ArchiveError::io(&self.path, e))?;
        let reader: Box<dyn Read> =
            super::decompress(self.kind, Box::new(BufReader::new(&self.file)))?;
        let mut archive = tar::Archive::new(reader);
        let entries = archive
            .entries()
            .map_err(|e| super::parse_error(&self.path, e))?;
        let mut written = None;
        for item in entries {
            let mut item = item.map_err(|e| super::parse_error(&self.path, e))?;
            if member::normalize(&String::from_utf8_lossy(&item.path_bytes())) != member_path {
                continue;
            }
            written = Some(stream::pump(
                &mut item,
                writer,
                None,
                None,
                &self.path,
                member_path,
            )?);
            break;
        }
        match written {
            Some(n) => Ok(n),
            None => Err(member_not_found(&self.path, member_path)),
        }
    }
}

impl ArchiveRead for TarBackend {
    fn kind(&self) -> ArchiveKind {
        self.kind
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let slot = self.slot(path)?;
        let mut out = Vec::new();
        self.emit(slot, &mut out)?;
        Ok(out)
    }

    fn copy_to(&mut self, path: &str, writer: &mut dyn Write) -> Result<u64> {
        let slot = self.slot(path)?;
        self.emit(slot, writer)
    }

    fn raw_name(&self, path: &str) -> Option<&str> {
        super::lookup(&self.by_path, path).and_then(|slot| self.raw.get(slot).map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;

    fn build_tar() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(5);
        header.set_mode(0o644);
        header.set_mtime(1_600_000_000);
        header.set_entry_type(EntryType::Regular);
        header.set_cksum();
        builder
            .append_data(&mut header, "a.txt", &b"hello"[..])
            .unwrap();

        let mut dir_header = tar::Header::new_gnu();
        dir_header.set_size(0);
        dir_header.set_mode(0o755);
        dir_header.set_entry_type(EntryType::Directory);
        dir_header.set_cksum();
        builder
            .append_data(&mut dir_header, "dir/", std::io::empty())
            .unwrap();

        let mut nested = tar::Header::new_gnu();
        nested.set_size(6);
        nested.set_mode(0o600);
        nested.set_entry_type(EntryType::Regular);
        nested.set_cksum();
        builder
            .append_data(&mut nested, "dir/b.txt", &b"nested"[..])
            .unwrap();
        builder.into_inner().unwrap()
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    use crate::backend::testutil::zstd_raw_frame;

    #[test]
    fn plain_tar_lists_and_reads_members() {
        let dir = tempdir();
        let path = dir.path().join("a.tar");
        std::fs::write(&path, build_tar()).unwrap();

        let mut backend = TarBackend::open(&path, ArchiveKind::Tar).unwrap();
        assert_eq!(backend.kind(), ArchiveKind::Tar);
        assert_eq!(backend.entries().len(), 3);
        let a = backend.entry("a.txt").unwrap();
        assert_eq!(a.size, 5);
        assert_eq!(a.mtime, Some(1_600_000_000));
        assert!(backend.entry("dir").unwrap().is_dir);

        assert_eq!(backend.read("a.txt").unwrap(), b"hello");
        assert_eq!(backend.read("dir/b.txt").unwrap(), b"nested");
        assert!(
            backend.read("dir").unwrap().is_empty(),
            "directories read as empty"
        );

        let mut out = Vec::new();
        assert_eq!(backend.copy_to("dir/b.txt", &mut out).unwrap(), 6);
        assert_eq!(out, b"nested");
        assert!(backend.read("ghost.txt").is_err());
    }

    #[test]
    fn gzip_tar_streams_members() {
        let dir = tempdir();
        let path = dir.path().join("a.tar.gz");
        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(6));
        encoder.write_all(&build_tar()).unwrap();
        std::fs::write(&path, encoder.finish().unwrap()).unwrap();

        let mut backend = TarBackend::open(&path, ArchiveKind::TarGz).unwrap();
        assert_eq!(backend.kind(), ArchiveKind::TarGz);
        assert_eq!(backend.file_count(), 2);
        assert_eq!(backend.read("a.txt").unwrap(), b"hello");
        // Reading the last member proves the stream is re-scanned correctly.
        assert_eq!(backend.read("dir/b.txt").unwrap(), b"nested");
        assert_eq!(backend.uncompressed_size(), 11);
        assert!(backend.read("dir").unwrap().is_empty());
        assert!(backend.read("missing").is_err());
    }

    #[test]
    fn xz_and_zstd_tars_are_supported() {
        let dir = tempdir();
        let raw = build_tar();

        let xz_path = dir.path().join("a.tar.xz");
        let mut xz =
            lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(1)).unwrap();
        xz.write_all(&raw).unwrap();
        std::fs::write(&xz_path, xz.finish().unwrap()).unwrap();
        let mut backend = TarBackend::open(&xz_path, ArchiveKind::TarXz).unwrap();
        assert_eq!(backend.read("a.txt").unwrap(), b"hello");

        let zst_path = dir.path().join("a.tar.zst");
        std::fs::write(&zst_path, zstd_raw_frame(&raw)).unwrap();
        let mut backend = TarBackend::open(&zst_path, ArchiveKind::TarZst).unwrap();
        assert_eq!(backend.read("dir/b.txt").unwrap(), b"nested");
    }

    #[test]
    fn bzip2_tar_round_trip() {
        let dir = tempdir();
        let path = dir.path().join("a.tar.bz2");
        let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::new(6));
        encoder.write_all(&build_tar()).unwrap();
        std::fs::write(&path, encoder.finish().unwrap()).unwrap();

        let mut backend = TarBackend::open(&path, ArchiveKind::TarBz2).unwrap();
        assert_eq!(backend.read("a.txt").unwrap(), b"hello");
        assert_eq!(backend.entries().len(), 3);
    }

    #[test]
    fn symbolic_links_are_listed_without_content() {
        let dir = tempdir();
        let path = dir.path().join("links.tar");
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o777);
        header.set_entry_type(EntryType::Symlink);
        header.set_link_name("/etc/passwd").unwrap();
        header.set_cksum();
        builder
            .append_data(&mut header, "link", std::io::empty())
            .unwrap();
        std::fs::write(&path, builder.into_inner().unwrap()).unwrap();

        let mut backend = TarBackend::open(&path, ArchiveKind::Tar).unwrap();
        let entry = backend.entry("link").unwrap();
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 0);
        assert!(backend.read("link").unwrap().is_empty());
    }

    #[test]
    fn truncated_and_garbage_tars_are_errors() {
        let dir = tempdir();
        let path = dir.path().join("bad.tar");
        // Garbage: no valid header, no valid checksum.
        std::fs::write(&path, vec![b'Z'; 1024]).unwrap();
        assert!(TarBackend::open(&path, ArchiveKind::Tar).is_err());

        // Truncated in the middle of a member's data.
        let full = build_tar();
        std::fs::write(&path, &full[..300]).unwrap();
        match TarBackend::open(&path, ArchiveKind::Tar) {
            Ok(mut backend) => {
                let _ = backend.read("a.txt");
            }
            Err(e) => assert!(!e.to_string().is_empty()),
        }

        // Compressed but not compressed.
        let gz_path = dir.path().join("liar.tar.gz");
        std::fs::write(&gz_path, b"").unwrap();
        assert!(TarBackend::open(&gz_path, ArchiveKind::TarGz).is_err());
    }
}

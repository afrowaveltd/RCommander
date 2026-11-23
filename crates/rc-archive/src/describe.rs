//! One-line human readable summaries for the status bar.

use std::fs;
use std::path::Path;

use rc_core::format::format_size_unit;

use crate::kind::{ArchiveKind, detect};
use crate::open;

/// Human readable summary, e.g. `ZIP archive, 42 files, 1.2 MiB`.
///
/// Never fails: a damaged or unreadable archive still yields a description
/// (`"ZIP archive, unreadable"`), and a file that is not an archive at all is
/// described by its size (`"unknown file, 12 KiB"`). Directories are only
/// mentioned when the archive actually stores them.
pub fn describe(path: &Path) -> String {
    let kind = detect(path);
    if kind == ArchiveKind::Unknown {
        return match fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => "directory".to_string(),
            Ok(metadata) => format!("unknown file, {}", format_size_unit(metadata.len())),
            Err(_) => "unknown file".to_string(),
        };
    }

    let label = kind.name();
    let archive = match open(path) {
        Ok(archive) => archive,
        Err(_) => return format!("{label} archive, unreadable"),
    };
    let files = archive.file_count() as u64;
    let dirs = archive.entries().iter().filter(|e| e.is_dir).count() as u64;
    let bytes = archive.uncompressed_size();

    let mut out = format!("{label} archive, {}", count(files, "file", "files"));
    if dirs > 0 {
        out.push_str(", ");
        out.push_str(&count(dirs, "directory", "directories"));
    }
    if kind.is_single_file_compressed() && bytes == 0 {
        // gzip reports its ISIZE but bz2/xz/zst cannot tell the size before
        // decoding; report the container size instead of a misleading `0 B`.
        if let Ok(metadata) = fs::metadata(path) {
            out.push_str(", ");
            out.push_str(&format_size_unit(metadata.len()));
            return out;
        }
    }
    out.push_str(", ");
    out.push_str(&format_size_unit(bytes));
    out
}

/// `1 file` / `2 files`.
fn count(value: u64, singular: &str, plural: &str) -> String {
    if value == 1 {
        format!("1 {singular}")
    } else {
        format!("{value} {plural}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::{PackOptions, create};
    use std::io::Write;
    use std::path::PathBuf;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("payload.txt");
        let mut file = fs::File::create(&src).unwrap();
        file.write_all(&vec![b'a'; 2048]).unwrap();
        (dir, src)
    }

    #[test]
    fn zip_summary_is_informative() {
        let (dir, src) = fixture();
        let zip = dir.path().join("a.zip");
        create(&zip, &[src], &PackOptions::default()).unwrap();

        let text = describe(&zip);
        assert!(text.starts_with("ZIP archive, 1 file"), "{text}");
        assert!(text.contains("KiB"), "{text}");
    }

    #[test]
    fn tar_gz_summary_is_informative() {
        let (dir, src) = fixture();
        let tgz = dir.path().join("a.tar.gz");
        create(&tgz, &[src], &PackOptions::default()).unwrap();
        let text = describe(&tgz);
        assert!(text.starts_with("TAR.GZ archive, 1 file"), "{text}");
        assert!(text.contains("2.00 KiB") || text.contains("KiB"), "{text}");
    }

    #[test]
    fn directories_are_mentioned_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        fs::create_dir_all(tree.join("sub")).unwrap();
        fs::write(tree.join("a.txt"), b"a").unwrap();
        fs::write(tree.join("sub").join("b.txt"), b"bb").unwrap();
        let zip = dir.path().join("tree.zip");
        create(&zip, &[tree], &PackOptions::default()).unwrap();

        let text = describe(&zip);
        assert!(text.contains("2 files"), "{text}");
        assert!(text.contains("2 directories"), "{text}");
    }

    #[test]
    fn empty_archives_still_describe_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("empty.zip");
        create(&zip, &[], &PackOptions::default()).unwrap();
        assert_eq!(describe(&zip), "ZIP archive, 0 files, 0 B");
    }

    #[test]
    fn unreadable_archives_do_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("bogus.zip");
        fs::write(&bogus, b"not a zip at all").unwrap();
        let text = describe(&bogus);
        assert!(text.contains("ZIP archive"), "{text}");
        assert!(!text.is_empty());
    }

    #[test]
    fn non_archives_are_described_by_size() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("notes.txt");
        fs::write(&plain, vec![b'n'; 40]).unwrap();
        assert_eq!(describe(&plain), "unknown file, 40 B");

        assert_eq!(describe(dir.path()), "directory");
        assert_eq!(
            describe(&dir.path().join("missing.zip")),
            "ZIP archive, unreadable"
        );
        assert_eq!(describe(&dir.path().join("missing.txt")), "unknown file");
    }

    #[test]
    fn single_file_compressed_summary_mentions_the_container_size() {
        let dir = tempfile::tempdir().unwrap();
        let bz = dir.path().join("notes.txt.bz2");
        let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::new(6));
        encoder.write_all(b"hello bzip2").unwrap();
        fs::write(&bz, encoder.finish().unwrap()).unwrap();
        let text = describe(&bz);
        assert!(text.starts_with("BZIP2 archive, 1 file"), "{text}");
        assert!(text.contains(" B"), "{text}");
    }

    #[test]
    fn count_pluralises() {
        assert_eq!(count(1, "file", "files"), "1 file");
        assert_eq!(count(0, "file", "files"), "0 files");
        assert_eq!(count(3, "directory", "directories"), "3 directories");
    }
}

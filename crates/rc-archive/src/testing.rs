//! The "Test archive" command.

use std::io;
use std::path::Path;

use rc_core::ops::CancelToken;

use crate::error::{ArchiveError, Result};
use crate::open;

/// Result of [`test()`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestReport {
    /// Number of members the archive claims to have (files plus directories).
    pub entries: u64,
    /// True when every member could be decoded.
    pub ok: bool,
    /// One message per damaged member; empty when [`TestReport::ok`].
    pub errors: Vec<String>,
    /// Number of uncompressed bytes actually decoded.
    pub bytes_read: u64,
}

/// Decode every member and report whether the archive is intact.
///
/// Members are streamed into [`io::sink`], so testing a 10 GiB archive needs a
/// 64 KiB buffer rather than 10 GiB of memory. Zip members are additionally
/// checked against their stored CRC-32, and decompressors verify their own
/// checksums (gzip/xz/zstd/bzip2 trailers, 7z block CRCs).
///
/// An archive that cannot even be opened — truncated, wrong format, encrypted
/// headers — is **not** an `Err`: the report comes back with `ok == false` and
/// the reason in `errors`, which is what the progress dialog shows. `Err` is
/// reserved for cancellation and for I/O problems that are not about the
/// archive's integrity.
pub fn test(path: &Path, cancel: &CancelToken) -> Result<TestReport> {
    let mut archive = match open(path) {
        Ok(archive) => archive,
        Err(error) => {
            return Ok(TestReport {
                entries: 0,
                ok: false,
                errors: vec![error.to_string()],
                bytes_read: 0,
            });
        }
    };
    let mut report = TestReport {
        entries: archive.entries().len() as u64,
        ok: true,
        errors: Vec::new(),
        bytes_read: 0,
    };
    let members: Vec<String> = archive
        .entries()
        .iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.path.clone())
        .collect();

    let mut sink = io::sink();
    for member in members {
        if cancel.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        match archive.copy_to(&member, &mut sink) {
            Ok(bytes) => report.bytes_read += bytes,
            Err(ArchiveError::Cancelled) => return Err(ArchiveError::Cancelled),
            Err(error) => report.errors.push(format!("{member}: {error}")),
        }
    }
    report.ok = report.errors.is_empty();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::{PackOptions, create};
    use std::path::PathBuf;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            Fixture { _dir: dir, root }
        }

        fn file(&self, name: &str, data: &[u8]) -> PathBuf {
            let path = self.root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, data).unwrap();
            path
        }
    }

    fn token() -> CancelToken {
        CancelToken::new()
    }

    #[test]
    fn a_valid_zip_is_ok() {
        let fixture = Fixture::new();
        let a = fixture.file("a.txt", b"hello");
        let b = fixture.file("dir/b.txt", b"nested content");
        let zip = fixture.root.join("good.zip");
        create(&zip, &[a, b], &PackOptions::default()).unwrap();

        let report = test(&zip, &token()).unwrap();
        assert!(report.ok, "{:?}", report.errors);
        assert!(report.errors.is_empty());
        assert_eq!(
            report.entries, 2,
            "two members, no explicit directory entry"
        );
        assert_eq!(report.bytes_read, 5 + 14);
    }

    #[test]
    fn a_valid_tar_gz_is_ok() {
        let fixture = Fixture::new();
        let a = fixture.file("a.txt", b"hello");
        let tgz = fixture.root.join("good.tar.gz");
        create(&tgz, &[a], &PackOptions::default()).unwrap();
        let report = test(&tgz, &token()).unwrap();
        assert!(report.ok, "{:?}", report.errors);
        assert_eq!(report.entries, 1);
        assert_eq!(report.bytes_read, 5);
    }

    #[test]
    fn single_file_compressed_archives_can_be_tested() {
        let fixture = Fixture::new();
        let gz = fixture.root.join("notes.txt.gz");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
        use std::io::Write;
        encoder.write_all(b"compressed body").unwrap();
        std::fs::write(&gz, encoder.finish().unwrap()).unwrap();

        let report = test(&gz, &token()).unwrap();
        assert!(report.ok, "{:?}", report.errors);
        assert_eq!(report.entries, 1);
        assert_eq!(report.bytes_read, 15);
    }

    #[test]
    fn a_truncated_zip_reports_a_failure() {
        let fixture = Fixture::new();
        let payload = vec![b'x'; 8192];
        let a = fixture.file("a.bin", &payload);
        let zip = fixture.root.join("cut.zip");
        create(&zip, &[a], &PackOptions::default()).unwrap();
        let bytes = std::fs::read(&zip).unwrap();
        std::fs::write(&zip, &bytes[..bytes.len() / 2]).unwrap();

        let report = test(&zip, &token()).unwrap();
        assert!(!report.ok);
        assert!(!report.errors.is_empty());
        assert_eq!(
            report.entries, 0,
            "a zip without a central directory has no members"
        );
        assert_eq!(report.bytes_read, 0);
    }

    #[test]
    fn a_damaged_member_is_reported() {
        let fixture = Fixture::new();
        // Large enough that corrupting the middle hits member data.
        let a = fixture.file("a.bin", &vec![b'q'; 16 * 1024]);
        let zip = fixture.root.join("damaged.zip");
        create(
            &zip,
            &[a],
            &PackOptions {
                level: 0,
                ..PackOptions::default()
            },
        )
        .unwrap();

        let mut bytes = std::fs::read(&zip).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xFF;
        std::fs::write(&zip, &bytes).unwrap();

        let report = test(&zip, &token()).unwrap();
        assert!(!report.ok, "a corrupted member must fail the test");
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn a_non_archive_is_reported_not_raised() {
        let fixture = Fixture::new();
        let bogus = fixture.file("bogus.zip", b"this is not a zip");
        let report = test(&bogus, &token()).unwrap();
        assert!(!report.ok);
        assert_eq!(report.entries, 0);
        assert!(report.errors[0].to_lowercase().contains("zip") || !report.errors[0].is_empty());

        let missing = fixture.root.join("missing.tar.gz");
        let report = test(&missing, &token()).unwrap();
        assert!(!report.ok);
    }

    #[test]
    fn an_empty_archive_tests_ok() {
        let fixture = Fixture::new();
        let zip = fixture.root.join("empty.zip");
        create(&zip, &[], &PackOptions::default()).unwrap();
        let report = test(&zip, &token()).unwrap();
        assert!(report.ok);
        assert_eq!(report.entries, 0);
        assert_eq!(report.bytes_read, 0);
    }

    #[test]
    fn cancellation_is_an_error() {
        let fixture = Fixture::new();
        let a = fixture.file("a.txt", b"hello");
        let zip = fixture.root.join("cancel.zip");
        create(&zip, &[a], &PackOptions::default()).unwrap();
        let cancel = CancelToken::new();
        cancel.cancel();
        let err = test(&zip, &cancel).unwrap_err();
        assert!(err.is_cancelled());
    }
}

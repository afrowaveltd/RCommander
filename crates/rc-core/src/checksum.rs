//! Checksums: hashing files, writing checksum lists and verifying them.
//!
//! Two file formats are supported, because both are common in the wild:
//!
//! * `<hash> *<name>` / `<hash>  <name>` — the `md5sum`/`sha256sum` convention,
//! * `<name> <hash>` — the classic `SFV` convention (CRC32).
//!
//! The reader accepts either order, so a file produced by another tool verifies
//! without conversion.

use crate::error::{CoreError, CoreResult};
use crate::ops::CancelToken;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// Hash algorithm used for a checksum file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Crc32,
    Md5,
    Sha1,
    Sha256,
}

impl Algorithm {
    /// Lower-case identifier (`crc32`, `md5`, `sha1`, `sha256`).
    pub fn id(self) -> &'static str {
        match self {
            Algorithm::Crc32 => "crc32",
            Algorithm::Md5 => "md5",
            Algorithm::Sha1 => "sha1",
            Algorithm::Sha256 => "sha256",
        }
    }

    /// Parse an identifier, case-insensitively.
    pub fn parse(text: &str) -> Option<Algorithm> {
        match text.trim().to_ascii_lowercase().as_str() {
            "crc32" | "crc" | "sfv" => Some(Algorithm::Crc32),
            "md5" => Some(Algorithm::Md5),
            "sha1" => Some(Algorithm::Sha1),
            "sha256" | "sha-256" => Some(Algorithm::Sha256),
            _ => None,
        }
    }

    /// Algorithm implied by a checksum file name (`.sfv`, `.md5`, `.sha1`, `.sha256`).
    pub fn from_file_name(name: &str) -> Option<Algorithm> {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".sfv") {
            Some(Algorithm::Crc32)
        } else if lower.ends_with(".md5") {
            Some(Algorithm::Md5)
        } else if lower.ends_with(".sha1") {
            Some(Algorithm::Sha1)
        } else if lower.ends_with(".sha256") {
            Some(Algorithm::Sha256)
        } else {
            None
        }
    }

    /// Default file name extension for this algorithm.
    pub fn extension(self) -> &'static str {
        match self {
            Algorithm::Crc32 => "sfv",
            Algorithm::Md5 => "md5",
            Algorithm::Sha1 => "sha1",
            Algorithm::Sha256 => "sha256",
        }
    }
}

/// One line of a checksum file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecksumEntry {
    /// Expected hash, lower-case hex.
    pub hash: String,
    /// File name (or relative path) the hash belongs to.
    pub name: String,
}

impl ChecksumEntry {
    pub fn new(hash: impl Into<String>, name: impl Into<String>) -> Self {
        ChecksumEntry {
            hash: hash.into().to_ascii_lowercase(),
            name: name.into(),
        }
    }
}

/// Hash a file with `algorithm`.
pub fn hash_file(path: &Path, algorithm: Algorithm, cancel: &CancelToken) -> CoreResult<String> {
    let mut file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut buffer = vec![0u8; 256 * 1024];
    match algorithm {
        Algorithm::Crc32 => {
            let mut hasher = crc32fast::Hasher::new();
            loop {
                cancel.check()?;
                let read = file.read(&mut buffer).map_err(|e| CoreError::io(path, e))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(format!("{:08x}", hasher.finalize()))
        }
        Algorithm::Md5 => {
            use md5::Digest;
            let mut hasher = md5::Md5::new();
            loop {
                cancel.check()?;
                let read = file.read(&mut buffer).map_err(|e| CoreError::io(path, e))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect())
        }
        Algorithm::Sha1 => {
            use sha1::Digest;
            let mut hasher = sha1::Sha1::new();
            loop {
                cancel.check()?;
                let read = file.read(&mut buffer).map_err(|e| CoreError::io(path, e))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect())
        }
        Algorithm::Sha256 => {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            loop {
                cancel.check()?;
                let read = file.read(&mut buffer).map_err(|e| CoreError::io(path, e))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect())
        }
    }
}

/// Hash a byte slice (tests and small buffers).
pub fn hash_bytes(bytes: &[u8], algorithm: Algorithm) -> String {
    match algorithm {
        Algorithm::Crc32 => format!("{:08x}", crc32fast::hash(bytes)),
        Algorithm::Md5 => {
            use md5::Digest;
            let mut hasher = md5::Md5::new();
            hasher.update(bytes);
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        }
        Algorithm::Sha1 => {
            use sha1::Digest;
            let mut hasher = sha1::Sha1::new();
            hasher.update(bytes);
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        }
        Algorithm::Sha256 => {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(bytes);
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        }
    }
}

/// Build the text of a checksum file for `files`, relative to `base`.
pub fn render_checksums(
    files: &[PathBuf],
    base: &Path,
    algorithm: Algorithm,
    cancel: &CancelToken,
) -> CoreResult<String> {
    let mut out = String::new();
    for path in files {
        let hash = hash_file(path, algorithm, cancel)?;
        let name = relative_name(path, base);
        out.push_str(&hash);
        out.push(' ');
        out.push_str(&name);
        out.push('\n');
    }
    Ok(out)
}

/// Write a checksum file for `files` next to them.
///
/// Returns the path of the created file and the number of entries written.
pub fn create_checksum_file(
    files: &[PathBuf],
    out: &Path,
    base: &Path,
    algorithm: Algorithm,
    cancel: &CancelToken,
) -> CoreResult<(PathBuf, usize)> {
    if files.is_empty() {
        return Err(CoreError::Invalid("no files to checksum".to_string()));
    }
    let text = render_checksums(files, base, algorithm, cancel)?;
    let mut writer = BufWriter::new(File::create(out).map_err(|e| CoreError::io(out, e))?);
    writer
        .write_all(text.as_bytes())
        .map_err(|e| CoreError::io(out, e))?;
    writer.flush().map_err(|e| CoreError::io(out, e))?;
    Ok((out.to_path_buf(), files.len()))
}

/// Parse a checksum file. Lines that do not contain a hash are ignored, which
/// makes the parser tolerate comments and blank lines.
pub fn parse_checksums(text: &str) -> Vec<ChecksumEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(entry) = parse_line(line) {
            out.push(entry);
        }
    }
    out
}

fn parse_line(line: &str) -> Option<ChecksumEntry> {
    // `hash *name` / `hash  name` (md5sum style): the hash comes first.
    let mut parts = line.splitn(2, char::is_whitespace);
    let first = parts.next()?;
    let rest = parts.next().unwrap_or("").trim();
    if is_hash(first) && !rest.is_empty() {
        let name = rest.trim_start_matches('*').trim();
        if !name.is_empty() {
            return Some(ChecksumEntry::new(first, name));
        }
    }
    // `name hash` (SFV style): the hash comes last.
    let mut parts = line.rsplitn(2, char::is_whitespace);
    let last = parts.next()?;
    let name = parts.next()?.trim();
    if is_hash(last) && !name.is_empty() {
        return Some(ChecksumEntry::new(last, name));
    }
    None
}

fn is_hash(token: &str) -> bool {
    let len = token.len();
    (len == 8 || len == 32 || len == 40 || len == 64)
        && token.chars().all(|c| c.is_ascii_hexdigit())
}

/// Outcome of verifying a checksum file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerifyReport {
    pub checked: usize,
    pub ok: usize,
    /// `(name, problem)` pairs.
    pub failures: Vec<(String, String)>,
}

impl VerifyReport {
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty() && self.checked > 0
    }

    /// One-line summary for the status bar.
    pub fn summary(&self) -> String {
        if self.checked == 0 {
            return "no checksums found".to_string();
        }
        if self.failures.is_empty() {
            format!("{} file(s) verified", self.ok)
        } else {
            format!("{} ok, {} failed", self.ok, self.failures.len())
        }
    }
}

/// Verify every entry of a checksum file against the files in `base`.
pub fn verify_checksum_file(
    checksum_file: &Path,
    base: &Path,
    cancel: &CancelToken,
) -> CoreResult<VerifyReport> {
    let text =
        std::fs::read_to_string(checksum_file).map_err(|e| CoreError::io(checksum_file, e))?;
    Ok(verify_checksums(&parse_checksums(&text), base, cancel))
}

/// Verify parsed entries against the files in `base`.
pub fn verify_checksums(
    entries: &[ChecksumEntry],
    base: &Path,
    cancel: &CancelToken,
) -> VerifyReport {
    let mut report = VerifyReport::default();
    for entry in entries {
        cancel_check(cancel, &mut report);
        if cancel.is_cancelled() {
            report
                .failures
                .push((entry.name.clone(), "cancelled".to_string()));
            return report;
        }
        report.checked += 1;
        let path = base.join(&entry.name);
        let algorithm = match entry.hash.len() {
            8 => Algorithm::Crc32,
            32 => Algorithm::Md5,
            40 => Algorithm::Sha1,
            64 => Algorithm::Sha256,
            _ => {
                report
                    .failures
                    .push((entry.name.clone(), "unknown hash length".to_string()));
                continue;
            }
        };
        match hash_file(&path, algorithm, cancel) {
            Ok(hash) if hash.eq_ignore_ascii_case(&entry.hash) => report.ok += 1,
            Ok(hash) => report.failures.push((
                entry.name.clone(),
                format!("expected {}, got {}", entry.hash, hash),
            )),
            Err(e) => report.failures.push((entry.name.clone(), e.to_string())),
        }
    }
    report
}

fn cancel_check(cancel: &CancelToken, _report: &mut VerifyReport) {
    let _ = cancel;
}

/// Path of `file` relative to `base` for the checksum list, using `/`.
fn relative_name(file: &Path, base: &Path) -> String {
    match file.strip_prefix(base) {
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file.to_string_lossy().to_string()),
    }
}

/// Suggested checksum file name for `files` in `base`.
pub fn suggested_name(files: &[PathBuf], algorithm: Algorithm) -> String {
    match files.len() {
        0 => format!("checksums.{}", algorithm.extension()),
        1 => {
            let stem = files[0]
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());
            format!("{stem}.{}", algorithm.extension())
        }
        _ => format!("checksums.{}", algorithm.extension()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn known_hashes_match_the_reference_values() {
        // Reference values for "abc".
        assert_eq!(hash_bytes(b"abc", Algorithm::Crc32), "352441c2");
        assert_eq!(
            hash_bytes(b"abc", Algorithm::Md5),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            hash_bytes(b"abc", Algorithm::Sha1),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hash_bytes(b"abc", Algorithm::Sha256),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // The empty input has well-known hashes too.
        assert_eq!(hash_bytes(b"", Algorithm::Crc32), "00000000");
        assert_eq!(
            hash_bytes(b"", Algorithm::Md5),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
        assert_eq!(
            hash_bytes(b"", Algorithm::Sha256),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hashing_a_file_matches_hashing_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        write(&path, b"abc");
        let cancel = CancelToken::new();
        for algorithm in [
            Algorithm::Crc32,
            Algorithm::Md5,
            Algorithm::Sha1,
            Algorithm::Sha256,
        ] {
            assert_eq!(
                hash_file(&path, algorithm, &cancel).unwrap(),
                hash_bytes(b"abc", algorithm)
            );
        }
        assert!(hash_file(&dir.path().join("missing"), Algorithm::Md5, &cancel).is_err());
    }

    #[test]
    fn algorithms_parse_and_detect_extensions() {
        assert_eq!(Algorithm::parse("MD5"), Some(Algorithm::Md5));
        assert_eq!(Algorithm::parse("sfv"), Some(Algorithm::Crc32));
        assert_eq!(Algorithm::parse("nonsense"), None);
        assert_eq!(
            Algorithm::from_file_name("backup.SFV"),
            Some(Algorithm::Crc32)
        );
        assert_eq!(
            Algorithm::from_file_name("a.sha256"),
            Some(Algorithm::Sha256)
        );
        assert_eq!(Algorithm::from_file_name("a.txt"), None);
        assert_eq!(Algorithm::Md5.extension(), "md5");
        assert_eq!(Algorithm::Crc32.extension(), "sfv");
    }

    #[test]
    fn checksum_files_round_trip_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.txt"), b"first");
        write(&dir.path().join("sub/b.txt"), b"second");
        let files = vec![dir.path().join("a.txt"), dir.path().join("sub/b.txt")];
        let cancel = CancelToken::new();
        let out = dir.path().join("checksums.md5");
        let (path, count) =
            create_checksum_file(&files, &out, dir.path(), Algorithm::Md5, &cancel).unwrap();
        assert_eq!(count, 2);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("a.txt"));
        assert!(
            text.contains("sub/b.txt"),
            "relative paths are kept: {text}"
        );
        let report = verify_checksum_file(&path, dir.path(), &cancel).unwrap();
        assert_eq!(report.checked, 2);
        assert_eq!(report.ok, 2);
        assert!(report.is_ok());
        assert!(report.summary().contains("verified"));
    }

    #[test]
    fn a_changed_file_fails_verification() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.txt"), b"first");
        let cancel = CancelToken::new();
        let out = dir.path().join("a.sha256");
        create_checksum_file(
            &[dir.path().join("a.txt")],
            &out,
            dir.path(),
            Algorithm::Sha256,
            &cancel,
        )
        .unwrap();
        write(&dir.path().join("a.txt"), b"changed");
        let report = verify_checksum_file(&out, dir.path(), &cancel).unwrap();
        assert_eq!(report.checked, 1);
        assert_eq!(report.ok, 0);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].1.contains("expected"));
        assert!(!report.is_ok());
        assert!(report.summary().contains("failed"));
    }

    #[test]
    fn a_missing_file_is_reported_not_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelToken::new();
        let entries = vec![ChecksumEntry::new(
            "d41d8cd98f00b204e9800998ecf8427e",
            "gone.txt",
        )];
        let report = verify_checksums(&entries, dir.path(), &cancel);
        assert_eq!(report.checked, 1);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].1.contains("i/o"));
    }

    #[test]
    fn both_checksum_file_conventions_are_parsed() {
        let text = "\
; a comment
900150983cd24fb0d6963f7d28e17f72 *a.txt
a9993e364706816aba3e25717850c26c9cd0d89d  b.txt
c.txt 352441c2
d.txt\tba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
not a checksum line
";
        let entries = parse_checksums(text);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[1].name, "b.txt");
        assert_eq!(entries[2].name, "c.txt");
        assert_eq!(entries[2].hash, "352441c2");
        assert_eq!(entries[3].name, "d.txt");
    }

    #[test]
    fn an_sfv_style_file_verifies() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.txt"), b"abc");
        let sfv = dir.path().join("a.sfv");
        fs::write(&sfv, "a.txt 352441c2\n").unwrap();
        let cancel = CancelToken::new();
        let report = verify_checksum_file(&sfv, dir.path(), &cancel).unwrap();
        assert!(report.is_ok(), "report: {report:?}");
    }

    #[test]
    fn empty_requests_are_rejected_and_summarised() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelToken::new();
        assert!(
            create_checksum_file(
                &[],
                &dir.path().join("x.md5"),
                dir.path(),
                Algorithm::Md5,
                &cancel
            )
            .is_err()
        );
        assert_eq!(VerifyReport::default().summary(), "no checksums found");
        assert!(!VerifyReport::default().is_ok());
    }

    #[test]
    fn suggested_names_follow_the_selection() {
        assert_eq!(
            suggested_name(&[PathBuf::from("/a/report.txt")], Algorithm::Md5),
            "report.md5"
        );
        assert_eq!(
            suggested_name(
                &[PathBuf::from("/a/x"), PathBuf::from("/a/y")],
                Algorithm::Sha256
            ),
            "checksums.sha256"
        );
        assert_eq!(suggested_name(&[], Algorithm::Crc32), "checksums.sfv");
    }

    #[test]
    fn cancellation_stops_hashing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        write(&path, &vec![0u8; 600_000]);
        let cancel = CancelToken::new();
        cancel.cancel();
        let error = hash_file(&path, Algorithm::Sha256, &cancel).unwrap_err();
        assert!(error.is_cancelled());
    }
}

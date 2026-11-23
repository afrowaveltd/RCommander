//! Finding duplicate files.
//!
//! The search is staged so that hashing is only done where it can pay off:
//! files are first grouped by size, and only files that share a size are hashed.
//! A directory with thousands of unique files therefore costs almost nothing.

use crate::checksum::{Algorithm, hash_file};
use crate::error::{CoreError, CoreResult};
use crate::ops::CancelToken;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Files that share their content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub size: u64,
    /// Hash that all `paths` share.
    pub hash: String,
    pub paths: Vec<PathBuf>,
}

impl DuplicateGroup {
    /// Bytes that could be freed by keeping one copy.
    pub fn wasted_bytes(&self) -> u64 {
        if self.paths.len() < 2 {
            0
        } else {
            self.size * (self.paths.len() as u64 - 1)
        }
    }
}

/// Statistics of a duplicate search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DupeReport {
    pub groups: Vec<DuplicateGroup>,
    /// Files that were examined.
    pub scanned: u64,
    /// Files that were hashed (the rest were unique by size).
    pub hashed: u64,
}

impl DupeReport {
    pub fn duplicate_files(&self) -> usize {
        self.groups.iter().map(|group| group.paths.len()).sum()
    }

    pub fn wasted_bytes(&self) -> u64 {
        self.groups.iter().map(|group| group.wasted_bytes()).sum()
    }

    pub fn summary(&self) -> String {
        format!(
            "{} group(s), {} duplicate file(s), {} wasted",
            self.groups.len(),
            self.duplicate_files(),
            crate::format::format_size_unit(self.wasted_bytes())
        )
    }
}

/// Find duplicate files below `root`.
///
/// Files smaller than `min_size` are ignored, and so are hidden files unless
/// `include_hidden` is set.
pub fn find_duplicates(
    root: &Path,
    min_size: u64,
    algorithm: Algorithm,
    include_hidden: bool,
    cancel: &CancelToken,
    mut progress: Option<&mut dyn FnMut(u64)>,
) -> CoreResult<DupeReport> {
    if !root.is_dir() {
        return Err(CoreError::Invalid(format!(
            "{} is not a directory",
            root.display()
        )));
    }
    // 1. Collect files grouped by size.
    let mut by_size: BTreeMap<u64, Vec<PathBuf>> = BTreeMap::new();
    let mut report = DupeReport::default();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        cancel.check()?;
        let read = std::fs::read_dir(&dir).map_err(|e| CoreError::io(&dir, e))?;
        for entry in read.flatten() {
            cancel.check()?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !include_hidden && name.starts_with('.') {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let size = metadata.len();
            if size < min_size {
                continue;
            }
            report.scanned += 1;
            by_size.entry(size).or_default().push(path);
        }
    }
    if let Some(callback) = progress.as_mut() {
        callback(report.scanned);
    }
    // 2. Hash only the candidates that share a size.
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for (size, paths) in by_size {
        if paths.len() < 2 {
            continue;
        }
        cancel.check()?;
        let mut by_hash: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for path in paths {
            cancel.check()?;
            report.hashed += 1;
            match hash_file(&path, algorithm, cancel) {
                Ok(hash) => by_hash.entry(hash).or_default().push(path),
                Err(e) if e.is_cancelled() => return Err(e),
                Err(_) => continue,
            }
        }
        for (hash, mut paths) in by_hash {
            if paths.len() < 2 {
                continue;
            }
            paths.sort();
            groups.push(DuplicateGroup { size, hash, paths });
        }
    }
    groups.sort_by(|a, b| {
        b.wasted_bytes()
            .cmp(&a.wasted_bytes())
            .then_with(|| a.hash.cmp(&b.hash))
    });
    report.groups = groups;
    Ok(report)
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
    fn identical_files_are_grouped_and_unique_ones_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.txt"), b"same content");
        write(&dir.path().join("copy.txt"), b"same content");
        write(&dir.path().join("other.txt"), b"different!");
        write(&dir.path().join("sub/third.txt"), b"same content");
        let cancel = CancelToken::new();
        let report = find_duplicates(dir.path(), 1, Algorithm::Md5, false, &cancel, None).unwrap();
        assert_eq!(report.groups.len(), 1);
        let group = &report.groups[0];
        assert_eq!(group.paths.len(), 3, "three copies share the content");
        assert_eq!(group.size, 12);
        assert_eq!(group.wasted_bytes(), 24);
        assert_eq!(report.duplicate_files(), 3);
        assert!(report.summary().contains("group"));
        // The unique file was never hashed: only the size duplicates were.
        assert_eq!(report.hashed, 3);
    }

    #[test]
    fn files_of_different_sizes_are_never_hashed() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.txt"), b"1234567890");
        write(&dir.path().join("b.txt"), b"12345");
        let cancel = CancelToken::new();
        let report =
            find_duplicates(dir.path(), 1, Algorithm::Sha256, false, &cancel, None).unwrap();
        assert!(report.groups.is_empty());
        assert_eq!(report.scanned, 2);
        assert_eq!(report.hashed, 0, "no size collision means no hashing");
    }

    #[test]
    fn the_minimum_size_and_hidden_flag_are_honoured() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("small1.txt"), b"x");
        write(&dir.path().join("small2.txt"), b"x");
        write(&dir.path().join(".hidden1"), b"secret");
        write(&dir.path().join(".hidden2"), b"secret");
        let cancel = CancelToken::new();
        let with_minimum =
            find_duplicates(dir.path(), 4, Algorithm::Crc32, false, &cancel, None).unwrap();
        assert!(
            with_minimum.groups.is_empty(),
            "tiny files are below the minimum"
        );
        let with_hidden =
            find_duplicates(dir.path(), 1, Algorithm::Crc32, true, &cancel, None).unwrap();
        assert_eq!(with_hidden.groups.len(), 2);
        let without_hidden =
            find_duplicates(dir.path(), 1, Algorithm::Crc32, false, &cancel, None).unwrap();
        assert_eq!(without_hidden.groups.len(), 1);
    }

    #[test]
    fn groups_are_ordered_by_wasted_space() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("small_a"), b"aaaa");
        write(&dir.path().join("small_b"), b"aaaa");
        write(&dir.path().join("big_a"), &vec![7u8; 1000]);
        write(&dir.path().join("big_b"), &vec![7u8; 1000]);
        let cancel = CancelToken::new();
        let report = find_duplicates(dir.path(), 1, Algorithm::Sha1, false, &cancel, None).unwrap();
        assert_eq!(report.groups.len(), 2);
        assert_eq!(report.groups[0].size, 1000, "the biggest waste comes first");
        assert_eq!(report.wasted_bytes(), 1004);
    }

    #[test]
    fn a_non_directory_is_rejected_and_cancellation_is_honoured() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.txt"), b"a");
        let cancel = CancelToken::new();
        assert!(
            find_duplicates(
                &dir.path().join("a.txt"),
                0,
                Algorithm::Md5,
                false,
                &cancel,
                None
            )
            .is_err()
        );
        cancel.cancel();
        let error =
            find_duplicates(dir.path(), 0, Algorithm::Md5, false, &cancel, None).unwrap_err();
        assert!(error.is_cancelled());
    }

    #[test]
    fn progress_reports_the_number_of_candidates() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.txt"), b"content");
        write(&dir.path().join("b.txt"), b"content");
        let cancel = CancelToken::new();
        let mut last = 0u64;
        let mut callback = |scanned: u64| last = scanned;
        let report = find_duplicates(
            dir.path(),
            1,
            Algorithm::Md5,
            false,
            &cancel,
            Some(&mut callback),
        )
        .unwrap();
        assert_eq!(last, report.scanned);
        assert_eq!(last, 2);
    }
}

//! Comparing files by content.

use crate::error::{CoreError, CoreResult};
use crate::ops::CancelToken;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Are the contents of `a` and `b` identical?
pub fn files_equal(a: &Path, b: &Path, cancel: &CancelToken) -> CoreResult<bool> {
    Ok(first_difference(a, b, cancel)?.is_none())
}

/// Offset of the first differing byte, or `None` when the files are identical.
pub fn first_difference(a: &Path, b: &Path, cancel: &CancelToken) -> CoreResult<Option<u64>> {
    let size_a = std::fs::metadata(a).map_err(|e| CoreError::io(a, e))?.len();
    let size_b = std::fs::metadata(b).map_err(|e| CoreError::io(b, e))?.len();
    if size_a != size_b {
        // Different sizes: report the offset where one of them ends.
        return Ok(Some(size_a.min(size_b)));
    }
    let mut file_a = File::open(a).map_err(|e| CoreError::io(a, e))?;
    let mut file_b = File::open(b).map_err(|e| CoreError::io(b, e))?;
    let mut buffer_a = vec![0u8; 128 * 1024];
    let mut buffer_b = vec![0u8; 128 * 1024];
    let mut offset = 0u64;
    loop {
        cancel.check()?;
        let read_a = file_a
            .read(&mut buffer_a)
            .map_err(|e| CoreError::io(a, e))?;
        let read_b = file_b
            .read(&mut buffer_b)
            .map_err(|e| CoreError::io(b, e))?;
        if read_a == 0 && read_b == 0 {
            return Ok(None);
        }
        if read_a != read_b {
            return Ok(Some(offset + read_a.min(read_b) as u64));
        }
        if buffer_a[..read_a] != buffer_b[..read_b] {
            let index = buffer_a[..read_a]
                .iter()
                .zip(buffer_b[..read_b].iter())
                .position(|(x, y)| x != y)
                .unwrap_or(0);
            return Ok(Some(offset + index as u64));
        }
        offset += read_a as u64;
    }
}

/// Result of comparing two directories by content.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentCompareReport {
    /// Names present on both sides with identical content.
    pub identical: Vec<String>,
    /// Names present on both sides with different content.
    pub different: Vec<String>,
    /// Names present only on the left side.
    pub only_left: Vec<String>,
    /// Names present only on the right side.
    pub only_right: Vec<String>,
}

impl ContentCompareReport {
    pub fn is_empty(&self) -> bool {
        self.identical.is_empty()
            && self.different.is_empty()
            && self.only_left.is_empty()
            && self.only_right.is_empty()
    }

    /// One-line summary.
    pub fn summary(&self) -> String {
        format!(
            "{} identical, {} different, {} only left, {} only right",
            self.identical.len(),
            self.different.len(),
            self.only_left.len(),
            self.only_right.len()
        )
    }
}

/// Compare two directories file by file, by content.
#[allow(clippy::type_complexity)]
pub fn compare_directories_by_content(
    left: &Path,
    right: &Path,
    recursive: bool,
    cancel: &CancelToken,
    mut progress: Option<&mut dyn FnMut(&str)>,
) -> CoreResult<ContentCompareReport> {
    let mut report = ContentCompareReport::default();
    let mut stack: Vec<String> = vec![String::new()];
    while let Some(prefix) = stack.pop() {
        cancel.check()?;
        let left_dir = if prefix.is_empty() {
            left.to_path_buf()
        } else {
            left.join(&prefix)
        };
        let right_dir = if prefix.is_empty() {
            right.to_path_buf()
        } else {
            right.join(&prefix)
        };
        let left_names = names_in(&left_dir)?;
        let right_names = names_in(&right_dir)?;
        for name in &left_names {
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let left_path = left_dir.join(name);
            if left_path.is_dir() {
                if recursive && right_dir.join(name).is_dir() {
                    stack.push(relative);
                } else if !right_dir.join(name).exists() {
                    report.only_left.push(relative);
                }
                continue;
            }
            if let Some(callback) = progress.as_mut() {
                callback(&relative);
            }
            let right_path = right_dir.join(name);
            if !right_path.exists() {
                report.only_left.push(relative);
            } else if files_equal(&left_path, &right_path, cancel)? {
                report.identical.push(relative);
            } else {
                report.different.push(relative);
            }
        }
        for name in &right_names {
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let right_path = right_dir.join(name);
            if right_path.is_dir() {
                continue;
            }
            if !left_dir.join(name).exists() {
                report.only_right.push(relative);
            }
        }
    }
    report.identical.sort();
    report.different.sort();
    report.only_left.sort();
    report.only_right.sort();
    Ok(report)
}

fn names_in(dir: &Path) -> CoreResult<Vec<String>> {
    let read = std::fs::read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
    let mut names: Vec<String> = read
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    Ok(names)
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
    fn identical_files_compare_equal() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a"), b"same content");
        write(&dir.path().join("b"), b"same content");
        let cancel = CancelToken::new();
        assert!(files_equal(&dir.path().join("a"), &dir.path().join("b"), &cancel).unwrap());
        assert_eq!(
            first_difference(&dir.path().join("a"), &dir.path().join("b"), &cancel).unwrap(),
            None
        );
    }

    #[test]
    fn the_first_differing_byte_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a"), b"abcdef");
        write(&dir.path().join("b"), b"abcXef");
        let cancel = CancelToken::new();
        assert_eq!(
            first_difference(&dir.path().join("a"), &dir.path().join("b"), &cancel).unwrap(),
            Some(3)
        );
        assert!(!files_equal(&dir.path().join("a"), &dir.path().join("b"), &cancel).unwrap());
    }

    #[test]
    fn different_lengths_are_reported_at_the_shorter_end() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a"), b"12345");
        write(&dir.path().join("b"), b"123");
        let cancel = CancelToken::new();
        assert_eq!(
            first_difference(&dir.path().join("a"), &dir.path().join("b"), &cancel).unwrap(),
            Some(3)
        );
    }

    #[test]
    fn files_larger_than_the_buffer_are_compared_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let big: Vec<u8> = (0..400_000u32).map(|i| (i % 253) as u8).collect();
        write(&dir.path().join("a"), &big);
        write(&dir.path().join("b"), &big);
        let cancel = CancelToken::new();
        assert!(files_equal(&dir.path().join("a"), &dir.path().join("b"), &cancel).unwrap());
        let mut changed = big.clone();
        changed[300_000] = changed[300_000].wrapping_add(1);
        write(&dir.path().join("c"), &changed);
        assert_eq!(
            first_difference(&dir.path().join("a"), &dir.path().join("c"), &cancel).unwrap(),
            Some(300_000)
        );
    }

    #[test]
    fn missing_files_are_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelToken::new();
        assert!(files_equal(&dir.path().join("nope"), &dir.path().join("nope2"), &cancel).is_err());
    }

    #[test]
    fn directories_are_compared_file_by_file() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        write(&left.path().join("same.txt"), b"same");
        write(&right.path().join("same.txt"), b"same");
        write(&left.path().join("changed.txt"), b"one");
        write(&right.path().join("changed.txt"), b"two");
        write(&left.path().join("left-only.txt"), b"x");
        write(&right.path().join("right-only.txt"), b"y");
        write(&left.path().join("sub/nested.txt"), b"deep");
        write(&right.path().join("sub/nested.txt"), b"deep");
        let cancel = CancelToken::new();
        let report =
            compare_directories_by_content(left.path(), right.path(), false, &cancel, None)
                .unwrap();
        assert_eq!(report.identical, vec!["same.txt".to_string()]);
        assert_eq!(report.different, vec!["changed.txt".to_string()]);
        assert_eq!(report.only_left, vec!["left-only.txt".to_string()]);
        assert_eq!(report.only_right, vec!["right-only.txt".to_string()]);
        assert!(report.summary().contains("identical"));
        // With recursion the nested file is compared too.
        let recursive =
            compare_directories_by_content(left.path(), right.path(), true, &cancel, None).unwrap();
        assert!(recursive.identical.contains(&"sub/nested.txt".to_string()));
    }

    #[test]
    fn progress_is_reported_per_file() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        write(&left.path().join("a.txt"), b"a");
        write(&right.path().join("a.txt"), b"a");
        let cancel = CancelToken::new();
        let mut seen: Vec<String> = Vec::new();
        let mut callback = |name: &str| seen.push(name.to_string());
        let report = compare_directories_by_content(
            left.path(),
            right.path(),
            false,
            &cancel,
            Some(&mut callback),
        )
        .unwrap();
        assert_eq!(seen, vec!["a.txt".to_string()]);
        assert!(!report.is_empty());
    }
}

//! Branch view: one flat list of everything below a directory.

use crate::entry::{Attrs, Entry, EntryKind};
use crate::error::{CoreError, CoreResult};
use crate::ops::CancelToken;
use std::path::{Path, PathBuf};

/// Build the branch view of `root`.
///
/// Every file below `root` becomes one row whose name is the path relative to
/// `root` (`sub/dir/file.txt`), which is how a branch view keeps the tree
/// visible while staying a single flat list that can be sorted and filtered.
/// `max_entries` bounds the work, and the returned flag reports truncation.
pub fn branch_listing(
    root: &Path,
    max_entries: usize,
    cancel: &CancelToken,
) -> CoreResult<(Vec<Entry>, bool)> {
    if !root.is_dir() {
        return Err(CoreError::Invalid(format!(
            "{} is not a directory",
            root.display()
        )));
    }
    let mut out: Vec<Entry> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut truncated = false;
    while let Some(dir) = stack.pop() {
        cancel.check()?;
        let read = match std::fs::read_dir(&dir) {
            Ok(read) => read,
            Err(_) => continue,
        };
        for item in read.flatten() {
            cancel.check()?;
            let path = item.path();
            let relative = match path.strip_prefix(root) {
                Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            let Ok(metadata) = item.metadata() else {
                continue;
            };
            let is_dir = metadata.is_dir();
            if is_dir {
                stack.push(path.clone());
            }
            if out.len() >= max_entries {
                truncated = true;
                return Ok((out, truncated));
            }
            let kind = if is_dir {
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            let mut entry = Entry::new(
                relative,
                path,
                kind,
                if is_dir { 0 } else { metadata.len() },
            );
            entry.mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            #[cfg(unix)]
            let unix_mode = {
                use std::os::unix::fs::MetadataExt;
                Some(metadata.mode() & 0o7777)
            };
            #[cfg(not(unix))]
            let unix_mode = None;
            entry.attrs = Attrs {
                readonly: metadata.permissions().readonly(),
                unix_mode,
                ..Attrs::default()
            };
            out.push(entry);
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok((out, truncated))
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
    fn the_branch_view_lists_the_whole_subtree_with_relative_names() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.txt"), b"a");
        write(&dir.path().join("sub/b.txt"), b"bb");
        write(&dir.path().join("sub/deep/c.txt"), b"ccc");
        let cancel = CancelToken::new();
        let (entries, truncated) = branch_listing(dir.path(), 1000, &cancel).unwrap();
        assert!(!truncated);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"sub/b.txt"));
        assert!(names.contains(&"sub/deep/c.txt"));
        assert!(
            names.contains(&"sub"),
            "directories are listed as well: {names:?}"
        );
        let nested = entries.iter().find(|e| e.name == "sub/b.txt").unwrap();
        assert_eq!(nested.size, 2);
        assert!(!nested.is_dir());
    }

    #[test]
    fn the_entry_limit_truncates_the_list() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..10 {
            write(&dir.path().join(format!("f{index}.txt")), b"x");
        }
        let cancel = CancelToken::new();
        let (entries, truncated) = branch_listing(dir.path(), 4, &cancel).unwrap();
        assert!(truncated);
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn a_file_is_rejected_and_cancellation_is_honoured() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.txt"), b"a");
        let cancel = CancelToken::new();
        assert!(branch_listing(&dir.path().join("a.txt"), 10, &cancel).is_err());
        cancel.cancel();
        assert!(
            branch_listing(dir.path(), 10, &cancel)
                .unwrap_err()
                .is_cancelled()
        );
    }

    #[test]
    fn an_empty_directory_produces_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelToken::new();
        let (entries, truncated) = branch_listing(dir.path(), 10, &cancel).unwrap();
        assert!(entries.is_empty());
        assert!(!truncated);
    }
}

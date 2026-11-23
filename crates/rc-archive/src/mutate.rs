//! Modifying an existing zip archive: add/replace members and remove them.
//!
//! Both operations rewrite the archive: the surviving members are copied
//! verbatim (no recompression) into a temporary file next to the original, the
//! change is applied, and only then is the temporary file renamed over the
//! original. A failure therefore leaves the original archive exactly as it was.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use zip::ZipWriter;

use crate::backend::zip::{ZipBackend, zip_error_for};
use crate::create::{PackOutcome, add_plans_to_zip, collect_plans, plan_names, reject_password};
use crate::error::{ArchiveError, Result};
use crate::member;
use crate::read::ArchiveRead;

/// Add — or replace — `items` inside an existing zip.
///
/// Names are computed exactly like [`crate::create`] does it but relative to
/// `base`, which is the directory the items are "inside": loading
/// `base/a.txt` stores `a.txt`, loading `base/dir` stores `dir/...`. Existing
/// members with one of those names are replaced, everything else is preserved
/// byte for byte.
///
/// `level` is `0` to store and `1..=9` to deflate the *new* members; the
/// surviving members keep their original compression. `password` is accepted for
/// API symmetry but must be `None`/empty — see [`crate::PackOptions::password`].
///
/// The rewrite is atomic: on any error the original zip is untouched.
pub fn add_to_zip(
    zip: &Path,
    items: &[PathBuf],
    base: &Path,
    level: u8,
    password: Option<&str>,
) -> Result<PackOutcome> {
    reject_password(password)?;
    let plans = collect_plans(items, Some(base))?;
    let mut backend = ZipBackend::open(zip)?;
    let replaced = plan_names(&plans);

    let parent = zip
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| ArchiveError::io(parent, e))?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| ArchiveError::io(parent, e))?;

    let (added, bytes_in) = {
        let mut writer = ZipWriter::new(temp.as_file_mut());
        // Everything that is not being replaced survives untouched.
        backend.copy_entries_into(&mut writer, &replaced)?;
        let outcome = add_plans_to_zip(&mut writer, &plans, level, zip)?;
        writer.finish().map_err(|e| zip_error_for(zip, None, e))?;
        outcome
    };

    temp.as_file_mut()
        .flush()
        .map_err(|e| ArchiveError::io(zip, e))?;
    let archive_size = temp
        .as_file()
        .metadata()
        .map_err(|e| ArchiveError::io(zip, e))?
        .len();
    temp.persist(zip)
        .map_err(|e| ArchiveError::io(zip, e.error))?;
    Ok(PackOutcome {
        added,
        bytes_in,
        archive_size,
    })
}

/// Remove members from an existing zip, returning how many were dropped.
///
/// Naming a directory removes its whole subtree. Names that are not in the
/// archive are ignored (the count reflects what actually disappeared), which is
/// what makes "remove these" idempotent for the panel.
///
/// The rewrite is atomic: on any error the original zip is untouched.
pub fn remove_from_zip(zip: &Path, members: &[String]) -> Result<u64> {
    let mut backend = ZipBackend::open(zip)?;
    let wanted: HashSet<String> = members.iter().map(|m| member::normalize(m)).collect();
    if wanted.is_empty() {
        return Ok(0);
    }
    let removed = backend
        .entries()
        .iter()
        .filter(|entry| {
            wanted.contains(&entry.path)
                || wanted
                    .iter()
                    .any(|want| crate::backend::zip::under(&entry.path, want))
        })
        .count() as u64;
    if removed == 0 {
        // Nothing to do: leave the file byte-identical instead of rewriting it.
        return Ok(0);
    }

    let parent = zip
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| ArchiveError::io(parent, e))?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| ArchiveError::io(parent, e))?;
    {
        let mut writer = ZipWriter::new(temp.as_file_mut());
        backend.copy_entries_into(&mut writer, &wanted)?;
        writer.finish().map_err(|e| zip_error_for(zip, None, e))?;
    }
    temp.as_file_mut()
        .flush()
        .map_err(|e| ArchiveError::io(zip, e))?;
    temp.persist(zip)
        .map_err(|e| ArchiveError::io(zip, e.error))?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::{PackOptions, create};
    use crate::open;

    fn listing(path: &Path) -> Vec<String> {
        let archive = open(path).unwrap();
        let mut names: Vec<String> = archive.entries().iter().map(|e| e.path.clone()).collect();
        names.sort();
        names
    }

    fn sample_zip(dir: &Path) -> PathBuf {
        let src = dir.join("src");
        fs::create_dir_all(src.join("dir")).unwrap();
        fs::write(src.join("a.txt"), b"alpha").unwrap();
        fs::write(src.join("b.txt"), b"beta").unwrap();
        fs::write(src.join("dir").join("c.txt"), b"gamma").unwrap();
        let zip = dir.join("mutable.zip");
        create(
            &zip,
            &[src.join("a.txt"), src.join("b.txt"), src.join("dir")],
            &PackOptions {
                store_paths_relative_to: Some(src),
                ..PackOptions::default()
            },
        )
        .unwrap();
        zip
    }

    #[test]
    fn add_inserts_new_members_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let zip = sample_zip(dir.path());
        let before = listing(&zip);
        assert_eq!(before, ["a.txt", "b.txt", "dir", "dir/c.txt"]);

        let extra = dir.path().join("extra");
        fs::create_dir_all(&extra).unwrap();
        fs::write(extra.join("new.txt"), b"newcomer").unwrap();
        fs::write(extra.join("sub.txt"), b"sub").unwrap();

        let outcome = add_to_zip(
            &zip,
            &[extra.join("new.txt"), extra.join("sub.txt")],
            &extra,
            6,
            None,
        )
        .unwrap();
        assert_eq!(outcome.added, 2);
        assert_eq!(outcome.bytes_in, 3 + 8);
        assert_eq!(outcome.archive_size, fs::metadata(&zip).unwrap().len());

        let after = listing(&zip);
        assert_eq!(
            after,
            ["a.txt", "b.txt", "dir", "dir/c.txt", "new.txt", "sub.txt"]
        );

        let mut archive = open(&zip).unwrap();
        assert_eq!(archive.read("new.txt").unwrap(), b"newcomer");
        assert_eq!(
            archive.read("a.txt").unwrap(),
            b"alpha",
            "surviving members keep their content"
        );
        assert_eq!(archive.read("dir/c.txt").unwrap(), b"gamma");
    }

    #[test]
    fn add_replaces_a_member_with_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let zip = sample_zip(dir.path());

        let replacement = dir.path().join("replacement");
        fs::create_dir_all(&replacement).unwrap();
        fs::write(replacement.join("a.txt"), b"ALPHA v2").unwrap();
        add_to_zip(&zip, &[replacement.join("a.txt")], &replacement, 0, None).unwrap();

        let after = listing(&zip);
        assert_eq!(
            after,
            ["a.txt", "b.txt", "dir", "dir/c.txt"],
            "no duplicate entry is created"
        );
        let mut archive = open(&zip).unwrap();
        assert_eq!(archive.read("a.txt").unwrap(), b"ALPHA v2");
    }

    #[test]
    fn add_with_a_missing_item_leaves_the_archive_valid() {
        let dir = tempfile::tempdir().unwrap();
        let zip = sample_zip(dir.path());
        let before = fs::read(&zip).unwrap();

        let missing = dir.path().join("ghost.txt");
        let err = add_to_zip(&zip, &[missing], dir.path(), 6, None).unwrap_err();
        assert!(matches!(err, ArchiveError::Io { .. }), "{err:?}");
        assert_eq!(
            fs::read(&zip).unwrap(),
            before,
            "the original bytes are untouched"
        );
        assert_eq!(listing(&zip), ["a.txt", "b.txt", "dir", "dir/c.txt"]);
    }

    #[test]
    fn add_refuses_a_password() {
        let dir = tempfile::tempdir().unwrap();
        let zip = sample_zip(dir.path());
        let before = fs::read(&zip).unwrap();
        let err = add_to_zip(
            &zip,
            &[dir.path().join("src/a.txt")],
            &dir.path().join("src"),
            6,
            Some("secret"),
        )
        .unwrap_err();
        assert!(matches!(err, ArchiveError::Unsupported(_)));
        assert_eq!(fs::read(&zip).unwrap(), before);
    }

    #[test]
    fn remove_drops_members_and_reports_the_count() {
        let dir = tempfile::tempdir().unwrap();
        let zip = sample_zip(dir.path());

        let removed = remove_from_zip(&zip, &["b.txt".to_string()]).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(listing(&zip), ["a.txt", "dir", "dir/c.txt"]);

        // Removing a directory takes its members with it.
        let removed = remove_from_zip(&zip, &["dir".to_string()]).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(listing(&zip), ["a.txt"]);

        let mut archive = open(&zip).unwrap();
        assert_eq!(archive.read("a.txt").unwrap(), b"alpha");
    }

    #[test]
    fn remove_ignores_missing_members() {
        let dir = tempfile::tempdir().unwrap();
        let zip = sample_zip(dir.path());
        let before = fs::read(&zip).unwrap();

        assert_eq!(
            remove_from_zip(&zip, &["ghost.txt".to_string()]).unwrap(),
            0
        );
        assert_eq!(remove_from_zip(&zip, &[]).unwrap(), 0);
        assert_eq!(
            fs::read(&zip).unwrap(),
            before,
            "the archive is not rewritten when nothing matches"
        );
        assert_eq!(listing(&zip), ["a.txt", "b.txt", "dir", "dir/c.txt"]);
    }

    #[test]
    fn a_failed_rewrite_keeps_the_original_archive() {
        let dir = tempfile::tempdir().unwrap();
        let zip = sample_zip(dir.path());
        let before = fs::read(&zip).unwrap();

        // Not a zip at all: the open fails, so the file is never touched.
        let not_a_zip = dir.path().join("broken.zip");
        fs::write(&not_a_zip, b"not a zip file").unwrap();
        assert!(
            add_to_zip(
                &not_a_zip,
                &[dir.path().join("src/a.txt")],
                &dir.path().join("src"),
                6,
                None
            )
            .is_err()
        );
        assert_eq!(fs::read(&not_a_zip).unwrap(), b"not a zip file");

        assert!(remove_from_zip(&not_a_zip, &["x".to_string()]).is_err());
        assert_eq!(fs::read(&zip).unwrap(), before);
    }

    #[test]
    fn rewritten_archives_are_still_testable() {
        let dir = tempfile::tempdir().unwrap();
        let zip = sample_zip(dir.path());
        let extra = dir.path().join("extra");
        fs::create_dir_all(&extra).unwrap();
        fs::write(extra.join("new.txt"), b"newcomer").unwrap();
        add_to_zip(&zip, &[extra.join("new.txt")], &extra, 1, None).unwrap();

        let report = crate::test(&zip, &rc_core::ops::CancelToken::new()).unwrap();
        assert!(report.ok, "{:?}", report.errors);
        assert_eq!(report.entries, 5);
        assert!(report.bytes_read > 0);
    }
}

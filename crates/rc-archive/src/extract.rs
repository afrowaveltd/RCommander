//! Extraction: writing archive members back to a real file system.
//!
//! Every extraction path validates the member names *before* the first write, so
//! an archive that contains `../evil.txt`, `/abs.txt` or `C:\evil.txt` is
//! rejected as a whole with [`ArchiveError::UnsafePath`] and nothing is created
//! outside the destination directory.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use rc_core::ops::CancelToken;

use crate::error::{ArchiveError, Result};
use crate::member;
use crate::open;
use crate::read::{ArchiveRead, member_not_found};
use crate::stream;

/// What an extraction produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractOutcome {
    /// Number of regular files written.
    pub files: u64,
    /// Number of directories created (or confirmed).
    pub dirs: u64,
    /// Total number of uncompressed bytes written.
    pub bytes: u64,
    /// Every path that was written, in archive order.
    pub written: Vec<PathBuf>,
}

/// Extract the whole archive below `dest`.
///
/// See [`extract_members`] for the details; this is the whole-archive case.
pub fn extract_all(path: &Path, dest: &Path, cancel: &CancelToken) -> Result<ExtractOutcome> {
    extract_members(path, dest, &[], cancel)
}

/// Extract only `only` members below `dest` (`only.empty()` means everything).
///
/// A member is selected when its path equals one of `only` or lives below such a
/// path, so naming a directory extracts its whole subtree. Directories are
/// created as needed, including the synthesized parents of a nested member.
///
/// The cancellation token is polled between members *and* between 64 KiB chunks
/// of a single member; a cancelled run returns [`ArchiveError::Cancelled`].
///
/// # Safety
///
/// Member names are normalised and then checked: absolute paths (`/abs.txt`),
/// drive-letter paths (`C:\abs.txt`, `\\server\share`) and any `..` component
/// are refused with [`ArchiveError::UnsafePath`], which reports the offending
/// member. The check runs for every selected member before anything is written,
/// so a hostile archive does not leave a half-extracted tree behind.
pub fn extract_members(
    path: &Path,
    dest: &Path,
    only: &[String],
    cancel: &CancelToken,
) -> Result<ExtractOutcome> {
    if cancel.is_cancelled() {
        return Err(ArchiveError::Cancelled);
    }
    let mut archive = open(path)?;
    let selected = select_members(archive.as_ref(), only);
    validate_members(archive.as_ref(), path, &selected)?;

    fs::create_dir_all(dest).map_err(|e| ArchiveError::io(dest, e))?;
    let mut outcome = ExtractOutcome::default();

    for &slot in &selected {
        if cancel.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        let (member_path, is_dir) = {
            let entry = &archive.entries()[slot];
            (entry.path.clone(), entry.is_dir)
        };
        // Names are validated above, so this cannot fail; an empty relative path
        // is the archive root itself and needs no directory.
        let Some(relative) = member::safe_relative(&member_path) else {
            continue;
        };
        let target = dest.join(&relative);

        if is_dir {
            fs::create_dir_all(&target).map_err(|e| ArchiveError::io(&target, e))?;
            outcome.dirs += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| ArchiveError::io(parent, e))?;
        }
        let file = File::create(&target).map_err(|e| ArchiveError::io(&target, e))?;
        let mut writer = BufWriter::new(file);
        let written = {
            let mut watchdog = stream::CancelWriter::new(&mut writer, cancel);
            resolve_copy(archive.copy_to(&member_path, &mut watchdog), cancel)?
        };
        writer.flush().map_err(|e| ArchiveError::io(&target, e))?;

        outcome.files += 1;
        outcome.bytes += written;
        outcome.written.push(target);
    }
    Ok(outcome)
}

/// Extract a single member into `dest_file`, creating parent directories.
///
/// Returns the number of bytes written. The destination is not restricted to a
/// directory root — the caller names the exact file — but the *member* name is
/// still validated against the same rules, so a hostile member is refused.
pub fn extract_member_to(path: &Path, member: &str, dest_file: &Path) -> Result<u64> {
    let mut archive = open(path)?;
    let wanted = member::normalize(member);
    let raw = archive
        .raw_name(&wanted)
        .map(str::to_owned)
        .unwrap_or_else(|| wanted.clone());
    if let Some(reason) = member::unsafe_reason(&raw) {
        return Err(ArchiveError::unsafe_path(&raw, &reason));
    }
    if archive.entry(&wanted).is_none() {
        return Err(member_not_found(path, member));
    }
    if let Some(parent) = dest_file.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| ArchiveError::io(parent, e))?;
    }
    let file = File::create(dest_file).map_err(|e| ArchiveError::io(dest_file, e))?;
    let mut writer = BufWriter::new(file);
    let written = archive.copy_to(&wanted, &mut writer)?;
    writer.flush().map_err(|e| ArchiveError::io(dest_file, e))?;
    Ok(written)
}

/// Turn the result of a streaming copy into a cancellation when the token is
/// set.
///
/// Backends see a write failure (they are handed a
/// [`stream::CancelWriter`](crate::stream::CancelWriter) that refuses writes
/// once the token fires); the user-visible error must be
/// [`ArchiveError::Cancelled`] instead.
fn resolve_copy(result: Result<u64>, cancel: &CancelToken) -> Result<u64> {
    match result {
        Ok(written) => Ok(written),
        Err(error) => {
            if cancel.is_cancelled() {
                Err(ArchiveError::Cancelled)
            } else {
                Err(error)
            }
        }
    }
}

/// Indices of the members selected by `only` (everything when it is empty).
fn select_members(archive: &dyn ArchiveRead, only: &[String]) -> Vec<usize> {
    if only.is_empty() {
        return (0..archive.entries().len()).collect();
    }
    let wanted: Vec<String> = only.iter().map(|m| member::normalize(m)).collect();
    archive
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            wanted.iter().any(|want| {
                entry.path == *want
                    || entry.path.starts_with(want)
                        && entry.path.as_bytes().get(want.len()) == Some(&b'/')
            })
        })
        .map(|(slot, _)| slot)
        .collect()
}

/// Refuse the whole run when any selected member name is unsafe.
fn validate_members(archive: &dyn ArchiveRead, path: &Path, selected: &[usize]) -> Result<()> {
    for &slot in selected {
        let entry = &archive.entries()[slot];
        if entry.path.is_empty() {
            continue;
        }
        let raw = archive.raw_name(&entry.path).unwrap_or(entry.path.as_str());
        if let Some(reason) = member::unsafe_reason(raw) {
            return Err(ArchiveError::UnsafePath(format!(
                "{reason} (member {raw:?} in {})",
                path.display()
            )));
        }
        if member::safe_relative(&entry.path).is_none() && !entry.path.is_empty() {
            return Err(ArchiveError::UnsafePath(format!(
                "unusable member path {:?} in {}",
                entry.path,
                path.display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::{PackOptions, create};
    use std::io::Write;

    struct Fixture {
        _dir: tempfile::TempDir,
        archive: PathBuf,
    }

    fn fixture(_name: &str, members: &[(&str, &[u8])]) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        fs::create_dir_all(&source).unwrap();
        for (name, data) in members {
            let path = source.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, data).unwrap();
        }
        let archive = dir.path().join("a.zip");
        let mut items: Vec<PathBuf> = fs::read_dir(&source)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        items.sort();
        create(
            &archive,
            &items,
            &PackOptions {
                store_paths_relative_to: Some(source),
                ..PackOptions::default()
            },
        )
        .unwrap();
        Fixture { _dir: dir, archive }
    }

    fn zip_with_hostile_members(path: &Path, members: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in members {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn extract_all_writes_files_and_sizes() {
        let fixture = fixture(
            "x",
            &[
                ("one.txt", b"hello"),
                ("dir/two.txt", b"nested"),
                ("empty.txt", b""),
            ],
        );
        let dest_root = tempfile::tempdir().unwrap();
        let dest = dest_root.path().join("out");

        let outcome = extract_all(&fixture.archive, &dest, &CancelToken::new()).unwrap();
        assert_eq!(outcome.files, 3);
        assert!(outcome.dirs >= 1, "the nested directory is created");
        assert_eq!(outcome.bytes, 11);

        assert_eq!(fs::read(dest.join("one.txt")).unwrap(), b"hello");
        assert_eq!(fs::read(dest.join("dir/two.txt")).unwrap(), b"nested");
        assert_eq!(fs::read(dest.join("empty.txt")).unwrap(), b"");
        assert_eq!(outcome.written.len(), 3);
        for path in &outcome.written {
            assert!(
                path.starts_with(&dest),
                "{path:?} must stay below the destination"
            );
        }
    }

    #[test]
    fn extract_members_selects_a_subset() {
        let fixture = fixture(
            "x",
            &[
                ("one.txt", b"1"),
                ("two.txt", b"22"),
                ("dir/three.txt", b"333"),
            ],
        );
        let dest_root = tempfile::tempdir().unwrap();
        let dest = dest_root.path().join("subset");

        let outcome = extract_members(
            &fixture.archive,
            &dest,
            &["dir".to_string()],
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(outcome.files, 1);
        assert_eq!(fs::read(dest.join("dir/three.txt")).unwrap(), b"333");
        assert!(!dest.join("one.txt").exists());

        let dest2 = dest_root.path().join("single");
        let outcome = extract_members(
            &fixture.archive,
            &dest2,
            &["two.txt".to_string(), "missing.txt".to_string()],
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(outcome.files, 1);
        assert_eq!(fs::read(dest2.join("two.txt")).unwrap(), b"22");
        assert!(!dest2.join("dir").exists());

        // An empty selection behaves like `extract_all`.
        let dest3 = dest_root.path().join("all");
        let outcome = extract_members(&fixture.archive, &dest3, &[], &CancelToken::new()).unwrap();
        assert_eq!(outcome.files, 3);
    }

    #[test]
    fn extract_member_to_writes_one_file() {
        let fixture = fixture("x", &[("one.txt", b"hello"), ("two.txt", b"bye")]);
        let dest_root = tempfile::tempdir().unwrap();
        let target = dest_root.path().join("deep/er/chosen.bin");
        let written = extract_member_to(&fixture.archive, "one.txt", &target).unwrap();
        assert_eq!(written, 5);
        assert_eq!(fs::read(&target).unwrap(), b"hello");

        // Leading slashes and backslashes are tolerated in the query.
        let other = dest_root.path().join("other.bin");
        assert_eq!(
            extract_member_to(&fixture.archive, "/two.txt", &other).unwrap(),
            3
        );
        assert_eq!(fs::read(&other).unwrap(), b"bye");

        // Unknown member: clean error, no file created.
        let missing = dest_root.path().join("missing.bin");
        assert!(extract_member_to(&fixture.archive, "ghost.txt", &missing).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn zip_slip_members_are_refused_and_nothing_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("hostile.zip");
        zip_with_hostile_members(
            &archive,
            &[
                ("../evil.txt", b"pwned"),
                ("/abs.txt", b"pwned"),
                ("good.txt", b"fine"),
            ],
        );

        let dest_root = tempfile::tempdir().unwrap();
        let dest = dest_root.path().join("sandbox");
        let err = extract_all(&archive, &dest, &CancelToken::new()).unwrap_err();
        match &err {
            ArchiveError::UnsafePath(message) => {
                assert!(
                    message.contains("evil.txt") || message.contains("abs.txt"),
                    "{message}"
                );
            }
            other => panic!("expected UnsafePath, got {other:?}"),
        }

        // Nothing outside the destination was created.
        assert!(!dest_root.path().join("evil.txt").exists());
        assert!(
            !dest.exists(),
            "validation happens before the destination is populated"
        );
        assert!(
            !dest.exists(),
            "the destination is never created for a refused archive"
        );
        assert!(!dest_root.path().join("evil.txt").exists());
        assert!(
            !dest_root.path().join("abs.txt").exists(),
            "an absolute member name is not relocated"
        );
        // The harmless member was not extracted either (all-or-nothing).
        assert!(!dest.join("good.txt").exists());
    }

    #[test]
    fn an_absolute_member_is_refused_on_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("absolute.zip");
        zip_with_hostile_members(&archive, &[("/abs.txt", b"pwned")]);
        let dest_root = tempfile::tempdir().unwrap();
        let dest = dest_root.path().join("sandbox");
        match extract_all(&archive, &dest, &CancelToken::new()) {
            Err(ArchiveError::UnsafePath(message)) => {
                assert!(message.contains("abs.txt"), "{message}")
            }
            other => panic!("expected UnsafePath, got {other:?}"),
        }
        assert!(!dest.exists());
        assert!(
            !dest_root.path().join("abs.txt").exists(),
            "the leading slash is not stripped and relocated"
        );
    }

    #[test]
    fn absolute_and_drive_letter_members_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("drive.zip");
        zip_with_hostile_members(&archive, &[("c:\\windows\\system32\\evil.dll", b"x")]);
        let dest = dir.path().join("out");
        let err = extract_all(&archive, &dest, &CancelToken::new()).unwrap_err();
        assert!(matches!(err, ArchiveError::UnsafePath(_)), "{err:?}");

        let archive2 = dir.path().join("unc.zip");
        zip_with_hostile_members(&archive2, &[("\\\\server\\share\\evil.txt", b"x")]);
        assert!(matches!(
            extract_all(&archive2, &dest, &CancelToken::new()),
            Err(ArchiveError::UnsafePath(_))
        ));
    }

    #[test]
    fn a_cancelled_token_stops_a_multi_member_extraction() {
        let members: Vec<(String, Vec<u8>)> = (0..8)
            .map(|i| (format!("f{i}.bin"), vec![b'x'; 4 * 1024 * 1024]))
            .collect();
        let refs: Vec<(&str, &[u8])> = members
            .iter()
            .map(|(n, d)| (n.as_str(), d.as_slice()))
            .collect();
        let fixture = fixture("cancel", &refs);

        // A token that is already set stops the run before the first write.
        let dest_root = tempfile::tempdir().unwrap();
        let early = dest_root.path().join("early");
        let cancelled = CancelToken::new();
        cancelled.cancel();
        let err = extract_all(&fixture.archive, &early, &cancelled).unwrap_err();
        assert!(err.is_cancelled(), "{err:?}");
        assert!(
            !early.exists(),
            "nothing is created when the run is cancelled from the start"
        );

        // A token set while the run is streaming also stops it, whatever the
        // member being copied at that moment.
        let dest = dest_root.path().join("late");
        let token = CancelToken::new();
        let watcher = {
            let token = token.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1));
                token.cancel();
            })
        };
        let result = extract_all(&fixture.archive, &dest, &token);
        watcher.join().unwrap();
        let err = result.expect_err("a cancellation during the run must be reported");
        assert!(err.is_cancelled(), "{err:?}");
    }

    #[test]
    fn write_side_cancellation_maps_to_the_cancelled_error() {
        let fixture = fixture("cancel-writer", &[("a.bin", &vec![b'y'; 256 * 1024])]);
        let mut archive = open(&fixture.archive).unwrap();

        let token = CancelToken::new();
        token.cancel();
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut writer = stream::CancelWriter::new(&mut sink, &token);
            let raw = archive.copy_to("a.bin", &mut writer).unwrap_err();
            // The backend only sees an interrupted write...
            assert!(matches!(raw, ArchiveError::Io { .. }), "{raw:?}");
            // ...which the extraction wrapper turns into a cancellation.
            let mapped = resolve_copy(Err(raw), &token).unwrap_err();
            assert!(mapped.is_cancelled(), "{mapped:?}");
        }
        assert!(sink.is_empty(), "nothing is written once the token is set");
    }

    #[test]
    fn resolve_copy_keeps_real_errors() {
        let token = CancelToken::new();
        assert_eq!(resolve_copy(Ok(7), &token).unwrap(), 7);
        let real = ArchiveError::Malformed("damaged".to_string());
        assert!(matches!(
            resolve_copy(Err(real), &token),
            Err(ArchiveError::Malformed(_))
        ));
    }

    #[test]
    fn tar_gz_extraction_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("payload");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("a.txt"), b"alpha").unwrap();
        fs::write(source.join("nested").join("b.txt"), b"beta").unwrap();

        let archive = dir.path().join("payload.tar.gz");
        create(&archive, &[source], &PackOptions::default()).unwrap();

        let dest = dir.path().join("out");
        let outcome = extract_all(&archive, &dest, &CancelToken::new()).unwrap();
        assert_eq!(outcome.files, 2);
        assert_eq!(fs::read(dest.join("payload/a.txt")).unwrap(), b"alpha");
        assert_eq!(
            fs::read(dest.join("payload/nested/b.txt")).unwrap(),
            b"beta"
        );
    }

    #[test]
    fn single_file_compressed_archive_extracts_one_member() {
        let dir = tempfile::tempdir().unwrap();
        let gz = dir.path().join("notes.txt.gz");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
        encoder.write_all(b"hello compressed world").unwrap();
        fs::write(&gz, encoder.finish().unwrap()).unwrap();

        let dest = dir.path().join("out");
        let outcome = extract_all(&gz, &dest, &CancelToken::new()).unwrap();
        assert_eq!(outcome.files, 1);
        assert_eq!(
            fs::read(dest.join("notes.txt")).unwrap(),
            b"hello compressed world"
        );
    }

    #[test]
    fn extracting_from_a_corrupt_archive_errors_without_panicking() {
        let fixture = fixture("corrupt", &[("a.txt", &vec![b'a'; 4096])]);
        let bytes = fs::read(&fixture.archive).unwrap();
        fs::write(&fixture.archive, &bytes[..bytes.len() / 2]).unwrap();
        let dest = fixture._dir.path().join("out");
        assert!(extract_all(&fixture.archive, &dest, &CancelToken::new()).is_err());
    }
}

//! File operations: copy, move, delete, rename, create.
//!
//! Every operation is cancellable and reports progress, so the same code can
//! drive a modal progress dialog on the desktop, a background transfer queue or
//! an MCP tool call.

use crate::error::{CoreError, CoreResult};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Shared cancellation flag.
#[derive(Clone, Debug, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> CoreResult<()> {
        if self.is_cancelled() {
            Err(CoreError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Progress of a long running operation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    /// File currently being processed.
    pub current: PathBuf,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        if self.bytes_total == 0 {
            if self.files_total == 0 {
                0.0
            } else {
                (self.files_done as f32 / self.files_total as f32).clamp(0.0, 1.0)
            }
        } else {
            (self.bytes_done as f32 / self.bytes_total as f32).clamp(0.0, 1.0)
        }
    }
}

/// What to do when the destination already exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverwriteMode {
    /// Fail with [`CoreError::Invalid`].
    #[default]
    Fail,
    /// Replace the destination.
    Overwrite,
    /// Leave the destination untouched and skip the source.
    Skip,
    /// Copy next to the existing file as `name (2).ext`.
    Rename,
    /// Append the source content to the destination file.
    Append,
}

/// Options shared by copy and move.
#[derive(Clone, Debug)]
pub struct TransferOptions {
    pub overwrite: OverwriteMode,
    /// Preserve file modification times.
    pub preserve_times: bool,
    /// Recursively copy directories.
    pub recursive: bool,
    /// Copy symbolic links as links instead of following them.
    pub keep_symlinks: bool,
    /// Copy unix permission bits.
    pub preserve_permissions: bool,
    /// Pass an overwrite decision hook (used by the UI to ask the user).
    pub confirm_overwrite: bool,
}

impl Default for TransferOptions {
    fn default() -> Self {
        TransferOptions {
            overwrite: OverwriteMode::Fail,
            preserve_times: true,
            recursive: true,
            keep_symlinks: true,
            preserve_permissions: true,
            confirm_overwrite: false,
        }
    }
}

/// Summary of a completed transfer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransferOutcome {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub skipped: u64,
    /// Destination paths that were written, in completion order.
    pub written: Vec<PathBuf>,
}

type ProgressFn<'a> = dyn FnMut(&Progress) + 'a;

/// Recursively calculate the size and number of files below `path`.
pub fn measure(path: &Path, cancel: &CancelToken) -> CoreResult<(u64, u64)> {
    let md = fs::symlink_metadata(path).map_err(|e| CoreError::io(path, e))?;
    if md.is_dir() {
        let mut bytes = 0;
        let mut files = 0;
        let rd = fs::read_dir(path).map_err(|e| CoreError::io(path, e))?;
        for item in rd {
            cancel.check()?;
            let item = match item {
                Ok(i) => i,
                Err(_) => continue,
            };
            let (b, f) = measure(&item.path(), cancel)?;
            bytes += b;
            files += f;
        }
        Ok((bytes, files))
    } else if md.file_type().is_symlink() {
        Ok((0, 1))
    } else {
        Ok((md.len(), 1))
    }
}

/// Copy a single path (file, symlink or directory tree) to `dest`.
pub fn copy(
    src: &Path,
    dest: &Path,
    opts: &TransferOptions,
    cancel: &CancelToken,
    progress: Option<&mut ProgressFn<'_>>,
) -> CoreResult<TransferOutcome> {
    let mut outcome = TransferOutcome::default();
    let mut progress = progress;
    copy_inner(src, dest, opts, cancel, &mut outcome, &mut progress)?;
    Ok(outcome)
}

fn copy_inner(
    src: &Path,
    dest: &Path,
    opts: &TransferOptions,
    cancel: &CancelToken,
    outcome: &mut TransferOutcome,
    progress: &mut Option<&mut ProgressFn<'_>>,
) -> CoreResult<()> {
    cancel.check()?;
    let md = fs::symlink_metadata(src).map_err(|e| CoreError::io(src, e))?;
    let ft = md.file_type();

    if ft.is_symlink() && opts.keep_symlinks {
        if dest.exists() {
            match opts.overwrite {
                OverwriteMode::Skip => {
                    outcome.skipped += 1;
                    return Ok(());
                }
                OverwriteMode::Fail => return Err(exists_error(dest)),
                OverwriteMode::Rename => {
                    let renamed = unique_name(dest);
                    return symlink_copy(src, &renamed, outcome, progress);
                }
                OverwriteMode::Overwrite | OverwriteMode::Append => {
                    remove_path_if_exists(dest)?;
                }
            }
        }
        return symlink_copy(src, dest, outcome, progress);
    }

    if ft.is_dir() {
        if !opts.recursive {
            return Err(CoreError::Invalid(format!(
                "{} is a directory",
                src.display()
            )));
        }
        if !dest.exists() {
            fs::create_dir_all(dest).map_err(|e| CoreError::io(dest, e))?;
            outcome.dirs += 1;
        }
        for item in fs::read_dir(src).map_err(|e| CoreError::io(src, e))? {
            cancel.check()?;
            let item = match item {
                Ok(i) => i,
                Err(_) => continue,
            };
            let name = item.file_name();
            let child_dest = dest.join(&name);
            copy_inner(&item.path(), &child_dest, opts, cancel, outcome, progress)?;
        }
        return Ok(());
    }

    // Regular file (or a special file which we refuse to copy).
    if !ft.is_file() {
        return Err(CoreError::Unsupported(format!(
            "cannot copy special file {}",
            src.display()
        )));
    }
    let mut dest_path = dest.to_path_buf();
    let mut mode = opts.overwrite;
    if dest_path.exists() {
        match mode {
            OverwriteMode::Skip => {
                outcome.skipped += 1;
                return Ok(());
            }
            OverwriteMode::Fail => return Err(exists_error(&dest_path)),
            OverwriteMode::Rename => {
                dest_path = unique_name(&dest_path);
                mode = OverwriteMode::Fail;
            }
            OverwriteMode::Overwrite | OverwriteMode::Append => {}
        }
    }
    if let Some(parent) = dest_path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    }
    let copied = if mode == OverwriteMode::Append {
        append_file(src, &dest_path, cancel, progress, outcome)?
    } else {
        copy_file(src, &dest_path, cancel, progress, outcome)?
    };
    if opts.preserve_times
        && let Ok(md) = fs::metadata(src)
        && let Ok(mtime) = md.modified()
    {
        let _ = set_mtime(&dest_path, mtime);
    }
    if opts.preserve_permissions
        && let Ok(md) = fs::metadata(src)
    {
        let _ = fs::set_permissions(&dest_path, md.permissions());
    }
    outcome.bytes += copied;
    outcome.files += 1;
    outcome.written.push(dest_path);
    Ok(())
}

fn copy_file(
    src: &Path,
    dest: &Path,
    cancel: &CancelToken,
    progress: &mut Option<&mut ProgressFn<'_>>,
    outcome: &mut TransferOutcome,
) -> CoreResult<u64> {
    let mut input = fs::File::open(src).map_err(|e| CoreError::io(src, e))?;
    let mut output = fs::File::create(dest).map_err(|e| CoreError::io(dest, e))?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut written = 0u64;
    loop {
        cancel.check()?;
        let n = input.read(&mut buf).map_err(|e| CoreError::io(src, e))?;
        if n == 0 {
            break;
        }
        output
            .write_all(&buf[..n])
            .map_err(|e| CoreError::io(dest, e))?;
        written += n as u64;
        outcome.bytes += n as u64;
        if let Some(cb) = progress {
            cb(&Progress {
                bytes_done: written,
                current: src.to_path_buf(),
                ..Progress::default()
            });
        }
    }
    output.flush().ok();
    Ok(written)
}

fn append_file(
    src: &Path,
    dest: &Path,
    cancel: &CancelToken,
    progress: &mut Option<&mut ProgressFn<'_>>,
    _outcome: &mut TransferOutcome,
) -> CoreResult<u64> {
    let mut input = fs::File::open(src).map_err(|e| CoreError::io(src, e))?;
    let mut output = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(dest)
        .map_err(|e| CoreError::io(dest, e))?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut written = 0u64;
    loop {
        cancel.check()?;
        let n = input.read(&mut buf).map_err(|e| CoreError::io(src, e))?;
        if n == 0 {
            break;
        }
        output
            .write_all(&buf[..n])
            .map_err(|e| CoreError::io(dest, e))?;
        written += n as u64;
        if let Some(cb) = progress {
            cb(&Progress {
                bytes_done: written,
                current: src.to_path_buf(),
                ..Progress::default()
            });
        }
    }
    Ok(written)
}

fn symlink_copy(
    src: &Path,
    dest: &Path,
    outcome: &mut TransferOutcome,
    progress: &mut Option<&mut ProgressFn<'_>>,
) -> CoreResult<()> {
    let target = fs::read_link(src).map_err(|e| CoreError::io(src, e))?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, dest).map_err(|e| CoreError::io(dest, e))?;
    }
    #[cfg(windows)]
    {
        let md = fs::metadata(src).map_err(|e| CoreError::io(src, e))?;
        if md.is_dir() {
            std::os::windows::fs::symlink_dir(&target, dest).map_err(|e| CoreError::io(dest, e))?;
        } else {
            std::os::windows::fs::symlink_file(&target, dest)
                .map_err(|e| CoreError::io(dest, e))?;
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        return Err(CoreError::Unsupported("symbolic links".to_string()));
    }
    outcome.files += 1;
    outcome.written.push(dest.to_path_buf());
    if let Some(cb) = progress {
        cb(&Progress {
            current: dest.to_path_buf(),
            ..Progress::default()
        });
    }
    Ok(())
}

fn exists_error(dest: &Path) -> CoreError {
    CoreError::io(
        dest,
        std::io::Error::new(std::io::ErrorKind::AlreadyExists, "destination exists"),
    )
}

fn remove_path_if_exists(path: &Path) -> CoreResult<()> {
    if !path.exists() && fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    let md = fs::symlink_metadata(path).map_err(|e| CoreError::io(path, e))?;
    if md.is_dir() {
        fs::remove_dir_all(path).map_err(|e| CoreError::io(path, e))
    } else {
        fs::remove_file(path).map_err(|e| CoreError::io(path, e))
    }
}

/// Produce a non-existing variant of `path` by inserting ` (n)` before the
/// extension.
pub fn unique_name(path: &Path) -> PathBuf {
    let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().to_string());
    for n in 2..10_000 {
        let name = match &ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    let fallback = format!("{stem} (copy)");
    match &ext {
        Some(e) => parent.join(format!("{fallback}.{e}")),
        None => parent.join(fallback),
    }
}

/// Move (rename) `src` to `dest`, falling back to copy+delete across devices.
pub fn move_path(
    src: &Path,
    dest: &Path,
    opts: &TransferOptions,
    cancel: &CancelToken,
) -> CoreResult<TransferOutcome> {
    cancel.check()?;
    let target = if dest.exists() {
        match opts.overwrite {
            OverwriteMode::Fail => return Err(exists_error(dest)),
            OverwriteMode::Skip => {
                return Ok(TransferOutcome {
                    skipped: 1,
                    ..Default::default()
                });
            }
            OverwriteMode::Rename => unique_name(dest),
            OverwriteMode::Overwrite | OverwriteMode::Append => {
                remove_path_if_exists(dest)?;
                dest.to_path_buf()
            }
        }
    } else {
        dest.to_path_buf()
    };
    match fs::rename(src, &target) {
        Ok(()) => {
            let (bytes, files) = measure(&target, cancel).unwrap_or((0, 0));
            Ok(TransferOutcome {
                files,
                dirs: 0,
                bytes,
                skipped: 0,
                written: vec![target],
            })
        }
        Err(_) => {
            // Cross-device move: copy then remove the source.
            let mut outcome = copy(src, &target, opts, cancel, None)?;
            remove(src, cancel)?;
            outcome.files = outcome.files.max(1);
            Ok(outcome)
        }
    }
}

/// Rename inside the same directory (or to a new path), without moving data.
pub fn rename(from: &Path, to: &Path) -> CoreResult<()> {
    if to.exists() {
        return Err(exists_error(to));
    }
    fs::rename(from, to).map_err(|e| CoreError::io(from, e))
}

/// Delete a file or a whole directory tree.
pub fn remove(path: &Path, cancel: &CancelToken) -> CoreResult<u64> {
    cancel.check()?;
    let md = fs::symlink_metadata(path).map_err(|e| CoreError::io(path, e))?;
    if md.is_dir() {
        let mut removed = 0u64;
        for item in fs::read_dir(path).map_err(|e| CoreError::io(path, e))? {
            cancel.check()?;
            let item = match item {
                Ok(i) => i,
                Err(_) => continue,
            };
            removed += remove(&item.path(), cancel)?;
        }
        // Make sure a read-only directory can be removed.
        let perms = fs::metadata(path)
            .map_err(|e| CoreError::io(path, e))?
            .permissions();
        if perms.readonly() {
            let _ = make_writable(path);
        }
        fs::remove_dir(path).map_err(|e| CoreError::io(path, e))?;
        Ok(removed + 1)
    } else {
        let perms = md.permissions();
        if perms.readonly() {
            let _ = make_writable(path);
        }
        fs::remove_file(path).map_err(|e| CoreError::io(path, e))?;
        Ok(1)
    }
}

/// Create a directory, including missing parents.
pub fn create_dir(path: &Path) -> CoreResult<()> {
    if path.exists() {
        return Err(exists_error(path));
    }
    fs::create_dir_all(path).map_err(|e| CoreError::io(path, e))
}

/// Create an empty file.
pub fn create_file(path: &Path) -> CoreResult<()> {
    if path.exists() {
        return Err(exists_error(path));
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    }
    fs::File::create(path).map_err(|e| CoreError::io(path, e))?;
    Ok(())
}

/// Clear the read-only flag of `path` so that it can be deleted.
fn make_writable(path: &Path) -> std::io::Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(path, perms)
}

fn set_mtime(path: &Path, mtime: std::time::SystemTime) -> std::io::Result<()> {
    let f = fs::OpenOptions::new().write(true).open(path)?;
    f.set_modified(mtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, content: &[u8]) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(content).unwrap();
    }

    #[test]
    fn copies_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("out/b.txt");
        write(&src, b"hello");
        let out = copy(
            &src,
            &dst,
            &TransferOptions::default(),
            &CancelToken::new(),
            None,
        )
        .unwrap();
        assert_eq!(out.files, 1);
        assert_eq!(fs::read(&dst).unwrap(), b"hello");
        assert!(src.exists(), "copy keeps the source");
    }

    #[test]
    fn refuses_to_overwrite_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("b.txt");
        write(&src, b"new");
        write(&dst, b"old");
        let err = copy(
            &src,
            &dst,
            &TransferOptions::default(),
            &CancelToken::new(),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::Io { .. }));
        assert_eq!(fs::read(&dst).unwrap(), b"old");
    }

    #[test]
    fn overwrite_skip_and_rename_modes_behave() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("b.txt");
        write(&src, b"new");
        write(&dst, b"old");

        let overwrite = TransferOptions {
            overwrite: OverwriteMode::Overwrite,
            ..Default::default()
        };
        copy(&src, &dst, &overwrite, &CancelToken::new(), None).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"new");

        write(&dst, b"old");
        let skip = TransferOptions {
            overwrite: OverwriteMode::Skip,
            ..Default::default()
        };
        let out = copy(&src, &dst, &skip, &CancelToken::new(), None).unwrap();
        assert_eq!(out.skipped, 1);
        assert_eq!(fs::read(&dst).unwrap(), b"old");

        let rename = TransferOptions {
            overwrite: OverwriteMode::Rename,
            ..Default::default()
        };
        write(&dst, b"old");
        let out = copy(&src, &dst, &rename, &CancelToken::new(), None).unwrap();
        assert_eq!(out.written[0].file_name().unwrap(), "b (2).txt");
        assert_eq!(fs::read(&out.written[0]).unwrap(), b"new");
    }

    #[test]
    fn append_mode_concatenates() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("b.txt");
        write(&src, b"B");
        write(&dst, b"A");
        let opts = TransferOptions {
            overwrite: OverwriteMode::Append,
            ..Default::default()
        };
        copy(&src, &dst, &opts, &CancelToken::new(), None).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"AB");
    }

    #[test]
    fn copies_directory_trees_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("tree");
        write(&src.join("a.txt"), b"a");
        write(&src.join("sub/b.txt"), b"bb");
        let dst = dir.path().join("copy");
        let out = copy(
            &src,
            &dst,
            &TransferOptions::default(),
            &CancelToken::new(),
            None,
        )
        .unwrap();
        assert_eq!(out.files, 2);
        assert!(out.dirs >= 1);
        assert_eq!(fs::read(dst.join("sub/b.txt")).unwrap(), b"bb");
    }

    #[test]
    fn non_recursive_copy_of_a_directory_fails() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("tree");
        fs::create_dir_all(&src).unwrap();
        let opts = TransferOptions {
            recursive: false,
            ..Default::default()
        };
        assert!(
            copy(
                &src,
                &dir.path().join("x"),
                &opts,
                &CancelToken::new(),
                None
            )
            .is_err()
        );
    }

    #[test]
    fn cancellation_stops_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        write(&src, &vec![0u8; 1024]);
        let token = CancelToken::new();
        token.cancel();
        let err = copy(
            &src,
            &dir.path().join("b.txt"),
            &TransferOptions::default(),
            &token,
            None,
        )
        .unwrap_err();
        assert!(err.is_cancelled());
    }

    #[test]
    fn moves_files_and_directories() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        write(&src, b"data");
        let dst = dir.path().join("sub/b.txt");
        move_path(&src, &dst, &TransferOptions::default(), &CancelToken::new()).unwrap();
        assert!(!src.exists());
        assert_eq!(fs::read(&dst).unwrap(), b"data");
    }

    #[test]
    fn rename_and_create_helpers_check_existence() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        write(&a, b"x");
        let b = dir.path().join("b.txt");
        rename(&a, &b).unwrap();
        assert!(b.exists());
        assert!(rename(&b, &b).is_err());
        let nd = dir.path().join("newdir");
        create_dir(&nd).unwrap();
        assert!(nd.is_dir());
        assert!(create_dir(&nd).is_err());
        let nf = dir.path().join("new.txt");
        create_file(&nf).unwrap();
        assert!(nf.exists());
        assert_eq!(fs::metadata(&nf).unwrap().len(), 0);
        assert!(create_file(&nf).is_err());
    }

    #[test]
    fn removes_trees_and_counts_entries() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("t");
        write(&tree.join("a"), b"1");
        write(&tree.join("s/b"), b"2");
        let removed = remove(&tree, &CancelToken::new()).unwrap();
        assert!(removed >= 3);
        assert!(!tree.exists());
    }

    #[test]
    fn measure_counts_bytes_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("t");
        write(&tree.join("a"), b"12345");
        write(&tree.join("s/b"), b"123");
        let (bytes, files) = measure(&tree, &CancelToken::new()).unwrap();
        assert_eq!(bytes, 8);
        assert_eq!(files, 2);
    }

    #[test]
    fn unique_name_numbers_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.txt");
        assert_eq!(unique_name(&f).file_name().unwrap(), "x (2).txt");
        write(&f, b"");
        write(&dir.path().join("x (2).txt"), b"");
        assert_eq!(unique_name(&f).file_name().unwrap(), "x (3).txt");
        let noext = dir.path().join("plain");
        write(&noext, b"");
        assert_eq!(unique_name(&noext).file_name().unwrap(), "plain (2)");
    }

    #[test]
    fn progress_reports_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("big.bin");
        write(&src, &vec![7u8; 700_000]);
        let mut seen = 0u64;
        {
            let mut cb = |p: &Progress| seen = seen.max(p.bytes_done);
            copy(
                &src,
                &dir.path().join("out.bin"),
                &TransferOptions::default(),
                &CancelToken::new(),
                Some(&mut cb),
            )
            .unwrap();
        }
        assert_eq!(seen, 700_000);
        assert!(
            (Progress {
                bytes_done: 5,
                bytes_total: 10,
                ..Default::default()
            }
            .fraction()
                - 0.5)
                .abs()
                < 1e-6
        );
        assert_eq!(Progress::default().fraction(), 0.0);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_preserved_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.txt");
        write(&target, b"t");
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink("target.txt", &link).unwrap();
        let dst = dir.path().join("out");
        copy(
            &link,
            &dst,
            &TransferOptions::default(),
            &CancelToken::new(),
            None,
        )
        .unwrap();
        assert!(fs::symlink_metadata(&dst).unwrap().file_type().is_symlink());
    }
}

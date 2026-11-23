//! Recursive transfers, implemented once on top of [`RemoteFs`].
//!
//! Both directions are generic over the trait, so FTP, FTPS and SFTP share the
//! traversal order, the cancellation granularity (once per directory and once
//! per file, plus inside every chunk of a file) and the safety rules:
//!
//! * a name coming from the server is never used verbatim as a local file name;
//!   `.`, `..`, absolute paths and anything containing a separator or NUL are
//!   rejected. A hostile server must not be able to write outside `local_dir`;
//! * symlinks are followed on download (the server resolves them) and skipped on
//!   upload, so a local symlink loop cannot hang the transfer;
//! * `mkdir` failures on upload are tolerated (most servers answer `550` for a
//!   directory that already exists); the following file upload then reports the
//!   real problem.

use std::fs;
use std::path::{Path, PathBuf};

use rc_core::ops::CancelToken;

use crate::error::RemoteError;
use crate::remote_fs::{RemoteFs, TransferStats};

/// Reject server-supplied names that would escape the target directory.
fn safe_component(name: &str) -> Option<&str> {
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return None;
    }
    Some(name)
}

/// Join a remote directory and a name with the protocol's separator.
fn join_remote(dir: &str, name: &str) -> String {
    if dir.is_empty() || dir == "." {
        return name.to_string();
    }
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Recursive download used by [`RemoteFs::get_tree`].
pub(crate) fn get_tree<S: RemoteFs + ?Sized>(
    fs: &mut S,
    remote: &str,
    local_dir: &Path,
    cancel: &CancelToken,
) -> Result<TransferStats, RemoteError> {
    check(cancel)?;
    let mut stats = TransferStats::default();
    match fs.list(Some(remote)) {
        Ok(entries) => {
            fs::create_dir_all(local_dir).map_err(RemoteError::Io)?;
            download_entries(fs, remote, local_dir, cancel, &mut stats, entries)?;
            Ok(stats)
        }
        Err(RemoteError::NotFound(_)) => {
            // Not a directory (or an empty path): treat it as a single file.
            let name = crate::parse::base_name(remote);
            let Some(file_name) = safe_component(&name) else {
                return Err(RemoteError::Protocol(format!(
                    "refusing to write the remote name {name:?}"
                )));
            };
            fs::create_dir_all(local_dir).map_err(RemoteError::Io)?;
            let target = local_dir.join(file_name);
            let bytes = fs.get(remote, &target, cancel)?;
            stats.files = 1;
            stats.bytes = bytes;
            Ok(stats)
        }
        Err(other) => Err(other),
    }
}

/// Download the children of one already-listed directory.
fn download_entries<S: RemoteFs + ?Sized>(
    fs: &mut S,
    remote_dir: &str,
    local_dir: &Path,
    cancel: &CancelToken,
    stats: &mut TransferStats,
    entries: Vec<crate::entry::RemoteEntry>,
) -> Result<(), RemoteError> {
    for entry in entries {
        check(cancel)?;
        let Some(name) = safe_component(&entry.name) else {
            continue;
        };
        let remote_child = join_remote(remote_dir, name);
        let local_child = local_dir.join(name);
        if entry.is_dir {
            fs::create_dir_all(&local_child).map_err(RemoteError::Io)?;
            stats.dirs += 1;
            let children = fs.list(Some(&remote_child))?;
            download_entries(fs, &remote_child, &local_child, cancel, stats, children)?;
        } else {
            let bytes = fs.get(&remote_child, &local_child, cancel)?;
            stats.files += 1;
            stats.bytes += bytes;
        }
    }
    Ok(())
}

/// Recursive upload used by [`RemoteFs::put_tree`].
pub(crate) fn put_tree<S: RemoteFs + ?Sized>(
    fs: &mut S,
    local: &Path,
    remote_dir: &str,
    cancel: &CancelToken,
) -> Result<TransferStats, RemoteError> {
    check(cancel)?;
    let mut stats = TransferStats::default();
    let metadata = fs::symlink_metadata(local).map_err(RemoteError::Io)?;

    if !metadata.is_dir() {
        let name = local
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| {
                RemoteError::Protocol(format!(
                    "cannot derive a remote name from {}",
                    local.display()
                ))
            })?;
        let bytes = fs.put(local, &join_remote(remote_dir, &name), cancel)?;
        stats.files = 1;
        stats.bytes = bytes;
        return Ok(stats);
    }

    // The target directory itself may already exist; a failure here is only
    // fatal if the files inside cannot be written either.
    let _ = fs.mkdir(remote_dir);
    upload_entries(fs, local, remote_dir, cancel, &mut stats)?;
    Ok(stats)
}

/// Upload the children of one local directory.
fn upload_entries<S: RemoteFs + ?Sized>(
    fs: &mut S,
    local_dir: &Path,
    remote_dir: &str,
    cancel: &CancelToken,
    stats: &mut TransferStats,
) -> Result<(), RemoteError> {
    let read_dir = fs::read_dir(local_dir).map_err(RemoteError::Io)?;
    for item in read_dir {
        check(cancel)?;
        let item = item.map_err(RemoteError::Io)?;
        let name = item.file_name().to_string_lossy().to_string();
        let metadata = item.metadata().map_err(RemoteError::Io)?;
        let path: PathBuf = item.path();
        let remote_child = join_remote(remote_dir, &name);
        if metadata.is_dir() {
            let _ = fs.mkdir(&remote_child);
            stats.dirs += 1;
            upload_entries(fs, &path, &remote_child, cancel, stats)?;
        } else if metadata.is_file() {
            let bytes = fs.put(&path, &remote_child, cancel)?;
            stats.files += 1;
            stats.bytes += bytes;
        } else {
            // Sockets, devices and symlinks are skipped: following a link can
            // leave the tree, or form a loop.
            continue;
        }
    }
    Ok(())
}

/// Turn a cancelled token into the protocol-independent error.
fn check(cancel: &CancelToken) -> Result<(), RemoteError> {
    if cancel.is_cancelled() {
        Err(RemoteError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::RemoteEntry;
    use std::collections::BTreeMap;

    /// An in-memory [`RemoteFs`] that records what it was asked to do.
    #[derive(Default)]
    struct FakeFs {
        dirs: BTreeMap<String, Vec<RemoteEntry>>,
        files: BTreeMap<String, Vec<u8>>,
        puts: Vec<String>,
        gets: Vec<String>,
        mkdirs: Vec<String>,
    }

    impl FakeFs {
        fn with_file(mut self, path: &str, data: &[u8]) -> Self {
            self.files.insert(path.to_string(), data.to_vec());
            self
        }

        fn with_dir(mut self, path: &str, children: Vec<RemoteEntry>) -> Self {
            self.dirs.insert(path.to_string(), children);
            self
        }
    }

    fn child(name: &str, is_dir: bool) -> RemoteEntry {
        RemoteEntry {
            name: name.to_string(),
            is_dir,
            ..RemoteEntry::default()
        }
    }

    impl RemoteFs for FakeFs {
        fn protocol(&self) -> crate::Protocol {
            crate::Protocol::Ftp
        }
        fn pwd(&mut self) -> Result<String, RemoteError> {
            Ok("/".into())
        }
        fn cwd(&mut self, path: &str) -> Result<String, RemoteError> {
            Ok(path.into())
        }
        fn list(&mut self, path: Option<&str>) -> Result<Vec<RemoteEntry>, RemoteError> {
            let path = path.unwrap_or("/");
            match self.dirs.get(path) {
                Some(children) => Ok(children.clone()),
                None => Err(RemoteError::NotFound(path.to_string())),
            }
        }
        fn stat(&mut self, path: &str) -> Result<RemoteEntry, RemoteError> {
            if self.files.contains_key(path) {
                Ok(RemoteEntry {
                    name: crate::parse::base_name(path),
                    size: self.files[path].len() as u64,
                    ..RemoteEntry::default()
                })
            } else if self.dirs.contains_key(path) {
                Ok(RemoteEntry {
                    name: crate::parse::base_name(path),
                    is_dir: true,
                    ..RemoteEntry::default()
                })
            } else {
                Err(RemoteError::NotFound(path.to_string()))
            }
        }
        fn mkdir(&mut self, path: &str) -> Result<(), RemoteError> {
            self.mkdirs.push(path.to_string());
            Ok(())
        }
        fn rmdir(&mut self, _path: &str) -> Result<(), RemoteError> {
            Ok(())
        }
        fn remove_file(&mut self, _path: &str) -> Result<(), RemoteError> {
            Ok(())
        }
        fn rename(&mut self, _from: &str, _to: &str) -> Result<(), RemoteError> {
            Ok(())
        }
        fn size(&mut self, path: &str) -> Result<u64, RemoteError> {
            Ok(self.files.get(path).map(|d| d.len() as u64).unwrap_or(0))
        }
        fn mdtm(&mut self, _path: &str) -> Result<Option<i64>, RemoteError> {
            Ok(None)
        }
        fn get(
            &mut self,
            remote: &str,
            local: &Path,
            _cancel: &CancelToken,
        ) -> Result<u64, RemoteError> {
            let data = self
                .files
                .get(remote)
                .cloned()
                .ok_or_else(|| RemoteError::NotFound(remote.to_string()))?;
            self.gets.push(remote.to_string());
            if let Some(parent) = local.parent() {
                fs::create_dir_all(parent).map_err(RemoteError::Io)?;
            }
            fs::write(local, &data).map_err(RemoteError::Io)?;
            Ok(data.len() as u64)
        }
        fn put(
            &mut self,
            local: &Path,
            remote: &str,
            _cancel: &CancelToken,
        ) -> Result<u64, RemoteError> {
            let data = fs::read(local).map_err(RemoteError::Io)?;
            self.puts.push(remote.to_string());
            self.files.insert(remote.to_string(), data.clone());
            Ok(data.len() as u64)
        }
        fn quit(&mut self) -> Result<(), RemoteError> {
            Ok(())
        }
    }

    #[test]
    fn get_tree_walks_two_levels_and_creates_directories() {
        let mut fs = FakeFs::default()
            .with_dir("/pub", vec![child("a.txt", false), child("sub", true)])
            .with_dir(
                "/pub/sub",
                vec![child("b.txt", false), child(".", true), child("..", true)],
            )
            .with_file("/pub/a.txt", b"aaaa")
            .with_file("/pub/sub/b.txt", b"bbbbbb");
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("copy");
        let cancel = CancelToken::new();
        let stats = fs.get_tree("/pub", &target, &cancel).unwrap();
        assert_eq!(stats.files, 2);
        assert_eq!(stats.bytes, 10);
        assert_eq!(stats.dirs, 1);
        assert_eq!(fs::read(target.join("a.txt")).unwrap(), b"aaaa");
        assert_eq!(fs::read(target.join("sub/b.txt")).unwrap(), b"bbbbbb");
        assert_eq!(
            fs.gets,
            vec!["/pub/a.txt".to_string(), "/pub/sub/b.txt".to_string()]
        );
    }

    #[test]
    fn put_tree_walks_two_levels_and_creates_directories() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("src");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("a.txt"), b"12345").unwrap();
        fs::write(root.join("nested/b.txt"), b"123").unwrap();
        let mut fs = FakeFs::default();
        let cancel = CancelToken::new();
        let stats = fs.put_tree(&root, "/upload", &cancel).unwrap();
        assert_eq!(stats.files, 2);
        assert_eq!(stats.bytes, 8);
        assert_eq!(stats.dirs, 1);
        assert!(fs.puts.contains(&"/upload/a.txt".to_string()));
        assert!(fs.puts.contains(&"/upload/nested/b.txt".to_string()));
        assert!(fs.mkdirs.contains(&"/upload".to_string()));
        assert!(fs.mkdirs.contains(&"/upload/nested".to_string()));
    }

    #[test]
    fn a_single_file_argument_is_uploaded_into_the_target_directory() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("single.bin");
        fs::write(&file, b"xyz").unwrap();
        let mut fs = FakeFs::default();
        let stats = fs
            .put_tree(&file, "/incoming", &CancelToken::new())
            .unwrap();
        assert_eq!(stats.files, 1);
        assert_eq!(stats.bytes, 3);
        assert_eq!(fs.puts, vec!["/incoming/single.bin".to_string()]);
    }

    #[test]
    fn a_file_url_downloads_into_the_target_directory() {
        let mut fs = FakeFs::default().with_file("/pub/one.txt", b"abc");
        let temp = tempfile::tempdir().unwrap();
        let stats = fs
            .get_tree("/pub/one.txt", temp.path(), &CancelToken::new())
            .unwrap();
        assert_eq!(stats.files, 1);
        assert_eq!(stats.bytes, 3);
        assert_eq!(fs::read(temp.path().join("one.txt")).unwrap(), b"abc");
    }

    #[test]
    fn hostile_names_never_escape_the_target_directory() {
        let mut fs = FakeFs::default()
            .with_dir(
                "/pub",
                vec![
                    child("../escape", false),
                    child("/abs", false),
                    child("ok.txt", false),
                    child("a/b", false),
                    child("", false),
                ],
            )
            .with_file("/pub/ok.txt", b"1")
            .with_file("/pub/../escape", b"x")
            .with_file("/pub//abs", b"x")
            .with_file("/pub/a/b", b"x");
        let temp = tempfile::tempdir().unwrap();
        let stats = fs
            .get_tree("/pub", temp.path(), &CancelToken::new())
            .unwrap();
        assert_eq!(stats.files, 1, "only the safe name may be written");
        assert_eq!(fs.gets, vec!["/pub/ok.txt".to_string()]);
        assert!(temp.path().join("ok.txt").exists());
        assert!(!temp.path().parent().unwrap().join("escape").exists());
    }

    #[test]
    fn a_cancelled_token_stops_before_the_first_byte() {
        let mut fs = FakeFs::default()
            .with_dir("/pub", vec![child("a.txt", false)])
            .with_file("/pub/a.txt", b"aaaa");
        let temp = tempfile::tempdir().unwrap();
        let cancel = CancelToken::new();
        cancel.cancel();
        assert!(matches!(
            fs.get_tree("/pub", temp.path(), &cancel),
            Err(RemoteError::Cancelled)
        ));
        assert!(fs.gets.is_empty());
        assert!(matches!(
            fs.put_tree(temp.path(), "/pub", &cancel),
            Err(RemoteError::Cancelled)
        ));
    }

    #[test]
    fn safe_component_rejects_traversal_and_accepts_ordinary_names() {
        assert_eq!(safe_component("file.txt"), Some("file.txt"));
        assert_eq!(safe_component(".hidden"), Some(".hidden"));
        assert_eq!(safe_component("with space"), Some("with space"));
        for bad in ["", ".", "..", "a/b", "a\\b", "a\0b"] {
            assert_eq!(safe_component(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn remote_paths_join_with_single_slashes() {
        assert_eq!(join_remote("/pub", "a"), "/pub/a");
        assert_eq!(join_remote("/pub/", "a"), "/pub/a");
        assert_eq!(join_remote("", "a"), "a");
        assert_eq!(join_remote(".", "a"), "a");
    }
}

//! Where a panel currently is: a local directory, a directory inside an
//! archive, or a directory on a remote server.

use crate::error::{CoreError, CoreResult};
use std::path::{Path, PathBuf};

/// A panel location.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Location {
    /// A plain file system directory.
    Local(PathBuf),
    /// A directory inside an archive file (`container` + `/`-separated `inner`).
    Archive { container: PathBuf, inner: String },
    /// A directory on a network service.
    Remote {
        scheme: String,
        host: String,
        user: Option<String>,
        path: String,
    },
    /// A Git repository: the working tree (`rev` empty) or a revision, with a
    /// `/`-separated `path` inside it.
    Git {
        root: PathBuf,
        rev: String,
        path: String,
    },
}

impl Location {
    /// A local directory location.
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Location::Local(normalize_path(&path.into()))
    }

    /// An archive location; `inner` may be empty for the archive root.
    pub fn archive(container: impl Into<PathBuf>, inner: impl Into<String>) -> Self {
        Location::Archive {
            container: container.into(),
            inner: normalize_inner(&inner.into()),
        }
    }

    /// A remote location such as `ftp://user@host/pub`.
    pub fn remote(
        scheme: impl Into<String>,
        host: impl Into<String>,
        user: Option<String>,
        path: impl Into<String>,
    ) -> Self {
        let inner = normalize_inner(&path.into());
        let path = if inner.is_empty() {
            "/".to_string()
        } else {
            format!("/{inner}")
        };
        Location::Remote {
            scheme: scheme.into(),
            host: host.into(),
            user,
            path,
        }
    }

    /// A Git repository view: `rev` is empty for the working tree, `path` is the
    /// directory inside it.
    pub fn git(root: impl Into<PathBuf>, rev: impl Into<String>, path: impl Into<String>) -> Self {
        Location::Git {
            root: root.into(),
            rev: rev.into(),
            path: normalize_inner(&path.into()),
        }
    }

    /// Git root, revision key and inner path.
    pub fn git_parts(&self) -> Option<(&Path, &str, &str)> {
        match self {
            Location::Git { root, rev, path } => {
                Some((root.as_path(), rev.as_str(), path.as_str()))
            }
            _ => None,
        }
    }

    pub fn is_git(&self) -> bool {
        matches!(self, Location::Git { .. })
    }

    /// True when this is a Git *working tree* view (not a revision).
    pub fn is_git_worktree(&self) -> bool {
        matches!(self, Location::Git { rev, .. } if rev.is_empty())
    }

    /// Parse user input: an `ftp://`, `ftps://`, `sftp://` or `zip://` URL, or a
    /// local path.
    pub fn parse(input: &str) -> CoreResult<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(CoreError::Location(String::new()));
        }
        if let Some((scheme, rest)) = trimmed.split_once("://") {
            let scheme_lower = scheme.to_ascii_lowercase();
            if scheme_lower == "zip" {
                // zip:///path/archive.zip/sub/dir
                let (container, inner) = split_archive_path(rest)?;
                return Ok(Location::archive(container, inner));
            }
            if matches!(
                scheme_lower.as_str(),
                "ftp" | "ftps" | "sftp" | "sftp4" | "sftp5" | "smb" | "webdav"
            ) {
                let (authority, path) = match rest.find('/') {
                    Some(i) => (&rest[..i], &rest[i..]),
                    None => (rest, "/"),
                };
                let (user, host) = match authority.rsplit_once('@') {
                    Some((u, h)) => (Some(u.to_string()), h.to_string()),
                    None => (None, authority.to_string()),
                };
                if host.is_empty() {
                    return Err(CoreError::Location(trimmed.to_string()));
                }
                return Ok(Location::remote(scheme_lower, host, user, path));
            }
            return Err(CoreError::Location(trimmed.to_string()));
        }
        Ok(Location::local(PathBuf::from(trimmed)))
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Location::Local(_))
    }

    pub fn is_archive(&self) -> bool {
        matches!(self, Location::Archive { .. })
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Location::Remote { .. })
    }

    /// The underlying path for local locations.
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Location::Local(p) => Some(p.as_path()),
            _ => None,
        }
    }

    /// The repository root of a Git location, even inside a revision.
    pub fn git_root(&self) -> Option<&Path> {
        match self {
            Location::Git { root, .. } => Some(root.as_path()),
            _ => None,
        }
    }

    /// Archive container and inner path.
    pub fn archive_parts(&self) -> Option<(&Path, &str)> {
        match self {
            Location::Archive { container, inner } => Some((container.as_path(), inner.as_str())),
            _ => None,
        }
    }

    /// Text shown in the panel's path bar, always using `/` separators.
    pub fn display(&self) -> String {
        match self {
            Location::Local(p) => {
                let s = p.to_string_lossy().replace('\\', "/");
                if s.is_empty() { "/".to_string() } else { s }
            }
            Location::Archive { container, inner } => {
                let c = container.to_string_lossy().replace('\\', "/");
                if inner.is_empty() {
                    format!("{c}/")
                } else {
                    format!("{c}/{inner}")
                }
            }
            Location::Remote {
                scheme,
                host,
                user,
                path,
            } => {
                let path = if path.is_empty() { "/" } else { path.as_str() };
                match user {
                    Some(u) => format!("{scheme}://{u}@{host}{path}"),
                    None => format!("{scheme}://{host}{path}"),
                }
            }
            Location::Git { root, rev, path } => {
                let root = root.to_string_lossy().replace('\\', "/");
                let rev = if rev.is_empty() {
                    String::new()
                } else {
                    format!(" [{rev}]")
                };
                if path.is_empty() {
                    format!("{root}{rev}")
                } else {
                    format!("{root}{rev}/{path}")
                }
            }
        }
    }

    /// The directory one level up, or `None` for a root directory.
    pub fn parent(&self) -> Option<Location> {
        match self {
            Location::Local(p) => p.parent().map(|par| {
                if par.as_os_str().is_empty() {
                    Location::local(PathBuf::from("/"))
                } else {
                    Location::local(par.to_path_buf())
                }
            }),
            Location::Archive { container, inner } => {
                if inner.is_empty() {
                    container.parent().map(|p| Location::local(p.to_path_buf()))
                } else {
                    let cut = inner.rfind('/').map(|i| &inner[..i]).unwrap_or("");
                    Some(Location::archive(container.clone(), cut.to_string()))
                }
            }
            Location::Remote {
                scheme,
                host,
                user,
                path,
            } => {
                let trimmed = path.trim_end_matches('/');
                if trimmed.is_empty() {
                    return None;
                }
                let cut = trimmed.rfind('/').map(|i| &trimmed[..i]).unwrap_or("");
                let cut = if cut.is_empty() { "/" } else { cut };
                Some(Location::remote(
                    scheme.clone(),
                    host.clone(),
                    user.clone(),
                    cut,
                ))
            }
            Location::Git { root, rev, path } => {
                if path.is_empty() {
                    return None;
                }
                let cut = path.rfind('/').map(|i| &path[..i]).unwrap_or("");
                Some(Location::git(root.clone(), rev.clone(), cut))
            }
        }
    }

    /// Location of `name` inside this location. `..` walks upwards.
    pub fn child(&self, name: &str) -> Option<Location> {
        if name == ".." {
            return self.parent();
        }
        if name.is_empty() || name == "." {
            return Some(self.clone());
        }
        if name.contains('/') && !matches!(self, Location::Local(_)) {
            return None;
        }
        Some(match self {
            Location::Local(p) => Location::local(p.join(name)),
            Location::Archive { container, inner } => {
                let inner = if inner.is_empty() {
                    name.to_string()
                } else {
                    format!("{inner}/{name}")
                };
                Location::archive(container.clone(), inner)
            }
            Location::Remote {
                scheme,
                host,
                user,
                path,
            } => {
                let base = path.trim_end_matches('/');
                let path = if base.is_empty() {
                    format!("/{name}")
                } else {
                    format!("{base}/{name}")
                };
                Location::remote(scheme.clone(), host.clone(), user.clone(), path)
            }
            Location::Git { root, rev, path } => {
                let path = if path.is_empty() {
                    name.to_string()
                } else {
                    format!("{path}/{name}")
                };
                Location::git(root.clone(), rev.clone(), path)
            }
        })
    }

    /// The last path component, if any.
    pub fn file_name(&self) -> Option<String> {
        match self {
            Location::Local(p) => p.file_name().map(|n| n.to_string_lossy().to_string()),
            Location::Archive { container, inner } => {
                if inner.is_empty() {
                    container
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                } else {
                    Some(inner.rsplit('/').next().unwrap_or(inner).to_string())
                }
            }
            Location::Remote { path, host, .. } => {
                let t = path.trim_end_matches('/');
                if t.is_empty() {
                    Some(host.clone())
                } else {
                    Some(t.rsplit('/').next().unwrap_or(t).to_string())
                }
            }
            Location::Git { root, path, .. } => {
                if path.is_empty() {
                    root.file_name().map(|n| n.to_string_lossy().to_string())
                } else {
                    Some(path.rsplit('/').next().unwrap_or(path).to_string())
                }
            }
        }
    }

    /// True when both locations denote the same directory.
    pub fn same_dir(&self, other: &Location) -> bool {
        match (self, other) {
            (Location::Local(a), Location::Local(b)) => canonicalish(a) == canonicalish(b),
            _ => self == other,
        }
    }
}

fn canonicalish(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn normalize_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if s.is_empty() {
        return PathBuf::from("/");
    }
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from("/")
    } else {
        out
    }
}

fn normalize_inner(inner: &str) -> String {
    inner
        .split('/')
        .filter(|c| !c.is_empty() && *c != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn split_archive_path(rest: &str) -> CoreResult<(PathBuf, String)> {
    // `rest` is `/abs/path/archive.zip/inner/dir`; we split at the first path
    // component that looks like an archive name.
    let parts: Vec<&str> = rest.trim_start_matches('/').split('/').collect();
    let idx = parts.iter().position(|c| {
        let lower = c.to_ascii_lowercase();
        [
            ".zip", ".7z", ".rar", ".tar", ".tar.gz", ".tgz", ".gz", ".bz2", ".xz", ".zst", ".cab",
            ".iso",
        ]
        .iter()
        .any(|ext| lower.ends_with(ext))
    });
    match idx {
        Some(i) => {
            let container = PathBuf::from("/").join(parts[..=i].join("/"));
            let inner = parts[i + 1..].join("/");
            Ok((container, inner))
        }
        None => Err(CoreError::Location(rest.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_paths_normalize_dot_components() {
        let l = Location::local("/tmp/./a/../b");
        assert_eq!(l.display(), "/tmp/b");
        assert!(l.is_local());
        assert_eq!(l.as_path().unwrap(), Path::new("/tmp/b"));
    }

    #[test]
    fn parent_walks_up_and_stops_at_root() {
        let l = Location::local("/tmp/a/b");
        assert_eq!(l.parent().unwrap().display(), "/tmp/a");
        let root = Location::local("/");
        assert!(root.parent().is_none());
    }

    #[test]
    fn children_join_component_names() {
        let l = Location::local("/tmp");
        assert_eq!(l.child("x").unwrap().display(), "/tmp/x");
        assert_eq!(l.child("..").unwrap().display(), "/");
        assert_eq!(l.child(".").unwrap(), l);
    }

    #[test]
    fn ftp_urls_parse_into_host_user_and_path() {
        let l = Location::parse("ftp://user@example.com/pub/files").unwrap();
        match &l {
            Location::Remote {
                scheme,
                host,
                user,
                path,
            } => {
                assert_eq!(scheme, "ftp");
                assert_eq!(host, "example.com");
                assert_eq!(user.as_deref(), Some("user"));
                assert_eq!(path, "/pub/files");
            }
            other => panic!("expected remote, got {other:?}"),
        }
        assert_eq!(l.display(), "ftp://user@example.com/pub/files");
        assert_eq!(l.file_name().unwrap(), "files");
        assert_eq!(l.parent().unwrap().display(), "ftp://user@example.com/pub");
    }

    #[test]
    fn remote_parent_stops_at_root_and_rejects_slash_children() {
        let root = Location::parse("ftp://host").unwrap();
        assert_eq!(root.display(), "ftp://host/");
        assert!(root.parent().is_none());
        assert!(root.child("a").is_some());
    }

    #[test]
    fn archive_urls_split_container_and_inner_path() {
        let l = Location::parse("zip:///data/backup.zip/sub/dir").unwrap();
        let (container, inner) = l.archive_parts().expect("archive parts");
        assert_eq!(container, Path::new("/data/backup.zip"));
        assert_eq!(inner, "sub/dir");
        assert_eq!(l.parent().unwrap().archive_parts().unwrap().1, "sub");
        assert_eq!(l.file_name().unwrap(), "dir");
    }

    #[test]
    fn archive_locations_navigate_with_child_and_parent() {
        let l = Location::archive("/data/a.zip", "");
        assert_eq!(l.display(), "/data/a.zip/");
        let child = l.child("docs").unwrap();
        assert_eq!(child.display(), "/data/a.zip/docs");
        assert_eq!(
            child.child("x.txt").unwrap().display(),
            "/data/a.zip/docs/x.txt"
        );
        assert_eq!(child.parent().unwrap().display(), "/data/a.zip/");
        assert_eq!(
            Location::archive("/data/a.zip", "")
                .parent()
                .unwrap()
                .display(),
            "/data"
        );
    }

    #[test]
    fn invalid_locations_are_rejected() {
        assert!(Location::parse("").is_err());
        assert!(Location::parse("weird://").is_err());
        assert!(Location::parse("zip:///no/archive/here").is_err());
        assert!(Location::parse("ftp://").is_err());
    }

    #[test]
    fn same_dir_compares_normalized_forms() {
        assert!(Location::local("/tmp/./a").same_dir(&Location::local("/tmp/a")));
        assert!(!Location::archive("/a.zip", "x").same_dir(&Location::archive("/a.zip", "y")));
    }
}

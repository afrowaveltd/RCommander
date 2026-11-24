//! Read-only Git repository access.
//!
//! A repository is a natural fit for a commander: the working tree is an ordinary
//! directory, so the value of this crate is everything a plain file manager
//! *cannot* do — showing branches, tags and commits as virtual folders and reading
//! files out of any revision **without ever running `checkout`**.
//!
//! The engine is deliberately read-only. Nothing here stages, commits, merges or
//! pushes, and nothing writes into the repository; the only way data leaves is
//! [`GitRepo::read_file`], which the application copies into a panel.
//!
//! Backend: `git2` with `vendored-libgit2`, so no `git` binary has to be installed
//! on the user's machine and the same code runs on Windows, Linux and Android. See
//! `analysis/git-backend.md` for the comparison with gitoxide and with shelling out.

use std::path::{Path, PathBuf};

/// Error of every operation in this crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitError {
    /// What went wrong, phrased for the status line.
    pub message: String,
}

impl GitError {
    fn new(message: impl Into<String>) -> Self {
        GitError {
            message: message.into(),
        }
    }

    /// True when the problem is "this is not a repository".
    pub fn is_not_a_repository(&self) -> bool {
        self.message.contains("not a git repository")
    }
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GitError {}

fn convert(error: git2::Error) -> GitError {
    GitError::new(error.message().to_string())
}

/// Which revision a virtual folder shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Revision {
    /// The working tree on disk (no revision is read at all).
    WorkingTree,
    /// Whatever `HEAD` points at.
    Head,
    /// A local branch by its short name (`main`).
    Branch(String),
    /// A tag by name (`v1.0`).
    Tag(String),
    /// A commit by (short) object id.
    Commit(String),
    /// A remote-tracking branch, e.g. `origin/main`.
    Remote(String),
}

impl Revision {
    /// A stable, filesystem-friendly spelling used by the panel locations.
    pub fn as_key(&self) -> String {
        match self {
            Revision::WorkingTree => String::new(),
            Revision::Head => "HEAD".to_string(),
            Revision::Branch(name) => format!("branch:{name}"),
            Revision::Tag(name) => format!("tag:{name}"),
            Revision::Commit(id) => format!("commit:{id}"),
            Revision::Remote(name) => format!("remote:{name}"),
        }
    }

    /// Parse the panel spelling back into a revision.
    pub fn from_key(key: &str) -> Revision {
        match key.split_once(':') {
            Some(("branch", name)) => Revision::Branch(name.to_string()),
            Some(("tag", name)) => Revision::Tag(name.to_string()),
            Some(("commit", id)) => Revision::Commit(id.to_string()),
            Some(("remote", name)) => Revision::Remote(name.to_string()),
            Some(("HEAD", _)) => Revision::Head,
            _ => {
                let trimmed = key.trim();
                if trimmed.is_empty() {
                    Revision::WorkingTree
                } else if trimmed.eq_ignore_ascii_case("head") {
                    Revision::Head
                } else {
                    Revision::Commit(trimmed.to_string())
                }
            }
        }
    }

    /// Human readable description for the panel header.
    pub fn label(&self) -> String {
        match self {
            Revision::WorkingTree => "working tree".to_string(),
            Revision::Head => "HEAD".to_string(),
            Revision::Branch(name) => name.clone(),
            Revision::Tag(name) => name.clone(),
            Revision::Commit(id) => id.chars().take(8).collect(),
            Revision::Remote(name) => name.clone(),
        }
    }
}

/// Which kind of reference a list contains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    Branch,
    Tag,
    Remote,
}

/// One branch, tag or remote-tracking reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRef {
    /// Full name (`refs/heads/main`, `refs/tags/v1.0`).
    pub full_name: String,
    /// Short name shown in the panel (`main`, `v1.0`, `origin/main`).
    pub name: String,
    /// Commit id the reference points at.
    pub id: String,
}

impl GitRef {
    /// The revision this reference describes.
    pub fn revision(&self, kind: RefKind) -> Revision {
        match kind {
            RefKind::Branch => Revision::Branch(self.name.clone()),
            RefKind::Tag => Revision::Tag(self.name.clone()),
            RefKind::Remote => Revision::Remote(self.name.clone()),
        }
    }
}

/// One commit from the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCommit {
    /// Full object id.
    pub id: String,
    /// First eight characters, the way a folder is named.
    pub short: String,
    /// First line of the commit message.
    pub summary: String,
    pub author: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
}

/// One entry of a tree (or of the working tree).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitEntry {
    pub name: String,
    /// `true` for a directory (a tree).
    pub is_dir: bool,
    /// `true` for a symbolic link.
    pub is_symlink: bool,
    /// Blob size in bytes; 0 for directories.
    pub size: u64,
}

/// A repository opened for reading.
pub struct GitRepo {
    repo: git2::Repository,
    root: PathBuf,
}

impl std::fmt::Debug for GitRepo {
    // `git2::Repository` has no Debug, and the root is what a test wants to see.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitRepo")
            .field("root", &self.root)
            .field("bare", &self.is_bare())
            .finish()
    }
}

impl GitRepo {
    /// Open the repository that contains `path` (walking upwards like git does).
    pub fn open(path: impl AsRef<Path>) -> Result<GitRepo, GitError> {
        let path = path.as_ref();
        let repo = git2::Repository::discover(path).map_err(|_| {
            GitError::new(format!(
                "{} is not a git repository (no .git directory found)",
                path.display()
            ))
        })?;
        let root = repo
            .workdir()
            .map(|p| p.to_path_buf())
            .or_else(|| repo.path().parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| path.to_path_buf());
        Ok(GitRepo { repo, root })
    }

    /// Open exactly the repository at `path` (a `.git` directory or a bare repo).
    pub fn open_exact(path: impl AsRef<Path>) -> Result<GitRepo, GitError> {
        let path = path.as_ref();
        let repo = git2::Repository::open(path).map_err(|_| {
            GitError::new(format!(
                "{} is not a git repository (no .git directory found)",
                path.display()
            ))
        })?;
        let root = repo
            .workdir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| path.to_path_buf());
        Ok(GitRepo { repo, root })
    }

    /// True when `path` is inside a repository.
    pub fn is_repo(path: impl AsRef<Path>) -> bool {
        git2::Repository::discover(path.as_ref()).is_ok()
    }

    /// The working tree root (or the repository directory for a bare repository).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// True for a bare repository, which has no working tree to show.
    pub fn is_bare(&self) -> bool {
        self.repo.is_bare()
    }

    /// The checked-out branch, if `HEAD` is not detached.
    pub fn head_branch(&self) -> Option<String> {
        let head = self.repo.head().ok()?;
        if !head.is_branch() {
            return None;
        }
        head.shorthand().ok().map(|s| s.to_string())
    }

    /// Short id of `HEAD`, for a detached state.
    pub fn head_id(&self) -> Option<String> {
        let head = self.repo.head().ok()?;
        head.target().map(|id| short_id(&id))
    }

    /// Local branches, sorted by name.
    pub fn branches(&self) -> Vec<GitRef> {
        self.references(|r| r.is_branch())
    }

    /// Tags, sorted by name.
    pub fn tags(&self) -> Vec<GitRef> {
        self.references(|r| r.is_tag())
    }

    /// Remote-tracking branches, sorted by name.
    pub fn remote_branches(&self) -> Vec<GitRef> {
        self.references(|r| r.is_remote())
    }

    fn references(&self, keep: impl Fn(&git2::Reference<'_>) -> bool) -> Vec<GitRef> {
        let mut out = Vec::new();
        if let Ok(references) = self.repo.references() {
            for reference in references.flatten() {
                if !keep(&reference) {
                    continue;
                }
                let Ok(name) = reference.shorthand() else {
                    continue;
                };
                let full_name = reference.name().unwrap_or_default().to_string();
                let id = match reference.target() {
                    Some(id) => id.to_string(),
                    None => match reference.peel_to_commit() {
                        // An annotated tag points at a tag object: peel it.
                        Ok(commit) => commit.id().to_string(),
                        Err(_) => continue,
                    },
                };
                out.push(GitRef {
                    name: name.trim_start_matches("refs/remotes/").to_string(),
                    full_name,
                    id,
                });
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Every reference in one list, for the virtual `refs/` folder.
    pub fn all_refs(&self) -> Vec<(RefKind, GitRef)> {
        let mut out: Vec<(RefKind, GitRef)> = Vec::new();
        for branch in self.branches() {
            out.push((RefKind::Branch, branch));
        }
        for tag in self.tags() {
            out.push((RefKind::Tag, tag));
        }
        for remote in self.remote_branches() {
            out.push((RefKind::Remote, remote));
        }
        out
    }

    /// The most recent commits, newest first.
    pub fn commits(&self, limit: usize) -> Vec<GitCommit> {
        let mut out = Vec::new();
        let Ok(mut walk) = self.repo.revwalk() else {
            return out;
        };
        if walk.push_head().is_err() {
            return out;
        }
        let _ = walk.set_sorting(git2::Sort::TIME);
        for id in walk.flatten().take(limit) {
            let Ok(commit) = self.repo.find_commit(id) else {
                continue;
            };
            let summary = commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string();
            out.push(GitCommit {
                id: id.to_string(),
                short: short_id(&id),
                summary,
                author: commit
                    .author()
                    .name()
                    .map(|name| name.to_string())
                    .unwrap_or_default(),
                time: commit.time().seconds(),
            });
        }
        out
    }

    /// Resolve a revision to a commit id.
    pub fn resolve(&self, revision: &Revision) -> Result<String, GitError> {
        Ok(self
            .object(revision)?
            .peel_to_commit()
            .map_err(convert)?
            .id()
            .to_string())
    }

    fn object(&self, revision: &Revision) -> Result<git2::Object<'_>, GitError> {
        let spec = match revision {
            Revision::WorkingTree | Revision::Head => "HEAD".to_string(),
            Revision::Branch(name) => format!("refs/heads/{name}"),
            Revision::Tag(name) => format!("refs/tags/{name}"),
            Revision::Remote(name) => format!("refs/remotes/{name}"),
            Revision::Commit(id) => id.clone(),
        };
        self.repo
            .revparse_single(&spec)
            .map_err(|e| GitError::new(format!("unknown revision “{spec}”: {}", e.message())))
    }

    /// List one directory inside a revision. `path` is `/`-separated and empty for
    /// the root. The working tree is read from disk.
    pub fn list(&self, revision: &Revision, path: &str) -> Result<Vec<GitEntry>, GitError> {
        let path = normalise_path(path)?;
        if matches!(revision, Revision::WorkingTree) {
            return self.list_workdir(&path);
        }
        let commit = self.object(revision)?.peel_to_commit().map_err(convert)?;
        let tree = if path.is_empty() {
            commit.tree().map_err(convert)?
        } else {
            let entry = commit
                .tree()
                .map_err(convert)?
                .get_path(Path::new(&path))
                .map_err(|_| {
                    GitError::new(format!("“{path}” does not exist in {}", revision.label()))
                })?;
            entry
                .to_object(&self.repo)
                .map_err(convert)?
                .into_tree()
                .map_err(|_| {
                    GitError::new(format!(
                        "“{path}” is not a directory in {}",
                        revision.label()
                    ))
                })?
        };
        let mut out = Vec::new();
        for entry in tree.iter() {
            let name = entry.name().unwrap_or_default().to_string();
            match entry.kind() {
                Some(git2::ObjectType::Tree) => out.push(GitEntry {
                    name,
                    is_dir: true,
                    is_symlink: false,
                    size: 0,
                }),
                Some(git2::ObjectType::Blob) => {
                    let is_symlink = entry.filemode() == 0o120000;
                    let size = entry
                        .to_object(&self.repo)
                        .ok()
                        .and_then(|object| object.as_blob().map(|blob| blob.size() as u64))
                        .unwrap_or(0);
                    out.push(GitEntry {
                        name,
                        is_dir: false,
                        is_symlink,
                        size,
                    });
                }
                // A submodule has no data we can read without cloning it.
                _ => out.push(GitEntry {
                    name,
                    is_dir: true,
                    is_symlink: false,
                    size: 0,
                }),
            }
        }
        out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        Ok(out)
    }

    fn list_workdir(&self, path: &str) -> Result<Vec<GitEntry>, GitError> {
        if self.is_bare() {
            return Err(GitError::new(
                "this is a bare repository: it has no working tree, open a branch, tag or commit instead"
                    .to_string(),
            ));
        }
        let dir = if path.is_empty() {
            self.root.clone()
        } else {
            self.root.join(path)
        };
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| GitError::new(format!("cannot read {}: {e}", dir.display())))?;
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = entry.metadata().ok();
            let file_type = entry.file_type().ok();
            out.push(GitEntry {
                name,
                is_dir: metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                is_symlink: file_type.map(|t| t.is_symlink()).unwrap_or(false),
                size: metadata.map(|m| m.len()).unwrap_or(0),
            });
        }
        out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        Ok(out)
    }

    /// Read a file out of a revision, refusing anything larger than `max_bytes`.
    pub fn read_file(
        &self,
        revision: &Revision,
        path: &str,
        max_bytes: u64,
    ) -> Result<Vec<u8>, GitError> {
        let path = normalise_path(path)?;
        if path.is_empty() {
            return Err(GitError::new("a revision root is not a file".to_string()));
        }
        if matches!(revision, Revision::WorkingTree) {
            if self.is_bare() {
                return Err(GitError::new(
                    "this is a bare repository: it has no working tree".to_string(),
                ));
            }
            let file = self.root.join(&path);
            let metadata = std::fs::metadata(&file)
                .map_err(|e| GitError::new(format!("cannot read {}: {e}", file.display())))?;
            if metadata.len() > max_bytes {
                return Err(GitError::new(format!(
                    "{} is {} bytes, more than the {max_bytes}-byte limit",
                    file.display(),
                    metadata.len()
                )));
            }
            return std::fs::read(&file)
                .map_err(|e| GitError::new(format!("cannot read {}: {e}", file.display())));
        }
        let commit = self.object(revision)?.peel_to_commit().map_err(convert)?;
        let entry = commit
            .tree()
            .map_err(convert)?
            .get_path(Path::new(&path))
            .map_err(|_| {
                GitError::new(format!("“{path}” does not exist in {}", revision.label()))
            })?;
        let Some(blob) = entry
            .to_object(&self.repo)
            .map_err(convert)?
            .as_blob()
            .map(|blob| blob.to_owned())
        else {
            return Err(GitError::new(format!(
                "“{path}” is not a file in {}",
                revision.label()
            )));
        };
        if blob.size() as u64 > max_bytes {
            return Err(GitError::new(format!(
                "“{path}” is {} bytes, more than the {max_bytes}-byte limit",
                blob.size()
            )));
        }
        Ok(blob.content().to_vec())
    }

    /// Size of a file inside a revision without reading it.
    pub fn file_size(&self, revision: &Revision, path: &str) -> Result<u64, GitError> {
        let path = normalise_path(path)?;
        if matches!(revision, Revision::WorkingTree) {
            let file = self.root.join(&path);
            return std::fs::metadata(&file)
                .map(|m| m.len())
                .map_err(|e| GitError::new(format!("cannot read {}: {e}", file.display())));
        }
        let commit = self.object(revision)?.peel_to_commit().map_err(convert)?;
        let entry = commit
            .tree()
            .map_err(convert)?
            .get_path(Path::new(&path))
            .map_err(|_| GitError::new(format!("“{path}” does not exist")))?;
        entry
            .to_object(&self.repo)
            .map_err(convert)?
            .as_blob()
            .map(|blob| blob.size() as u64)
            .ok_or_else(|| GitError::new(format!("“{path}” is not a file")))
    }

    /// True when a path inside a revision is a directory.
    pub fn is_dir(&self, revision: &Revision, path: &str) -> Result<bool, GitError> {
        let path = normalise_path(path)?;
        if path.is_empty() {
            return Ok(true);
        }
        if matches!(revision, Revision::WorkingTree) {
            return Ok(self.root.join(&path).is_dir());
        }
        let commit = self.object(revision)?.peel_to_commit().map_err(convert)?;
        let entry = commit
            .tree()
            .map_err(convert)?
            .get_path(Path::new(&path))
            .map_err(|_| GitError::new(format!("“{path}” does not exist")))?;
        Ok(entry.kind() == Some(git2::ObjectType::Tree))
    }

    /// Names inside `path` of a revision.
    pub fn names(&self, revision: &Revision, path: &str) -> Result<Vec<String>, GitError> {
        Ok(self
            .list(revision, path)?
            .into_iter()
            .map(|entry| entry.name)
            .collect())
    }
}

/// Reject absolute paths and `..` segments: a revision's tree is a closed world.
fn normalise_path(path: &str) -> Result<String, GitError> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => {
                return Err(GitError::new(format!(
                    "“{path}” leaves the revision (no “..” allowed)"
                )));
            }
            other => parts.push(other),
        }
    }
    Ok(parts.join("/"))
}

fn short_id(id: &git2::Oid) -> String {
    id.to_string().chars().take(8).collect()
}

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;

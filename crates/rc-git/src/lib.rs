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

pub mod config;

pub use config::{GitConfig, GitConfigSummary, Identity, StoredCredential, UrlRewrite};

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

/// State of one path in the working tree, as the panel needs to colour it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileStatus {
    /// Tracked and identical to `HEAD`.
    Unchanged,
    Modified,
    Added,
    Deleted,
    Renamed,
    /// Not tracked yet, but not ignored either.
    Untracked,
    /// Excluded by a `.gitignore` rule.
    Ignored,
    /// A merge conflict waiting to be resolved.
    Conflicted,
    /// The file type changed (for example a file became a symlink).
    TypeChanged,
}

impl FileStatus {
    /// One-letter code in the style of `git status --short`.
    pub fn code(self) -> char {
        match self {
            FileStatus::Unchanged => ' ',
            FileStatus::Modified => 'M',
            FileStatus::Added => 'A',
            FileStatus::Deleted => 'D',
            FileStatus::Renamed => 'R',
            FileStatus::Untracked => '?',
            FileStatus::Ignored => '!',
            FileStatus::Conflicted => 'U',
            FileStatus::TypeChanged => 'T',
        }
    }

    /// Localisation key of the colour role this state uses.
    pub fn label_key(self) -> &'static str {
        match self {
            FileStatus::Unchanged => "git.status.unchanged",
            FileStatus::Modified => "git.status.modified",
            FileStatus::Added => "git.status.added",
            FileStatus::Deleted => "git.status.deleted",
            FileStatus::Renamed => "git.status.renamed",
            FileStatus::Untracked => "git.status.untracked",
            FileStatus::Ignored => "git.status.ignored",
            FileStatus::Conflicted => "git.status.conflicted",
            FileStatus::TypeChanged => "git.status.type_changed",
        }
    }

    /// True when the file is only in the index (staged) and not modified further.
    pub fn is_staged(self) -> bool {
        matches!(
            self,
            FileStatus::Added | FileStatus::Renamed | FileStatus::Deleted
        )
    }
}

/// The index and working-tree state of one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct StatusEntry {
    /// What the index holds compared to `HEAD` (the "ready to commit" side).
    pub staged: Option<FileStatus>,
    /// What the working tree holds compared to the index.
    pub unstaged: Option<FileStatus>,
}

impl StatusEntry {
    /// Nothing to report.
    pub fn is_clean(&self) -> bool {
        self.staged.is_none() && self.unstaged.is_none()
    }

    /// True when something is ready to be committed.
    pub fn has_staged(&self) -> bool {
        self.staged
            .is_some_and(|status| status != FileStatus::Unchanged)
    }

    /// The status a panel should colour the row with: the worktree state wins,
    /// because that is what the user sees on disk.
    pub fn display(&self) -> Option<FileStatus> {
        self.unstaged.or(self.staged)
    }

    /// `XY` in the style of `git status --short`.
    pub fn codes(&self) -> String {
        let first = self.staged.map(|s| s.code()).unwrap_or(' ');
        let second = self.unstaged.map(|s| s.code()).unwrap_or(' ');
        format!("{first}{second}")
    }

    fn from_git(status: git2::Status) -> StatusEntry {
        use git2::Status as S;
        let staged = if status.intersects(S::INDEX_NEW) {
            Some(FileStatus::Added)
        } else if status.intersects(S::INDEX_MODIFIED) {
            Some(FileStatus::Modified)
        } else if status.intersects(S::INDEX_DELETED) {
            Some(FileStatus::Deleted)
        } else if status.intersects(S::INDEX_RENAMED) {
            Some(FileStatus::Renamed)
        } else if status.intersects(S::INDEX_TYPECHANGE) {
            Some(FileStatus::TypeChanged)
        } else {
            None
        };
        let unstaged = if status.intersects(S::CONFLICTED) {
            Some(FileStatus::Conflicted)
        } else if status.intersects(S::WT_NEW) {
            Some(if status.intersects(S::IGNORED) {
                FileStatus::Ignored
            } else {
                FileStatus::Untracked
            })
        } else if status.intersects(S::WT_MODIFIED) {
            Some(FileStatus::Modified)
        } else if status.intersects(S::WT_DELETED) {
            Some(FileStatus::Deleted)
        } else if status.intersects(S::WT_RENAMED) {
            Some(FileStatus::Renamed)
        } else if status.intersects(S::WT_TYPECHANGE) {
            Some(FileStatus::TypeChanged)
        } else {
            None
        };
        StatusEntry { staged, unstaged }
    }
}

/// Everything the status of a working tree reports, plus a summary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkTreeStatus {
    /// `path -> state`, only for entries that are not clean.
    pub entries: Vec<(String, StatusEntry)>,
}

impl WorkTreeStatus {
    /// The state of one path (`/`-separated, relative to the root).
    pub fn get(&self, path: &str) -> StatusEntry {
        self.entries
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, status)| *status)
            .unwrap_or_default()
    }

    /// Number of paths the index would commit.
    pub fn staged_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, status)| status.has_staged())
            .count()
    }

    /// Number of paths with unstaged changes.
    pub fn modified_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, status)| status.unstaged.is_some())
            .count()
    }

    /// True when there is nothing to report.
    pub fn is_clean(&self) -> bool {
        self.entries.is_empty()
    }

    /// A one-line summary for the status bar.
    pub fn summary(&self) -> String {
        if self.is_clean() {
            return "clean".to_string();
        }
        format!(
            "{} staged, {} changed",
            self.staged_count(),
            self.modified_count()
        )
    }
}

impl GitRepo {
    /// Read the status of the whole working tree, including ignored files.
    pub fn status(&self) -> Result<WorkTreeStatus, GitError> {
        let mut options = git2::StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(true)
            .renames_head_to_index(true)
            .renames_index_to_workdir(true);
        let statuses = self.repo.statuses(Some(&mut options)).map_err(convert)?;
        let mut entries = Vec::new();
        for entry in statuses.iter() {
            let Ok(path) = entry.path() else { continue };
            let status = StatusEntry::from_git(entry.status());
            if status.is_clean() {
                continue;
            }
            entries.push((path.to_string(), status));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(WorkTreeStatus { entries })
    }

    /// True when a path is excluded by a `.gitignore` rule.
    ///
    /// Ask this per entry instead of relying on the bulk status list: libgit2 only
    /// reports ignored paths that its own walk happens to visit, while the panel
    /// asks about exactly the names it is about to draw.
    pub fn is_ignored(&self, path: &str) -> bool {
        let path = path.trim_start_matches('/').replace('\\', "/");
        if path.is_empty() {
            return false;
        }
        self.repo
            .status_should_ignore(Path::new(&path))
            .unwrap_or(false)
    }

    /// The status of a single path, walked through the parent directories.
    pub fn status_of(&self, status: &WorkTreeStatus, path: &str) -> StatusEntry {
        let path = path.trim_start_matches('/').replace('\\', "/");
        if path.is_empty() {
            return StatusEntry::default();
        }
        let direct = status.get(&path);
        if !direct.is_clean() {
            return direct;
        }
        if self.is_ignored(&path) {
            return StatusEntry {
                staged: None,
                unstaged: Some(FileStatus::Ignored),
            };
        }
        // A directory is "changed" when anything inside it is: the panel shows
        // directories too, and a grey folder with modified files would be wrong.
        let prefix = format!("{path}/");
        let mut combined = StatusEntry::default();
        for (name, entry) in &status.entries {
            if !name.starts_with(&prefix) {
                continue;
            }
            combined.staged = combined.staged.or(entry.staged);
            combined.unstaged = combined.unstaged.or(entry.unstaged);
        }
        combined
    }

    /// True when the working tree has staged or unstaged changes.
    pub fn is_dirty(&self) -> bool {
        self.status()
            .map(|status| !status.is_clean())
            .unwrap_or(false)
    }

    /// Switch the working tree to another local branch.
    ///
    /// `allow_dirty` is the "I know what I am doing" switch: without it a working
    /// tree with changes is refused instead of half-merged. The checkout itself is
    /// `safe`, so libgit2 will not overwrite a file that has local changes even when
    /// `allow_dirty` is set — it reports the conflict instead.
    pub fn checkout_branch(&self, name: &str, allow_dirty: bool) -> Result<(), GitError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(GitError::new("a branch name is required".to_string()));
        }
        if self.branches().iter().all(|branch| branch.name != name) {
            return Err(GitError::new(format!("there is no branch “{name}”")));
        }
        if self.head_branch().as_deref() == Some(name) {
            return Err(GitError::new(format!("“{name}” is already checked out")));
        }
        if !allow_dirty && self.is_dirty() {
            return Err(GitError::new(
                "the working tree has changes; commit or discard them first (or switch with the force option)"
                    .to_string(),
            ));
        }
        let reference = format!("refs/heads/{name}");
        self.repo.set_head(&reference).map_err(convert)?;
        let mut builder = git2::build::CheckoutBuilder::new();
        // A clean tree can be replaced outright — that is what makes files of the
        // other branch disappear instead of lingering as untracked leftovers. A tree
        // with local changes is handled with `safe`, which never overwrites them.
        if allow_dirty {
            builder.safe();
        } else {
            builder.force();
        }
        builder.update_index(true);
        self.repo
            .checkout_head(Some(&mut builder))
            .map_err(|error| {
                GitError::new(format!(
                    "could not check out “{name}”: {} (your local changes were kept)",
                    error.message()
                ))
            })?;
        // The switch itself must not look like a pile of staged changes.
        self.reset_index_to_head();
        Ok(())
    }

    /// Make the index match `HEAD` without touching the working tree.
    fn reset_index_to_head(&self) {
        if let Ok(head) = self.repo.head()
            && let Ok(commit) = head.peel_to_commit()
        {
            let _ = self
                .repo
                .reset(commit.as_object(), git2::ResetType::Mixed, None);
        }
    }

    /// Create a branch at `HEAD` and switch to it.
    pub fn create_branch(&self, name: &str) -> Result<(), GitError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(GitError::new("a branch name is required".to_string()));
        }
        if self.branches().iter().any(|branch| branch.name == name) {
            return Err(GitError::new(format!("the branch “{name}” already exists")));
        }
        let head = self.repo.head().map_err(convert)?;
        let commit = head.peel_to_commit().map_err(convert)?;
        self.repo.branch(name, &commit, false).map_err(convert)?;
        self.repo
            .set_head(&format!("refs/heads/{name}"))
            .map_err(convert)?;
        self.reset_index_to_head();
        let mut builder = git2::build::CheckoutBuilder::new();
        builder.safe().update_index(true);
        self.repo.checkout_head(Some(&mut builder)).map_err(convert)
    }
}

/// Result of a commit made by the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitResult {
    pub id: String,
    pub short: String,
    pub summary: String,
    /// Files that went into the commit.
    pub files: usize,
}

/// A push attempt that has not been performed yet, so the caller can run it on a
/// worker thread with credentials it obtained from the (approved) Git configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushRequest {
    /// Remote name (`origin`).
    pub remote: String,
    /// Refspec, normally `refs/heads/<branch>:refs/heads/<branch>`.
    pub refspec: String,
    /// Host of the remote URL, so the caller can look the credential up.
    pub url: String,
}

impl GitRepo {
    /// Stage one path (relative to the working tree root).
    pub fn stage_path(&self, path: &str) -> Result<(), GitError> {
        let path = normalise_path(path)?;
        if path.is_empty() {
            return Err(GitError::new("a path is required".to_string()));
        }
        let mut index = self.repo.index().map_err(convert)?;
        let file = self.root.join(&path);
        if file.exists() {
            index.add_path(Path::new(&path)).map_err(|error| {
                GitError::new(format!("cannot stage “{path}”: {}", error.message()))
            })?;
        } else {
            // A deleted file has to leave the index too.
            index.remove_path(Path::new(&path)).map_err(|error| {
                GitError::new(format!("cannot stage “{path}”: {}", error.message()))
            })?;
        }
        index.write().map_err(convert)
    }

    /// Stage every change (tracked modifications, additions and deletions).
    pub fn stage_all(&self) -> Result<usize, GitError> {
        let mut index = self.repo.index().map_err(convert)?;
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(convert)?;
        index.update_all(["*"].iter(), None).map_err(convert)?;
        index.write().map_err(convert)?;
        // Deletions `add_all` does not cover.
        let status = self.status()?;
        let mut index = self.repo.index().map_err(convert)?;
        for (path, entry) in &status.entries {
            if matches!(entry.unstaged, Some(FileStatus::Deleted))
                || matches!(entry.staged, Some(FileStatus::Deleted))
            {
                let _ = index.remove_path(Path::new(path));
            }
        }
        index.write().map_err(convert)?;
        Ok(status.entries.len())
    }

    /// Create a commit from the current index.
    ///
    /// `author` lets the caller use the identity from the machine's Git configuration;
    /// without it libgit2 falls back to the repository configuration.
    pub fn commit_staged(
        &self,
        message: &str,
        author: Option<(&str, &str)>,
    ) -> Result<CommitResult, GitError> {
        let message = message.trim();
        if message.is_empty() {
            return Err(GitError::new("a commit needs a message".to_string()));
        }
        let mut index = self.repo.index().map_err(convert)?;
        let tree_id = index.write_tree().map_err(convert)?;
        let tree = self.repo.find_tree(tree_id).map_err(convert)?;
        let (name, email) = author.unwrap_or(("RCommander", "rcommander@localhost"));
        let signature = git2::Signature::now(name, email)
            .or_else(|_| self.repo.signature())
            .map_err(convert)?;
        let parent = self
            .repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|id| self.repo.find_commit(id).ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        // `HEAD` may be unborn on a fresh repository: `update_ref` would fail, so the
        // commit is created without moving the reference and then set explicitly.
        let id = self
            .repo
            .commit(None, &signature, &signature, message, &tree, &parents)
            .map_err(convert)?;
        let head_name = self
            .repo
            .head()
            .ok()
            .and_then(|head| head.name().ok().map(|n| n.to_string()))
            .unwrap_or_else(|| "refs/heads/main".to_string());
        self.repo
            .reference(&head_name, id, true, "commit")
            .map_err(convert)?;
        self.repo.set_head(&head_name).map_err(convert)?;
        // The working tree now matches the commit.
        self.reset_index_to_head();
        let head = self.repo.find_commit(id).map_err(convert)?;
        let summary = head
            .summary()
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_string();
        let files = head
            .tree()
            .map_err(convert)?
            .iter()
            .filter(|entry| entry.kind() == Some(git2::ObjectType::Blob))
            .count();
        Ok(CommitResult {
            short: short_id(&id),
            id: id.to_string(),
            summary,
            files,
        })
    }

    /// Remote names configured in the repository.
    pub fn remotes(&self) -> Vec<String> {
        match self.repo.remotes() {
            // `StringArray::iter` yields `Option<&str>` as the bytes may not be UTF-8.
            Ok(names) => names
                .iter()
                .filter_map(|item| item.ok().flatten())
                .map(|name| name.to_string())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The remote and refspec a push of the current branch would use, or `None` when
    /// the branch has no upstream and no `origin`.
    pub fn push_request(&self) -> Result<PushRequest, GitError> {
        let branch = self
            .head_branch()
            .ok_or_else(|| GitError::new("HEAD is not on a branch, nothing to push".to_string()))?;
        let remotes = self.remotes();
        if remotes.is_empty() {
            return Err(GitError::new(
                "this repository has no remote to push to".to_string(),
            ));
        }
        // Prefer the configured upstream, fall back to `origin`, then the first remote.
        let upstream_name = self
            .repo
            .find_branch(&branch, git2::BranchType::Local)
            .ok()
            .and_then(|local| local.upstream().ok())
            .and_then(|upstream| upstream.name().ok().flatten().map(|n| n.to_string()));
        let (remote, refspec) = match upstream_name {
            Some(upstream) => {
                // `refs/remotes/origin/main` → remote `origin`, branch `main`.
                let rest = upstream
                    .strip_prefix("refs/remotes/")
                    .unwrap_or(&upstream)
                    .to_string();
                let (remote_name, remote_branch) = match rest.split_once('/') {
                    Some((remote_name, remote_branch)) => {
                        (remote_name.to_string(), remote_branch.to_string())
                    }
                    None => (rest.clone(), branch.clone()),
                };
                (
                    remote_name,
                    format!("refs/heads/{branch}:refs/heads/{remote_branch}"),
                )
            }
            None if remotes.iter().any(|r| r == "origin") => (
                "origin".to_string(),
                format!("refs/heads/{branch}:refs/heads/{branch}"),
            ),
            None => (
                remotes[0].clone(),
                format!("refs/heads/{branch}:refs/heads/{branch}"),
            ),
        };
        let url = self
            .repo
            .find_remote(&remote)
            .ok()
            .and_then(|r| r.url().ok().map(|u| u.to_string()))
            .unwrap_or_default();
        Ok(PushRequest {
            remote,
            refspec,
            url,
        })
    }

    /// Fetch the given remote (all branches), updating remote-tracking references.
    pub fn fetch(
        &self,
        remote: &str,
        credentials: Option<(&str, &str)>,
    ) -> Result<usize, GitError> {
        let mut remote = self
            .repo
            .find_remote(remote)
            .map_err(|error| GitError::new(format!("no remote “{remote}”: {}", error.message())))?;
        let mut callbacks = git2::RemoteCallbacks::new();
        let username = credentials.map(|(user, _)| user.to_string());
        let password = credentials.map(|(_, password)| password.to_string());
        callbacks.credentials(move |url, username_from_url, allowed| {
            if let (Some(user), Some(pass)) = (&username, &password)
                && allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT)
            {
                return git2::Cred::userpass_plaintext(user, pass);
            }
            if allowed.contains(git2::CredentialType::SSH_KEY)
                && let Some(user) = username_from_url
            {
                return git2::Cred::ssh_key_from_agent(user);
            }
            if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT)
                && let Ok(config) = git2::Config::open_default()
            {
                return git2::Cred::credential_helper(&config, url, username_from_url);
            }
            Err(git2::Error::from_str(
                "no credentials are available for this remote",
            ))
        });
        let mut options = git2::FetchOptions::new();
        options.remote_callbacks(callbacks);
        // Prune deleted branches so the panel does not show stale remotes.
        options.prune(git2::FetchPrune::On);
        remote
            .fetch(&[] as &[&str], Some(&mut options), None)
            .map_err(|error| GitError::new(format!("fetch failed: {}", error.message())))?;
        Ok(self.remote_branches().len())
    }

    /// How far the branch is ahead of and behind its upstream.
    ///
    /// Returns `(ahead, behind)`; without an upstream both are zero.
    pub fn ahead_behind(&self) -> (usize, usize) {
        let Some(branch) = self.head_branch() else {
            return (0, 0);
        };
        let Ok(local) = self.repo.find_branch(&branch, git2::BranchType::Local) else {
            return (0, 0);
        };
        let Ok(upstream) = local.upstream() else {
            return (0, 0);
        };
        let (Some(local_id), Some(upstream_id)) = (local.get().target(), upstream.get().target())
        else {
            return (0, 0);
        };
        self.repo
            .graph_ahead_behind(local_id, upstream_id)
            .unwrap_or((0, 0))
    }

    /// Fast-forward `HEAD` to its upstream after a fetch.
    ///
    /// Only a fast-forward is performed: a real merge could produce conflicts, and a
    /// file manager must never leave a half-merged tree behind without the user asking
    /// for it. Divergence is reported instead.
    pub fn pull_fast_forward(&self) -> Result<usize, GitError> {
        let (ahead, behind) = self.ahead_behind();
        if behind == 0 {
            if ahead > 0 {
                return Err(GitError::new(format!(
                    "{ahead} local commit(s) are not on the remote yet; push them first"
                )));
            }
            return Ok(0);
        }
        if ahead > 0 {
            return Err(GitError::new(format!(
                "the branch has diverged ({ahead} local, {behind} remote commit(s)); merge it by hand"
            )));
        }
        let branch_name = self
            .head_branch()
            .ok_or_else(|| GitError::new("HEAD is not on a branch".to_string()))?;
        let local = self
            .repo
            .find_branch(&branch_name, git2::BranchType::Local)
            .map_err(convert)?;
        let upstream = local.upstream().map_err(|error| {
            GitError::new(format!(
                "the branch “{branch_name}” has no usable upstream: {}",
                error.message()
            ))
        })?;
        let target = upstream.get().peel_to_commit().map_err(convert)?.id();
        // The tree is replaced outright only when it is clean.
        if self.is_dirty() {
            return Err(GitError::new(
                "the working tree has changes; commit or discard them before pulling".to_string(),
            ));
        }
        let object = self.repo.find_object(target, None).map_err(convert)?;
        self.repo
            .checkout_tree(&object, None)
            .map_err(|error| GitError::new(format!("pull failed: {}", error.message())))?;
        let reference = format!("refs/heads/{branch_name}");
        self.repo
            .reference(&reference, target, true, "pull: fast-forward")
            .map_err(convert)?;
        self.repo.set_head(&reference).map_err(convert)?;
        self.reset_index_to_head();
        Ok(behind)
    }

    /// Merge `name` into the current branch, fast-forward when possible.
    ///
    /// A non-fast-forward merge is refused instead of producing a conflict-marked tree:
    /// the caller can then decide (merge by hand, or push/pull first).
    pub fn merge_branch(&self, name: &str) -> Result<usize, GitError> {
        let target = self
            .repo
            .find_branch(name, git2::BranchType::Local)
            .map_err(|_| GitError::new(format!("there is no branch “{name}”")))?
            .get()
            .peel_to_commit()
            .map_err(convert)?
            .id();
        let head = self
            .repo
            .head()
            .map_err(convert)?
            .peel_to_commit()
            .map_err(convert)?
            .id();
        if head == target {
            return Ok(0);
        }
        match self.repo.merge_base(head, target) {
            // `name` is already contained in the current branch: nothing to do.
            Ok(base) if base == target => return Ok(0),
            // The current branch is an ancestor: a plain fast-forward.
            Ok(base) if base == head => {}
            Ok(_) => {
                return Err(GitError::new(format!(
                    "“{name}” cannot be merged without a conflict-capable merge; rebase or merge it by hand"
                )));
            }
            Err(error) => return Err(convert(error)),
        }
        if self.is_dirty() {
            return Err(GitError::new(
                "the working tree has changes; commit or discard them before merging".to_string(),
            ));
        }
        let object = self.repo.find_object(target, None).map_err(convert)?;
        self.repo
            .checkout_tree(&object, None)
            .map_err(|error| GitError::new(format!("merge failed: {}", error.message())))?;
        let branch_name = self
            .head_branch()
            .ok_or_else(|| GitError::new("HEAD is not on a branch".to_string()))?;
        let reference = format!("refs/heads/{branch_name}");
        self.repo
            .reference(&reference, target, true, &format!("merge {name}"))
            .map_err(convert)?;
        self.repo.set_head(&reference).map_err(convert)?;
        self.reset_index_to_head();
        Ok(1)
    }

    /// Push the current branch. Credentials come from the caller (the approved Git
    /// configuration or the credential store) and are used for this call only.
    pub fn push(
        &self,
        request: &PushRequest,
        credentials: Option<(&str, &str)>,
    ) -> Result<(), GitError> {
        let mut remote = self.repo.find_remote(&request.remote).map_err(|error| {
            GitError::new(format!(
                "no remote “{}”: {}",
                request.remote,
                error.message()
            ))
        })?;
        let mut callbacks = git2::RemoteCallbacks::new();
        let username = credentials.map(|(user, _)| user.to_string());
        let password = credentials.map(|(_, password)| password.to_string());
        callbacks.credentials(move |url, username_from_url, allowed| {
            // The stored credential wins; otherwise fall back to the ssh agent.
            if let (Some(user), Some(pass)) = (&username, &password)
                && allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT)
            {
                return git2::Cred::userpass_plaintext(user, pass);
            }
            if allowed.contains(git2::CredentialType::SSH_KEY)
                && let Some(user) = username_from_url
            {
                return git2::Cred::ssh_key_from_agent(user);
            }
            // Last resort: ask the credential helper the machine has configured.
            if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT)
                && let Ok(config) = git2::Config::open_default()
            {
                return git2::Cred::credential_helper(&config, url, username_from_url);
            }
            Err(git2::Error::from_str(
                "no credentials are available for this remote",
            ))
        });
        let mut options = git2::PushOptions::new();
        options.remote_callbacks(callbacks);
        remote
            .push(&[request.refspec.as_str()], Some(&mut options))
            .map_err(|error| GitError::new(format!("push failed: {}", error.message())))
    }
}

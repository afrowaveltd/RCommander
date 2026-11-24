//! Git repositories as panels.
//!
//! A repository is shown as a location with three parts: the root, the revision (empty
//! for the working tree) and the directory inside it. The root of a repository also
//! carries four virtual folders — `branches`, `tags`, `commits` and `remotes` — which
//! are what a plain file manager cannot show.
//!
//! Everything here is read-only: the panel lists, and copying out happens through the
//! normal copy machinery reading one file at a time.

use rc_core::entry::{Attrs, Entry, EntryKind};
use rc_core::location::Location;
use rc_git::{FileStatus, GitRepo, Revision, StatusEntry};
use std::collections::HashMap;
use std::path::PathBuf;

/// Names of the virtual folders a repository root shows.
pub const VIRTUAL_FOLDERS: [&str; 4] = ["branches", "tags", "commits", "remotes"];

/// True when `name` is one of the virtual folders.
pub fn is_virtual_folder(name: &str) -> bool {
    VIRTUAL_FOLDERS.contains(&name)
}

/// One row of a Git panel plus the state the renderer colours it with.
#[derive(Clone, Debug)]
pub struct GitListing {
    pub entries: Vec<Entry>,
    /// Status of the entries in the *working tree* view, keyed by entry name.
    pub status: HashMap<String, StatusEntry>,
    /// The branch (or revision) the panel is showing, for the header.
    pub label: String,
}

/// A file read out of a revision, ready to be written into the other panel.
#[derive(Clone, Debug)]
pub struct GitFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// Open the repository a location points at.
pub fn open(location: &Location) -> Result<GitRepo, String> {
    let Some((root, _, _)) = location.git_parts() else {
        return Err(format!("{} is not a git location", location.display()));
    };
    GitRepo::open_exact(root).map_err(|error| error.to_string())
}

/// List a Git location as panel entries.
pub fn read_listing(location: &Location) -> Result<GitListing, String> {
    let Some((_root, rev, path)) = location.git_parts() else {
        return Err(format!("{} is not a git location", location.display()));
    };
    let repo = open(location)?;
    let revision = Revision::from_key(rev);
    let label = match &revision {
        Revision::WorkingTree => repo
            .head_branch()
            .or_else(|| repo.head_id().map(|id| format!("detached {id}")))
            .unwrap_or_else(|| "no commits".to_string()),
        other => other.label(),
    };

    // `branches`, `tags`, `commits` and `remotes` are only folders at the root of the
    // working tree: inside them the panel shows the references themselves.
    if path.is_empty() && rev.is_empty() {
        let mut entries: Vec<Entry> = VIRTUAL_FOLDERS
            .iter()
            .map(|name| directory_entry(name))
            .collect();
        let tree = tree_entries(&repo, &revision, "")?;
        let mut status = status_map(&repo, &revision)?;
        add_ignored(&repo, &revision, "", &tree, &mut status);
        entries.extend(tree);
        return Ok(GitListing {
            entries,
            status,
            label,
        });
    }
    if rev.is_empty() && VIRTUAL_FOLDERS.contains(&path) {
        let entries = reference_entries(&repo, path)?;
        return Ok(GitListing {
            entries,
            status: HashMap::new(),
            label,
        });
    }

    // A branch, tag or commit that is being opened: `branch:main`, `tag:v1`, …
    if let Some((folder, name)) = path.split_once('/')
        && rev.is_empty()
        && VIRTUAL_FOLDERS.contains(&folder)
    {
        return Err(format!(
            "{name} cannot be opened as a folder; pick a reference from {folder}"
        ));
    }

    let entries = tree_entries(&repo, &revision, path)?;
    let mut status = status_map(&repo, &revision)?;
    add_ignored(&repo, &revision, path, &entries, &mut status);
    Ok(GitListing {
        entries,
        status,
        label,
    })
}

/// Convert a reference name to the location of its tree.
pub fn location_for_reference(location: &Location, folder: &str, name: &str) -> Option<Location> {
    let (root, _, _) = location.git_parts()?;
    let revision = match folder {
        "branches" => Revision::Branch(name.to_string()),
        "tags" => Revision::Tag(name.to_string()),
        "commits" => Revision::Commit(name.to_string()),
        "remotes" => Revision::Remote(name.to_string()),
        _ => return None,
    };
    Some(Location::git(root, revision.as_key(), ""))
}

/// Add the ignored state of the entries that libgit2's bulk walk did not report.
fn add_ignored(
    repo: &GitRepo,
    revision: &Revision,
    path: &str,
    entries: &[Entry],
    status: &mut HashMap<String, StatusEntry>,
) {
    if !matches!(revision, Revision::WorkingTree) {
        return;
    }
    for entry in entries {
        if status.contains_key(&entry.name) {
            continue;
        }
        let inner = if path.is_empty() {
            entry.name.clone()
        } else {
            format!("{path}/{}", entry.name)
        };
        if repo.is_ignored(&inner) {
            status.insert(
                entry.name.clone(),
                StatusEntry {
                    staged: None,
                    unstaged: Some(FileStatus::Ignored),
                },
            );
        }
    }
}

/// The four reference folders, as entries of a repository root.
fn reference_entries(repo: &GitRepo, folder: &str) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    match folder {
        "branches" => {
            for branch in repo.branches() {
                out.push(directory_entry(&branch.name));
            }
        }
        "tags" => {
            for tag in repo.tags() {
                out.push(directory_entry(&tag.name));
            }
        }
        "remotes" => {
            for remote in repo.remote_branches() {
                out.push(directory_entry(&remote.name));
            }
        }
        "commits" => {
            for commit in repo.commits(500) {
                // `<short id> <summary>` — opening it resolves the short id.
                let name = if commit.summary.is_empty() {
                    commit.short.clone()
                } else {
                    format!("{} {}", commit.short, commit.summary)
                };
                let mut entry = directory_entry(&name);
                entry.mtime = Some(commit.time);
                out.push(entry);
            }
        }
        _ => return Err(format!("“{folder}” is not a Git folder")),
    }
    Ok(out)
}

/// Entries of the working tree or of a revision, as panel entries.
fn tree_entries(repo: &GitRepo, revision: &Revision, path: &str) -> Result<Vec<Entry>, String> {
    let listing = repo
        .list(revision, path)
        .map_err(|error| error.to_string())?;
    let root = repo.root();
    let mut out = Vec::new();
    for item in listing {
        let kind = if item.is_symlink {
            EntryKind::SymlinkToFile
        } else if item.is_dir {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        let mut attrs = Attrs::default();
        if item.is_symlink {
            attrs.unix_mode = Some(0o777);
        }
        out.push(Entry {
            name: item.name.clone(),
            path: root.join(path).join(&item.name),
            kind,
            size: item.size,
            mtime: None,
            ctime: None,
            attrs,
            link_target: None,
        });
    }
    Ok(out)
}

/// Status of the working-tree entries, if the panel shows the working tree.
fn status_map(repo: &GitRepo, revision: &Revision) -> Result<HashMap<String, StatusEntry>, String> {
    if !matches!(revision, Revision::WorkingTree) {
        return Ok(HashMap::new());
    }
    let status = repo.status().map_err(|error| error.to_string())?;
    Ok(status.entries.into_iter().collect())
}

fn directory_entry(name: &str) -> Entry {
    Entry {
        name: name.to_string(),
        path: PathBuf::new(),
        kind: EntryKind::Dir,
        size: 0,
        mtime: None,
        ctime: None,
        attrs: Attrs::default(),
        link_target: None,
    }
}

/// Read one file out of a Git location, for the copy-out path.
pub fn read_file(location: &Location, name: &str, max_bytes: u64) -> Result<GitFile, String> {
    let Some((root, rev, path)) = location.git_parts() else {
        return Err(format!("{} is not a git location", location.display()));
    };
    let repo = GitRepo::open_exact(root).map_err(|error| error.to_string())?;
    let revision = Revision::from_key(rev);
    let inner = if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}/{name}")
    };
    let bytes = repo
        .read_file(&revision, &inner, max_bytes)
        .map_err(|error| error.to_string())?;
    let is_dir = repo
        .is_dir(&revision, &inner)
        .map_err(|error| error.to_string())?;
    if is_dir {
        return Err(format!("“{name}” is a directory in {}", revision.label()));
    }
    Ok(GitFile {
        path: PathBuf::from(inner),
        bytes,
    })
}

/// Limits for taking data out of a revision.
#[derive(Clone, Copy, Debug)]
pub struct ExtractLimits {
    /// Largest single file.
    pub max_file_bytes: u64,
    /// Largest total for one extracted entry (a directory tree).
    pub max_total_bytes: u64,
    /// How deep a directory tree may be.
    pub max_depth: usize,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        ExtractLimits {
            max_file_bytes: 128 * 1024 * 1024,
            max_total_bytes: 1024 * 1024 * 1024,
            max_depth: 32,
        }
    }
}

/// What an extraction produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extracted {
    pub path: PathBuf,
    pub files: u64,
    pub bytes: u64,
}

/// Write `name` (a file or a whole directory tree) from a revision into `dest_root`.
///
/// The revision is only read: nothing is checked out and no `.git` data is touched. The
/// caller copies the result where it wants it, which keeps the rename-on-collision and
/// progress behaviour of the ordinary copy path.
pub fn extract(
    location: &Location,
    name: &str,
    dest_root: &std::path::Path,
    limits: ExtractLimits,
) -> Result<Extracted, String> {
    let Some((root, rev, path)) = location.git_parts() else {
        return Err(format!("{} is not a git location", location.display()));
    };
    let repo = GitRepo::open_exact(root).map_err(|error| error.to_string())?;
    let revision = Revision::from_key(rev);
    let inner = if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}/{name}")
    };
    let mut out = Extracted {
        path: dest_root.join(name),
        ..Default::default()
    };
    extract_entry(
        &repo,
        &revision,
        &inner,
        &dest_root.join(name),
        &mut out,
        limits,
        0,
    )?;
    Ok(out)
}

fn extract_entry(
    repo: &GitRepo,
    revision: &Revision,
    inner: &str,
    target: &std::path::Path,
    out: &mut Extracted,
    limits: ExtractLimits,
    depth: usize,
) -> Result<(), String> {
    if depth > limits.max_depth {
        return Err(format!("“{inner}” is nested too deeply"));
    }
    let is_dir = repo
        .is_dir(revision, inner)
        .map_err(|error| error.to_string())?;
    if !is_dir {
        let bytes = repo
            .read_file(revision, inner, limits.max_file_bytes)
            .map_err(|error| error.to_string())?;
        if out.bytes + bytes.len() as u64 > limits.max_total_bytes {
            return Err(format!(
                "“{inner}” would exceed the {} byte limit for one extraction",
                limits.max_total_bytes
            ));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        std::fs::write(target, &bytes)
            .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
        out.files += 1;
        out.bytes += bytes.len() as u64;
        return Ok(());
    }
    std::fs::create_dir_all(target)
        .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
    for entry in repo
        .list(revision, inner)
        .map_err(|error| error.to_string())?
    {
        // A revision holds no recursion into submodules; skip those quietly.
        if entry.is_dir && !name_is_tree(repo, revision, &format!("{inner}/{}", entry.name)) {
            continue;
        }
        extract_entry(
            repo,
            revision,
            &format!("{inner}/{}", entry.name),
            &target.join(&entry.name),
            out,
            limits,
            depth + 1,
        )?;
    }
    Ok(())
}

/// True when the path really is a directory in the revision (submodules are not).
fn name_is_tree(repo: &GitRepo, revision: &Revision, inner: &str) -> bool {
    repo.is_dir(revision, inner).unwrap_or(false) && repo.list(revision, inner).is_ok()
}

/// The colour role a status should be drawn with, or `None` for a plain row.
///
/// The mapping is the one the plan asked for: a staged change is green, a working-tree
/// change is the warning colour, an untracked file stands out in the link colour and an
/// ignored file is muted.
pub fn status_role(status: StatusEntry) -> Option<rc_core::theme::Role> {
    use rc_core::theme::Role;
    let status = status.display()?;
    Some(match status {
        FileStatus::Ignored => Role::Hidden,
        FileStatus::Untracked => Role::Symlink,
        FileStatus::Conflicted => Role::Warning,
        FileStatus::Modified | FileStatus::Deleted | FileStatus::TypeChanged => {
            if status.is_staged() {
                Role::Archive
            } else {
                Role::Warning
            }
        }
        FileStatus::Added | FileStatus::Renamed => Role::Archive,
        FileStatus::Unchanged => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rc_core::location::Location;

    /// Build a repository the way the panel tests need it.
    pub fn fixture_repo(name: &str) -> (tempfile::TempDir, Location) {
        let dir = tempfile::Builder::new()
            .prefix(&format!("rc-git-panel-{name}-"))
            .tempdir()
            .unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let mut options = git2::RepositoryInitOptions::new();
        options.initial_head("main");
        let repo = git2::Repository::init_opts(&root, &options).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test Author").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }
        std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("readme.md"), "hi\n").unwrap();
        commit(&repo, "first");
        // A second commit so `old` can point at the first one.
        std::fs::write(root.join("readme.md"), "hi there\n").unwrap();
        let second = commit(&repo, "second");
        let first = repo.revparse_single("HEAD~1").unwrap().id().to_string();
        let first_commit = repo
            .find_commit(git2::Oid::from_str(&first).unwrap())
            .unwrap();
        repo.branch("old", &first_commit, false).unwrap();
        repo.tag_lightweight("v1.0", first_commit.as_object(), false)
            .unwrap();
        let _ = second;
        (dir, Location::git(&root, "", ""))
    }

    fn commit(repo: &git2::Repository, message: &str) -> String {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Test Author", "test@example.com").unwrap();
        let parent = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|id| repo.find_commit(id).ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .unwrap()
        .to_string()
    }

    #[test]
    fn a_repository_root_shows_the_virtual_folders_and_the_working_tree() {
        let (_dir, location) = fixture_repo("root");
        let listing = read_listing(&location).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        for folder in VIRTUAL_FOLDERS {
            assert!(names.contains(&folder), "{folder} missing from {names:?}");
        }
        assert!(names.contains(&"src"));
        assert!(names.contains(&"readme.md"));
        assert_eq!(listing.label, "main");
    }

    #[test]
    fn the_reference_folders_list_branches_tags_and_commits() {
        let (_dir, location) = fixture_repo("refs");
        let branches = read_listing(&location.child("branches").unwrap()).unwrap();
        let names: Vec<&str> = branches.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"main"), "{names:?}");
        assert!(names.contains(&"old"), "{names:?}");

        let tags = read_listing(&location.child("tags").unwrap()).unwrap();
        assert_eq!(tags.entries[0].name, "v1.0");

        let commits = read_listing(&location.child("commits").unwrap()).unwrap();
        assert!(commits.entries.len() >= 2, "{:?}", commits.entries);
        assert!(
            commits.entries[0]
                .name
                .starts_with(|c: char| c.is_ascii_hexdigit())
        );

        let remotes = read_listing(&location.child("remotes").unwrap()).unwrap();
        assert!(
            remotes.entries.is_empty(),
            "there is no remote in this fixture"
        );
    }

    #[test]
    fn a_reference_opens_as_a_tree_and_reads_its_files() {
        let (_dir, location) = fixture_repo("ref");
        let old = location_for_reference(&location, "branches", "old").unwrap();
        let listing = read_listing(&old).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"readme.md"));
        assert_eq!(listing.label, "old");

        // The file content belongs to that revision, not to the working tree.
        let file = read_file(&old, "readme.md", 1024).unwrap();
        assert_eq!(file.bytes, b"hi\n");
        let head = read_file(&location, "readme.md", 1024).unwrap();
        assert_eq!(head.bytes, b"hi there\n");

        // A directory inside the revision.
        let src = old.child("src").unwrap();
        let listing = read_listing(&src).unwrap();
        assert_eq!(listing.entries[0].name, "main.rs");
        // And reading a directory as a file is refused.
        assert!(read_file(&src, "main.rs", 1024).is_ok());
        assert!(read_file(&old, "src", 1024).is_err());
    }

    #[test]
    fn the_working_tree_reports_the_status_of_its_entries() {
        let (dir, location) = fixture_repo("status");
        std::fs::write(dir.path().join("repo/readme.md"), "changed\n").unwrap();
        std::fs::write(dir.path().join("repo/new.txt"), "new\n").unwrap();
        std::fs::write(dir.path().join("repo/debug.log"), "ignored\n").unwrap();
        let listing = read_listing(&location).unwrap();
        let readme = listing.status.get("readme.md").copied().unwrap_or_default();
        assert_eq!(readme.display(), Some(FileStatus::Modified));
        assert_eq!(status_role(readme), Some(rc_core::theme::Role::Warning));
        let new = listing.status.get("new.txt").copied().unwrap_or_default();
        assert_eq!(status_role(new), Some(rc_core::theme::Role::Symlink));
        // A directory inherits the state of its content, so `src` stays clean.
        assert!(
            listing
                .status
                .get("src")
                .copied()
                .unwrap_or_default()
                .is_clean()
        );
    }

    #[test]
    fn a_branch_name_is_not_opened_as_a_folder() {
        let (_dir, location) = fixture_repo("bad");
        // `child("branches")` then `child("main")` builds the path the panel would
        // create when a branch name is entered as a directory.
        let inside = location
            .child("branches")
            .and_then(|l| l.child("main"))
            .unwrap();
        let error = read_listing(&inside).unwrap_err();
        assert!(error.contains("cannot be opened as a folder"), "{error}");
        assert!(location_for_reference(&location, "nope", "x").is_none());
        assert!(is_virtual_folder("tags"));
        assert!(!is_virtual_folder("main"));
    }

    #[test]
    fn staged_changes_and_ignored_files_get_their_own_colours() {
        use rc_core::theme::Role;
        assert_eq!(
            status_role(StatusEntry {
                staged: Some(FileStatus::Added),
                unstaged: None
            }),
            Some(Role::Archive),
            "a staged addition is green"
        );
        assert_eq!(
            status_role(StatusEntry {
                staged: None,
                unstaged: Some(FileStatus::Ignored)
            }),
            Some(Role::Hidden),
            "an ignored file is muted"
        );
        assert_eq!(
            status_role(StatusEntry {
                staged: None,
                unstaged: Some(FileStatus::Conflicted)
            }),
            Some(Role::Warning)
        );
        assert_eq!(status_role(StatusEntry::default()), None);
    }
}

#[cfg(test)]
mod extract_tests {
    use super::tests::fixture_repo;
    use super::*;

    #[test]
    fn a_file_and_a_whole_tree_can_be_taken_out_of_a_revision() {
        let (dir, location) = fixture_repo("extract");
        let branch = location_for_reference(&location, "branches", "old").unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();

        // A single file.
        let file = extract(&branch, "readme.md", &out, ExtractLimits::default()).unwrap();
        assert_eq!(file.files, 1);
        assert_eq!(std::fs::read_to_string(&file.path).unwrap(), "hi\n");

        // A directory tree, recursively, from the repository root.
        let tree = extract(&location, "src", &out, ExtractLimits::default()).unwrap();
        assert_eq!(tree.files, 1);
        assert!(
            out.join("src/main.rs").is_file(),
            "the nested file is written"
        );
        assert!(tree.bytes > 0);

        // The working tree is readable the same way.
        let live = extract(&location, "readme.md", &out, ExtractLimits::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&live.path).unwrap(), "hi there\n");
    }

    #[test]
    fn extraction_respects_the_limits() {
        let (dir, location) = fixture_repo("extract-limits");
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        let tight = ExtractLimits {
            max_file_bytes: 2,
            ..Default::default()
        };
        let error = extract(&location, "readme.md", &out, tight).unwrap_err();
        assert!(error.contains("limit"), "{error}");

        let shallow = ExtractLimits {
            max_depth: 0,
            ..Default::default()
        };
        assert!(
            extract(&location, "src", &out, shallow).is_err(),
            "depth is limited"
        );
    }
}

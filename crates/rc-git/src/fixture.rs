//! Fixture repositories built by the tests themselves, so every later step can be
//! verified offline without a network and without a `git` binary.

use git2::{IndexAddOption, Repository, Signature};
use std::path::PathBuf;

/// A temporary repository with two commits, a branch, a tag, an ignored file and a
/// remote.
pub struct Fixture {
    /// Kept alive so the repository outlives the fixture value.
    _dir: tempfile::TempDir,
    /// Working tree root.
    pub root: PathBuf,
    /// Id of the first commit (`first.txt` = "one").
    pub first: String,
    /// Id of the second commit (`first.txt` = "two", adds `second.txt`).
    pub second: String,
    /// Path of a bare clone used as a `file://` remote.
    pub remote: PathBuf,
}

impl Fixture {
    pub fn new(name: &str) -> Fixture {
        let dir = tempfile::Builder::new()
            .prefix(&format!("rc-git-{name}-"))
            .tempdir()
            .expect("temp dir");
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        // `initial_head` keeps the fixtures identical no matter what the machine's
        // own `init.defaultBranch` says.
        let mut options = git2::RepositoryInitOptions::new();
        options.initial_head("main");
        let repo = Repository::init_opts(&root, &options).expect("init");
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test Author").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }

        std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("first.txt"), "one").unwrap();
        let first = commit(&repo, "first commit");

        std::fs::write(root.join("first.txt"), "two").unwrap();
        std::fs::write(root.join("second.txt"), "second file\n").unwrap();
        std::fs::create_dir_all(root.join("nested/deep")).unwrap();
        std::fs::write(root.join("nested/deep/leaf.txt"), "leaf\n").unwrap();
        let second = commit(&repo, "second commit");

        // A branch and a tag that do not move with the working tree.
        let first_commit = repo
            .find_commit(git2::Oid::from_str(&first).unwrap())
            .unwrap();
        repo.branch("old", &first_commit, false).expect("branch");
        repo.tag_lightweight("v1.0", first_commit.as_object(), false)
            .expect("tag");

        // A bare repository used as a file:// remote.
        let remote = dir.path().join("remote.git");
        let mut bare_options = git2::RepositoryInitOptions::new();
        bare_options.bare(true).initial_head("main");
        Repository::init_opts(&remote, &bare_options).expect("bare init");
        {
            let mut origin = repo.remote("origin", remote.to_str().unwrap()).unwrap();
            origin
                .push(&["refs/heads/main:refs/heads/main"], None)
                .unwrap();
            origin
                .push(&["refs/heads/old:refs/heads/old"], None)
                .unwrap();
        }

        // An ignored file that must not appear in a commit but does exist on disk.
        std::fs::write(root.join("debug.log"), "ignored\n").unwrap();

        Fixture {
            _dir: dir,
            root,
            first,
            second,
            remote,
        }
    }
}

fn commit(repo: &Repository, message: &str) -> String {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test Author", "test@example.com").unwrap();
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

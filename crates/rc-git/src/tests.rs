//! Tests against repositories the suite builds itself.

use super::*;
use crate::fixture::Fixture;

#[test]
fn a_directory_that_is_not_a_repository_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let error = GitRepo::open(dir.path()).unwrap_err();
    assert!(error.is_not_a_repository(), "{error}");
    assert!(error.to_string().contains("not a git repository"));
    assert!(!GitRepo::is_repo(dir.path()));
}

#[test]
fn a_fixture_opens_and_reports_its_branch() {
    let fixture = Fixture::new("open");
    assert!(GitRepo::is_repo(&fixture.root));
    assert!(
        GitRepo::is_repo(fixture.root.join("nested/deep")),
        "discovery walks upwards"
    );
    let repo = GitRepo::open(&fixture.root).unwrap();
    assert!(!repo.is_bare());
    assert_eq!(repo.root(), fixture.root.as_path());
    assert_eq!(repo.head_branch().as_deref(), Some("main"));
    assert!(repo.head_id().is_some());
}

#[test]
fn branches_and_tags_are_listed_sorted() {
    let fixture = Fixture::new("refs");
    let repo = GitRepo::open(&fixture.root).unwrap();
    let branches: Vec<String> = repo.branches().into_iter().map(|r| r.name).collect();
    assert_eq!(branches, vec!["main".to_string(), "old".to_string()]);
    let tags: Vec<String> = repo.tags().into_iter().map(|r| r.name).collect();
    assert_eq!(tags, vec!["v1.0".to_string()]);

    // A tag points at the first commit, the branch at the second.
    let tag = repo.tags().into_iter().next().unwrap();
    assert_eq!(tag.id, fixture.first);
    let main = repo.branches().into_iter().next().unwrap();
    assert_eq!(main.id, fixture.second);
    assert_eq!(tag.revision(RefKind::Tag), Revision::Tag("v1.0".into()));
}

#[test]
fn remote_branches_come_from_the_file_remote() {
    let fixture = Fixture::new("remotes");
    assert!(fixture.remote.is_dir());
    let repo = GitRepo::open(&fixture.root).unwrap();
    let remotes: Vec<String> = repo.remote_branches().into_iter().map(|r| r.name).collect();
    assert_eq!(
        remotes,
        vec!["origin/main".to_string(), "origin/old".to_string()]
    );
    // And the list of every reference names the kinds.
    let all = repo.all_refs();
    assert!(all.iter().any(|(kind, _)| *kind == RefKind::Remote));
    assert!(all.iter().any(|(kind, _)| *kind == RefKind::Tag));
}

#[test]
fn the_log_is_newest_first_and_filtered() {
    let fixture = Fixture::new("log");
    let repo = GitRepo::open(&fixture.root).unwrap();
    let commits = repo.commits(10);
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].id, fixture.second);
    assert_eq!(commits[0].summary, "second commit");
    assert_eq!(commits[0].author, "Test Author");
    assert_eq!(commits[0].short.len(), 8);
    assert_eq!(commits[1].id, fixture.first);
    assert!(commits[0].time >= commits[1].time);
    assert_eq!(repo.commits(1).len(), 1, "the limit is honoured");
}

#[test]
fn the_working_tree_and_a_revision_are_listed_separately() {
    let fixture = Fixture::new("list");
    let repo = GitRepo::open(&fixture.root).unwrap();

    let workdir = repo.list(&Revision::WorkingTree, "").unwrap();
    let names: Vec<&str> = workdir.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"first.txt"));
    assert!(
        names.contains(&"debug.log"),
        "the ignored file exists on disk"
    );
    assert!(names.contains(&"src"));
    // Directories first.
    assert!(workdir[0].is_dir);

    let head = repo.list(&Revision::Head, "").unwrap();
    let names: Vec<&str> = head.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"first.txt"));
    assert!(
        !names.contains(&"debug.log"),
        "the ignored file was never committed"
    );

    // A subdirectory of a revision.
    let src = repo.list(&Revision::Head, "src").unwrap();
    assert_eq!(src.len(), 1);
    assert_eq!(src[0].name, "main.rs");
    assert!(!src[0].is_dir);
    assert_eq!(src[0].size, "fn main() {}\n".len() as u64);
}

#[test]
fn an_old_commit_and_a_tag_show_their_own_content() {
    let fixture = Fixture::new("history");
    let repo = GitRepo::open(&fixture.root).unwrap();

    let head = repo.read_file(&Revision::Head, "first.txt", 1024).unwrap();
    assert_eq!(head, b"two");
    let old = repo
        .read_file(&Revision::Commit(fixture.first.clone()), "first.txt", 1024)
        .unwrap();
    assert_eq!(old, b"one", "an older commit keeps its bytes");
    let tagged = repo
        .read_file(&Revision::Tag("v1.0".into()), "first.txt", 1024)
        .unwrap();
    assert_eq!(tagged, b"one");
    let branch = repo
        .read_file(&Revision::Branch("old".into()), "second.txt", 1024)
        .unwrap_err();
    assert!(branch.to_string().contains("does not exist"), "{branch}");

    // Sizes and directory checks agree with the same view.
    assert_eq!(repo.file_size(&Revision::Head, "first.txt").unwrap(), 3);
    assert!(repo.is_dir(&Revision::Head, "src").unwrap());
    assert!(!repo.is_dir(&Revision::Head, "first.txt").unwrap());
    assert_eq!(
        repo.names(&Revision::Head, "nested").unwrap(),
        vec!["deep".to_string()]
    );
}

#[test]
fn a_missing_path_is_an_error_not_an_empty_list() {
    let fixture = Fixture::new("missing");
    let repo = GitRepo::open(&fixture.root).unwrap();
    let missing = repo.list(&Revision::Head, "nope").unwrap_err();
    assert!(missing.to_string().contains("does not exist"), "{missing}");
    let not_a_dir = repo.list(&Revision::Head, "first.txt").unwrap_err();
    assert!(
        not_a_dir.to_string().contains("not a directory"),
        "{not_a_dir}"
    );
    let unknown = repo
        .list(&Revision::Commit("deadbeef".into()), "")
        .unwrap_err();
    assert!(
        unknown.to_string().contains("unknown revision"),
        "{unknown}"
    );
}

#[test]
fn a_path_may_not_leave_the_revision() {
    let fixture = Fixture::new("escape");
    let repo = GitRepo::open(&fixture.root).unwrap();
    let error = repo
        .read_file(&Revision::Head, "../../etc/passwd", 1024)
        .unwrap_err();
    assert!(error.to_string().contains("leaves the revision"), "{error}");
    assert!(
        repo.list(&Revision::Head, "/etc")
            .unwrap_err()
            .to_string()
            .contains("does not exist")
    );
    // A “..” is refused even in the middle of a path.
    assert!(
        repo.read_file(&Revision::Head, "./src/../first.txt", 1024)
            .is_err(),
        "“..” is refused in the middle too"
    );
    // Harmless spellings are normalised instead of rejected.
    assert!(repo.read_file(&Revision::Head, "./first.txt", 1024).is_ok());
    assert!(repo.read_file(&Revision::Head, "//first.txt", 1024).is_ok());
}

#[test]
fn the_size_limit_protects_the_memory() {
    let fixture = Fixture::new("limit");
    let repo = GitRepo::open(&fixture.root).unwrap();
    let error = repo.read_file(&Revision::Head, "first.txt", 2).unwrap_err();
    assert!(
        error.to_string().contains("more than the 2-byte limit"),
        "{error}"
    );
    // The working-tree path enforces it too.
    let error = repo
        .read_file(&Revision::WorkingTree, "first.txt", 1)
        .unwrap_err();
    assert!(error.to_string().contains("limit"), "{error}");
}

#[test]
fn revisions_round_trip_through_their_panel_spelling() {
    for revision in [
        Revision::WorkingTree,
        Revision::Head,
        Revision::Branch("main".into()),
        Revision::Tag("v1.0".into()),
        Revision::Commit("abc12345".into()),
        Revision::Remote("origin/main".into()),
    ] {
        let key = revision.as_key();
        let back = Revision::from_key(&key);
        assert_eq!(back, revision, "key {key:?}");
    }
    assert_eq!(Revision::from_key(""), Revision::WorkingTree);
    assert_eq!(Revision::from_key("main"), Revision::Commit("main".into()));
    assert_eq!(
        Revision::Branch("feature/x".into()).as_key(),
        "branch:feature/x"
    );
    assert_eq!(Revision::Tag("v1.0".into()).label(), "v1.0");
    assert_eq!(Revision::Commit("0123456789".into()).label(), "01234567");
    assert_eq!(Revision::WorkingTree.label(), "working tree");
}

#[test]
fn a_bare_repository_opens_but_has_no_working_tree() {
    let fixture = Fixture::new("bare");
    let repo = GitRepo::open_exact(&fixture.remote).unwrap();
    assert!(repo.is_bare());
    // Revisions still work: a bare repository is exactly what a clone holds.
    let branches: Vec<String> = repo.branches().into_iter().map(|r| r.name).collect();
    assert_eq!(branches, vec!["main".to_string(), "old".to_string()]);
    assert_eq!(
        repo.read_file(&Revision::Branch("main".into()), "first.txt", 1024)
            .unwrap(),
        b"two"
    );
    // A bare repository still has a symbolic HEAD, so the branch name is known even
    // though nothing is checked out.
    assert_eq!(repo.head_branch().as_deref(), Some("main"));
    assert!(
        repo.list(&Revision::WorkingTree, "").is_err(),
        "a bare repository has no working tree"
    );
}

#[test]
fn the_status_reports_modified_new_ignored_and_staged_files() {
    let fixture = Fixture::new("status");
    let repo = GitRepo::open(&fixture.root).unwrap();

    // A clean tree has nothing to report.
    let clean = repo.status().unwrap();
    assert!(clean.is_clean(), "{:?}", clean.entries);
    assert!(!repo.is_dirty());
    assert_eq!(clean.summary(), "clean");

    // Modify a tracked file, add a new one, and leave the ignored one alone.
    std::fs::write(fixture.root.join("first.txt"), "changed").unwrap();
    std::fs::write(fixture.root.join("brand-new.txt"), "new").unwrap();
    std::fs::write(fixture.root.join("another.log"), "ignored").unwrap();
    let status = repo.status().unwrap();
    assert_eq!(
        status.get("first.txt").unstaged,
        Some(FileStatus::Modified),
        "{:?}",
        status.entries
    );
    assert_eq!(
        status.get("brand-new.txt").unstaged,
        Some(FileStatus::Untracked)
    );
    // Ignored files are answered per path, which is what the panel needs.
    assert!(repo.is_ignored("another.log"));
    assert!(repo.is_ignored("debug.log"));
    assert!(!repo.is_ignored("first.txt"));
    assert_eq!(
        repo.status_of(&status, "another.log").unstaged,
        Some(FileStatus::Ignored)
    );
    assert!(repo.is_dirty());
    assert_eq!(status.modified_count(), 2, "{:?}", status.entries);
    assert_eq!(status.staged_count(), 0);

    // Stage everything: the index side becomes "ready to commit".
    let mut index = repo.repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let staged = repo.status().unwrap();
    assert_eq!(staged.staged_count(), 2, "{:?}", staged.entries);
    assert_eq!(staged.get("first.txt").staged, Some(FileStatus::Modified));
    assert_eq!(staged.get("brand-new.txt").staged, Some(FileStatus::Added));
    assert!(staged.get("first.txt").has_staged());
    assert_eq!(staged.get("first.txt").codes(), "M ");
    assert!(
        staged.summary().contains("2 staged"),
        "{}",
        staged.summary()
    );
}

#[test]
fn a_directory_is_changed_when_something_inside_it_is() {
    let fixture = Fixture::new("dir-status");
    let repo = GitRepo::open(&fixture.root).unwrap();
    std::fs::write(fixture.root.join("src/main.rs"), "fn main() { changed }\n").unwrap();
    let status = repo.status().unwrap();
    assert_eq!(
        repo.status_of(&status, "src").unstaged,
        Some(FileStatus::Modified),
        "the directory inherits the state of its content"
    );
    assert_eq!(
        repo.status_of(&status, "src/main.rs").unstaged,
        Some(FileStatus::Modified)
    );
    assert!(repo.status_of(&status, "nested").is_clean());
    // A path outside the tree is simply clean.
    assert!(repo.status_of(&status, "").is_clean());
}

#[test]
fn switching_branches_moves_the_working_tree_and_never_loses_an_edit() {
    let fixture = Fixture::new("checkout");
    let repo = GitRepo::open(&fixture.root).unwrap();
    assert_eq!(repo.head_branch().as_deref(), Some("main"));

    // A clean tree switches, and the files follow the branch: `old` has no
    // second.txt and no nested directory, so they leave the tree.
    repo.checkout_branch("old", false).unwrap();
    assert_eq!(repo.head_branch().as_deref(), Some("old"));
    assert_eq!(
        repo.read_file(&Revision::WorkingTree, "first.txt", 64)
            .unwrap(),
        b"one"
    );
    assert!(
        !fixture.root.join("second.txt").exists(),
        "a file of the other tree is removed"
    );
    assert!(
        repo.status().unwrap().is_clean(),
        "the switch itself is not a change: {:?}",
        repo.status().unwrap().entries
    );

    // And back again, with the files returning.
    repo.checkout_branch("main", false).unwrap();
    assert_eq!(repo.head_branch().as_deref(), Some("main"));
    assert_eq!(
        repo.read_file(&Revision::WorkingTree, "first.txt", 64)
            .unwrap(),
        b"two"
    );
    assert!(fixture.root.join("second.txt").exists());
    assert!(repo.status().unwrap().is_clean());

    // A local edit blocks a plain switch…
    std::fs::write(fixture.root.join("first.txt"), "uncommitted").unwrap();
    let refused = repo.checkout_branch("old", false).unwrap_err();
    assert!(refused.to_string().contains("has changes"), "{refused}");
    assert_eq!(repo.head_branch().as_deref(), Some("main"), "nothing moved");

    // …and the force flag switches while keeping the edit, which then shows up as a
    // modification of the new branch.
    repo.checkout_branch("old", true).unwrap();
    assert_eq!(repo.head_branch().as_deref(), Some("old"));
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("first.txt")).unwrap(),
        "uncommitted",
        "the local edit survived the switch"
    );
    assert!(repo.status().unwrap().get("first.txt").unstaged.is_some());

    // Unknown and already-current branches are refused with a clear message.
    let unknown = repo.checkout_branch("nope", true).unwrap_err();
    assert!(unknown.to_string().contains("no branch"), "{unknown}");
    let current = repo.checkout_branch("old", true).unwrap_err();
    assert!(
        current.to_string().contains("already checked out"),
        "{current}"
    );
}

#[test]
fn a_branch_can_be_created_from_head() {
    let fixture = Fixture::new("create-branch");
    let repo = GitRepo::open(&fixture.root).unwrap();
    repo.create_branch("feature").unwrap();
    assert_eq!(repo.head_branch().as_deref(), Some("feature"));
    let names: Vec<String> = repo.branches().into_iter().map(|r| r.name).collect();
    assert!(names.contains(&"feature".to_string()), "{names:?}");
    // Creating the same branch twice is refused.
    assert!(repo.create_branch("feature").is_err());
    assert!(repo.create_branch("  ").is_err());
}

#[test]
fn status_codes_and_labels_cover_every_state() {
    for status in [
        FileStatus::Unchanged,
        FileStatus::Modified,
        FileStatus::Added,
        FileStatus::Deleted,
        FileStatus::Renamed,
        FileStatus::Untracked,
        FileStatus::Ignored,
        FileStatus::Conflicted,
        FileStatus::TypeChanged,
    ] {
        assert!(status.label_key().starts_with("git.status."));
    }
    assert_eq!(FileStatus::Modified.code(), 'M');
    assert!(FileStatus::Added.is_staged());
    assert!(!FileStatus::Untracked.is_staged());
    let entry = StatusEntry {
        staged: Some(FileStatus::Added),
        unstaged: Some(FileStatus::Modified),
    };
    assert_eq!(entry.codes(), "AM");
    assert!(entry.has_staged());
    assert_eq!(entry.display(), Some(FileStatus::Modified));
    assert!(StatusEntry::default().is_clean());
    assert_eq!(StatusEntry::default().codes(), "  ");
}

#[test]
fn a_file_can_be_staged_and_committed() {
    let fixture = Fixture::new("commit");
    let repo = GitRepo::open(&fixture.root).unwrap();
    std::fs::write(fixture.root.join("first.txt"), "edited").unwrap();
    std::fs::write(fixture.root.join("added.txt"), "new file\n").unwrap();

    assert!(repo.is_dirty());
    repo.stage_path("first.txt").unwrap();
    repo.stage_path("added.txt").unwrap();
    let status = repo.status().unwrap();
    assert_eq!(status.get("first.txt").staged, Some(FileStatus::Modified));
    assert_eq!(status.get("added.txt").staged, Some(FileStatus::Added));
    assert!(
        status.get("first.txt").unstaged.is_none(),
        "nothing left unstaged"
    );

    let commit = repo
        .commit_staged(
            "editor: save first.txt",
            Some(("Alice", "alice@example.com")),
        )
        .unwrap();
    assert_eq!(commit.summary, "editor: save first.txt");
    assert_eq!(commit.short.len(), 8);
    assert_eq!(commit.id.len(), 40);
    assert!(
        repo.status().unwrap().is_clean(),
        "the tree is clean after committing"
    );
    let head = repo.commits(1).remove(0);
    assert_eq!(head.id, commit.id);
    assert_eq!(head.author, "Alice");
    assert_eq!(repo.head_branch().as_deref(), Some("main"));

    assert!(
        repo.commit_staged("   ", None).is_err(),
        "a message is required"
    );
}

#[test]
fn staging_everything_covers_additions_modifications_and_deletions() {
    let fixture = Fixture::new("stage-all");
    std::fs::write(fixture.root.join("first.txt"), "changed").unwrap();
    std::fs::write(fixture.root.join("new.txt"), "new\n").unwrap();
    std::fs::remove_file(fixture.root.join("second.txt")).unwrap();
    let repo = GitRepo::open(&fixture.root).unwrap();
    repo.stage_all().unwrap();
    let status = repo.status().unwrap();
    assert_eq!(status.get("first.txt").staged, Some(FileStatus::Modified));
    assert_eq!(status.get("new.txt").staged, Some(FileStatus::Added));
    assert_eq!(status.get("second.txt").staged, Some(FileStatus::Deleted));
    let commit = repo.commit_staged("all at once", None).unwrap();
    assert!(commit.files >= 2, "{} files", commit.files);
    assert!(repo.status().unwrap().is_clean());
}

#[test]
fn a_commit_is_pushed_to_the_file_remote() {
    let fixture = Fixture::new("push");
    let repo = GitRepo::open(&fixture.root).unwrap();
    repo.repo
        .find_branch("main", git2::BranchType::Local)
        .unwrap()
        .set_upstream(Some("origin/main"))
        .unwrap();
    std::fs::write(fixture.root.join("first.txt"), "pushed").unwrap();
    repo.stage_path("first.txt").unwrap();
    let commit = repo.commit_staged("push me", None).unwrap();

    let request = repo.push_request().unwrap();
    assert_eq!(request.remote, "origin");
    assert_eq!(request.refspec, "refs/heads/main:refs/heads/main");
    repo.push(&request, None).unwrap();

    let remote = git2::Repository::open(&fixture.remote).unwrap();
    let remote_head = remote
        .find_branch("main", git2::BranchType::Local)
        .unwrap()
        .get()
        .target()
        .unwrap()
        .to_string();
    assert_eq!(remote_head, commit.id, "the commit arrived at the remote");
}

#[test]
fn a_repository_without_a_remote_cannot_be_pushed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("solo");
    std::fs::create_dir_all(&root).unwrap();
    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head("main");
    let repo = git2::Repository::init_opts(&root, &options).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "T").unwrap();
        config.set_str("user.email", "t@example.com").unwrap();
    }
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("T", "t@example.com").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "one", &tree, &[])
        .unwrap();

    let opened = GitRepo::open(&root).unwrap();
    assert!(opened.remotes().is_empty());
    let error = opened.push_request().unwrap_err();
    assert!(error.to_string().contains("no remote"), "{error}");
}

#[test]
fn fetching_updates_the_remote_branches_and_ahead_behind() {
    let fixture = Fixture::new("fetch");
    let repo = GitRepo::open(&fixture.root).unwrap();
    // The remote-tracking reference appears with the first fetch.
    repo.fetch("origin", None).unwrap();
    repo.repo
        .find_branch("main", git2::BranchType::Local)
        .unwrap()
        .set_upstream(Some("origin/main"))
        .unwrap();
    // Locally ahead: one commit that the remote does not have.
    std::fs::write(fixture.root.join("first.txt"), "local").unwrap();
    repo.stage_path("first.txt").unwrap();
    repo.commit_staged("local only", None).unwrap();
    let (ahead, behind) = repo.ahead_behind();
    assert_eq!(ahead, 1, "one commit to push");
    assert_eq!(behind, 0);

    // A fetch of an unchanged remote changes nothing and reports the references.
    let count = repo.fetch("origin", None).unwrap();
    assert!(count >= 1, "remote branches: {count}");
    assert_eq!(repo.ahead_behind(), (1, 0));

    // Pushing brings the two sides back in line.
    let request = repo.push_request().unwrap();
    repo.push(&request, None).unwrap();
    assert_eq!(repo.ahead_behind(), (0, 0));
}

#[test]
fn a_pull_fast_forwards_from_the_remote() {
    let fixture = Fixture::new("pull");
    let repo = GitRepo::open(&fixture.root).unwrap();
    let count = repo.fetch("origin", None).unwrap();
    println!(
        "AFTER FETCH branches={:?} count={count}",
        repo.remote_branches()
            .iter()
            .map(|r| r.name.clone())
            .collect::<Vec<_>>()
    );
    repo.repo
        .find_branch("main", git2::BranchType::Local)
        .unwrap()
        .set_upstream(Some("origin/main"))
        .unwrap();

    // Someone else adds a commit to the remote: clone it, commit, push back.
    let other = git2::Repository::clone(fixture.remote.to_str().unwrap(), fixture.extra()).unwrap();
    {
        let mut config = other.config().unwrap();
        config.set_str("user.name", "Other").unwrap();
        config.set_str("user.email", "o@example.com").unwrap();
    }
    std::fs::write(fixture.extra().join("remote.txt"), "from the other side\n").unwrap();
    let mut index = other.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = other.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Other", "o@example.com").unwrap();
    let parent = other.head().unwrap().peel_to_commit().unwrap();
    other
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "remote work",
            &tree,
            &[&parent],
        )
        .unwrap();
    other
        .find_remote("origin")
        .unwrap()
        .push(&["refs/heads/main:refs/heads/main"], None)
        .unwrap();

    // Fetching alone only moves the tracking branch…
    repo.fetch("origin", None).unwrap();
    assert_eq!(repo.ahead_behind(), (0, 1));
    assert!(
        !fixture.root.join("remote.txt").exists(),
        "the tree is untouched by a fetch"
    );

    // …and the pull fast-forwards the branch and the files.
    let moved = repo.pull_fast_forward().unwrap();
    assert_eq!(moved, 1);
    assert!(fixture.root.join("remote.txt").exists(), "the file arrived");
    assert_eq!(repo.ahead_behind(), (0, 0));
    // Nothing left to pull.
    assert_eq!(repo.pull_fast_forward().unwrap(), 0);
}

#[test]
fn a_pull_refuses_to_run_on_a_dirty_tree_or_a_diverged_branch() {
    let fixture = Fixture::new("pull-guards");
    let repo = GitRepo::open(&fixture.root).unwrap();
    repo.fetch("origin", None).unwrap();
    repo.repo
        .find_branch("main", git2::BranchType::Local)
        .unwrap()
        .set_upstream(Some("origin/main"))
        .unwrap();

    // Nothing to pull: a dirty tree is not in the way, and nothing happens.
    std::fs::write(fixture.root.join("first.txt"), "dirty").unwrap();
    assert_eq!(repo.pull_fast_forward().unwrap(), 0);
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("first.txt")).unwrap(),
        "dirty",
        "the local edit is untouched"
    );

    // Somebody pushed in the meantime: now the dirty tree blocks the pull.
    let other = git2::Repository::clone(fixture.remote.to_str().unwrap(), fixture.extra()).unwrap();
    {
        let mut config = other.config().unwrap();
        config.set_str("user.name", "Other").unwrap();
        config.set_str("user.email", "o@example.com").unwrap();
    }
    std::fs::write(fixture.extra().join("remote.txt"), "theirs\n").unwrap();
    let mut index = other.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = other.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Other", "o@example.com").unwrap();
    let parent = other.head().unwrap().peel_to_commit().unwrap();
    other
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "remote work",
            &tree,
            &[&parent],
        )
        .unwrap();
    other
        .find_remote("origin")
        .unwrap()
        .push(&["refs/heads/main:refs/heads/main"], None)
        .unwrap();
    repo.fetch("origin", None).unwrap();
    assert_eq!(repo.ahead_behind(), (0, 1));
    let error = repo.pull_fast_forward().unwrap_err();
    assert!(error.to_string().contains("has changes"), "{error}");

    // A clean tree that is ahead of the remote has nothing to fast-forward to.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.repo.checkout_head(Some(&mut checkout)).unwrap();
    std::fs::write(fixture.root.join("local.txt"), "mine\n").unwrap();
    repo.stage_path("local.txt").unwrap();
    repo.commit_staged("ahead", None).unwrap();
    let error = repo.pull_fast_forward().unwrap_err();
    assert!(
        error.to_string().contains("diverged") || error.to_string().contains("push them first"),
        "{error}"
    );
}

#[test]
fn a_branch_with_its_commits_merges_fast_forward() {
    let fixture = Fixture::new("merge");
    let repo = GitRepo::open(&fixture.root).unwrap();
    // Merging an ancestor into the current branch changes nothing.
    assert_eq!(repo.merge_branch("old").unwrap(), 0);
    assert_eq!(repo.head_branch().as_deref(), Some("main"));

    // From `old` (the first commit), `main` is ahead: that is a fast-forward.
    repo.checkout_branch("old", false).unwrap();
    let moved = repo.merge_branch("main").unwrap();
    assert_eq!(moved, 1);
    assert_eq!(repo.commits(1).remove(0).summary, "second commit");
    assert!(repo.status().unwrap().is_clean());
    // Merging the same branch again is a no-op.
    assert_eq!(repo.merge_branch("main").unwrap(), 0);
    // An unknown branch is a clear error.
    assert!(
        repo.merge_branch("nope")
            .unwrap_err()
            .to_string()
            .contains("no branch")
    );
}

#[test]
fn paths_can_be_staged_and_unstaged_one_by_one() {
    let fixture = Fixture::new("stage-paths");
    let repo = GitRepo::open(&fixture.root).unwrap();
    std::fs::write(fixture.root.join("first.txt"), "one edit").unwrap();
    std::fs::write(fixture.root.join("other.txt"), "new").unwrap();

    let staged = repo
        .stage_paths(&["first.txt".to_string(), "other.txt".to_string()])
        .unwrap();
    assert_eq!(staged, 2);
    let status = repo.status().unwrap();
    assert!(status.get("first.txt").has_staged());
    assert!(status.get("other.txt").has_staged());

    // Unstaging one path leaves the other staged.
    let unstaged = repo.unstage_paths(&["first.txt".to_string()]).unwrap();
    assert_eq!(unstaged, 1);
    let status = repo.status().unwrap();
    assert!(
        !status.get("first.txt").has_staged(),
        "back to the HEAD version"
    );
    assert_eq!(status.get("first.txt").unstaged, Some(FileStatus::Modified));
    assert!(status.get("other.txt").has_staged());

    // Only the staged file goes into the commit.
    let commit = repo.commit_staged("stage one", None).unwrap();
    assert!(commit.files >= 1);
    let status = repo.status().unwrap();
    assert!(
        status.get("other.txt").is_clean(),
        "the staged addition was committed"
    );
    assert_eq!(
        status.get("first.txt").unstaged,
        Some(FileStatus::Modified),
        "the unstaged edit is still there"
    );
    // Unknown paths are reported instead of ignored silently.
    assert!(repo.stage_paths(&["nope/missing.txt".to_string()]).is_err());
}

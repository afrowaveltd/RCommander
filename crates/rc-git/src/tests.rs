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

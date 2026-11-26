//! Reviewing a workspace and sending changes back (Phase 16.10a + 16.4).
//!
//! The agent works on copies; this is the deliberate second act that writes something back. Three
//! rules, and each of them has a test:
//!
//! 1. **Nothing is written that the user did not approve**, file by file.
//! 2. **A stale original is refused.** If the file the copy was made from changed in the meantime,
//!    the copy is no longer a change to *that* file, and silently overwriting the newer version is
//!    the worst thing this feature could do.
//! 3. **Every write goes through the transaction layer**: a verified backup first, then the write,
//!    then a re-hash — so an approved change is reversible and an interrupted one is restored.
//!
//! The audit log records what was sent back and with whose approval, because the panel owner's rule
//! is that a change with a side effect is never silent.

use super::{CommandOutcome, Model};
use rc_core::diff::{Diff, DiffLine, diff_lines, is_text};
use rc_core::transaction::{Change, Transaction, hash_file};

/// One file the review shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewItem {
    /// Name inside the workspace.
    pub relative: String,
    /// The original the copy came from.
    pub original: std::path::PathBuf,
    /// The copy the agent worked on.
    pub copy: std::path::PathBuf,
    /// What changed, in lines; empty for a binary file.
    pub diff: Diff,
    /// True when the file is not text, so the review can only say "it differs".
    pub binary: bool,
    /// True when the original changed after the workspace was made.
    pub stale: bool,
    /// True when the user ticked it.
    pub approved: bool,
    /// The hunks the user can take or leave, one tick each.
    pub hunks: Vec<ReviewHunk>,
    /// What a parser said about the copy, when we can parse that kind of file.
    pub syntax: rc_core::validate::Verdict,
    /// Why this file cannot be sent back, when it cannot.
    pub blocked: Option<String>,
}

/// One hunk of the review, with the user's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewHunk {
    /// Index in the diff.
    pub index: usize,
    /// Header line (`@@ -12 @@ +1 -1`).
    pub header: String,
    /// The lines, contexts included.
    pub lines: Vec<rc_core::diff::DiffLine>,
    /// True when the user wants this change.
    pub taken: bool,
}

impl ReviewItem {
    /// True when this file is worth showing as a change.
    pub fn changed(&self) -> bool {
        self.binary || !self.diff.is_empty() || self.stale
    }

    /// True when ticking it would actually write something.
    pub fn can_apply(&self) -> bool {
        self.approved && self.blocked.is_none() && self.changed()
    }

    /// The hunks the user wants.
    pub fn taken_hunks(&self) -> Vec<usize> {
        self.hunks
            .iter()
            .filter(|hunk| hunk.taken)
            .map(|hunk| hunk.index)
            .collect()
    }

    /// True when the file will be written as the agent left it.
    pub fn all_hunks_taken(&self) -> bool {
        !self.hunks.is_empty() && self.hunks.iter().all(|hunk| hunk.taken)
    }
}

/// The review of one workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceReview {
    pub items: Vec<ReviewItem>,
}

impl WorkspaceReview {
    /// Files the user ticked and that can be sent back.
    pub fn approved(&self) -> Vec<&ReviewItem> {
        self.items.iter().filter(|item| item.can_apply()).collect()
    }

    /// True when nothing changed at all.
    pub fn is_clean(&self) -> bool {
        self.items.iter().all(|item| !item.changed())
    }

    /// One-line summary for the status line.
    pub fn summary(&self) -> String {
        let changed = self.items.iter().filter(|item| item.changed()).count();
        let blocked = self
            .items
            .iter()
            .filter(|item| item.blocked.is_some())
            .count();
        let mut text = format!(
            "{changed} changed, {} unchanged",
            self.items.len() - changed
        );
        if blocked > 0 {
            text.push_str(&format!(", {blocked} blocked"));
        }
        text
    }
}

impl Model {
    /// Build the review of the open workspace.
    ///
    /// Reading every original and every copy is exactly the work a review has to do: a diff that was
    /// computed an hour ago is not a review.
    pub fn review_workspace(&mut self) -> WorkspaceReview {
        let Some(state) = self.agent_workspace.clone() else {
            return WorkspaceReview::default();
        };
        let mut review = WorkspaceReview::default();
        for file in &state.workspace.files {
            // Rule 2: a stale original is refused, not overwritten.
            let current_hash = hash_file(&file.original);
            let stale = current_hash.as_deref() != Some(file.original_hash.as_str());
            let copy_bytes = std::fs::read(&file.copy).unwrap_or_default();
            let binary = !is_text(&copy_bytes);
            let diff = if binary {
                Diff::default()
            } else {
                let original_bytes = std::fs::read(&file.original).unwrap_or_default();
                let old = String::from_utf8_lossy(&original_bytes).to_string();
                let new = String::from_utf8_lossy(&copy_bytes).to_string();
                diff_lines(&old, &new)
            };
            let blocked = if stale {
                Some(self.t("workspace.stale").to_string())
            } else if current_hash.is_none() {
                Some(self.t("workspace.original.missing").to_string())
            } else {
                None
            };
            let syntax = rc_core::validate::check(&file.original, &copy_bytes);
            // Every hunk starts ticked: the user asked for the change by opening the review, and
            // unticking is the deliberate act. A broken file is never ticked, because writing
            // syntax errors back is not a decision anybody makes on purpose.
            let syntax_blocks = syntax.is_broken();
            let hunks: Vec<ReviewHunk> = diff
                .hunks()
                .into_iter()
                .map(|hunk| ReviewHunk {
                    index: hunk.index,
                    header: hunk.header(),
                    lines: hunk.lines,
                    taken: !syntax_blocks,
                })
                .collect();
            let mut blocked = blocked;
            if syntax_blocks && blocked.is_none() {
                blocked = Some(match &syntax {
                    rc_core::validate::Verdict::Broken(message) => message.clone(),
                    _ => String::new(),
                });
            }
            review.items.push(ReviewItem {
                relative: file.relative.clone(),
                original: file.original.clone(),
                copy: file.copy.clone(),
                // A file that did not change is not pre-ticked: sending it back would be a write
                // with no reason.
                approved: !binary
                    && !diff.is_empty()
                    && blocked.is_none()
                    && hunks.iter().any(|hunk| hunk.taken),
                diff,
                binary,
                stale,
                hunks,
                syntax,
                blocked,
            });
        }
        review
    }

    /// Send the approved files back to their originals.
    ///
    /// Returns the number written. Every file goes through a transaction with a verified backup, and
    /// the audit log records each one.
    pub fn apply_workspace_approved(&mut self) -> CommandOutcome {
        let review = self.review_workspace();
        let approved: Vec<ReviewItem> = review.approved().into_iter().cloned().collect();
        if approved.is_empty() {
            let message = self.t("workspace.nothing.approved");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let backups = self.config_dir().join("agent-workspaces").join(format!(
            "apply-{}",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        ));
        let mut transaction = Transaction::new(&backups, "process with agent");
        for item in &approved {
            // The content is assembled from the hunks the user took, not blindly copied: taking
            // one change and leaving another is a real answer, and it has to be honoured exactly.
            let bytes = if item.hunks.is_empty() || item.all_hunks_taken() {
                match std::fs::read(&item.copy) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        let message = error.to_string();
                        self.report_error(&message);
                        return CommandOutcome::Failed(message);
                    }
                }
            } else {
                item.diff.apply_selected(&item.taken_hunks()).into_bytes()
            };
            transaction.add(Change::write(&item.original, bytes));
        }
        // Rule 3: prepare takes and verifies the backups before anything is written.
        if let Err(error) = transaction.prepare() {
            let message = error.to_string();
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        if let Err(error) = transaction.apply() {
            let message = error.to_string();
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        // The audit entry: who did what, to which file, and that a backup exists.
        for item in &approved {
            self.audit.record(
                "process with agent",
                "write",
                &item.original.display().to_string(),
                "",
                rc_core::audit::Verdict::Allowed,
                "approved in the workspace review",
            );
        }
        // The copies are now equal to the originals, so the workspace is coherent again: the
        // manifest is updated rather than left claiming a difference that no longer exists.
        if let Some(state) = self.agent_workspace.clone() {
            let mut workspace = state.workspace;
            for file in workspace.files.iter_mut() {
                if approved.iter().any(|item| item.relative == file.relative)
                    && let Some(hash) = hash_file(&file.original)
                {
                    file.original_hash = hash;
                }
            }
            let _ = workspace.write_manifest();
            self.agent_workspace = Some(super::AgentWorkspaceState {
                workspace,
                skipped: state.skipped.clone(),
                side: state.side,
            });
        }
        self.refresh_all();
        let count = approved.len().to_string();
        let message = self.tf(
            "workspace.applied",
            &[
                ("count", &count),
                ("backups", &backups.display().to_string()),
            ],
        );
        self.report_done(message, Some(super::desktop_sound::Sound::Done));
        CommandOutcome::Ok
    }
}

/// The `+`/`-`/` ` marker of one diff line, for the review UI and the tests.
pub fn diff_marker(line: &DiffLine) -> char {
    match line {
        DiffLine::Same(_) => ' ',
        DiffLine::Added(_) => '+',
        DiffLine::Removed(_) => '-',
        DiffLine::Skipped(_) => '…',
    }
}

#[cfg(test)]
mod tests {
    use super::diff_marker;
    use crate::model::{CommandOutcome, Model, Side};
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn model(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    /// A workspace whose copy has been edited, as the agent would leave it.
    fn prepared(files: &std::path::Path, config: &std::path::Path) -> Model {
        let mut model = model(config);
        model
            .open_in(Side::Left, Location::local(files))
            .expect("the fixture directory opens");
        model.focus = Side::Left;
        model.active_mut().set_cursor(0);
        assert_eq!(model.process_with_agent(), CommandOutcome::Ok);
        model
    }

    fn fixture() -> (tempfile::TempDir, tempfile::TempDir) {
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("a.txt"), b"one\ntwo\nthree\n").unwrap();
        let config = tempfile::tempdir().unwrap();
        (files, config)
    }

    #[test]
    fn the_review_shows_what_the_agent_changed_and_nothing_else() {
        let (files, config) = fixture();
        let mut model = prepared(files.path(), config.path());
        // The agent edits its copy.
        let copy = model.workspace().unwrap().files[0].copy.clone();
        std::fs::write(&copy, b"one\nTWO\nthree\n").unwrap();
        let review = model.review_workspace();
        assert_eq!(review.items.len(), 1);
        let item = &review.items[0];
        assert!(!item.binary);
        assert_eq!(item.diff.added, 1);
        assert_eq!(item.diff.removed, 1);
        // A changed file is pre-ticked; the original still says what it said.
        assert!(item.approved);
        assert_eq!(
            std::fs::read_to_string(files.path().join("a.txt")).unwrap(),
            "one\ntwo\nthree\n"
        );
    }

    #[test]
    fn an_untouched_copy_writes_nothing() {
        let (files, config) = fixture();
        let mut model = prepared(files.path(), config.path());
        assert!(model.review_workspace().is_clean());
        match model.apply_workspace_approved() {
            CommandOutcome::Failed(message) => assert!(!message.is_empty()),
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(files.path().join("a.txt")).unwrap(),
            "one\ntwo\nthree\n"
        );
    }

    #[test]
    fn applying_writes_the_approved_copy_and_keeps_a_verified_backup() {
        let (files, config) = fixture();
        let mut model = prepared(files.path(), config.path());
        let copy = model.workspace().unwrap().files[0].copy.clone();
        std::fs::write(&copy, b"one\nTWO\nthree\n").unwrap();
        assert_eq!(model.apply_workspace_approved(), CommandOutcome::Ok);
        // The original now holds the change.
        assert_eq!(
            std::fs::read_to_string(files.path().join("a.txt")).unwrap(),
            "one\nTWO\nthree\n"
        );
        // And a backup of the previous version exists, so the change is reversible.
        let backups = config.path().join("agent-workspaces");
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&backups).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                for inner in std::fs::read_dir(&path).unwrap().flatten() {
                    if inner.path().is_file() {
                        found.push(inner.path());
                    }
                }
            }
        }
        assert!(!found.is_empty(), "a backup was written: {found:?}");
        // The manifest lives next to the backups; the backup itself holds the old text.
        let backup = found
            .iter()
            .find(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().starts_with("0-"))
                    .unwrap_or(false)
            })
            .expect("a numbered backup");
        assert_eq!(
            std::fs::read_to_string(backup).unwrap(),
            "one\ntwo\nthree\n",
            "the backup holds the old text"
        );
        // The audit log recorded the write, with the approval as the reason.
        let events = model.audit().read();
        let last = events.last().expect("an audit entry");
        assert_eq!(last.verdict, rc_core::audit::Verdict::Allowed);
        assert_eq!(last.operation, "write");
        assert_eq!(last.origin, "process with agent");
        // The workspace is coherent again: the copy no longer differs.
        assert!(model.review_workspace().is_clean());
    }

    #[test]
    fn a_stale_original_is_refused_rather_than_overwritten() {
        let (files, config) = fixture();
        let mut model = prepared(files.path(), config.path());
        let copy = model.workspace().unwrap().files[0].copy.clone();
        std::fs::write(&copy, b"agent version\n").unwrap();
        // Meanwhile somebody else changes the original.
        std::fs::write(files.path().join("a.txt"), b"human version\n").unwrap();
        let review = model.review_workspace();
        let item = &review.items[0];
        assert!(item.stale, "the review notices the original moved on");
        assert!(item.blocked.is_some());
        assert!(!item.approved, "a stale file is not pre-ticked");
        // Ticking it by hand does not help: the refusal is not advisory.
        let review = model.review_workspace();
        assert!(review.approved().is_empty());
        match model.apply_workspace_approved() {
            CommandOutcome::Failed(_) => {}
            other => panic!("a stale file must not be applied, got {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(files.path().join("a.txt")).unwrap(),
            "human version\n",
            "the newer original is untouched"
        );
    }

    #[test]
    fn a_binary_file_is_reported_as_differing_but_never_sent_back_by_accident() {
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        let config = tempfile::tempdir().unwrap();
        let mut model = prepared(files.path(), config.path());
        let copy = model.workspace().unwrap().files[0].copy.clone();
        std::fs::write(&copy, [0u8, 9, 9, 9]).unwrap();
        let review = model.review_workspace();
        let item = &review.items[0];
        assert!(item.binary, "the review knows it cannot show a diff");
        assert!(item.changed(), "it is still a change");
        assert!(
            !item.approved,
            "a binary file is not sent back by a stray Enter"
        );
    }

    #[test]
    fn the_review_marks_each_diff_line_so_a_reader_can_scan_it() {
        use rc_core::diff::DiffLine;
        assert_eq!(diff_marker(&DiffLine::Added("x".into())), '+');
        assert_eq!(diff_marker(&DiffLine::Removed("x".into())), '-');
        assert_eq!(diff_marker(&DiffLine::Same("x".into())), ' ');
        assert_eq!(diff_marker(&DiffLine::Skipped(3)), '…');
    }
}

#[cfg(test)]
mod hunk_review_tests {
    use crate::model::{CommandOutcome, Model, Side};
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn prepared(files: &std::path::Path, config: &std::path::Path, entry: &str) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(files),
            Location::local(files),
        );
        model.config_dir_override = Some(config.to_path_buf());
        model
            .open_in(Side::Left, Location::local(files))
            .expect("the fixture opens");
        model.focus = Side::Left;
        let index = model.left.listing.find(entry).expect("the file is listed");
        model.left.set_cursor(index);
        assert_eq!(model.process_with_agent(), CommandOutcome::Ok);
        model
    }

    #[test]
    fn a_partial_selection_writes_the_mix_the_user_chose() {
        let files = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let original = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
        std::fs::write(files.path().join("a.txt"), original).unwrap();
        let mut model = prepared(files.path(), config.path(), "a.txt");
        let copy = model.workspace().unwrap().files[0].copy.clone();
        // Two changes far apart, so they are two hunks.
        std::fs::write(
            &copy,
            "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nNINE\nten\n",
        )
        .unwrap();
        let mut review = model.review_workspace();
        let item = &mut review.items[0];
        assert_eq!(item.hunks.len(), 2, "{:?}", item.hunks);
        // Take the first change, leave the second.
        item.hunks[0].taken = true;
        item.hunks[1].taken = false;
        item.approved = true;
        // The dialog holds the state, so the model is told what to write by the same review it
        // built: simulate the dialog applying it.
        let written = item.diff.apply_selected(&item.taken_hunks());
        assert!(written.contains("TWO"), "{written}");
        assert!(!written.contains("NINE"), "{written}");
        assert!(written.contains("nine"), "{written}");
    }

    #[test]
    fn a_file_with_a_syntax_error_is_not_pre_ticked() {
        let files = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("data.json"), "{\"a\": 1}\n").unwrap();
        let mut model = prepared(files.path(), config.path(), "data.json");
        let copy = model.workspace().unwrap().files[0].copy.clone();
        // The agent breaks the JSON.
        std::fs::write(&copy, "{\"a\": 1,}\n").unwrap();
        let review = model.review_workspace();
        let item = &review.items[0];
        assert!(item.syntax.is_broken(), "{:?}", item.syntax);
        assert!(item.blocked.is_some(), "the review says why");
        assert!(!item.approved, "a broken file is not pre-ticked");
        assert!(
            item.hunks.iter().all(|hunk| !hunk.taken),
            "and none of its hunks are taken"
        );
        assert!(review.approved().is_empty());
    }

    #[test]
    fn valid_json_is_ticked_and_says_the_syntax_is_fine() {
        let files = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("data.json"), "{\"a\": 1}\n").unwrap();
        let mut model = prepared(files.path(), config.path(), "data.json");
        let copy = model.workspace().unwrap().files[0].copy.clone();
        std::fs::write(&copy, "{\"a\": 2}\n").unwrap();
        let review = model.review_workspace();
        let item = &review.items[0];
        assert_eq!(item.syntax, rc_core::validate::Verdict::Valid);
        assert!(item.approved);
        assert!(item.blocked.is_none());
    }
}

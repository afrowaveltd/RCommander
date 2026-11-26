//! "Process with agent" in the application model (Phase 16.10a).
//!
//! The heavy lifting — copies, hashes, the manifest, discarding — is `rc_core::workspace`. This
//! module connects it to the app: it builds the workspace from the current selection, opens the
//! chat beside it, seeds the conversation with what the agent may look at, and holds the workspace
//! open until the user discards it.
//!
//! The safety story is in the two sentences above the code: **the agent gets copies**, and
//! **the originals are untouched until a reviewed apply** (which is Phase 16.4 and not part of this
//! slice). Everything here is therefore reversible by construction: discarding the workspace is
//! enough.

use super::{CommandOutcome, Model, Side};
use rc_core::workspace::{Skipped, Workspace};

impl Model {
    /// The workspace that is currently open, if any.
    pub fn workspace(&self) -> Option<&Workspace> {
        self.agent_workspace.as_ref().map(|state| &state.workspace)
    }

    /// True when a workspace is open, so the menu can offer to close it.
    pub fn has_workspace(&self) -> bool {
        self.agent_workspace.is_some()
    }

    /// What the workspace cannot do, for the transcript and the status line.
    pub fn workspace_skipped(&self) -> &[Skipped] {
        self.agent_workspace
            .as_ref()
            .map(|state| state.skipped.as_slice())
            .unwrap_or(&[])
    }

    /// Open a workspace for the current selection and start the conversation about it.
    ///
    /// The chat goes into the *other* panel, so the files the user was looking at stay visible —
    /// which is the arrangement the whole feature assumes.
    pub fn process_with_agent(&mut self) -> CommandOutcome {
        if self.agent_workspace.is_some() {
            let message = self.t("workspace.busy");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        // Marks when there are any, otherwise the cursor row — the same rule every other file
        // operation follows.
        let entries: Vec<rc_core::entry::Entry> = self.active().operation_targets().to_vec();
        if entries.is_empty() {
            let message = self.t("workspace.empty");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let label = if entries.len() == 1 {
            entries[0].name.clone()
        } else {
            self.tf("workspace.label", &[("count", &entries.len().to_string())])
        };
        // Each workspace gets its own directory, named after the moment it was made: two runs a
        // minute apart must not share copies.
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let directory = self
            .config_dir()
            .join("agent-workspaces")
            .join(format!("{stamp}-{}", std::process::id()));
        let prepared = Workspace::prepare(&directory, label.clone(), &entries);
        let (workspace, skipped) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };

        // The conversation starts with what the agent may see, and with the reminder that it is a
        // copy. A model that does not know that will happily offer to edit the original.
        let summary = workspace.summary();
        let directory_text = workspace.directory.display().to_string();
        let note = self.tf(
            "workspace.note",
            &[("files", &summary), ("directory", &directory_text)],
        );
        self.chat_session.lines.clear();
        self.chat_note(note);
        for skip in &skipped {
            let line = self.tf(
                "workspace.skipped",
                &[
                    ("path", &skip.path.display().to_string()),
                    ("reason", &skip.reason),
                ],
            );
            self.chat_note(line);
        }
        self.chat_session.input = self.t("workspace.question").to_string();
        self.agent_workspace = Some(super::AgentWorkspaceState {
            workspace,
            skipped,
            side: self.focus.other(),
        });
        // The chat goes beside the files, and the files keep the focus so the user can see what
        // they asked about while reading the question.
        self.set_panel_chat(self.focus.other(), true);
        self.focus = self.focus.other().other();
        let message = self.tf("workspace.opened", &[("files", &summary)]);
        self.report_done(message, None);
        CommandOutcome::Ok
    }

    /// Close the workspace and delete its copies.
    pub fn discard_workspace(&mut self) -> CommandOutcome {
        let Some(state) = self.agent_workspace.take() else {
            return CommandOutcome::None;
        };
        let side = state.side;
        if self.panel_shows_chat(side) {
            self.set_panel_chat(side, false);
        }
        match state.workspace.discard() {
            Ok(()) => {
                self.chat_note(self.t("workspace.discarded").to_string());
                self.report(self.t("workspace.discarded"));
                CommandOutcome::Ok
            }
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// Show the workspace in a panel, so the user can look at what the agent sees.
    pub fn open_workspace_directory(&mut self) -> CommandOutcome {
        let Some(workspace) = self.workspace().cloned() else {
            return CommandOutcome::None;
        };
        let location = rc_core::location::Location::local(&workspace.directory);
        let side = self.focus;
        match self.open_in(side, location) {
            Ok(()) => CommandOutcome::Ok,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// Which panel hosts the chat of a running workspace.
    pub fn workspace_side(&self) -> Option<Side> {
        self.agent_workspace.as_ref().map(|state| state.side)
    }
}

#[cfg(test)]
mod tests {
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

    fn fixture() -> (tempfile::TempDir, tempfile::TempDir) {
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("report.txt"), b"the original").unwrap();
        let config = tempfile::tempdir().unwrap();
        (files, config)
    }

    #[test]
    fn processing_with_the_agent_gives_it_a_copy_and_opens_the_chat_beside_the_files() {
        let (files, config) = fixture();
        let mut model = model(config.path());
        model
            .open_in(Side::Left, Location::local(files.path()))
            .unwrap();
        model.focus = Side::Left;
        model.active_mut().set_cursor(0);
        assert_eq!(model.process_with_agent(), CommandOutcome::Ok);
        // The workspace exists with a copy, and the original is untouched.
        let workspace = model.workspace().cloned().expect("a workspace");
        assert_eq!(workspace.files.len(), 1);
        assert!(workspace.files[0].copy.is_file());
        assert_eq!(
            std::fs::read_to_string(files.path().join("report.txt")).unwrap(),
            "the original"
        );
        // The chat is in the other panel, and it says what the agent may see.
        assert!(model.panel_shows_chat(Side::Right));
        assert!(!model.panel_shows_chat(Side::Left));
        // The transcript opens with a note from the application (the test translator echoes the
        // key, so the structure is what is checked here; the rendered wording is covered by the
        // i18n key test and by the help topic).
        use crate::model::chat::Speaker;
        let first = model.chat_session.lines.first().expect("a note");
        assert_eq!(
            first.speaker,
            Speaker::System,
            "the note is the application's"
        );
        assert!(!first.text.is_empty());
        // The workspace summary names the copy, without any translation in the way.
        assert!(
            model.workspace().unwrap().summary().contains("report.txt"),
            "{}",
            model.workspace().unwrap().summary()
        );
        // And the question is waiting in the input.
        assert!(!model.chat_session.input.trim().is_empty());
    }

    #[test]
    fn discarding_removes_the_copies_and_leaves_the_original() {
        let (files, config) = fixture();
        let mut model = model(config.path());
        model
            .open_in(Side::Left, Location::local(files.path()))
            .unwrap();
        model.focus = Side::Left;
        model.active_mut().set_cursor(0);
        model.process_with_agent();
        let directory = model.workspace().unwrap().directory.clone();
        assert!(directory.exists());
        assert_eq!(model.discard_workspace(), CommandOutcome::Ok);
        assert!(!directory.exists(), "the copies are gone");
        assert!(
            files.path().join("report.txt").is_file(),
            "the original stays"
        );
        assert!(model.workspace().is_none());
        // The panel that hosted the chat shows files again.
        assert!(!model.panel_shows_chat(Side::Right));
    }

    #[test]
    fn an_empty_selection_is_refused_with_an_explanation() {
        let empty = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let mut model = model(config.path());
        model
            .open_in(Side::Left, Location::local(empty.path()))
            .unwrap();
        model.focus = Side::Left;
        // An empty folder has nothing under the cursor, so there is nothing to copy.
        match model.process_with_agent() {
            CommandOutcome::Failed(message) => assert!(!message.is_empty()),
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert!(model.workspace().is_none());
    }

    #[test]
    fn a_second_workspace_is_refused_while_one_is_open() {
        let (files, config) = fixture();
        let mut model = model(config.path());
        model
            .open_in(Side::Left, Location::local(files.path()))
            .unwrap();
        model.focus = Side::Left;
        model.active_mut().set_cursor(0);
        model.process_with_agent();
        match model.process_with_agent() {
            CommandOutcome::Failed(_) => {}
            other => panic!("a second workspace must be refused, got {other:?}"),
        }
    }
}

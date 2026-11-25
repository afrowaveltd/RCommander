//! Showing the agent chat **inside a panel** instead of in its own window (Phase 16.1).
//!
//! A floating window covers the files it is talking about. A panel can host the conversation and
//! leave the other panel a normal file list, which is the arrangement the plan calls for: the chat
//! on one side, the directory it is discussing on the other.
//!
//! The whole point of putting it in a panel is that the panel keeps its state: the directory, the
//! selection, the tab history and the cursor row are untouched while the chat is shown, so
//! switching back is instant and lands exactly where the user was.

use super::{CommandOutcome, Model, Side};

impl Model {
    /// True when this panel is showing the chat instead of a file list.
    pub fn panel_shows_chat(&self, side: Side) -> bool {
        self.panel(side).chat
    }

    /// Show or hide the chat in a panel; the listing stays loaded either way.
    pub fn set_panel_chat(&mut self, side: Side, chat: bool) -> CommandOutcome {
        let shown = match side {
            Side::Left => &mut self.left.chat,
            Side::Right => &mut self.right.chat,
        };
        if *shown == chat {
            return CommandOutcome::None;
        }
        *shown = chat;
        if chat {
            // The conversation is what the user wants to look at, so the pane takes the focus.
            self.focus = side;
            let text = self.t("agent.pane.opened");
            self.report(text);
        } else {
            let text = self.t("agent.pane.closed");
            self.report(text);
        }
        CommandOutcome::Ok
    }

    /// Toggle the chat in a panel.
    pub fn toggle_panel_chat(&mut self, side: Side) -> CommandOutcome {
        let chat = !self.panel_shows_chat(side);
        self.set_panel_chat(side, chat)
    }

    /// Put the chat in the panel that is *not* focused, so the current files stay visible.
    pub fn open_chat_beside_files(&mut self) -> CommandOutcome {
        // The other panel is the passive one: the user asked to keep looking at these files.
        self.set_panel_chat(self.focus.other(), true)
    }

    /// True when either panel is showing the chat.
    pub fn any_panel_shows_chat(&self) -> bool {
        self.left.chat || self.right.chat
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{CommandOutcome, Model, Side};
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn model(fixture: &std::path::Path) -> Model {
        let dir = std::env::temp_dir().join(format!("rc-chatpane-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(fixture),
            Location::local(&dir),
        );
        model.config_dir_override = Some(dir);
        model
    }

    #[test]
    fn opening_the_chat_beside_the_files_keeps_them_visible() {
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("a.txt"), b"a").unwrap();
        let mut model = model(files.path());
        model.focus = Side::Left;
        assert_eq!(model.open_chat_beside_files(), CommandOutcome::Ok);
        assert!(!model.panel_shows_chat(Side::Left), "the files stay");
        assert!(
            model.panel_shows_chat(Side::Right),
            "the chat is beside them"
        );
        assert_eq!(model.focus, Side::Right, "the chat takes the focus");
        assert!(model.any_panel_shows_chat());
    }

    #[test]
    fn switching_back_lands_exactly_where_the_user_was() {
        let files = tempfile::tempdir().unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(files.path().join(name), b"x").unwrap();
        }
        let mut model = model(files.path());
        model.focus = Side::Left;
        // Move the cursor and mark a file, then hide the list behind the chat.
        model.active_mut().set_cursor(1);
        model.active_mut().toggle_current();
        let cursor_before = model.active().cursor();
        let directory_before = model
            .panel(Side::Left)
            .location()
            .as_path()
            .map(|path| path.to_path_buf())
            .expect("a local directory");
        model.set_panel_chat(Side::Left, true);
        assert!(model.panel_shows_chat(Side::Left));
        // The listing is still there underneath, untouched.
        assert_eq!(model.panel(Side::Left).cursor(), cursor_before);
        assert_eq!(model.panel(Side::Left).entries().len(), 3);
        model.set_panel_chat(Side::Left, false);
        assert!(!model.panel_shows_chat(Side::Left));
        assert_eq!(model.panel(Side::Left).cursor(), cursor_before);
        assert_eq!(
            model
                .panel(Side::Left)
                .location()
                .as_path()
                .map(|path| path.to_path_buf()),
            Some(directory_before)
        );
    }

    #[test]
    fn toggling_is_idempotent_and_reports() {
        let files = tempfile::tempdir().unwrap();
        let mut model = model(files.path());
        assert_eq!(model.toggle_panel_chat(Side::Right), CommandOutcome::Ok);
        assert!(model.panel_shows_chat(Side::Right));
        // Asking for what is already true changes nothing.
        assert_eq!(
            model.set_panel_chat(Side::Right, true),
            CommandOutcome::None
        );
        assert_eq!(model.toggle_panel_chat(Side::Right), CommandOutcome::Ok);
        assert!(!model.panel_shows_chat(Side::Right));
        assert!(model.status.contains("agent.pane"), "{}", model.status);
    }

    #[test]
    fn the_commands_are_wired() {
        let files = tempfile::tempdir().unwrap();
        let mut model = model(files.path());
        model.focus = Side::Left;
        assert_eq!(model.execute("cm_SrcChat"), CommandOutcome::Ok);
        assert!(model.panel_shows_chat(Side::Left));
        assert_eq!(model.execute("cm_TrgChat"), CommandOutcome::Ok);
        assert!(model.panel_shows_chat(Side::Right));
        // Closing gives *both* panels back their file lists in one command.
        assert_eq!(model.execute("cm_ChatFiles"), CommandOutcome::Ok);
        assert!(!model.panel_shows_chat(Side::Left));
        assert!(!model.panel_shows_chat(Side::Right));
        // The pane command always targets the passive panel, so the files stay visible.
        model.focus = Side::Left;
        assert_eq!(model.execute("cm_ChatPane"), CommandOutcome::Ok);
        assert!(!model.panel_shows_chat(Side::Left));
        assert!(model.panel_shows_chat(Side::Right));
    }
}

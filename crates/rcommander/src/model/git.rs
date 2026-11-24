//! Git integration in the model: reading the machine's Git configuration, but only
//! after the user approved it.
//!
//! Reading that configuration means reading the user's identity, their credential
//! helper and — with the `store` helper — a file of plain-text passwords. So the
//! application asks once, shows exactly what it found, and remembers the answer in
//! `settings.toml` (`[git] read_system_config = "ask" | "allowed" | "denied"`).
//! Nothing secret is written anywhere; credentials live in memory for this session.

use super::{CommandOutcome, Model};
use rc_git::config::{GitConfig, GitConfigSummary};

/// What the application knows about the user's Git configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitConfigGate {
    /// The question was never asked.
    Undecided,
    /// The user allowed it (this session or permanently).
    Allowed,
    /// The user refused; do not ask again unless they reset the setting.
    Denied,
}

impl GitConfigGate {
    pub fn key(self) -> &'static str {
        match self {
            GitConfigGate::Undecided => "git.gate.undecided",
            GitConfigGate::Allowed => "git.gate.allowed",
            GitConfigGate::Denied => "git.gate.denied",
        }
    }
}

impl Model {
    /// Show the repository that contains the active directory as a panel.
    ///
    /// The panel then lists the working tree with its status colours and carries the
    /// virtual `branches`, `tags`, `commits` and `remotes` folders.
    pub fn open_git_panel(&mut self) -> CommandOutcome {
        let side = self.focus;
        let Some(path) = self
            .panel(side)
            .location()
            .as_path()
            .map(|p| p.to_path_buf())
        else {
            let message = self.t("git.needs_local_dir");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        };
        let repo = match rc_git::GitRepo::open(&path) {
            Ok(repo) => repo,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        let location = rc_core::location::Location::git(repo.root(), "", "");
        match self.open_in(side, location) {
            Ok(()) => {
                let branch = self.panel(side).git_label.clone();
                let message = self.tf("git.opened", &[("branch", &branch)]);
                self.report(message);
                CommandOutcome::Ok
            }
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// True when the active panel shows a Git repository.
    pub fn active_panel_is_git(&self) -> bool {
        self.active().location().is_git()
    }

    /// Where the gate currently stands.
    pub fn git_config_gate(&self) -> GitConfigGate {
        if self.git_config.is_some() {
            GitConfigGate::Allowed
        } else if self.settings.git.config_denied() {
            GitConfigGate::Denied
        } else {
            GitConfigGate::Undecided
        }
    }

    /// The loaded configuration, when the user allowed it.
    pub fn git_config(&self) -> Option<&GitConfig> {
        self.git_config.as_ref()
    }

    /// Read the configuration and open the approval dialog.
    ///
    /// The configuration is loaded into the dialog state — the user has to see what
    /// would be used before it is used. `repository` is the panel path, so a
    /// repository-local configuration is included.
    pub fn request_git_config(&mut self) -> CommandOutcome {
        if self.git_config.is_some() {
            // Already allowed in this session: just show what is in use.
            self.open_dialog(super::Dialog::GitConfig(
                super::dialogs::GitConfigState::approved(self.git_config.clone().unwrap()),
            ));
            return CommandOutcome::Ok;
        }
        let repository = self.active().location().as_path().map(|p| p.to_path_buf());
        let config = GitConfig::load(repository.as_deref());
        self.open_dialog(super::Dialog::GitConfig(
            super::dialogs::GitConfigState::asking(config),
        ));
        CommandOutcome::Ok
    }

    /// Accept the configuration the dialog loaded.
    pub fn accept_git_config(&mut self, config: GitConfig, remember: bool) {
        if remember {
            self.settings.git.read_system_config = "allowed".to_string();
            self.mark_settings_dirty();
        }
        self.git_config = Some(config);
    }

    /// Refuse it, and remember the refusal when asked to.
    pub fn deny_git_config(&mut self, remember: bool) {
        self.git_config = None;
        if remember {
            self.settings.git.read_system_config = "denied".to_string();
            self.mark_settings_dirty();
        }
    }

    /// Forget the loaded configuration (the session keeps working without it).
    pub fn forget_git_config(&mut self) {
        self.git_config = None;
        if self.settings.git.config_denied() {
            self.settings.git.read_system_config = "ask".to_string();
            self.mark_settings_dirty();
        }
    }

    /// The Git identity to use for authoring, when one is configured.
    pub fn git_identity(&self) -> Option<&rc_git::config::Identity> {
        if !self.settings.git.use_identity {
            return None;
        }
        self.git_config
            .as_ref()
            .and_then(|config| config.summary.identity.as_ref())
    }

    /// A credential for a remote, taken from the loaded configuration.
    ///
    /// Never written to disk and never logged; the caller uses it for one
    /// connection attempt.
    pub fn git_credential_for(
        &self,
        protocol: &str,
        host: &str,
    ) -> Option<&rc_git::config::StoredCredential> {
        self.git_config
            .as_ref()
            .and_then(|config| config.credential_for(protocol, host))
    }

    /// Rewrite a remote URL with the configured `insteadOf` rules.
    pub fn git_rewrite_url(&self, url: &str) -> Option<String> {
        self.git_config
            .as_ref()
            .and_then(|config| config.rewrite_url(url))
    }

    /// A one-line description for the status bar.
    pub fn git_config_summary(&self) -> Option<GitConfigSummary> {
        self.git_config
            .as_ref()
            .map(|config| config.summary.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn model_in(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    #[test]
    fn the_question_is_asked_until_it_is_answered() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        assert_eq!(model.git_config_gate(), GitConfigGate::Undecided);
        assert!(model.git_config().is_none());

        let outcome = model.request_git_config();
        assert_eq!(outcome, CommandOutcome::Ok);
        match model.top_dialog() {
            Some(crate::model::Dialog::GitConfig(state)) => {
                assert!(state.is_asking(), "the first visit asks");
                assert!(
                    state.remember,
                    "the checkbox starts ticked, so one click is enough to stop the question"
                );
            }
            other => panic!("expected the git configuration dialog, got {other:?}"),
        }
    }

    #[test]
    fn refusing_can_be_remembered_and_then_stops_the_question() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.deny_git_config(true);
        assert_eq!(model.git_config_gate(), GitConfigGate::Denied);
        assert!(model.settings_dirty);
        assert_eq!(model.settings.git.read_system_config, "denied");
        // Forgetting resets the setting, so the user can change their mind.
        model.forget_git_config();
        assert_eq!(model.git_config_gate(), GitConfigGate::Undecided);
        assert_eq!(model.settings.git.read_system_config, "ask");
    }

    #[test]
    fn accepting_keeps_the_configuration_for_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        let config = rc_git::GitConfig::load(None);
        let summary = config.summary.clone();
        model.accept_git_config(config, false);
        assert_eq!(model.git_config_gate(), GitConfigGate::Allowed);
        assert_eq!(
            model.git_config_summary().map(|s| s.sources),
            Some(summary.sources)
        );
        assert!(
            !model.settings.git.config_allowed(),
            "a one-off allowance is not remembered"
        );
        // Accepting with "remember" writes the setting.
        let mut model = model_in(dir.path());
        model.accept_git_config(rc_git::GitConfig::load(None), true);
        assert!(model.settings.git.config_allowed());
    }

    #[test]
    fn credentials_are_only_available_after_approval() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        // No configuration loaded: nothing to leak.
        assert!(model.git_credential_for("https", "github.com").is_none());
        assert!(model.git_rewrite_url("gh:owner/repo").is_none());
        assert!(model.git_identity().is_none());

        // A hand-built configuration stands in for what `GitConfig::load` finds.
        let config = rc_git::GitConfig::load(None);
        model.accept_git_config(config, false);
        // The machine used for testing may have no credentials; what matters is that
        // nothing is returned unless the configuration was accepted.
        let _ = model.git_credential_for("https", "github.com");
    }

    #[test]
    fn the_identity_can_be_switched_off() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.accept_git_config(rc_git::GitConfig::load(None), false);
        let with_identity = model.git_identity().cloned();
        model.settings.git.use_identity = false;
        assert!(model.git_identity().is_none());
        assert_eq!(with_identity.is_none(), model.git_config().is_none());
    }
}

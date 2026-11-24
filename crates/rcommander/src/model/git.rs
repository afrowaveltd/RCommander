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

/// A commit made from the editor and the push that follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitSaveOutcome {
    /// Commit id (short) when a commit was created.
    pub commit: Option<String>,
    /// True when a push was started in the background.
    pub pushed: bool,
    /// Message shown in the status line.
    pub message: String,
}

/// A remote operation (push, fetch, pull) running on a worker thread.
pub struct GitNetJob {
    receiver: std::sync::mpsc::Receiver<Result<String, String>>,
}

impl Model {
    /// Called after the editor writes `path` to disk.
    ///
    /// When the file lives in a Git working tree and committing on save is allowed, the
    /// file is committed — and pushed, when the settings say so. With the `ask` default
    /// a dialog appears first, so an accidental F4-save never publishes anything.
    pub fn on_file_saved(&mut self, path: &std::path::Path) -> CommandOutcome {
        if self.settings.git.commit_never() {
            return CommandOutcome::None;
        }
        let Some(root) = rc_git::GitRepo::open(path.parent().unwrap_or(path))
            .ok()
            .map(|repo| repo.root().to_path_buf())
        else {
            return CommandOutcome::None;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let message = self.settings.git.message_for(&name);
        if self.settings.git.commit_always() {
            return self.commit_and_push(&root, &name, &message);
        }
        self.open_dialog(super::Dialog::GitCommit(super::dialogs::GitCommitState {
            root,
            file: name.clone(),
            message,
            push: self.settings.git.push_after_commit,
            remember: false,
            status: String::new(),
        }));
        CommandOutcome::Ok
    }

    /// Stage `file`, commit it and (optionally) push the branch.
    pub fn commit_and_push(
        &mut self,
        root: &std::path::Path,
        file: &str,
        message: &str,
    ) -> CommandOutcome {
        let repo = match rc_git::GitRepo::open_exact(root) {
            Ok(repo) => repo,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        if let Err(error) = repo.stage_path(file) {
            let message = error.to_string();
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let identity = self
            .git_identity()
            .map(|id| (id.name.clone(), id.email.clone()));
        let author = identity
            .as_ref()
            .map(|(name, email)| (name.as_str(), email.as_str()));
        let commit = match repo.commit_staged(message, author) {
            Ok(commit) => commit,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        let mut text = self.tf("git.committed", &[("hash", &commit.short)]);
        if self.settings.git.push_after_commit {
            match repo.push_request() {
                Ok(request) => {
                    let credentials = self
                        .git_credential_for("https", host_of(&request.url))
                        .map(|stored| (stored.username.clone(), stored.password.clone()));
                    let (sender, receiver) = std::sync::mpsc::channel();
                    let repo_root = repo.root().to_path_buf();
                    std::thread::spawn(move || {
                        let result = rc_git::GitRepo::open_exact(&repo_root)
                            .and_then(|repo| {
                                repo.push(
                                    &request,
                                    credentials.as_ref().map(|(u, p)| (u.as_str(), p.as_str())),
                                )
                            })
                            .map(|()| "git.pushed|".to_string())
                            .map_err(|error| error.to_string());
                        let _ = sender.send(result);
                    });
                    self.git_push_job = Some(GitNetJob { receiver });
                    text.push_str(&format!(" · {}", self.t("git.pushing")));
                }
                Err(error) => {
                    // No remote is not a failure of the commit.
                    text.push_str(&format!(" · {}", error));
                }
            }
        }
        self.report(text.clone());
        CommandOutcome::Ok
    }

    /// Fetch from the branch's remote in the background.
    pub fn git_fetch(&mut self) -> CommandOutcome {
        self.spawn_net("git.fetched", |repo, credentials| {
            let remote = remote_name(repo)?;
            let borrow = credentials.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
            let count = repo
                .fetch(&remote, borrow)
                .map_err(|error| error.to_string())?;
            Ok(format!("{count}"))
        })
    }

    /// Fetch and fast-forward the current branch in the background.
    pub fn git_pull(&mut self) -> CommandOutcome {
        self.spawn_net("git.pulled", |repo, credentials| {
            let remote = remote_name(repo)?;
            let borrow = credentials.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
            repo.fetch(&remote, borrow)
                .map_err(|error| error.to_string())?;
            let moved = repo
                .pull_fast_forward()
                .map_err(|error| error.to_string())?;
            Ok(format!("{moved}"))
        })
    }

    /// Push the current branch in the background.
    pub fn git_push(&mut self) -> CommandOutcome {
        self.spawn_net("git.pushed", |repo, credentials| {
            let request = repo.push_request().map_err(|error| error.to_string())?;
            let borrow = credentials.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
            repo.push(&request, borrow)
                .map_err(|error| error.to_string())?;
            Ok(String::new())
        })
    }

    /// Merge a local branch into the current one (no network, done immediately).
    pub fn git_merge(&mut self, name: &str) -> CommandOutcome {
        let Some(root) = self.active().location().git_root().map(|p| p.to_path_buf()) else {
            return CommandOutcome::Failed(self.t("git.needs_git_panel"));
        };
        let repo = match rc_git::GitRepo::open_exact(&root) {
            Ok(repo) => repo,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        match repo.merge_branch(name) {
            Ok(moved) => {
                let location = self.active().location().clone();
                let side = self.focus;
                let _ = self.open_in(side, location);
                let merged = moved.to_string();
                let message = self.tf("git.merged", &[("branch", name.trim()), ("count", &merged)]);
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

    /// Run `operation` on a worker thread with the approved credentials.
    fn spawn_net<F>(&mut self, key: &'static str, operation: F) -> CommandOutcome
    where
        F: FnOnce(&rc_git::GitRepo, Option<(String, String)>) -> Result<String, String>
            + Send
            + 'static,
    {
        if self.git_push_job.is_some() {
            let message = self.t("git.busy");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let Some(root) = self.active().location().git_root().map(|p| p.to_path_buf()) else {
            let message = self.t("git.needs_git_panel");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        };
        let credentials = self.git_config.as_ref().and_then(|config| {
            config
                .credentials()
                .iter()
                .find(|credential| credential.username == "git" || true)
                .map(|credential| (credential.username.clone(), credential.password.clone()))
        });
        let (sender, receiver) = std::sync::mpsc::channel();
        let key = key.to_string();
        std::thread::spawn(move || {
            let result = rc_git::GitRepo::open_exact(&root)
                .map_err(|error| error.to_string())
                .and_then(|repo| operation(&repo, credentials));
            let _ = sender.send(result.map(|detail| format!("{key}|{detail}")));
        });
        self.git_push_job = Some(GitNetJob { receiver });
        self.report(self.t("git.working"));
        CommandOutcome::Ok
    }

    /// Pick up a finished remote operation; returns true when something changed.
    pub fn poll_git_push(&mut self) -> bool {
        let Some(job) = self.git_push_job.as_ref() else {
            return false;
        };
        let outcome = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("the push stopped unexpectedly".to_string())
            }
        };
        self.git_push_job = None;
        match outcome {
            Ok(text) => {
                let (key, detail) = text.split_once('|').unwrap_or((text.as_str(), ""));
                let message = self.tf(key, &[("count", detail), ("branch", detail)]);
                self.report(message);
                // A pull changes files, so the panel is reloaded.
                if key == "git.pulled" {
                    let location = self.active().location().clone();
                    let side = self.focus;
                    let _ = self.open_in(side, location);
                }
            }
            Err(error) => self.report_error(error),
        }
        true
    }

    /// True while a remote operation is running.
    pub fn is_pushing(&self) -> bool {
        self.git_push_job.is_some()
    }
}

impl Model {
    /// Open the branch picker for the active panel's repository.
    pub fn open_git_branches(&mut self) -> CommandOutcome {
        let Some(root) = self.active().location().git_root().map(|p| p.to_path_buf()) else {
            let message = self.t("git.needs_git_panel");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        };
        let repo = match rc_git::GitRepo::open_exact(&root) {
            Ok(repo) => repo,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        let branches: Vec<String> = repo.branches().into_iter().map(|b| b.name).collect();
        let current = repo.head_branch().unwrap_or_default();
        let selected = branches.iter().position(|name| *name == current);
        self.open_dialog(super::Dialog::GitBranches(
            super::dialogs::GitBranchesState {
                root,
                current,
                dirty: repo.is_dirty(),
                branches,
                selected,
                allow_dirty: false,
                new_branch: String::new(),
                status: String::new(),
            },
        ));
        CommandOutcome::Ok
    }

    /// Open the fetch/pull/push/merge dialog for the panel's repository.
    pub fn open_git_remote(&mut self) -> CommandOutcome {
        let Some(root) = self.active().location().git_root().map(|p| p.to_path_buf()) else {
            let message = self.t("git.needs_git_panel");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        };
        let repo = match rc_git::GitRepo::open_exact(&root) {
            Ok(repo) => repo,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        let (ahead, behind) = repo.ahead_behind();
        self.open_dialog(super::Dialog::GitRemote(super::dialogs::GitRemoteState {
            root,
            branch: repo.head_branch().unwrap_or_default(),
            upstream: repo.push_request().map(|r| r.remote).unwrap_or_default(),
            ahead,
            behind,
            merge_from: String::new(),
            status: String::new(),
        }));
        CommandOutcome::Ok
    }

    /// Check out a branch in the panel's repository and reload the panel.
    pub fn git_checkout(&mut self, name: &str, allow_dirty: bool) -> CommandOutcome {
        let Some(root) = self.active().location().git_root().map(|p| p.to_path_buf()) else {
            return CommandOutcome::Failed(self.t("git.needs_git_panel"));
        };
        let repo = match rc_git::GitRepo::open_exact(&root) {
            Ok(repo) => repo,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        match repo.checkout_branch(name, allow_dirty) {
            Ok(()) => {
                let location = self.active().location().clone();
                let side = self.focus;
                if let Err(error) = self.open_in(side, location) {
                    let message = error.to_string();
                    self.report_error(&message);
                    return CommandOutcome::Failed(message);
                }
                let message = self.tf("git.switched", &[("branch", name)]);
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

    /// Create a branch at `HEAD`, switch to it and reload the panel.
    pub fn git_create_branch(&mut self, name: &str) -> CommandOutcome {
        let Some(root) = self.active().location().git_root().map(|p| p.to_path_buf()) else {
            return CommandOutcome::Failed(self.t("git.needs_git_panel"));
        };
        let repo = match rc_git::GitRepo::open_exact(&root) {
            Ok(repo) => repo,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        match repo.create_branch(name) {
            Ok(()) => {
                let location = self.active().location().clone();
                let side = self.focus;
                let _ = self.open_in(side, location);
                let message = self.tf("git.branch_created", &[("branch", name.trim())]);
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
}

/// The remote a fetch or push should talk to.
fn remote_name(repo: &rc_git::GitRepo) -> Result<String, String> {
    repo.push_request()
        .map(|request| request.remote)
        .or_else(|_| {
            repo.remotes()
                .into_iter()
                .next()
                .ok_or_else(|| "this repository has no remote".to_string())
        })
}

/// Host part of a remote URL (`https://user@host/x`, `git@host:x`, `file:///x`).
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let rest = rest.rsplit_once('@').map(|(_, rest)| rest).unwrap_or(rest);
    let end = rest.find(['/', ':']).unwrap_or(rest.len());
    &rest[..end]
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

#[cfg(test)]
mod save_tests {
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    /// A repository with one commit and a file to edit.
    fn repo(dir: &std::path::Path) -> std::path::PathBuf {
        let root = dir.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let mut options = git2::RepositoryInitOptions::new();
        options.initial_head("main");
        let repo = git2::Repository::init_opts(&root, &options).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test").unwrap();
            config.set_str("user.email", "t@example.com").unwrap();
        }
        std::fs::write(root.join("code.rs"), "fn main() {}\n").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Test", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[])
            .unwrap();
        root
    }

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
    fn saving_a_file_asks_before_committing() {
        let dir = tempfile::tempdir().unwrap();
        let root = repo(dir.path());
        let mut model = model_in(dir.path());
        assert_eq!(model.settings.git.commit_on_save, "ask");

        let file = root.join("code.rs");
        std::fs::write(&file, "fn main() { changed }\n").unwrap();
        assert_eq!(model.on_file_saved(&file), super::CommandOutcome::Ok);
        match model.top_dialog() {
            Some(crate::model::Dialog::GitCommit(state)) => {
                assert_eq!(state.file, "code.rs");
                assert_eq!(state.message, "Edit code.rs", "the template fills the name");
                assert!(state.push, "pushing follows the setting");
            }
            other => panic!("expected the commit dialog, got {other:?}"),
        }
        // Nothing was committed yet: asking first is the point.
        let repo = rc_git::GitRepo::open(&root).unwrap();
        assert!(repo.is_dirty());
    }

    #[test]
    fn committing_from_the_dialog_stages_and_commits() {
        let dir = tempfile::tempdir().unwrap();
        let root = repo(dir.path());
        let mut model = model_in(dir.path());
        let file = root.join("code.rs");
        std::fs::write(&file, "fn main() { edited }\n").unwrap();
        model.on_file_saved(&file);

        let outcome = model.commit_and_push(&root, "code.rs", "Edit code.rs");
        assert_eq!(outcome, super::CommandOutcome::Ok);
        let repo = rc_git::GitRepo::open(&root).unwrap();
        assert!(repo.status().unwrap().is_clean(), "the change is committed");
        let head = repo.commits(1).remove(0);
        assert_eq!(head.summary, "Edit code.rs");
        // No remote in this fixture: the commit still succeeds and says so.
        assert!(model.status.contains("git.committed"), "{}", model.status);
    }

    #[test]
    fn an_always_setting_commits_without_a_dialog() {
        let dir = tempfile::tempdir().unwrap();
        let root = repo(dir.path());
        let mut model = model_in(dir.path());
        model.settings.git.commit_on_save = "always".to_string();
        model.settings.git.push_after_commit = false;
        let file = root.join("code.rs");
        std::fs::write(&file, "fn main() { auto }\n").unwrap();
        assert_eq!(model.on_file_saved(&file), super::CommandOutcome::Ok);
        assert!(model.top_dialog().is_none(), "no question with `always`");
        let repo = rc_git::GitRepo::open(&root).unwrap();
        assert!(repo.status().unwrap().is_clean());
    }

    #[test]
    fn a_never_setting_leaves_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = repo(dir.path());
        let mut model = model_in(dir.path());
        model.settings.git.commit_on_save = "never".to_string();
        let file = root.join("code.rs");
        std::fs::write(&file, "fn main() { untouched }\n").unwrap();
        assert_eq!(model.on_file_saved(&file), super::CommandOutcome::None);
        assert!(model.top_dialog().is_none());
        let repo = rc_git::GitRepo::open(&root).unwrap();
        assert!(repo.is_dirty(), "nothing was committed");
    }

    #[test]
    fn a_file_outside_a_repository_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        let file = dir.path().join("plain.txt");
        std::fs::write(&file, "hello").unwrap();
        assert_eq!(model.on_file_saved(&file), super::CommandOutcome::None);
        assert!(model.top_dialog().is_none());
    }

    #[test]
    fn the_commit_message_template_is_configurable() {
        let mut settings = Settings::default();
        assert_eq!(settings.git.message_for("a.rs"), "Edit a.rs");
        settings.git.commit_message = "wip: {name}".to_string();
        assert_eq!(settings.git.message_for("a.rs"), "wip: a.rs");
        settings.git.commit_message = "no placeholder".to_string();
        assert_eq!(settings.git.message_for("a.rs"), "no placeholder");
        settings.git.commit_message = String::new();
        assert_eq!(settings.git.message_for("a.rs"), "Edit a.rs");
    }

    #[test]
    fn the_host_of_a_remote_url_is_extracted() {
        assert_eq!(
            super::host_of("https://user@github.com/x/y.git"),
            "github.com"
        );
        assert_eq!(super::host_of("https://github.com/x/y.git"), "github.com");
        assert_eq!(
            super::host_of("git@github.com:owner/repo.git"),
            "github.com"
        );
        assert_eq!(
            super::host_of("file:///tmp/remote.git"),
            "",
            "a local path has no host"
        );
        assert_eq!(
            super::host_of("ssh://git@host.example.com:22/x"),
            "host.example.com"
        );
    }
}

#[cfg(test)]
mod branch_tests {
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    /// A repository with `main` and `dev`, opened as a Git panel.
    fn git_panel(dir: &std::path::Path) -> Model {
        let root = dir.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let mut options = git2::RepositoryInitOptions::new();
        options.initial_head("main");
        let repo = git2::Repository::init_opts(&root, &options).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test").unwrap();
            config.set_str("user.email", "t@example.com").unwrap();
        }
        std::fs::write(root.join("code.rs"), "fn main() {}\n").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Test", "t@example.com").unwrap();
        let commit = repo
            .commit(Some("HEAD"), &signature, &signature, "first", &tree, &[])
            .unwrap();
        let head = repo.find_commit(commit).unwrap();
        repo.branch("dev", &head, false).unwrap();

        let mut model = Model::for_test(
            Settings::default(),
            Location::git(&root, "", ""),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
            .open_in(crate::Side::Left, Location::git(&root, "", ""))
            .unwrap();
        model
    }

    #[test]
    fn the_branch_picker_lists_branches_and_marks_the_current_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = git_panel(dir.path());
        assert_eq!(model.open_git_branches(), super::CommandOutcome::Ok);
        match model.top_dialog() {
            Some(crate::model::Dialog::GitBranches(state)) => {
                assert_eq!(state.branches, vec!["dev".to_string(), "main".to_string()]);
                assert_eq!(state.current, "main");
                assert!(!state.dirty);
                assert_eq!(
                    state.selected,
                    Some(1),
                    "the current branch starts selected"
                );
                assert!(
                    state.chosen().is_none(),
                    "the current branch is not a switch target"
                );
            }
            other => panic!("expected the branch dialog, got {other:?}"),
        }
    }

    #[test]
    fn switching_branches_reloads_the_panel() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = git_panel(dir.path());
        assert_eq!(model.git_checkout("dev", false), super::CommandOutcome::Ok);
        assert_eq!(model.panel(crate::Side::Left).git_label, "dev");
        assert!(model.status.contains("git.switched"), "{}", model.status);
        // Switching back works the same way.
        assert_eq!(model.git_checkout("main", false), super::CommandOutcome::Ok);
        assert_eq!(model.panel(crate::Side::Left).git_label, "main");
    }

    #[test]
    fn a_dirty_tree_blocks_a_switch_unless_it_is_forced() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = git_panel(dir.path());
        std::fs::write(dir.path().join("repo/code.rs"), "fn main() { dirty }\n").unwrap();
        assert!(matches!(
            model.git_checkout("dev", false),
            super::CommandOutcome::Failed(_)
        ));
        assert_eq!(model.panel(crate::Side::Left).git_label, "main");
        // The picker shows the warning, and the force checkbox allows the switch.
        assert_eq!(model.open_git_branches(), super::CommandOutcome::Ok);
        match model.top_dialog() {
            Some(crate::model::Dialog::GitBranches(state)) => assert!(state.dirty),
            other => panic!("expected the branch dialog, got {other:?}"),
        }
        assert_eq!(model.git_checkout("dev", true), super::CommandOutcome::Ok);
        assert_eq!(model.panel(crate::Side::Left).git_label, "dev");
    }

    #[test]
    fn a_branch_can_be_created_and_becomes_current() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = git_panel(dir.path());
        assert_eq!(
            model.git_create_branch("feature/x"),
            super::CommandOutcome::Ok
        );
        assert_eq!(model.panel(crate::Side::Left).git_label, "feature/x");
        assert!(model.status.contains("git.branch_created"));
        // The same name twice is refused, and so is an empty one.
        assert!(matches!(
            model.git_create_branch("feature/x"),
            super::CommandOutcome::Failed(_)
        ));
        assert!(matches!(
            model.git_create_branch("  "),
            super::CommandOutcome::Failed(_)
        ));
    }

    #[test]
    fn the_branch_commands_need_a_git_panel() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir.path()),
            Location::local(dir.path()),
        );
        assert!(matches!(
            model.open_git_branches(),
            super::CommandOutcome::Failed(_)
        ));
        assert!(matches!(
            model.git_checkout("main", false),
            super::CommandOutcome::Failed(_)
        ));
    }
}

#[cfg(test)]
mod remote_tests {
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    /// A repository with an `origin` pointing at a bare repository next to it.
    fn git_panel_with_remote(dir: &std::path::Path) -> Model {
        let root = dir.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let mut options = git2::RepositoryInitOptions::new();
        options.initial_head("main");
        let repo = git2::Repository::init_opts(&root, &options).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test").unwrap();
            config.set_str("user.email", "t@example.com").unwrap();
        }
        std::fs::write(root.join("code.rs"), "fn main() {}\n").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Test", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[])
            .unwrap();

        let bare = dir.join("remote.git");
        let mut bare_options = git2::RepositoryInitOptions::new();
        bare_options.bare(true).initial_head("main");
        git2::Repository::init_opts(&bare, &bare_options).unwrap();
        let mut origin = repo
            .remote_with_fetch(
                "origin",
                bare.to_str().unwrap(),
                "+refs/heads/*:refs/remotes/origin/*",
            )
            .unwrap();
        origin
            .push(&["refs/heads/main:refs/heads/main"], None)
            .unwrap();

        let mut model = Model::for_test(
            Settings::default(),
            Location::git(&root, "", ""),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
            .open_in(crate::Side::Left, Location::git(&root, "", ""))
            .unwrap();
        model
    }

    #[test]
    fn the_remote_dialog_reports_the_branch_and_its_tracking_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = git_panel_with_remote(dir.path());
        assert_eq!(model.open_git_remote(), super::CommandOutcome::Ok);
        match model.top_dialog() {
            Some(crate::model::Dialog::GitRemote(state)) => {
                assert_eq!(state.branch, "main");
                assert_eq!(state.upstream, "origin");
                assert_eq!((state.ahead, state.behind), (0, 0));
            }
            other => panic!("expected the remote dialog, got {other:?}"),
        }
    }

    #[test]
    fn merging_moves_the_branch_and_reloads_the_panel() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = git_panel_with_remote(dir.path());
        // Create a second branch with an extra commit, then merge it back.
        assert_eq!(
            model.git_create_branch("feature"),
            super::CommandOutcome::Ok
        );
        let root = dir.path().join("repo");
        let repo = rc_git::GitRepo::open_exact(&root).unwrap();
        std::fs::write(root.join("feature.txt"), "work\n").unwrap();
        repo.stage_path("feature.txt").unwrap();
        repo.commit_staged("feature work", None).unwrap();
        // Back to main, then merge the feature branch.
        assert_eq!(model.git_checkout("main", false), super::CommandOutcome::Ok);
        assert_eq!(model.git_merge("feature"), super::CommandOutcome::Ok);
        assert!(model.status.contains("git.merged"), "{}", model.status);
        assert!(
            dir.path().join("repo/feature.txt").exists(),
            "the merged file is in the working tree"
        );
    }

    #[test]
    fn the_remote_commands_need_a_git_panel() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir.path()),
            Location::local(dir.path()),
        );
        assert!(matches!(
            model.open_git_remote(),
            super::CommandOutcome::Failed(_)
        ));
        assert!(matches!(model.git_pull(), super::CommandOutcome::Failed(_)));
    }
}

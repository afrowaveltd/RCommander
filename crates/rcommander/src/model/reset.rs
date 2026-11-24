//! Factory resets behind the "Dangerous" page of the options dialog.
//!
//! Every reset is reversible only by the user (there is no undo), so each one
//! touches exactly the files named by its [`ResetScope`] and nothing else. The
//! scopes exist because the encrypted credential store is deliberately
//! unrecoverable without its master passphrase: forgetting the passphrase must
//! still leave a way back into the application, and that way is "delete the
//! store, lose the saved passwords".

use super::Model;

/// What a factory reset wipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResetScope {
    /// Every option goes back to its default value.
    Settings,
    /// Panel tabs, directory history, hotlist and the command line history.
    Layout,
    /// The encrypted credential store, including its master passphrase.
    Credentials,
    /// Trusted SSH host keys; the next connection trusts on first use again.
    KnownHosts,
    /// All of the above plus the saved session.
    Everything,
}

impl ResetScope {
    /// Every scope in the order the options page lists them, least destructive
    /// first and the "everything" option last.
    pub fn all() -> [ResetScope; 5] {
        [
            ResetScope::Settings,
            ResetScope::Layout,
            ResetScope::KnownHosts,
            ResetScope::Credentials,
            ResetScope::Everything,
        ]
    }

    /// Localisation key of the button label.
    pub fn label_key(self) -> &'static str {
        match self {
            ResetScope::Settings => "reset.settings",
            ResetScope::Layout => "reset.layout",
            ResetScope::KnownHosts => "reset.known_hosts",
            ResetScope::Credentials => "reset.credentials",
            ResetScope::Everything => "reset.everything",
        }
    }

    /// Localisation key of the one-line explanation.
    pub fn hint_key(self) -> &'static str {
        match self {
            ResetScope::Settings => "reset.settings.hint",
            ResetScope::Layout => "reset.layout.hint",
            ResetScope::KnownHosts => "reset.known_hosts.hint",
            ResetScope::Credentials => "reset.credentials.hint",
            ResetScope::Everything => "reset.everything.hint",
        }
    }

    /// True when the scope removes saved credentials, so the view can warn
    /// louder than for the harmless scopes.
    pub fn destroys_credentials(self) -> bool {
        matches!(self, ResetScope::Credentials | ResetScope::Everything)
    }
}

/// Outcome of a reset, so the view can report what actually happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetReport {
    /// Files removed from disk (0 when nothing existed).
    pub removed_files: usize,
    /// Human readable list of what was cleared, one entry per item.
    pub cleared: Vec<String>,
}

impl ResetReport {
    fn new() -> Self {
        ResetReport {
            removed_files: 0,
            cleared: Vec::new(),
        }
    }

    fn removed(&mut self, existed: bool) {
        if existed {
            self.removed_files += 1;
        }
    }

    /// Nothing at all was changed.
    pub fn is_empty(&self) -> bool {
        self.removed_files == 0 && self.cleared.is_empty()
    }
}

impl Model {
    /// Path of a per-user file, honouring [`Model::config_dir_override`].
    pub fn config_file(&self, name: &str) -> std::path::PathBuf {
        self.config_dir().join(name)
    }

    /// Delete `path` when it exists and count it in the report.
    fn remove_config_file(&mut self, report: &mut ResetReport, path: &std::path::Path) -> bool {
        let existed = path.is_file();
        if existed {
            let _ = std::fs::remove_file(path);
        }
        report.removed(existed);
        existed
    }

    /// Run a factory reset and return what it wiped.
    pub fn reset(&mut self, scope: ResetScope) -> ResetReport {
        let mut report = ResetReport::new();
        match scope {
            ResetScope::Settings => self.reset_settings(&mut report),
            ResetScope::Layout => self.reset_layout(&mut report),
            ResetScope::KnownHosts => self.reset_known_hosts(&mut report),
            ResetScope::Credentials => self.reset_credentials(&mut report),
            ResetScope::Everything => {
                self.reset_settings(&mut report);
                self.reset_layout(&mut report);
                self.reset_known_hosts(&mut report);
                self.reset_credentials(&mut report);
                let session = self.config_file("session.toml");
                if self.remove_config_file(&mut report, &session) {
                    report.cleared.push("session".to_string());
                }
            }
        }
        report
    }

    /// Restore every option to its default and forget the settings file.
    fn reset_settings(&mut self, report: &mut ResetReport) {
        let path = self.config_file("settings.toml");
        self.remove_config_file(report, &path);
        self.settings = rc_core::config::Settings::default();
        self.settings_dirty = true;
        self.keymap = {
            let mut keymap = rc_core::keymap::KeyMap::defaults();
            keymap.apply_overrides(&self.settings.shortcuts);
            keymap
        };
        let language = self.settings.language.clone();
        self.set_language(&language);
        report.cleared.push("settings".to_string());
    }

    /// Forget the hotlist, the histories and every panel tab.
    fn reset_layout(&mut self, report: &mut ResetReport) {
        let hotlist = self.config_file("hotlist.toml");
        if self.remove_config_file(report, &hotlist) {
            report.cleared.push("hotlist".to_string());
        }
        if !self.command_history.is_empty() {
            report.cleared.push("command history".to_string());
            self.command_history.clear();
        }
        self.left.history.clear();
        self.right.history.clear();
        // A fresh tab bar is one tab on the current directory, which is exactly
        // what "forget my tabs" means.
        self.left.tabs = rc_core::tabs::TabBar::new(self.left.location().clone());
        self.right.tabs = rc_core::tabs::TabBar::new(self.right.location().clone());
        report
            .cleared
            .push("tabs and directory history".to_string());
    }

    /// Forget which SSH host keys were trusted.
    fn reset_known_hosts(&mut self, report: &mut ResetReport) {
        let path = self.config_file("known_hosts");
        if self.remove_config_file(report, &path) {
            report.cleared.push("known hosts".to_string());
        }
    }

    /// Delete the encrypted store and drop the in-memory secrets with it.
    fn reset_credentials(&mut self, report: &mut ResetReport) {
        let path = self.secrets_file();
        self.remove_config_file(report, &path);
        report.cleared.push("credentials".to_string());
        self.reset_secret_store();
    }
}

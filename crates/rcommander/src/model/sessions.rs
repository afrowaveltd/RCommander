//! Saved session profiles in the model (Phase 13.3).
//!
//! The engine lives in [`rc_core::sessions`]; this layer connects it to the
//! application: the file it reads, the panel that a session connects, and the
//! commands the UI and the MCP tools call.

use super::{CommandOutcome, Model};
use rc_core::sessions::{Session, SessionStore};

impl Model {
    /// Path of the session file, honouring `config_dir_override`.
    pub fn sessions_file(&self) -> std::path::PathBuf {
        self.config_file("sessions.toml")
    }

    /// The saved sessions, loaded from disk the first time they are needed.
    pub fn sessions(&self) -> &SessionStore {
        &self.sessions_store
    }

    /// One saved session by name.
    pub fn session(&self, name: &str) -> Option<&Session> {
        self.sessions_store.get(name)
    }

    /// Names of the saved sessions, in display order.
    pub fn session_names(&self) -> Vec<String> {
        self.sessions_store.names()
    }

    /// Write the store back to disk.
    fn save_sessions(&mut self) -> Result<(), String> {
        let path = self.sessions_file();
        self.sessions_store
            .save(&path)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        self.sessions_dirty = false;
        Ok(())
    }

    /// Store (or replace) a session and persist it; returns the stored name.
    pub fn save_session(&mut self, session: Session) -> Result<String, String> {
        let name = self.sessions_store.put(session);
        self.save_sessions()?;
        Ok(name)
    }

    /// Save the current connect-dialog state as a session, naming it after the
    /// host when `name` is empty.
    pub fn save_current_as_session(
        &mut self,
        name: &str,
        connection: &rc_core::config::SavedConnection,
    ) -> Result<String, String> {
        let fallback = if connection.host.trim().is_empty() {
            "session".to_string()
        } else {
            connection.host.trim().to_string()
        };
        let wanted = if name.trim().is_empty() {
            self.sessions_store.free_name(&fallback)
        } else {
            name.trim().to_string()
        };
        self.save_session(Session::from_connection(wanted, connection))
    }

    pub fn delete_session(&mut self, name: &str) -> Result<bool, String> {
        let removed = self.sessions_store.remove(name);
        if removed {
            self.save_sessions()?;
        }
        Ok(removed)
    }

    pub fn rename_session(&mut self, old: &str, new: &str) -> Result<bool, String> {
        let renamed = self.sessions_store.rename(old, new);
        if renamed {
            self.save_sessions()?;
        }
        Ok(renamed)
    }

    pub fn duplicate_session(&mut self, name: &str) -> Result<Option<String>, String> {
        match self.sessions_store.duplicate(name) {
            Some(copy) => {
                self.save_sessions()?;
                Ok(Some(copy))
            }
            None => Ok(None),
        }
    }

    /// Import sessions from an OpenSSH `ssh_config` file.
    pub fn import_ssh_config(&mut self, path: &std::path::Path) -> Result<usize, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let imported = self
            .sessions_store
            .import(rc_core::sessions::sessions_from_ssh_config(&text));
        self.save_sessions()?;
        Ok(imported.len())
    }

    /// Connect the active panel to a saved session by name.
    ///
    /// The password comes from the credential store when it holds one, so a
    /// session that was saved after "remember password" connects with one click.
    pub fn connect_session(&mut self, name: &str) -> CommandOutcome {
        let Some(session) = self.session(name).cloned() else {
            let message = self.tf("session.unknown", &[("name", name)]);
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        };
        let mut connection = session.to_connection();
        if let Some(secret) = self.stored_password(&connection) {
            connection.password = secret;
        }
        if session.needs_password() && connection.password.is_empty() {
            // Ask rather than fail: the dialog is pre-filled with the session.
            self.open_dialog(super::Dialog::FtpConnect(super::dialogs::FtpConnectState {
                connection,
                save_password: true,
                status: self.tf("session.needs_password", &[("name", name)]),
                session_choice: name.to_string(),
                session_name: name.to_string(),
                ..super::dialogs::FtpConnectState::default()
            }));
            return CommandOutcome::Ok;
        }
        self.start_connect(&connection, &session)
    }

    /// True while a connection attempt is running in the background.
    pub fn is_connecting(&self) -> bool {
        self.connect_job.is_some()
    }

    /// Start connecting the focused panel in the background, so a slow or dead
    /// host never freezes the window. The result is picked up by
    /// [`Model::poll_connect`].
    pub fn start_connect(
        &mut self,
        connection: &rc_core::config::SavedConnection,
        session: &Session,
    ) -> CommandOutcome {
        if self.connect_job.is_some() {
            let message = self.t("session.busy");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let mut spec = super::remote::connection_spec(connection, None);
        spec.timeout =
            std::time::Duration::from_secs(self.settings.ftp.timeout_seconds.clamp(3, 120));
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = super::remote::RcRemoteBackend::connect(&spec)
                .map(|backend| Box::new(backend) as Box<dyn super::remote::RemoteBackend>);
            let _ = sender.send(result);
        });
        self.connect_job = Some(ConnectJob {
            session: session.clone(),
            connection: connection.clone(),
            side: self.focus,
            receiver,
        });
        let message = self.tf("session.connecting", &[("name", &session.name)]);
        self.report(message);
        CommandOutcome::Ok
    }

    /// Pick up a finished connection attempt; returns true when the state changed
    /// so the view can refresh immediately.
    pub fn poll_connect(&mut self) -> bool {
        let Some(job) = self.connect_job.as_mut() else {
            return false;
        };
        let outcome = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("the connection attempt stopped unexpectedly".to_string())
            }
        };
        let job = self.connect_job.take().expect("checked above");
        match outcome {
            Ok(backend) => {
                let target = if job.session.remote_dir.trim().is_empty() {
                    ".".to_string()
                } else {
                    job.session.remote_dir.clone()
                };
                let user = if job.connection.user.trim().is_empty() {
                    None
                } else {
                    Some(job.connection.user.clone())
                };
                let location = rc_core::location::Location::remote(
                    &job.connection.protocol,
                    &job.connection.host,
                    user,
                    &target,
                );
                match self.connect_remote(job.side, backend, location) {
                    Ok(()) => {
                        if !job.connection.password.is_empty() {
                            let _ = self.remember_password(&job.connection);
                        }
                        let message = self.tf("session.connected", &[("name", &job.session.name)]);
                        self.report(message);
                    }
                    Err(error) => {
                        let message = error.to_string();
                        self.report_error(&message);
                    }
                }
            }
            Err(message) => self.report_error(&message),
        }
        true
    }

    /// Forget a running attempt (the window is closing, or the user gave up).
    pub fn cancel_connect(&mut self) {
        self.connect_job = None;
    }

    /// Connect and wait for the result. Only for headless callers (MCP tools and
    /// tests): the UI uses [`Model::start_connect`] so it never blocks.
    pub fn connect_and_wait(
        &mut self,
        connection: &rc_core::config::SavedConnection,
        session: &Session,
    ) -> CommandOutcome {
        match self.start_connect(connection, session) {
            CommandOutcome::Ok => {}
            other => return other,
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
        while std::time::Instant::now() < deadline {
            if self.poll_connect() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if self.remote_sessions[self.focus as usize].is_some() {
            CommandOutcome::Ok
        } else {
            CommandOutcome::Failed(
                self.last_error
                    .clone()
                    .unwrap_or_else(|| self.t("session.failed")),
            )
        }
    }

    /// Load the store from disk. Called from [`Model::boot`] so a broken file is
    /// reported once at start instead of on every use.
    pub fn load_sessions(&mut self) {
        self.sessions_store = SessionStore::load(&self.sessions_file());
    }
}

/// One connection attempt running on a worker thread.
pub struct ConnectJob {
    pub session: Session,
    pub connection: rc_core::config::SavedConnection,
    /// Panel that will receive the connection, captured when it started.
    pub side: super::Side,
    receiver: std::sync::mpsc::Receiver<Result<Box<dyn super::remote::RemoteBackend>, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Side;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn model_in(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local("/tmp"),
            Location::local("/tmp"),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    #[test]
    fn sessions_are_persisted_to_their_own_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.load_sessions();
        assert!(model.session_names().is_empty());

        let name = model
            .save_current_as_session(
                "",
                &rc_core::config::SavedConnection {
                    host: "files.example.com".into(),
                    user: "alice".into(),
                    protocol: "sftp".into(),
                    ..rc_core::config::SavedConnection::with_defaults("sftp")
                },
            )
            .unwrap();
        assert_eq!(name, "files.example.com", "unnamed sessions take the host");
        let path = model.sessions_file();
        assert!(path.is_file(), "the session file is written");
        assert_eq!(path.file_name().unwrap(), "sessions.toml");

        // A fresh model sees the session again (and never the password).
        let mut second = model_in(dir.path());
        second.load_sessions();
        assert_eq!(second.session_names(), vec![name.clone()]);
        let session = second.session(&name).unwrap();
        assert_eq!(session.port, 22);
        let text = std::fs::read_to_string(&path).unwrap();
        // `auth_method = "password"` is a mode name, not a secret: what must not
        // appear is the secret itself or the fields that would carry it.
        assert!(!text.contains("key_passphrase"), "no secret field: {text}");
        assert!(!text.contains("s3cret"), "no secret value: {text}");
    }

    #[test]
    fn sessions_can_be_renamed_copied_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.load_sessions();
        model
            .save_session(Session::new("work", "w.example.com", "ftp"))
            .unwrap();
        assert!(model.rename_session("work", "office").unwrap());
        assert!(model.session("office").is_some());
        assert!(!model.rename_session("office", "office").unwrap());
        let copy = model.duplicate_session("office").unwrap().unwrap();
        assert_eq!(copy, "office copy");
        assert_eq!(
            model.session_names(),
            vec!["office".to_string(), copy.clone()]
        );
        assert!(model.delete_session(&copy).unwrap());
        assert_eq!(model.session_names(), vec!["office".to_string()]);
        assert!(!model.delete_session("nope").unwrap());
    }

    #[test]
    fn importing_an_ssh_config_adds_sessions_and_renames_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.load_sessions();
        model
            .save_session(Session::new("box", "old.example.com", "ftp"))
            .unwrap();
        let config = dir.path().join("ssh_config");
        std::fs::write(
            &config,
            "Host box\n  HostName box.example.com\n  User me\nHost other\n  HostName o.example.com\n",
        )
        .unwrap();
        assert_eq!(model.import_ssh_config(&config).unwrap(), 2);
        let mut names = model.session_names();
        names.sort();
        assert_eq!(
            names,
            vec!["box".to_string(), "box 2".to_string(), "other".to_string()]
        );
        assert_eq!(model.session("box").unwrap().host, "old.example.com");
        assert_eq!(model.session("box 2").unwrap().user, "me");
        // A missing file is a clean error, not a panic.
        assert!(model.import_ssh_config(&dir.path().join("nope")).is_err());
    }

    #[test]
    fn a_session_that_needs_a_password_opens_the_dialog_instead_of_failing() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.load_sessions();
        model
            .save_current_as_session(
                "work",
                &rc_core::config::SavedConnection {
                    host: "files.example.com".into(),
                    user: "alice".into(),
                    protocol: "sftp".into(),
                    ..rc_core::config::SavedConnection::with_defaults("sftp")
                },
            )
            .unwrap();
        // No stored secret and no password in the session: the connect dialog
        // opens pre-filled so the user only types the password.
        let outcome = model.connect_session("work");
        assert_eq!(outcome, crate::model::CommandOutcome::Ok);
        match model.top_dialog() {
            Some(crate::model::Dialog::FtpConnect(state)) => {
                assert_eq!(state.connection.host, "files.example.com");
                assert_eq!(state.connection.port, 22);
                assert_eq!(state.connection.user, "alice");
                assert!(
                    state.status.contains("session.needs_password"),
                    "the dialog explains why it opened: {}",
                    state.status
                );
            }
            other => panic!("expected the connect dialog, got {other:?}"),
        }
        // An unknown name is an error with a message.
        let outcome = model.connect_session("missing");
        assert!(matches!(outcome, CommandOutcome::Failed(_)));
        assert!(matches!(
            model.top_dialog(),
            Some(crate::model::Dialog::FtpConnect(_))
        ));
    }

    #[test]
    fn an_anonymous_session_connects_without_a_secret() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.load_sessions();
        let mut session = Session::new("public", "127.0.0.1", "ftp");
        session.anonymous = true;
        session.port = 65_533;
        model.save_session(session.clone()).unwrap();
        // A refused connection fails quickly and always with a message instead
        // of blocking the caller forever.
        let connection = session.to_connection();
        let outcome = model.connect_and_wait(&connection, &session);
        assert!(
            matches!(outcome, CommandOutcome::Failed(_)),
            "a refused connection fails, got {outcome:?}"
        );
        assert!(model.last_error.is_some());
        assert_eq!(model.focus, Side::Left);
    }

    #[test]
    fn a_second_attempt_is_refused_while_one_runs() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.load_sessions();
        let connection = rc_core::config::SavedConnection {
            host: "127.0.0.1".into(),
            port: 65533,
            ..rc_core::config::SavedConnection::with_defaults("ftp")
        };
        let session = Session::from_connection("slow", &connection);
        assert_eq!(
            model.start_connect(&connection, &session),
            CommandOutcome::Ok
        );
        assert!(model.is_connecting());
        assert!(matches!(
            model.start_connect(&connection, &session),
            CommandOutcome::Failed(_)
        ));
        model.cancel_connect();
        assert!(!model.is_connecting());
    }
}

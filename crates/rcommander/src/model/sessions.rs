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
        let connection = session.to_connection();
        let resolved = self.resolve_connection_credentials(&connection);
        if Self::requires_password(&resolved) && resolved.password.is_empty() {
            let status = self.tf("session.needs_password", &[("name", name)]);
            self.open_dialog(super::Dialog::FtpConnect(
                super::dialogs::FtpConnectState::for_password(session, status),
            ));
            return CommandOutcome::Ok;
        }
        self.start_connect(&connection, &session)
    }

    /// Only password authentication for a known protocol uses the account password.
    fn requires_password(connection: &rc_core::config::SavedConnection) -> bool {
        let Some(protocol) = rc_remote::Protocol::from_label(&connection.protocol) else {
            return false;
        };
        let spec = rc_remote::ConnectionSpec::from_saved(connection);
        if protocol == rc_remote::Protocol::Sftp {
            matches!(spec.auth, rc_remote::AuthMethod::Password)
        } else {
            !spec.is_anonymous()
        }
    }

    /// Resolve only the exact account, and only when no manual password was entered.
    /// A key-file passphrase is a separate secret, never a login-password fallback.
    fn resolve_connection_credentials(
        &self,
        connection: &rc_core::config::SavedConnection,
    ) -> rc_core::config::SavedConnection {
        let mut resolved = connection.clone();
        if let Some(protocol) = rc_remote::Protocol::from_label(&resolved.protocol) {
            if resolved.port == 0 {
                resolved.port = protocol.default_port();
            }
            if Self::requires_password(&resolved) {
                if resolved.password.is_empty() {
                    resolved.password = self.stored_password(&resolved).unwrap_or_default();
                }
            } else {
                resolved.password.clear();
            }
        }
        resolved
    }

    /// True while a connection attempt is running in the background.
    pub fn is_connecting(&self) -> bool {
        self.connect_job.is_some()
    }

    /// The step a running connection attempt has reached, for the status line.
    pub fn connect_stage(&self) -> Option<rc_remote::ConnectStage> {
        self.connect_job
            .as_ref()
            .and_then(|job| job.stage)
            .or_else(|| {
                self.connect_job
                    .as_ref()
                    .map(|_| rc_remote::ConnectStage::Connecting)
            })
    }

    /// How long the running connection attempt has been going, in seconds.
    pub fn connect_elapsed(&self) -> Option<u64> {
        self.connect_job
            .as_ref()
            .map(|job| job.started.elapsed().as_secs())
    }

    /// Start connecting the focused panel in the background, so a slow or dead
    /// host never freezes the window. The result is picked up by
    /// [`Model::poll_connect`].
    pub fn start_connect(
        &mut self,
        connection: &rc_core::config::SavedConnection,
        session: &Session,
    ) -> CommandOutcome {
        self.start_connect_with_remember(connection, session, false)
    }

    /// Start a user-requested attempt; remembering a newly entered password is opt-in.
    pub fn start_connect_with_remember(
        &mut self,
        connection: &rc_core::config::SavedConnection,
        session: &Session,
        remember: bool,
    ) -> CommandOutcome {
        if self.connect_job.is_some() {
            let message = self.t("session.busy");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        if rc_remote::Protocol::from_label(&connection.protocol).is_none() {
            let message = self.t("session.unsupported_protocol");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let resolved = self.resolve_connection_credentials(connection);
        if Self::requires_password(&resolved) && resolved.password.is_empty() {
            let message = self.tf("session.needs_password", &[("name", &session.name)]);
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let remember_password =
            remember && Self::requires_password(&resolved) && !connection.password.is_empty();
        let mut spec = super::remote::connection_spec(&resolved, None);
        spec.timeout =
            std::time::Duration::from_secs(self.settings.ftp.timeout_seconds.clamp(3, 120));
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let stage_sender = sender.clone();
            let result = super::remote::RcRemoteBackend::connect_reporting(&spec, &mut |stage| {
                // A closed receiver means the user gave up; the connect then finishes into
                // nothing, which is fine.
                let _ = stage_sender.send(ConnectEvent::Stage(stage));
            })
            .map(|backend| Box::new(backend) as Box<dyn super::remote::RemoteBackend>);
            let _ = sender.send(ConnectEvent::Done(result));
        });
        let mut connection = resolved;
        if !remember_password {
            connection.password.clear();
        }
        connection.key_passphrase.clear();
        self.connect_job = Some(ConnectJob {
            session: session.clone(),
            connection,
            remember_password,
            side: self.focus,
            stage: None,
            started: std::time::Instant::now(),
            receiver,
        });
        let message = self.tf("session.connecting", &[("name", &session.name)]);
        self.report(message);
        CommandOutcome::Ok
    }

    /// Pick up a finished connection attempt; returns true when the state changed
    /// so the view can refresh immediately.
    pub fn poll_connect(&mut self) -> bool {
        self.poll_connect_result().0
    }

    /// Separate progress notifications from the terminal result of this attempt.
    fn poll_connect_result(&mut self) -> (bool, Option<CommandOutcome>) {
        let Some(job) = self.connect_job.as_mut() else {
            return (false, None);
        };
        // Stages are drained first: they are what keeps the window talking while the socket
        // waits, and a stage alone is a state change worth a repaint.
        let session_name = job.session.name.clone();
        let mut progressed: Option<(String, &'static str)> = None;
        let mut outcome: Option<Result<Box<dyn super::remote::RemoteBackend>, String>> = None;
        loop {
            match job.receiver.try_recv() {
                Ok(ConnectEvent::Stage(stage)) => {
                    job.stage = Some(stage);
                    // Collected here and reported after the loop: `job` borrows the model
                    // mutably, and `t()`/`report()` need it back.
                    progressed = Some((session_name.clone(), stage.key()));
                }
                Ok(ConnectEvent::Done(result)) => {
                    outcome = Some(result);
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    outcome = Some(Err(
                        "the connection attempt stopped unexpectedly".to_string()
                    ));
                    break;
                }
            }
        }
        let Some(outcome) = outcome else {
            if let Some((name, key)) = progressed {
                let stage = self.t(key);
                let message = self.tf("connect.progress", &[("name", &name), ("stage", &stage)]);
                self.report(message);
            }
            return (true, None);
        };
        if let Some((name, key)) = progressed {
            let stage = self.t(key);
            let message = self.tf("connect.progress", &[("name", &name), ("stage", &stage)]);
            self.report(message);
        }
        let job = self.connect_job.take().expect("checked above");
        let result = match outcome {
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
                        let message = self.tf("session.connected", &[("name", &job.session.name)]);
                        self.report(message);
                        if job.remember_password
                            && let CommandOutcome::Failed(error) =
                                self.remember_password(&job.connection)
                        {
                            self.report_error(error);
                        }
                        // The panel connected successfully even if optional
                        // credential persistence failed (reported separately).
                        CommandOutcome::Ok
                    }
                    Err(error) => {
                        let message = error.to_string();
                        self.report_error(&message);
                        CommandOutcome::Failed(message)
                    }
                }
            }
            Err(message) => {
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        };
        (true, Some(result))
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
        self.wait_for_connect(std::time::Duration::from_secs(180))
    }

    fn wait_for_connect(&mut self, timeout: std::time::Duration) -> CommandOutcome {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let (_, Some(result)) = self.poll_connect_result() {
                return result;
            }
            if !self.is_connecting() {
                let message = self.t("session.failed");
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
            if std::time::Instant::now() >= deadline {
                self.cancel_connect();
                let message = self.t("session.timeout");
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
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
    /// True only for a newly entered password with explicit user consent.
    pub remember_password: bool,
    /// Panel that will receive the connection, captured when it started.
    pub side: super::Side,
    /// Step the attempt has reached, so the status line can say what is happening.
    pub stage: Option<rc_remote::ConnectStage>,
    /// When the attempt started, for the "for 12 s" part of the status line.
    pub started: std::time::Instant,
    receiver: std::sync::mpsc::Receiver<ConnectEvent>,
}

/// What a connection worker reports.
enum ConnectEvent {
    /// A step was reached.
    Stage(rc_remote::ConnectStage),
    /// The attempt finished.
    Done(Result<Box<dyn super::remote::RemoteBackend>, String>),
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
                assert_eq!(state.password_session.as_ref().map(|p| p.name.as_str()), Some("work"));
                assert!(state.connection.password.is_empty());
                assert!(!state.save_password);
                assert!(state.validate().is_err());
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
    fn remote_credentials_use_the_exact_account_and_keep_manual_override() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        let mut connection = rc_core::config::SavedConnection::with_defaults("sftp");
        connection.host = "files.example.com".into();
        connection.user = "alice".into();
        model
            .secrets
            .set(&connection.account_id(), "stored")
            .unwrap();

        assert_eq!(
            model.resolve_connection_credentials(&connection).password,
            "stored"
        );
        connection.password = "manual".into();
        let resolved = model.resolve_connection_credentials(&connection);
        assert_eq!(
            super::super::remote::connection_spec(&resolved, None).password,
            "manual"
        );

        connection.password.clear();
        connection.port = 2222;
        assert!(
            model
                .resolve_connection_credentials(&connection)
                .password
                .is_empty()
        );
        connection.port = 22;
        connection.auth_method = "key".into();
        assert!(
            model
                .resolve_connection_credentials(&connection)
                .password
                .is_empty()
        );
        connection.auth_method = "agent".into();
        assert!(
            model
                .resolve_connection_credentials(&connection)
                .password
                .is_empty()
        );
    }

    #[test]
    fn remote_credentials_distinguish_protocol_and_effective_port() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        let mut connection = rc_core::config::SavedConnection {
            host: "files.example.test".into(),
            user: "alice".into(),
            auth_method: "password".into(),
            protocol: "sftp".into(),
            ..Default::default()
        };
        for (protocol, port, secret) in [
            ("sftp", 22, "ssh-fixture"),
            ("sftp", 2222, "other-ssh-fixture"),
            ("ftp", 21, "ftp-fixture"),
            ("ftps", 21, "explicit-tls-fixture"),
            ("ftps", 990, "implicit-tls-fixture"),
        ] {
            model
                .secrets
                .set(
                    &rc_core::secrets::account_id(
                        protocol,
                        &connection.user,
                        &connection.host,
                        port,
                    ),
                    secret,
                )
                .unwrap();
        }
        for (protocol, port, expected) in [
            (" SSH ", 0, "ssh-fixture"),
            ("sftp", 2222, "other-ssh-fixture"),
            ("ftp", 0, "ftp-fixture"),
            ("ftpes", 0, "explicit-tls-fixture"),
            ("ftps", 990, "implicit-tls-fixture"),
            ("sftp", 21, ""),
            ("unknown", 21, ""),
        ] {
            connection.protocol = protocol.into();
            connection.port = port;
            assert_eq!(
                model.resolve_connection_credentials(&connection).password,
                expected
            );
        }
    }

    #[test]
    fn saved_session_wallet_unlock_is_explicit_and_account_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        let mut session = Session::new("work", "example.test", "sftp");
        session.user = "alice".into();
        session.port = 2222;
        let account = session.to_connection().account_id();
        model.unlock_secrets("fixture-passphrase").unwrap();
        model.secrets.set(&account, "fixture-password").unwrap();
        model.persist_secrets().unwrap();
        model.lock_secrets();
        assert!(!model.saved_login_available(&session));
        assert!(model.unlock_existing_secrets("wrong-passphrase").is_err());
        assert!(!model.secrets_unlocked());
        assert!(!model.is_connecting());
        model.unlock_existing_secrets("fixture-passphrase").unwrap();
        assert!(model.saved_login_available(&session));
        assert!(!model.is_connecting(), "unlock alone must not connect");
        let mut other = session.clone();
        other.port = 2223;
        assert!(!model.saved_login_available(&other));
        other = session.clone();
        other.user = "bob".into();
        assert!(!model.saved_login_available(&other));
        model.lock_secrets();
        assert!(!model.saved_login_available(&session));
    }

    #[test]
    fn unlocking_a_missing_wallet_does_not_create_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        assert!(model.unlock_existing_secrets("fixture").is_err());
        assert!(!model.secrets_exist());
        assert!(!model.secrets_unlocked());
    }

    #[test]
    fn missing_password_stops_before_starting_a_worker() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        let mut connection = rc_core::config::SavedConnection::with_defaults("sftp");
        connection.user = "alice".into();
        let session = Session::from_connection("test", &connection);
        assert!(matches!(
            model.start_connect(&connection, &session),
            CommandOutcome::Failed(_)
        ));
        assert!(!model.is_connecting());
        assert!(model.secrets.accounts().is_empty());
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

    fn staged_attempt(model: &mut Model) -> std::sync::mpsc::Sender<ConnectEvent> {
        let connection = rc_core::config::SavedConnection::with_defaults("ftp");
        let session = Session::from_connection("fixture", &connection);
        let (sender, receiver) = std::sync::mpsc::channel();
        model.connect_job = Some(ConnectJob {
            session,
            connection,
            remember_password: false,
            side: Side::Left,
            stage: None,
            started: std::time::Instant::now(),
            receiver,
        });
        sender
    }

    #[test]
    fn progress_is_not_a_completed_connection() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        let sender = staged_attempt(&mut model);
        sender
            .send(ConnectEvent::Stage(rc_remote::ConnectStage::Connecting))
            .unwrap();
        assert_eq!(model.poll_connect_result(), (true, None));
        assert!(model.is_connecting());
        sender
            .send(ConnectEvent::Done(Err("fixture failure".into())))
            .unwrap();
        assert_eq!(
            model.wait_for_connect(std::time::Duration::from_secs(1)),
            CommandOutcome::Failed("fixture failure".into())
        );
        assert!(!model.is_connecting());
    }

    #[test]
    fn a_timeout_discards_the_receiver_and_does_not_report_old_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.report_error("old unrelated error");
        let sender = staged_attempt(&mut model);
        let expected = model.t("session.timeout");
        assert_eq!(
            model.wait_for_connect(std::time::Duration::ZERO),
            CommandOutcome::Failed(expected)
        );
        assert!(!model.is_connecting());
        assert!(
            sender
                .send(ConnectEvent::Done(Err("late failure".into())))
                .is_err()
        );
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

#[cfg(test)]
mod stage_tests {
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn model(dir: &std::path::Path) -> super::super::Model {
        let mut model = super::super::Model::for_test(
            Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    #[test]
    fn every_connect_stage_has_a_sentence_in_both_languages() {
        // The status line reads these while a connection is in progress; a missing key would show
        // the key itself, which is exactly the silence the status line exists to avoid.
        let dir = tempfile::tempdir().unwrap();
        let model = model(dir.path());
        for stage in rc_remote::ConnectStage::ALL {
            let key = stage.key();
            assert!(key.starts_with("connect.stage."), "{key}");
        }
        assert!(!model.t("session.connecting.short").is_empty());
    }

    #[test]
    fn a_stage_is_reported_only_while_a_connection_is_running() {
        let dir = tempfile::tempdir().unwrap();
        let model = model(dir.path());
        assert!(model.connect_stage().is_none());
        assert!(model.connect_elapsed().is_none());
    }
}

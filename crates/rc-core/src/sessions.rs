//! Saved connection profiles (Phase 13.3).
//!
//! A *session* is everything needed to reopen a remote connection except the
//! secret itself: host, port, user, protocol, authentication method, key
//! reference, the initial directories and the transfer flags. Sessions live in
//! their own file (`sessions.toml`) so the settings file stays short and so a
//! session can be shared or imported without dragging the whole configuration
//! along. Passwords never appear here; they belong to the credential store.
//!
//! The module also reads OpenSSH's `ssh_config`, because that is where the
//! addresses most people already have written down actually live.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One saved connection profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// Name shown in the list and used by the command line and MCP tools.
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// `ftp`, `ftps` or `sftp`.
    pub protocol: String,
    /// `password`, `key` or `agent` (SFTP only).
    pub auth_method: String,
    /// Private key file for `key` authentication.
    pub key_path: String,
    /// `strict`, `tofu` or `ignore`.
    pub host_key_policy: String,
    /// Directory the local panel should show after connecting.
    pub local_dir: String,
    /// Directory the remote panel should show after connecting.
    pub remote_dir: String,
    /// Passive mode for FTP; ignored for SFTP.
    pub passive: bool,
    pub anonymous: bool,
    /// Name-to-encoding used for FTP listings (`auto` when empty).
    pub encoding: String,
    /// Prefer `MLSD` over `LIST` when the server supports it.
    pub use_mlsd: bool,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            name: String::new(),
            host: "localhost".to_string(),
            port: 21,
            user: String::new(),
            protocol: "ftp".to_string(),
            auth_method: "password".to_string(),
            key_path: String::new(),
            host_key_policy: "tofu".to_string(),
            local_dir: String::new(),
            remote_dir: String::new(),
            passive: true,
            anonymous: false,
            encoding: "auto".to_string(),
            use_mlsd: true,
        }
    }
}

impl Session {
    /// A profile for `host` using the protocol's default port.
    pub fn new(name: impl Into<String>, host: impl Into<String>, protocol: &str) -> Self {
        Session {
            name: name.into(),
            host: host.into(),
            port: crate::config::SavedConnection::default_port(protocol),
            protocol: protocol.to_string(),
            ..Default::default()
        }
    }

    /// Build a profile from the connection editor's state.
    pub fn from_connection(
        name: impl Into<String>,
        connection: &crate::config::SavedConnection,
    ) -> Self {
        Session {
            name: name.into(),
            host: connection.host.clone(),
            port: if connection.port == 0 {
                crate::config::SavedConnection::default_port(&connection.protocol)
            } else {
                connection.port
            },
            user: connection.user.clone(),
            protocol: connection.protocol.clone(),
            auth_method: connection.auth_method.clone(),
            key_path: connection.key_path.clone(),
            host_key_policy: connection.host_key_policy.clone(),
            local_dir: connection.local_dir.clone(),
            remote_dir: connection.remote_dir.clone(),
            passive: connection.passive,
            anonymous: connection.anonymous,
            ..Default::default()
        }
    }

    /// Turn the profile back into editable connection state. The password and
    /// the key passphrase stay empty: they come from the credential store.
    pub fn to_connection(&self) -> crate::config::SavedConnection {
        crate::config::SavedConnection {
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            password: String::new(),
            protocol: self.protocol.clone(),
            auth_method: self.auth_method.clone(),
            key_path: self.key_path.clone(),
            key_passphrase: String::new(),
            host_key_policy: self.host_key_policy.clone(),
            remote_dir: self.remote_dir.clone(),
            local_dir: self.local_dir.clone(),
            passive: self.passive,
            anonymous: self.anonymous,
        }
    }

    /// True when the profile needs a password that the store has to provide.
    pub fn needs_password(&self) -> bool {
        !self.anonymous && self.auth_method.eq_ignore_ascii_case("password")
    }

    /// `user@host:port`, the line shown next to the name.
    pub fn address(&self) -> String {
        let user = if self.anonymous {
            "anonymous@".to_string()
        } else if self.user.is_empty() {
            String::new()
        } else {
            format!("{}@", self.user)
        };
        format!("{user}{}:{}", self.host, self.port)
    }

    /// One-line description for lists: `name — user@host:port (protocol)`.
    pub fn summary(&self) -> String {
        format!(
            "{} — {} ({})",
            self.name,
            self.address(),
            self.protocol.to_uppercase()
        )
    }
}

/// The session file: an ordered list plus a few conveniences.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionStore {
    pub sessions: Vec<Session>,
}

impl SessionStore {
    pub fn new() -> Self {
        SessionStore::default()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn names(&self) -> Vec<String> {
        self.sessions.iter().map(|s| s.name.clone()).collect()
    }

    pub fn get(&self, name: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.name == name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.name == name)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.sessions.iter().position(|s| s.name == name)
    }

    /// A name that is not taken yet: `base`, then `base 2`, `base 3`, …
    pub fn free_name(&self, base: &str) -> String {
        let base = if base.trim().is_empty() {
            "session"
        } else {
            base.trim()
        };
        if self.get(base).is_none() {
            return base.to_string();
        }
        for n in 2.. {
            let candidate = format!("{base} {n}");
            if self.get(&candidate).is_none() {
                return candidate;
            }
        }
        unreachable!("the counter has no upper bound")
    }

    /// `<stem> copy`, `<stem> copy 2`, … — used by [`SessionStore::duplicate`]
    /// so a copy of a copy is `dev copy 2`, not `dev copy copy`.
    fn free_copy_name(&self, stem: &str) -> String {
        let first = format!("{stem} copy");
        if self.get(&first).is_none() {
            return first;
        }
        for n in 2.. {
            let candidate = format!("{stem} copy {n}");
            if self.get(&candidate).is_none() {
                return candidate;
            }
        }
        unreachable!("the counter has no upper bound")
    }

    /// Add or replace a session by name; returns the name that was stored.
    pub fn put(&mut self, session: Session) -> String {
        let name = if session.name.trim().is_empty() {
            self.free_name("session")
        } else {
            session.name.trim().to_string()
        };
        let mut session = session;
        session.name = name.clone();
        match self.index_of(&name) {
            Some(index) => self.sessions[index] = session,
            None => self.sessions.push(session),
        }
        name
    }

    /// Copy a session under a free name; returns the new name.
    pub fn duplicate(&mut self, name: &str) -> Option<String> {
        let source = self.get(name)?.clone();
        let new_name = self.free_copy_name(&copy_stem(name));
        let mut copy = source;
        copy.name = new_name.clone();
        self.sessions.push(copy);
        Some(new_name)
    }

    /// Rename a session; refuses duplicate target names and empty names.
    pub fn rename(&mut self, old: &str, new: &str) -> bool {
        let new = new.trim();
        if new.is_empty() || self.get(new).is_some() {
            return false;
        }
        match self.get_mut(old) {
            Some(session) => {
                session.name = new.to_string();
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        match self.index_of(name) {
            Some(index) => {
                self.sessions.remove(index);
                true
            }
            None => false,
        }
    }

    /// Move a session one place up, returning true when it moved.
    pub fn move_up(&mut self, name: &str) -> bool {
        let Some(index) = self.index_of(name) else {
            return false;
        };
        if index == 0 {
            return false;
        }
        self.sessions.swap(index, index - 1);
        true
    }

    /// Move a session one place down, returning true when it moved.
    pub fn move_down(&mut self, name: &str) -> bool {
        let Some(index) = self.index_of(name) else {
            return false;
        };
        if index + 1 >= self.sessions.len() {
            return false;
        }
        self.sessions.swap(index, index + 1);
        true
    }

    /// The whole store as TOML.
    pub fn to_text(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Parse a store from TOML; an empty or unreadable file yields an empty store.
    pub fn from_text(text: &str) -> Self {
        toml::from_str(text).unwrap_or_default()
    }

    /// Load the store, returning an empty one when the file does not exist yet.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => SessionStore::from_text(&text),
            Err(_) => SessionStore::new(),
        }
    }

    /// Write the store, creating the directory when needed.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_text())
    }
}

/// Default location of the session file.
pub fn default_path() -> PathBuf {
    crate::config::config_dir().join("sessions.toml")
}

/// One host block of an OpenSSH configuration file.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SshConfigHost {
    pub aliases: Vec<String>,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

impl SshConfigHost {
    /// The name a session would get: the first concrete alias.
    pub fn name(&self) -> Option<&str> {
        self.aliases
            .iter()
            .find(|a| !a.contains('*') && !a.contains('?'))
            .map(|a| a.as_str())
    }

    /// True when the block only describes a pattern (`Host *`, `Host 192.168.*`).
    pub fn is_pattern(&self) -> bool {
        self.name().is_none()
    }

    /// Convert to a session profile, expanding `~` in the identity path.
    pub fn to_session(&self) -> Option<Session> {
        let name = self.name()?.to_string();
        let host = self.host_name.clone().unwrap_or_else(|| name.clone());
        let mut session = Session::new(name, host, "sftp");
        session.user = self.user.clone().unwrap_or_default();
        session.port = self.port.unwrap_or(22);
        if let Some(identity) = &self.identity_file {
            session.auth_method = "key".to_string();
            session.key_path = expand_home(identity);
        }
        Some(session)
    }
}

/// Expand a leading `~` or `$HOME` in a path.
pub fn expand_home(path: &str) -> String {
    let path = path.trim();
    let rest = path
        .strip_prefix("~/")
        .or_else(|| path.strip_prefix("$HOME/"))
        .or(if path == "~" { Some("") } else { None });
    match rest {
        Some(rest) => match crate::drive::home_dir() {
            Some(home) => home.join(rest).to_string_lossy().to_string(),
            None => path.to_string(),
        },
        None => path.to_string(),
    }
}

/// Parse the host blocks of an OpenSSH configuration file.
///
/// Only the keys that matter for a session are read; everything else is ignored
/// so an exotic file cannot break the import. `Include` is not followed (it would
/// need the filesystem), which is documented behaviour.
pub fn parse_ssh_config(text: &str) -> Vec<SshConfigHost> {
    let mut hosts: Vec<SshConfigHost> = Vec::new();
    let mut current: Option<SshConfigHost> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // OpenSSH accepts `Key value` and `Key=value`.
        let (key, value) = match line.split_once('=') {
            Some((k, v)) if !k.trim().contains(char::is_whitespace) => (k.trim(), v.trim()),
            _ => match line.split_once(char::is_whitespace) {
                Some((k, v)) => (k.trim(), v.trim()),
                None => continue,
            },
        };
        // Drop trailing comments that are not inside quotes.
        let value = strip_inline_comment(value);
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                if let Some(host) = current.take() {
                    hosts.push(host);
                }
                current = Some(SshConfigHost {
                    aliases: value.split_whitespace().map(|a| a.to_string()).collect(),
                    ..Default::default()
                });
            }
            "hostname" => {
                if let Some(host) = current.as_mut() {
                    host.host_name = Some(value.to_string());
                }
            }
            "user" => {
                if let Some(host) = current.as_mut() {
                    host.user = Some(value.to_string());
                }
            }
            "port" => {
                if let Some(host) = current.as_mut()
                    && let Ok(port) = value.parse::<u16>()
                {
                    host.port = Some(port);
                }
            }
            "identityfile" => {
                if let Some(host) = current.as_mut() {
                    host.identity_file = Some(value.to_string());
                }
            }
            _ => {}
        }
    }
    if let Some(host) = current.take() {
        hosts.push(host);
    }
    hosts
}

/// Strip a trailing ` copy` or ` copy <n>` from a name, so copies of copies keep
/// one readable stem: `dev copy 3` → `dev`.
fn copy_stem(name: &str) -> String {
    let mut text = name.trim();
    if let Some((head, tail)) = text.rsplit_once(' ') {
        if tail == "copy" {
            text = head.trim();
        } else if tail.chars().all(|c| c.is_ascii_digit())
            && let Some((head2, marker)) = head.rsplit_once(' ')
            && marker == "copy"
        {
            text = head2.trim();
        }
    }
    text.to_string()
}

/// Remove a `# comment` from a value unless it is quoted.
fn strip_inline_comment(value: &str) -> &str {
    let mut quoted = false;
    for (index, ch) in value.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '#' if !quoted => return value[..index].trim(),
            _ => {}
        }
    }
    value.trim()
}

/// Sessions for the non-pattern host blocks of an OpenSSH configuration.
pub fn sessions_from_ssh_config(text: &str) -> Vec<Session> {
    parse_ssh_config(text)
        .iter()
        .filter(|host| !host.is_pattern())
        .filter_map(|host| host.to_session())
        .collect()
}

impl SessionStore {
    /// Merge imported sessions, renaming collisions instead of overwriting them.
    /// Returns the names that were added.
    pub fn import(&mut self, sessions: impl IntoIterator<Item = Session>) -> Vec<String> {
        let mut added = Vec::new();
        for session in sessions {
            let name = self.free_name(&session.name);
            let mut session = session;
            session.name = name.clone();
            self.sessions.push(session);
            added.push(name);
        }
        added
    }

    /// Import an OpenSSH configuration file, returning the imported names.
    pub fn import_ssh_config(&mut self, path: &Path) -> std::io::Result<Vec<String>> {
        let text = std::fs::read_to_string(path)?;
        Ok(self.import(sessions_from_ssh_config(&text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rc-sessions-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("sessions.toml")
    }

    #[test]
    fn a_session_round_trips_through_toml() {
        let mut store = SessionStore::new();
        let mut session = Session::new("work", "files.example.com", "sftp");
        session.user = "alice".into();
        session.remote_dir = "/srv/data".into();
        session.local_dir = "/home/alice/data".into();
        session.auth_method = "key".into();
        session.key_path = "/home/alice/.ssh/id_ed25519".into();
        store.put(session);

        let text = store.to_text();
        let back = SessionStore::from_text(&text);
        assert_eq!(store, back, "a store survives a save/load cycle");
        let loaded = back.get("work").unwrap();
        assert_eq!(loaded.port, 22, "SFTP sessions default to port 22");
        assert_eq!(loaded.address(), "alice@files.example.com:22");
    }

    #[test]
    fn a_store_survives_a_real_file() {
        let path = temp_path("file");
        let _ = std::fs::remove_file(&path);
        let mut store = SessionStore::new();
        store.put(Session::new("one", "a.example.com", "ftp"));
        assert!(store.save(&path).is_ok());
        assert!(path.is_file());
        let loaded = SessionStore::load(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.names(), vec!["one".to_string()]);
        // A missing file is an empty store, not an error.
        let missing = SessionStore::load(&path.with_extension("nope"));
        assert!(missing.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rename_duplicate_and_delete_work_by_name() {
        let mut store = SessionStore::new();
        store.put(Session::new("alpha", "a.example.com", "ftp"));
        store.put(Session::new("beta", "b.example.com", "ftp"));
        assert_eq!(store.len(), 2);

        assert!(store.rename("alpha", "gamma"));
        assert!(store.get("gamma").is_some());
        assert!(
            store.rename("gamma", "beta").eq(&false),
            "duplicate names are refused"
        );
        assert!(!store.rename("missing", "x"));

        let copy = store.duplicate("beta").unwrap();
        assert_eq!(copy, "beta copy");
        assert_eq!(store.len(), 3);
        assert_eq!(store.duplicate("beta copy").unwrap(), "beta copy 2");

        assert!(store.remove("beta"));
        assert!(store.get("beta").is_none());
        assert!(!store.remove("beta"), "removing twice reports false");
        assert_eq!(
            store.names(),
            vec![
                "gamma".to_string(),
                "beta copy".to_string(),
                "beta copy 2".to_string()
            ]
        );
    }

    #[test]
    fn a_session_without_credentials_is_still_listed() {
        let mut store = SessionStore::new();
        let mut session = Session::new("", "anon.example.com", "ftp");
        session.anonymous = true;
        let name = store.put(session);
        assert_eq!(name, "session", "an unnamed session gets a default name");
        let listed = store.get("session").unwrap();
        assert!(listed.user.is_empty());
        assert_eq!(listed.address(), "anonymous@anon.example.com:21");
        assert!(!listed.needs_password(), "anonymous needs no secret");
        // …and a password session does.
        let mut with_user = Session::new("bob-srv", "bob.example.com", "ftp");
        with_user.user = "bob".into();
        assert!(with_user.needs_password());
    }

    #[test]
    fn reordering_moves_one_place_and_stops_at_the_edges() {
        let mut store = SessionStore::new();
        for name in ["a", "b", "c"] {
            store.put(Session::new(name, "h.example.com", "ftp"));
        }
        assert!(!store.move_up("a"), "the first entry cannot move up");
        assert!(store.move_up("c"));
        assert_eq!(
            store.names(),
            vec!["a".to_string(), "c".to_string(), "b".to_string()]
        );
        assert!(!store.move_down("b"), "the last entry cannot move down");
        assert!(store.move_down("a"));
        assert_eq!(
            store.names(),
            vec!["c".to_string(), "a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn put_replaces_a_session_with_the_same_name() {
        let mut store = SessionStore::new();
        store.put(Session::new("dev", "localhost", "ftp"));
        let mut changed = store.get("dev").unwrap().clone();
        changed.user = "alice".into();
        store.put(changed);
        assert_eq!(store.len(), 1, "the name is the key");
        assert_eq!(store.get("dev").unwrap().user, "alice");
    }

    #[test]
    fn a_connection_and_a_session_convert_both_ways() {
        let mut connection = crate::config::SavedConnection::with_defaults("sftp");
        connection.user = "alice".into();
        connection.password = "secret".into();
        connection.key_passphrase = "phrase".into();
        let session = Session::from_connection("dev", &connection);
        assert_eq!(session.port, 22);
        assert_eq!(session.user, "alice");
        let back = session.to_connection();
        assert_eq!(back.host, connection.host);
        assert_eq!(back.port, connection.port);
        assert_eq!(back.user, "alice");
        assert!(
            back.password.is_empty(),
            "the password is not part of a session"
        );
        assert!(back.key_passphrase.is_empty());
        // A connection without a port gets the protocol default.
        let mut portless = crate::config::SavedConnection::with_defaults("ftp");
        portless.port = 0;
        assert_eq!(Session::from_connection("x", &portless).port, 21);
    }

    #[test]
    fn openssh_config_is_parsed_including_aliases_and_patterns() {
        let text = "\
# my servers
Host work
    HostName files.example.com
    User alice
    Port 2222
    IdentityFile ~/.ssh/id_work

Host short alias2
    HostName alias2.example.com

Host *
    ServerAliveInterval 60
    User default-user

Host 192.168.*
    User nobody
";
        let hosts = parse_ssh_config(text);
        assert_eq!(hosts.len(), 4);
        assert_eq!(hosts[0].name(), Some("work"));
        assert_eq!(hosts[0].host_name.as_deref(), Some("files.example.com"));
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(
            hosts[1].aliases,
            vec!["short".to_string(), "alias2".to_string()]
        );
        assert!(hosts[2].is_pattern(), "Host * is a pattern, not a session");
        assert!(hosts[3].is_pattern());

        let sessions = sessions_from_ssh_config(text);
        assert_eq!(sessions.len(), 2, "patterns are skipped");
        let work = &sessions[0];
        assert_eq!(work.name, "work");
        assert_eq!(work.host, "files.example.com");
        assert_eq!(work.user, "alice");
        assert_eq!(work.port, 2222);
        assert_eq!(work.auth_method, "key");
        assert!(work.key_path.ends_with("id_work"));
        // An alias without HostName connects to the alias itself.
        assert_eq!(sessions[1].host, "alias2.example.com");
        assert_eq!(sessions[1].port, 22, "SSH sessions default to 22");
    }

    #[test]
    fn openssh_config_accepts_equals_and_inline_comments() {
        let text = "Host = prod\n  HostName=prod.example.com # the production box\n  Port=2022\n  User=deploy\n";
        let hosts = parse_ssh_config(text);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].host_name.as_deref(), Some("prod.example.com"));
        assert_eq!(hosts[0].port, Some(2022));
        assert_eq!(hosts[0].user.as_deref(), Some("deploy"));
    }

    #[test]
    fn importing_into_a_store_never_overwrites_and_reports_names() {
        let mut store = SessionStore::new();
        store.put(Session::new("work", "old.example.com", "ftp"));
        let added = store.import(vec![
            Session::new("work", "files.example.com", "sftp"),
            Session::new("home", "home.example.com", "sftp"),
        ]);
        assert_eq!(added, vec!["work 2".to_string(), "home".to_string()]);
        assert_eq!(store.get("work").unwrap().host, "old.example.com");
        assert_eq!(store.get("work 2").unwrap().host, "files.example.com");
    }

    #[test]
    fn an_ssh_config_file_can_be_imported_from_disk() {
        let path = temp_path("sshconfig");
        let config = path.with_file_name("ssh_config");
        std::fs::write(&config, "Host box\n  HostName box.example.com\n  User me\n").unwrap();
        let mut store = SessionStore::new();
        let added = store.import_ssh_config(&config).unwrap();
        assert_eq!(added, vec!["box".to_string()]);
        assert_eq!(store.get("box").unwrap().user, "me");
        let _ = std::fs::remove_file(&config);
    }

    #[test]
    fn a_broken_session_file_does_not_panic() {
        let store = SessionStore::from_text("this is not toml [[[");
        assert!(store.is_empty());
        let store = SessionStore::from_text("");
        assert!(store.is_empty());
    }

    #[test]
    fn the_summary_line_names_the_target() {
        let mut session = Session::new("work", "files.example.com", "sftp");
        session.user = "alice".into();
        assert_eq!(
            session.summary(),
            "work — alice@files.example.com:22 (SFTP)"
        );
        let anonymous = Session {
            anonymous: true,
            name: "pub".into(),
            host: "ftp.example.com".into(),
            ..Default::default()
        };
        assert_eq!(
            anonymous.summary(),
            "pub — anonymous@ftp.example.com:21 (FTP)"
        );
    }
}

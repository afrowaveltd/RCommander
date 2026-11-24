//! Connection parameters: the [`Protocol`] enum and [`ConnectionSpec`].

use std::path::PathBuf;
use std::time::Duration;

use rc_core::config::SavedConnection;

use crate::error::RemoteError;

/// The wire protocol a panel speaks.
///
/// `Ftps` is FTP on top of TLS (explicit `AUTH TLS`, or implicit on port 990)
/// and is only compiled in with the `ftps` cargo feature; `Sftp` needs the
/// `sftp` cargo feature. [`crate::connect`] returns
/// [`RemoteError::Unsupported`] with a clear message when the requested
/// protocol is not part of the build, so the GUI can degrade gracefully.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// Plain FTP (RFC 959).
    #[default]
    Ftp,
    /// FTP over TLS.
    Ftps,
    /// SSH file transfer protocol.
    Sftp,
}

impl Protocol {
    /// Parse the protocol name used in the configuration file
    /// (`"ftp"`, `"ftps"`, `"sftp"`; case-insensitive).
    pub fn from_label(label: &str) -> Option<Protocol> {
        match label.trim().to_ascii_lowercase().as_str() {
            "" | "ftp" => Some(Protocol::Ftp),
            "ftps" | "ftpes" | "ftp+tls" => Some(Protocol::Ftps),
            "sftp" | "ssh" => Some(Protocol::Sftp),
            _ => None,
        }
    }

    /// The protocol name as stored in the configuration file.
    pub fn label(self) -> &'static str {
        match self {
            Protocol::Ftp => "ftp",
            Protocol::Ftps => "ftps",
            Protocol::Sftp => "sftp",
        }
    }

    /// Default TCP port of the protocol (`0` in a spec means this one).
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Ftp | Protocol::Ftps => 21,
            Protocol::Sftp => 22,
        }
    }

    /// True when the protocol is encrypted end to end.
    pub fn is_secure(self) -> bool {
        matches!(self, Protocol::Ftps | Protocol::Sftp)
    }
}

/// Everything needed to open one connection, plus the display/policy knobs the
/// GUI keeps next to it.
///
/// The struct is deliberately plain data so it can be cloned into a worker
/// thread; it is produced from [`rc_core::config::SavedConnection`] with
/// [`ConnectionSpec::from_saved`] and written back with
/// [`ConnectionSpec::to_saved`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionSpec {
    /// Display name of the saved connection.
    pub name: String,
    /// Host name or IP literal (IPv6 literals may be bracketed).
    pub host: String,
    /// TCP port; `0` means "use the protocol default" (see
    /// [`ConnectionSpec::effective_port`]).
    pub port: u16,
    /// User name; an empty string or `"anonymous"` means anonymous FTP.
    pub user: String,
    /// Password (empty for anonymous connections).
    pub password: String,
    /// Wire protocol.
    pub protocol: Protocol,
    /// `true` for passive mode, `false` for active mode (FTP only).
    pub passive: bool,
    /// Character-set label for remote names, e.g. `"utf-8"` or `"latin-1"`.
    pub encoding: String,
    /// Directory to change into right after login; empty means the home
    /// directory the server starts in.
    pub remote_dir: String,
    /// Socket timeout used for the control channel and every transfer.
    pub timeout: Duration,
    /// How to prove who we are (SSH only; FTP always uses `password`).
    pub auth: AuthMethod,
    /// How strictly the server's host key is checked (SSH only).
    pub host_key_policy: rc_core::knownhosts::HostKeyPolicy,
    /// Where trusted host keys are stored; `None` uses the default location.
    pub known_hosts_path: Option<PathBuf>,
}

/// Authentication method for SSH-based protocols.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AuthMethod {
    /// Password from the credential store.
    #[default]
    Password,
    /// Private key file, optionally protected by a passphrase.
    KeyFile {
        /// Path of the private key (the matching `.pub` file is used when present).
        path: PathBuf,
        /// Passphrase that unlocks the key; `None` for an unprotected key.
        passphrase: Option<String>,
    },
    /// Delegate to a running `ssh-agent`.
    Agent,
}

impl AuthMethod {
    /// Identifier used in the settings file.
    pub fn id(&self) -> &'static str {
        match self {
            AuthMethod::Password => "password",
            AuthMethod::KeyFile { .. } => "key",
            AuthMethod::Agent => "agent",
        }
    }

    /// Parse an identifier, falling back to password authentication.
    pub fn parse(text: &str) -> AuthMethod {
        match text.trim().to_ascii_lowercase().as_str() {
            "key" | "keyfile" | "publickey" => AuthMethod::KeyFile {
                path: PathBuf::new(),
                passphrase: None,
            },
            "agent" | "ssh-agent" => AuthMethod::Agent,
            _ => AuthMethod::Password,
        }
    }
}

impl Default for ConnectionSpec {
    fn default() -> Self {
        ConnectionSpec {
            name: String::new(),
            host: String::new(),
            port: 0,
            user: String::new(),
            password: String::new(),
            protocol: Protocol::Ftp,
            passive: true,
            encoding: "utf-8".to_string(),
            remote_dir: String::new(),
            timeout: DEFAULT_TIMEOUT,
            auth: AuthMethod::default(),
            host_key_policy: rc_core::knownhosts::HostKeyPolicy::TrustOnFirstUse,
            known_hosts_path: None,
        }
    }
}

/// Timeout used when a spec carries a zero duration.
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

impl ConnectionSpec {
    /// Build a spec from a stored connection.
    ///
    /// [`rc_core::config::SavedConnection`] has no fields for the character set
    /// or the timeout, so those fall back to `"utf-8"` and 30 s.
    pub fn from_saved(saved: &SavedConnection) -> Self {
        ConnectionSpec {
            name: saved.name.clone(),
            host: saved.host.clone(),
            port: saved.port,
            user: saved.user.clone(),
            password: saved.password.clone(),
            protocol: Protocol::from_label(&saved.protocol).unwrap_or(Protocol::Ftp),
            passive: saved.passive,
            encoding: "utf-8".to_string(),
            remote_dir: saved.remote_dir.clone(),
            timeout: DEFAULT_TIMEOUT,
            auth: match saved.auth_method.trim().to_ascii_lowercase().as_str() {
                "key" => AuthMethod::KeyFile {
                    path: PathBuf::from(saved.key_path.trim()),
                    passphrase: if saved.key_passphrase.is_empty() {
                        None
                    } else {
                        Some(saved.key_passphrase.clone())
                    },
                },
                "agent" => AuthMethod::Agent,
                _ => AuthMethod::Password,
            },
            host_key_policy: rc_core::knownhosts::HostKeyPolicy::parse(&saved.host_key_policy),
            known_hosts_path: None,
        }
    }

    /// Build a stored connection from this spec.
    ///
    /// The character set and the timeout are not representable in
    /// [`rc_core::config::SavedConnection`] and are dropped; `anonymous` is
    /// derived from the user name (see [`ConnectionSpec::is_anonymous`]).
    pub fn to_saved(&self) -> SavedConnection {
        SavedConnection {
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            password: self.password.clone(),
            protocol: self.protocol.label().to_string(),
            remote_dir: self.remote_dir.clone(),
            local_dir: String::new(),
            passive: self.passive,
            anonymous: self.is_anonymous(),
            auth_method: self.auth.id().to_string(),
            key_path: match &self.auth {
                AuthMethod::KeyFile { path, .. } => path.to_string_lossy().to_string(),
                _ => String::new(),
            },
            key_passphrase: match &self.auth {
                AuthMethod::KeyFile {
                    passphrase: Some(passphrase),
                    ..
                } => passphrase.clone(),
                _ => String::new(),
            },
            host_key_policy: self.host_key_policy.id().to_string(),
        }
    }

    /// The port actually connected to: `port`, or the protocol default when the
    /// spec says `0`.
    pub fn effective_port(&self) -> u16 {
        if self.port == 0 {
            self.protocol.default_port()
        } else {
            self.port
        }
    }

    /// True when the spec logs in anonymously, i.e. the user name is empty or
    /// `"anonymous"` (any case).
    pub fn is_anonymous(&self) -> bool {
        let user = self.user.trim();
        user.is_empty() || user.eq_ignore_ascii_case("anonymous")
    }

    /// Reject specs that cannot possibly connect.
    ///
    /// Checks, in order:
    ///
    /// * the host must not be empty, must not contain whitespace, a slash or a
    ///   URL scheme, and must not embed a port (`"host:2121"` — the port has its
    ///   own field, and that mistake is otherwise silently sent to DNS);
    /// * an IPv6 literal must be bracketed and complete (`"[::1]"`);
    /// * `port` is a `u16`, so out-of-range values are unrepresentable; `0`
    ///   means "protocol default" and every other value is used verbatim;
    /// * SFTP needs a real user name — anonymous SFTP does not exist.
    pub fn validate(&self) -> Result<(), RemoteError> {
        let host = self.host.trim();
        let port = self.effective_port();
        let connect_error = |reason: &str| RemoteError::Connect {
            host: self.host.clone(),
            port,
            reason: reason.to_string(),
        };

        if host.is_empty() {
            return Err(connect_error("the host is empty"));
        }
        if host.contains("://") {
            return Err(connect_error("the host must not contain a URL scheme"));
        }
        if host.chars().any(|c| c.is_whitespace()) {
            return Err(connect_error("the host must not contain whitespace"));
        }
        if host.contains('/') || host.contains('\\') {
            return Err(connect_error("the host must not contain a path separator"));
        }
        if let Some(rest) = host.strip_prefix('[') {
            let Some((_literal, after)) = rest.split_once(']') else {
                return Err(connect_error("the bracketed IPv6 address is not closed"));
            };
            if !after.is_empty() {
                return Err(connect_error(
                    "the host must not embed a port; set the port field instead",
                ));
            }
        } else if host.contains(':') {
            return Err(connect_error(
                "the host must not embed a port; set the port field instead",
            ));
        }
        if self.protocol == Protocol::Sftp && self.user.trim().is_empty() {
            return Err(RemoteError::Authentication(
                "sftp requires a user name; anonymous sftp does not exist".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ConnectionSpec {
        ConnectionSpec {
            name: "web".into(),
            host: "ftp.example.com".into(),
            port: 0,
            user: "anonymous".into(),
            password: String::new(),
            protocol: Protocol::Ftp,
            passive: true,
            encoding: "utf-8".into(),
            remote_dir: "/pub".into(),
            timeout: DEFAULT_TIMEOUT,
            ..ConnectionSpec::default()
        }
    }

    #[test]
    fn saved_connection_round_trips() {
        let original = spec();
        let saved = original.to_saved();
        assert_eq!(saved.protocol, "ftp");
        assert!(saved.anonymous);
        assert_eq!(saved.passive, original.passive);
        let back = ConnectionSpec::from_saved(&saved);
        assert_eq!(back, original);
    }

    #[test]
    fn protocol_labels_round_trip_and_fall_back_to_ftp() {
        for protocol in [Protocol::Ftp, Protocol::Ftps, Protocol::Sftp] {
            assert_eq!(Protocol::from_label(protocol.label()), Some(protocol));
        }
        assert_eq!(Protocol::from_label("SFTP"), Some(Protocol::Sftp));
        assert_eq!(Protocol::from_label("ftpes"), Some(Protocol::Ftps));
        assert_eq!(Protocol::from_label("gopher"), None);
        // An unknown protocol in the config must not make the entry unusable.
        let mut saved = spec().to_saved();
        saved.protocol = "gopher".into();
        assert_eq!(ConnectionSpec::from_saved(&saved).protocol, Protocol::Ftp);
        assert_eq!(ConnectionSpec::from_saved(&saved).encoding, "utf-8");
    }

    #[test]
    fn effective_port_uses_protocol_defaults() {
        let mut s = spec();
        s.port = 0;
        assert_eq!(s.effective_port(), 21);
        s.protocol = Protocol::Ftps;
        assert_eq!(s.effective_port(), 21);
        s.protocol = Protocol::Sftp;
        assert_eq!(s.effective_port(), 22);
        s.port = 2121;
        assert_eq!(s.effective_port(), 2121);
    }

    #[test]
    fn anonymous_is_empty_or_the_word_anonymous() {
        let mut s = spec();
        assert!(s.is_anonymous());
        s.user = String::new();
        assert!(s.is_anonymous());
        s.user = "  Anonymous ".into();
        assert!(s.is_anonymous());
        s.user = "alice".into();
        assert!(!s.is_anonymous());
    }

    #[test]
    fn validate_accepts_a_normal_spec() {
        assert!(spec().validate().is_ok());
        let mut s = spec();
        s.host = "[::1]".into();
        s.protocol = Protocol::Sftp;
        s.user = "alice".into();
        assert!(s.validate().is_ok());
        s.host = "192.0.2.10".into();
        assert!(s.validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_host() {
        let mut s = spec();
        s.host = "   ".into();
        let error = s.validate().unwrap_err();
        assert!(matches!(error, RemoteError::Connect { .. }), "{error:?}");
        assert!(error.to_string().contains("empty"));
    }

    #[test]
    fn validate_rejects_bad_hosts_and_embedded_ports() {
        let cases = [
            ("ftp://example.com", "URL scheme"),
            ("ftp.example .com", "whitespace"),
            ("example.com/pub", "path separator"),
            ("example.com:2121", "must not embed a port"),
            ("[::1]:21", "must not embed a port"),
            ("[::1", "not closed"),
        ];
        for (host, needle) in cases {
            let mut s = spec();
            s.host = host.into();
            let error = s.validate().unwrap_err();
            assert!(
                matches!(error, RemoteError::Connect { .. }),
                "{host}: {error:?}"
            );
            assert!(error.to_string().contains(needle), "{host}: {error}");
        }
    }

    #[test]
    fn validate_rejects_sftp_without_a_user() {
        let mut s = spec();
        s.protocol = Protocol::Sftp;
        s.user = String::new();
        let error = s.validate().unwrap_err();
        assert!(matches!(error, RemoteError::Authentication(_)), "{error:?}");
        s.user = "anonymous".into();
        assert!(s.validate().is_ok());
        s.user = "alice".into();
        assert!(s.validate().is_ok());
        // Anonymous FTP stays valid, of course.
        let mut ftp = spec();
        ftp.user = String::new();
        assert!(ftp.validate().is_ok());
    }

    #[test]
    fn default_spec_is_passive_utf8_ftp() {
        let d = ConnectionSpec::default();
        assert_eq!(d.protocol, Protocol::Ftp);
        assert!(d.passive);
        assert_eq!(d.encoding, "utf-8");
        assert_eq!(d.effective_port(), 21);
        assert_eq!(d.timeout, DEFAULT_TIMEOUT);
        assert!(!Protocol::default().is_secure());
        assert!(Protocol::Sftp.is_secure() && Protocol::Ftps.is_secure());
    }

    #[test]
    fn default_ports_match_the_protocols() {
        assert_eq!(Protocol::Ftp.default_port(), 21);
        assert_eq!(Protocol::Ftps.default_port(), 21);
        assert_eq!(Protocol::Sftp.default_port(), 22);
    }
}

//! Known SSH host keys, as a small, human-readable equivalent of OpenSSH's
//! `known_hosts`.
//!
//! One line per key:
//!
//! ```text
//! # host:port algorithm <base64 host key>
//! example.com:22 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...
//! [::1]:2222 ssh-ed25519 AAAAC3...
//! ```
//!
//! The key material is stored verbatim (base64 of the server's public host key),
//! not a hash of it, which makes a mismatch message able to show both keys and
//! keeps the file readable. Comparison is on the decoded bytes.

use crate::error::{CoreError, CoreResult};
use std::path::{Path, PathBuf};

/// One trusted host key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownHostEntry {
    /// Host name or IP literal; IPv6 literals are stored bracketed.
    pub host: String,
    pub port: u16,
    /// Key algorithm as the server reports it (`ssh-ed25519`, `ssh-rsa`, …).
    pub algorithm: String,
    /// The server's public host key, base64 encoded.
    pub key_base64: String,
}

impl KnownHostEntry {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        algorithm: impl Into<String>,
        key_base64: impl Into<String>,
    ) -> Self {
        KnownHostEntry {
            host: host.into(),
            port,
            algorithm: algorithm.into(),
            key_base64: key_base64.into(),
        }
    }

    /// `host:port` as used in the file and in messages.
    pub fn address(&self) -> String {
        format!("{}:{}", bracket_host(&self.host), self.port)
    }
}

/// What the store thinks about a key the server presented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostKeyVerdict {
    /// The same key is already trusted for this host and port.
    Trusted,
    /// Nothing is known about this host yet (first connection).
    Unknown,
    /// A different key is trusted for this host: a possible man in the middle.
    Changed {
        trusted_algorithm: String,
        trusted_key: String,
    },
}

impl HostKeyVerdict {
    pub fn is_trusted(&self) -> bool {
        matches!(self, HostKeyVerdict::Trusted)
    }
}

/// How to behave when a host key is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Refuse to connect to a host that is not in the store.
    Strict,
    /// Trust an unknown key once and remember it (the common default).
    TrustOnFirstUse,
    /// Do not verify at all (dangerous, offered for broken devices only).
    Ignore,
}

impl HostKeyPolicy {
    pub fn parse(text: &str) -> HostKeyPolicy {
        match text.trim().to_ascii_lowercase().as_str() {
            "strict" => HostKeyPolicy::Strict,
            "ignore" | "off" | "none" => HostKeyPolicy::Ignore,
            _ => HostKeyPolicy::TrustOnFirstUse,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            HostKeyPolicy::Strict => "strict",
            HostKeyPolicy::TrustOnFirstUse => "tofu",
            HostKeyPolicy::Ignore => "ignore",
        }
    }
}

/// The trusted keys of this installation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownHosts {
    entries: Vec<KnownHostEntry>,
}

impl KnownHosts {
    pub fn new() -> Self {
        KnownHosts::default()
    }

    pub fn entries(&self) -> &[KnownHostEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Parse the file format; unknown lines and comments are ignored so a file
    /// edited by hand never makes the application fail to start.
    pub fn parse(text: &str) -> Self {
        let mut entries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(address), Some(algorithm), Some(key)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if parts.next().is_some() {
                continue;
            }
            let (host, port) = match split_address(address) {
                Some(pair) => pair,
                None => continue,
            };
            entries.push(KnownHostEntry::new(host, port, algorithm, key));
        }
        KnownHosts { entries }
    }

    /// Render the store back to text (sorted, with a short header).
    pub fn to_text(&self) -> String {
        let mut sorted = self.entries.clone();
        sorted.sort_by(|a, b| a.host.cmp(&b.host).then(a.port.cmp(&b.port)));
        let mut out = String::from("# RCommander known SSH host keys\n");
        for entry in &sorted {
            out.push_str(&format!(
                "{} {} {}\n",
                entry.address(),
                entry.algorithm,
                entry.key_base64
            ));
        }
        out
    }

    pub fn load(path: &Path) -> CoreResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(KnownHosts::parse(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(KnownHosts::new()),
            Err(e) => Err(CoreError::io(path, e)),
        }
    }

    pub fn save(&self, path: &Path) -> CoreResult<()> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        std::fs::write(path, self.to_text()).map_err(|e| CoreError::io(path, e))
    }

    /// Compare a key presented by a server against the store.
    pub fn verdict(
        &self,
        host: &str,
        port: u16,
        _algorithm: &str,
        key_base64: &str,
    ) -> HostKeyVerdict {
        let normalised = normalise_host(host);
        match self
            .entries
            .iter()
            .find(|entry| normalise_host(&entry.host) == normalised && entry.port == port)
        {
            Some(entry) if entry.key_base64 == key_base64 => HostKeyVerdict::Trusted,
            Some(entry) => HostKeyVerdict::Changed {
                trusted_algorithm: entry.algorithm.clone(),
                trusted_key: entry.key_base64.clone(),
            },
            None => HostKeyVerdict::Unknown,
        }
    }

    /// Remember a key, replacing a previous one for the same host and port.
    pub fn trust(&mut self, host: &str, port: u16, algorithm: &str, key_base64: &str) {
        let normalised = normalise_host(host);
        self.entries
            .retain(|entry| !(normalise_host(&entry.host) == normalised && entry.port == port));
        self.entries
            .push(KnownHostEntry::new(normalised, port, algorithm, key_base64));
    }

    /// Forget a host (used after the user confirmed a changed key).
    pub fn forget(&mut self, host: &str, port: u16) -> bool {
        let normalised = normalise_host(host);
        let before = self.entries.len();
        self.entries
            .retain(|entry| !(normalise_host(&entry.host) == normalised && entry.port == port));
        self.entries.len() != before
    }
}

/// Decide whether a connection may proceed, updating the store for TOFU.
pub fn check_and_update(
    store: &mut KnownHosts,
    policy: HostKeyPolicy,
    host: &str,
    port: u16,
    algorithm: &str,
    key_base64: &str,
) -> CoreResult<HostKeyVerdict> {
    let verdict = store.verdict(host, port, algorithm, key_base64);
    match (&verdict, policy) {
        (HostKeyVerdict::Trusted, _) | (_, HostKeyPolicy::Ignore) => Ok(verdict),
        (HostKeyVerdict::Unknown, HostKeyPolicy::TrustOnFirstUse) => {
            store.trust(host, port, algorithm, key_base64);
            Ok(verdict)
        }
        (HostKeyVerdict::Unknown, HostKeyPolicy::Strict) => Err(CoreError::Invalid(format!(
            "the host key of {host}:{port} is not trusted; add it to the known hosts list first"
        ))),
        (HostKeyVerdict::Changed { .. }, _) => Err(CoreError::Invalid(format!(
            "the host key of {host}:{port} CHANGED — possible man in the middle; remove the stored key only if you know why it changed"
        ))),
    }
}

fn normalise_host(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase()
}

fn bracket_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn split_address(address: &str) -> Option<(String, u16)> {
    if let Some(rest) = address.strip_prefix('[') {
        // [IPv6]:port
        let end = rest.find(']')?;
        let host = &rest[..end];
        let port = rest[end + 1..].strip_prefix(':')?.parse().ok()?;
        return Some((host.to_string(), port));
    }
    let (host, port) = address.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}

/// Default location of the store: `<config dir>/known_hosts`.
pub fn default_path() -> PathBuf {
    crate::config::config_dir().join("known_hosts")
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const KEY_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

    #[test]
    fn an_unknown_host_is_unknown_and_becomes_trusted_after_tofu() {
        let mut store = KnownHosts::new();
        assert_eq!(
            store.verdict("example.com", 22, "ssh-ed25519", KEY_A),
            HostKeyVerdict::Unknown
        );
        let verdict = check_and_update(
            &mut store,
            HostKeyPolicy::TrustOnFirstUse,
            "example.com",
            22,
            "ssh-ed25519",
            KEY_A,
        )
        .unwrap();
        assert_eq!(verdict, HostKeyVerdict::Unknown);
        assert!(
            store
                .verdict("example.com", 22, "ssh-ed25519", KEY_A)
                .is_trusted()
        );
        // The same key on a different port is a different host entry.
        assert_eq!(
            store.verdict("example.com", 2222, "ssh-ed25519", KEY_A),
            HostKeyVerdict::Unknown
        );
    }

    #[test]
    fn strict_mode_refuses_an_unknown_host() {
        let mut store = KnownHosts::new();
        let error = check_and_update(
            &mut store,
            HostKeyPolicy::Strict,
            "example.com",
            22,
            "ssh-ed25519",
            KEY_A,
        )
        .unwrap_err();
        assert!(error.to_string().contains("not trusted"), "error: {error}");
        assert!(
            store.is_empty(),
            "nothing is stored when the connection is refused"
        );
    }

    #[test]
    fn a_changed_key_is_reported_with_both_keys() {
        let mut store = KnownHosts::new();
        store.trust("example.com", 22, "ssh-ed25519", KEY_A);
        match store.verdict("example.com", 22, "ssh-ed25519", KEY_B) {
            HostKeyVerdict::Changed {
                trusted_algorithm,
                trusted_key,
            } => {
                assert_eq!(trusted_algorithm, "ssh-ed25519");
                assert_eq!(trusted_key, KEY_A);
            }
            other => panic!("expected a changed verdict, got {other:?}"),
        }
        // Even a permissive policy refuses a changed key.
        let error = check_and_update(
            &mut store,
            HostKeyPolicy::TrustOnFirstUse,
            "example.com",
            22,
            "ssh-ed25519",
            KEY_B,
        )
        .unwrap_err();
        assert!(error.to_string().contains("CHANGED"), "error: {error}");
        // Forgetting the entry allows the new key to be trusted again.
        assert!(store.forget("example.com", 22));
        assert!(!store.forget("example.com", 22));
        assert!(
            check_and_update(
                &mut store,
                HostKeyPolicy::TrustOnFirstUse,
                "example.com",
                22,
                "ssh-ed25519",
                KEY_B
            )
            .is_ok()
        );
    }

    #[test]
    fn ignore_mode_accepts_anything_without_storing_it() {
        let mut store = KnownHosts::new();
        assert!(
            check_and_update(&mut store, HostKeyPolicy::Ignore, "h", 22, "ssh-rsa", KEY_A).is_ok()
        );
        assert!(store.is_empty());
    }

    #[test]
    fn host_names_are_case_insensitive_and_ipv6_is_bracketed() {
        let mut store = KnownHosts::new();
        store.trust("Example.COM", 22, "ssh-ed25519", KEY_A);
        assert!(
            store
                .verdict("example.com", 22, "ssh-ed25519", KEY_A)
                .is_trusted()
        );
        store.trust("::1", 2222, "ssh-ed25519", KEY_B);
        let text = store.to_text();
        assert!(text.contains("[::1]:2222 ssh-ed25519"), "{text}");
        let parsed = KnownHosts::parse(&text);
        assert!(
            parsed
                .verdict("::1", 2222, "ssh-ed25519", KEY_B)
                .is_trusted()
        );
    }

    #[test]
    fn the_file_round_trips_and_tolerates_manual_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let mut store = KnownHosts::new();
        store.trust("a.example", 22, "ssh-ed25519", KEY_A);
        store.trust("b.example", 22, "ssh-rsa", KEY_B);
        store.save(&path).unwrap();
        let loaded = KnownHosts::load(&path).unwrap();
        assert_eq!(loaded, store);
        assert_eq!(loaded.len(), 2);
        // An empty store loads from a missing file, and garbage lines are skipped.
        assert!(
            KnownHosts::load(&dir.path().join("absent"))
                .unwrap()
                .is_empty()
        );
        std::fs::write(
            &path,
            "# comment\nbroken line\n\na.example:22 ssh-ed25519 KEY\n",
        )
        .unwrap();
        let loaded = KnownHosts::load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(
            loaded
                .verdict("a.example", 22, "ssh-ed25519", "KEY")
                .is_trusted()
        );
    }

    #[test]
    fn trusting_a_new_key_replaces_the_old_one() {
        let mut store = KnownHosts::new();
        store.trust("h", 22, "ssh-ed25519", KEY_A);
        store.trust("h", 22, "ssh-ed25519", KEY_B);
        assert_eq!(store.len(), 1, "one entry per host and port");
        assert!(store.verdict("h", 22, "ssh-ed25519", KEY_B).is_trusted());
    }

    #[test]
    fn policies_parse_and_render() {
        assert_eq!(HostKeyPolicy::parse("strict"), HostKeyPolicy::Strict);
        assert_eq!(HostKeyPolicy::parse("TOFU"), HostKeyPolicy::TrustOnFirstUse);
        assert_eq!(HostKeyPolicy::parse("ignore"), HostKeyPolicy::Ignore);
        assert_eq!(
            HostKeyPolicy::parse("nonsense"),
            HostKeyPolicy::TrustOnFirstUse
        );
        assert_eq!(HostKeyPolicy::Strict.id(), "strict");
        assert_eq!(HostKeyPolicy::TrustOnFirstUse.id(), "tofu");
    }
}

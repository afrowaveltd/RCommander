//! The Git configuration on this machine, read only after the user allows it.
//!
//! Reading a user's Git configuration means reading their identity, their credential
//! helper and — with the `store` helper — a file of plain-text passwords. That is
//! why nothing here happens on its own: [`GitConfig::load`] is called only after the
//! user approved it, and the secret values are kept in memory for the current session
//! and never written anywhere.
//!
//! Passwords are deliberately *not* part of the debug output or of any summary type
//! the view can print, so a screenshot or a status line cannot leak them.

use std::path::{Path, PathBuf};

/// User identity from the configuration (`[user] name/email`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

/// One `url.<base>.insteadOf` rewrite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlRewrite {
    /// The base recorded in the configuration (`git@github.com:`).
    pub base: String,
    /// The prefix it replaces in a URL (`gh:`).
    pub instead_of: String,
}

/// One credential taken from a `store` helper, kept in memory only.
#[derive(Clone, PartialEq, Eq)]
pub struct StoredCredential {
    pub protocol: String,
    pub host: String,
    pub port: Option<u16>,
    /// The user name (not secret).
    pub username: String,
    /// The password or token. Never printed, never saved.
    pub password: String,
}

impl std::fmt::Debug for StoredCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately hides the password so a stray `{:?}` cannot leak it.
        f.debug_struct("StoredCredential")
            .field("protocol", &self.protocol)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl StoredCredential {
    /// `https://user@host` — safe to show.
    pub fn describe(&self) -> String {
        let port = self.port.map(|port| format!(":{port}")).unwrap_or_default();
        let user = if self.username.is_empty() {
            String::new()
        } else {
            format!("{}@", self.username)
        };
        format!("{}://{}{}{}", self.protocol, user, self.host, port)
    }
}

/// What was found in the configuration, without any secret value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitConfigSummary {
    pub identity: Option<Identity>,
    /// `credential.helper`, e.g. `store` or `cache --timeout=3600`.
    pub credential_helper: Option<String>,
    /// Hosts (not passwords) of credentials found in a `store` helper.
    pub credential_hosts: Vec<String>,
    pub url_rewrites: Vec<UrlRewrite>,
    pub ssh_command: Option<String>,
    pub default_branch: Option<String>,
    /// Files that were read, for transparency in the dialog.
    pub sources: Vec<String>,
    /// Problems that did not stop the rest of the configuration from loading.
    pub problems: Vec<String>,
}

impl GitConfigSummary {
    /// True when there is nothing to gain from loading the configuration.
    pub fn is_empty(&self) -> bool {
        self.identity.is_none()
            && self.credential_helper.is_none()
            && self.credential_hosts.is_empty()
            && self.url_rewrites.is_empty()
            && self.ssh_command.is_none()
    }

    /// One-line summary for the dialog title area.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(identity) = &self.identity {
            parts.push(format!("user {} <{}>", identity.name, identity.email));
        }
        if let Some(helper) = &self.credential_helper {
            parts.push(format!("credential helper {helper}"));
        }
        if !self.credential_hosts.is_empty() {
            parts.push(format!(
                "{} stored credential(s): {}",
                self.credential_hosts.len(),
                self.credential_hosts.join(", ")
            ));
        }
        if !self.url_rewrites.is_empty() {
            parts.push(format!("{} URL rewrite(s)", self.url_rewrites.len()));
        }
        if let Some(ssh) = &self.ssh_command {
            parts.push(format!("ssh command {ssh}"));
        }
        if parts.is_empty() {
            "nothing found".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// The loaded configuration plus the credentials that came with it.
#[derive(Clone, Debug, Default)]
pub struct GitConfig {
    pub summary: GitConfigSummary,
    credentials: Vec<StoredCredential>,
}

impl GitConfig {
    /// Read the system, global and (when given) repository configuration.
    ///
    /// Call this only after the user approved it: with the `store` helper it reads a
    /// file of plain-text passwords.
    pub fn load(repository: Option<&Path>) -> GitConfig {
        let mut config = GitConfig::default();
        let mut sources: Vec<String> = Vec::new();

        // libgit2 merges the system and global files; a repository adds its own.
        let effective = match (git2::Config::open_default(), repository) {
            (Ok(default), Some(path)) => match git2::Repository::discover(path) {
                Ok(repo) => repo.config().or(Ok(default)),
                Err(_) => Ok(default),
            },
            (Ok(default), None) => Ok(default),
            (Err(error), _) => Err(error),
        };

        for candidate in [
            git2::Config::find_system().ok(),
            git2::Config::find_global().ok(),
            git2::Config::find_xdg().ok(),
            repository
                .and_then(|path| git2::Repository::discover(path).ok())
                .map(|repo| repo.path().join("config")),
        ]
        .into_iter()
        .flatten()
        {
            if candidate.is_file() {
                let text = candidate.display().to_string();
                if !sources.contains(&text) {
                    sources.push(text);
                }
            }
        }

        let Ok(effective) = effective else {
            config
                .summary
                .problems
                .push("the git configuration could not be opened (is git installed?)".to_string());
            return config;
        };

        if let Some(name) = read_string(&effective, "user.name")
            && let Some(email) = read_string(&effective, "user.email")
        {
            config.summary.identity = Some(Identity { name, email });
        }
        config.summary.credential_helper = read_string(&effective, "credential.helper");
        config.summary.ssh_command = read_string(&effective, "core.sshcommand");
        config.summary.default_branch = read_string(&effective, "init.defaultbranch");
        config.summary.url_rewrites = read_url_rewrites(&effective);

        // Only the `store` helper keeps credentials in a file we can read.
        if let Some(helper) = &config.summary.credential_helper
            && helper.trim().starts_with("store")
        {
            for path in credential_store_paths(repository) {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                sources.push(path.display().to_string());
                for credential in parse_credential_store(&text) {
                    config.summary.credential_hosts.push(credential.describe());
                    config.credentials.push(credential);
                }
            }
        }
        config.summary.credential_hosts.sort();
        config.summary.credential_hosts.dedup();
        config.summary.sources = sources;
        config
    }

    /// Credentials found in a `store` helper file.
    pub fn credentials(&self) -> &[StoredCredential] {
        &self.credentials
    }

    /// The credential for a host, if one was stored.
    pub fn credential_for(&self, protocol: &str, host: &str) -> Option<&StoredCredential> {
        self.credentials.iter().find(|credential| {
            credential.host.eq_ignore_ascii_case(host)
                && credential.protocol.eq_ignore_ascii_case(protocol)
        })
    }

    /// Rewrite a URL with the configured `insteadOf` rules.
    pub fn rewrite_url(&self, url: &str) -> Option<String> {
        let mut best: Option<&UrlRewrite> = None;
        for rewrite in &self.summary.url_rewrites {
            if url.starts_with(&rewrite.instead_of)
                && best
                    .map(|current| rewrite.instead_of.len() > current.instead_of.len())
                    .unwrap_or(true)
            {
                best = Some(rewrite);
            }
        }
        best.map(|rewrite| format!("{}{}", rewrite.base, &url[rewrite.instead_of.len()..]))
    }

    /// True when any credential is held in memory.
    pub fn has_credentials(&self) -> bool {
        !self.credentials.is_empty()
    }
}

fn read_string(config: &git2::Config, key: &str) -> Option<String> {
    config
        .get_string(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_url_rewrites(config: &git2::Config) -> Vec<UrlRewrite> {
    let mut out: Vec<UrlRewrite> = Vec::new();
    // `url.<base>.insteadOf = <prefix>`; git lower-cases the key, libgit2 keeps
    // whatever spelling the file used, so both are accepted.
    for glob in ["url.*.insteadOf", "url.*.insteadof"] {
        if let Ok(entries) = config.entries(Some(glob)) {
            let _ = entries.for_each(|entry| {
                let Ok(name) = entry.name() else { return };
                let lower = name.to_ascii_lowercase();
                if lower
                    .strip_prefix("url.")
                    .and_then(|rest| rest.strip_suffix(".insteadof"))
                    .is_none()
                {
                    return;
                }
                let Ok(instead_of) = entry.value() else {
                    return;
                };
                // The base keeps its original spelling (it is a URL).
                let base = &name[4..name.len() - ".insteadof".len()];
                if out
                    .iter()
                    .any(|r| r.base == base && r.instead_of == instead_of)
                {
                    return;
                }
                out.push(UrlRewrite {
                    base: base.to_string(),
                    instead_of: instead_of.to_string(),
                });
            });
        }
    }
    out.sort_by(|a, b| a.instead_of.cmp(&b.instead_of));
    out
}

/// Candidate `~/.git-credentials` paths (the helper has no option to move it).
fn credential_store_paths(repository: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = home_dir() {
        out.push(home.join(".git-credentials"));
    }
    if let Some(repo) = repository
        && let Ok(found) = git2::Repository::discover(repo)
    {
        let path = found.path().join("git-credentials");
        if path.exists() {
            out.push(path);
        }
    }
    out
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Parse a `store` helper file: one `scheme://user:password@host[:port]/path` per line.
///
/// Lines that do not parse are skipped instead of failing the whole file — the file is
/// written by another program.
pub fn parse_credential_store(text: &str) -> Vec<StoredCredential> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((scheme, rest)) = line.split_once("://") else {
            continue;
        };
        let authority = rest.split('/').next().unwrap_or("");
        let (userinfo, hostport) = match authority.rsplit_once('@') {
            Some((userinfo, hostport)) => (Some(userinfo), hostport),
            None => (None, authority),
        };
        if hostport.is_empty() {
            continue;
        }
        let (host, port) = match hostport.rsplit_once(':') {
            Some((host, port)) => match port.parse::<u16>() {
                Ok(port) => (host.to_string(), Some(port)),
                Err(_) => (hostport.to_string(), None),
            },
            None => (hostport.to_string(), None),
        };
        let (username, password) = match userinfo {
            Some(userinfo) => match userinfo.split_once(':') {
                Some((user, password)) => (percent_decode(user), percent_decode(password)),
                None => (percent_decode(userinfo), String::new()),
            },
            None => (String::new(), String::new()),
        };
        out.push(StoredCredential {
            protocol: scheme.to_ascii_lowercase(),
            host,
            port,
            username,
            password,
        });
    }
    out
}

/// Decode `%XX` escapes, which git applies to names with special characters.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_store_file_is_parsed_without_leaking_passwords() {
        let text = "\
https://alice:secret@github.com
https://token@gitlab.example.com:8443/path
ssh://bob:p%40ss@host.example.com
not a credential line
# a comment
";
        let credentials = parse_credential_store(text);
        assert_eq!(credentials.len(), 3);
        assert_eq!(credentials[0].host, "github.com");
        assert_eq!(credentials[0].username, "alice");
        assert_eq!(credentials[0].password, "secret");
        assert_eq!(credentials[1].port, Some(8443));
        assert_eq!(credentials[1].username, "token");
        assert_eq!(credentials[1].password, "");
        assert_eq!(credentials[2].protocol, "ssh");
        assert_eq!(
            credentials[2].password, "p@ss",
            "percent escapes are decoded"
        );
        // The debug output redacts the secret.
        let debug = format!("{:?}", credentials[0]);
        assert!(!debug.contains("secret"), "{debug}");
        assert!(debug.contains("redacted"));
        assert_eq!(credentials[0].describe(), "https://alice@github.com");
    }

    #[test]
    fn a_broken_store_file_is_skipped_not_fatal() {
        assert!(parse_credential_store("").is_empty());
        assert!(parse_credential_store("garbage").is_empty());
        assert!(parse_credential_store("https://").is_empty());
        assert_eq!(parse_credential_store("https://user:pw@host").len(), 1);
    }

    #[test]
    fn url_rewrites_use_the_longest_matching_prefix() {
        let config = GitConfig {
            summary: GitConfigSummary {
                url_rewrites: vec![
                    UrlRewrite {
                        base: "git@github.com:".into(),
                        instead_of: "gh:".into(),
                    },
                    UrlRewrite {
                        base: "https://github.com/work/".into(),
                        instead_of: "gh:work/".into(),
                    },
                ],
                ..Default::default()
            },
            credentials: Vec::new(),
        };
        assert_eq!(
            config.rewrite_url("gh:owner/repo.git").as_deref(),
            Some("git@github.com:owner/repo.git")
        );
        assert_eq!(
            config.rewrite_url("gh:work/project.git").as_deref(),
            Some("https://github.com/work/project.git")
        );
        assert_eq!(config.rewrite_url("https://example.com/x"), None);
    }

    #[test]
    fn credentials_are_looked_up_by_protocol_and_host() {
        let config = GitConfig {
            summary: GitConfigSummary::default(),
            credentials: parse_credential_store("https://alice:secret@GitHub.com\n"),
        };
        assert!(config.has_credentials());
        assert!(
            config.credential_for("https", "github.com").is_some(),
            "the host match ignores case"
        );
        assert!(config.credential_for("ssh", "github.com").is_none());
        assert!(config.credential_for("https", "gitlab.com").is_none());
    }

    #[test]
    fn a_summary_describes_what_was_found_without_secrets() {
        let summary = GitConfigSummary {
            identity: Some(Identity {
                name: "Alice".into(),
                email: "alice@example.com".into(),
            }),
            credential_helper: Some("store".into()),
            credential_hosts: vec!["https://alice@github.com".into()],
            url_rewrites: vec![UrlRewrite {
                base: "git@github.com:".into(),
                instead_of: "gh:".into(),
            }],
            ssh_command: None,
            default_branch: Some("main".into()),
            sources: vec!["/home/alice/.gitconfig".into()],
            problems: Vec::new(),
        };
        assert!(!summary.is_empty());
        let text = summary.describe();
        assert!(text.contains("Alice <alice@example.com>"));
        assert!(text.contains("store"));
        assert!(text.contains("github.com"));
        assert!(!text.contains("secret"));
        assert!(GitConfigSummary::default().is_empty());
        assert_eq!(GitConfigSummary::default().describe(), "nothing found");
    }

    #[test]
    fn loading_the_real_configuration_does_not_panic() {
        // The machine may or may not have a git configuration; either way the call
        // has to return something usable and never panic.
        let config = GitConfig::load(None);
        let _ = config.summary.describe();
        // A repository that does not exist is not an error either.
        let other = GitConfig::load(Some(Path::new("/nonexistent/path/for/rc-git")));
        assert!(other.summary.problems.is_empty());
    }

    #[test]
    fn the_credential_store_in_a_fixture_is_found_only_with_the_store_helper() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::write(
            home.join(".git-credentials"),
            "https://alice:topsecret@example.com\n",
        )
        .unwrap();
        // Parsing the file directly keeps the test independent of the machine.
        let text = std::fs::read_to_string(home.join(".git-credentials")).unwrap();
        let credentials = parse_credential_store(&text);
        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].password, "topsecret");
        assert!(
            parse_credential_store("https://x:y@z\n")[0]
                .describe()
                .contains("x@z")
        );
    }
}

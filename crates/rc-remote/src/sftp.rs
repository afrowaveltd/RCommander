//! SFTP client (cargo feature `sftp`, built on `ssh2`/libssh2).
//!
//! # What it does
//!
//! * password authentication (`userauth_password`); no key files, no agent and
//!   no keyboard-interactive yet — that is a deliberate scope decision, the panel
//!   only ever stores a password;
//! * `readdir` for listings with real `size`, `mtime` and permission bits,
//!   `lstat`+`stat`+`readlink` to resolve symlinks (the one thing SFTP knows much
//!   better than FTP);
//! * `mkdir`, `rmdir`, `unlink`, `rename`, `open`/`create` for transfers.
//!
//! # `cwd`
//!
//! SFTP has no per-connection working directory, so this client keeps the
//! current directory itself (starting at the server's home directory from
//! `realpath(".")`) and resolves relative paths against it. `pwd` therefore
//! always answers, and `CWD`-style navigation behaves like the FTP panel.
//!
//! # Security limitation: no host-key verification
//!
//! libssh2 does not check the server key by itself and this client installs no
//! known-hosts handler, because [`crate::ConnectionSpec`] has no field to store
//! one. Authentication is therefore vulnerable to a man-in-the-middle that
//! presents its own key. Adding verification means calling
//! `Session::host_key()` (compare against a stored fingerprint) or
//! `Session::known_hosts`/`KnownHosts` (a `known_hosts` file) right after
//! `handshake`; the hook belongs in this function, and the storage side belongs
//! in the connection dialog.
//!
//! # Status
//!
//! The code is compiled and unit-tested (`cargo test -p rc-remote --features
//! sftp` covers the path helpers), but it has **not been tested against a real
//! SSH server** from this repository: there is no in-process SSH server to talk
//! to, and the test suite must run offline. Treat it as untested integration.
//!
//! Without the feature, [`crate::connect`] returns
//! `Unsupported("sftp feature disabled")`.

use std::fs::File as LocalFile;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use rc_core::ops::CancelToken;
use ssh2::{ErrorCode, FileStat, Session};

use crate::entry::RemoteEntry;
use crate::error::{ConnectStage, RemoteError, StageReporter};
use crate::remote_fs::RemoteFs;
use crate::spec::{ConnectionSpec, DEFAULT_TIMEOUT, Protocol};

/// Transfer chunk; also the cancellation granularity.
const CHUNK: usize = 64 * 1024;

/// Bit mask of the file-type field of a POSIX mode.
const S_IFMT: u32 = 0o170000;
/// `S_IFDIR`.
const S_IFDIR: u32 = 0o040000;
/// `S_IFLNK`.
const S_IFLNK: u32 = 0o120000;

/// A live SFTP session.
pub(crate) struct SftpClient {
    /// The SSH session; SFTP channels are opened per operation.
    session: Session,
    /// Directory relative paths are resolved against.
    cwd: String,
    /// Socket timeout, also used as the libssh2 block timeout.
    timeout: Duration,
}

impl std::fmt::Debug for SftpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SftpClient")
            .field("cwd", &self.cwd)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// File-type bits of a mode.
fn kind_of(mode: u32) -> u32 {
    mode & S_IFMT
}

/// Translate a libssh2 error into the protocol-neutral variant.
fn map_error(error: ssh2::Error, path: &str) -> RemoteError {
    match error.code() {
        ErrorCode::SFTP(2) => RemoteError::NotFound(format!("{path}: {}", error.message())),
        ErrorCode::SFTP(3) => RemoteError::PermissionDenied(format!("{path}: {}", error.message())),
        ErrorCode::SFTP(8) => RemoteError::Unsupported(format!("{path}: {}", error.message())),
        ErrorCode::SFTP(11) => {
            RemoteError::Protocol(format!("{path}: the server ran out of space"))
        }
        _ => RemoteError::Protocol(format!("{path}: {error}")),
    }
}

/// Join and normalise an SFTP path (always POSIX separated, always absolute).
pub(crate) fn normalize_sftp(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

/// Resolve `path` against `cwd` and normalise it.
pub(crate) fn absolute_sftp(cwd: &str, path: &str) -> String {
    let path = path.trim();
    if path.is_empty() || path == "." {
        return normalize_sftp(cwd);
    }
    if path.starts_with('/') {
        normalize_sftp(path)
    } else {
        normalize_sftp(&format!("{cwd}/{path}"))
    }
}

/// Resolve `host:port` into socket addresses.
fn resolve(host: &str, port: u16) -> std::io::Result<Vec<std::net::SocketAddr>> {
    use std::net::ToSocketAddrs;
    Ok((host, port).to_socket_addrs()?.collect())
}

/// Build an entry from an SFTP `stat` result.
fn entry_from_stat(name: &str, stat: &FileStat) -> RemoteEntry {
    let mode = stat.perm.unwrap_or(0);
    let kind = kind_of(mode);
    RemoteEntry {
        name: name.to_string(),
        is_dir: kind == S_IFDIR,
        is_symlink: kind == S_IFLNK,
        size: if kind == S_IFDIR {
            0
        } else {
            stat.size.unwrap_or(0)
        },
        mtime: stat.mtime.map(|mtime| mtime as i64),
        permissions: stat.perm.map(|perm| perm & 0o7777),
        target: None,
    }
}

/// Last path segment of an SFTP path.
fn base_name(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or_default().to_string()
}

impl SftpClient {
    /// Connect, authenticate and resolve the home directory.
    pub(crate) fn connect(spec: &ConnectionSpec) -> Result<Self, RemoteError> {
        Self::connect_reporting(spec, &mut |_stage| {})
    }

    /// Connect and report each step, so the window can say what is happening.
    pub(crate) fn connect_reporting(
        spec: &ConnectionSpec,
        on_stage: StageReporter<'_>,
    ) -> Result<Self, RemoteError> {
        let timeout = if spec.timeout.is_zero() {
            DEFAULT_TIMEOUT
        } else {
            spec.timeout
        };
        let port = spec.effective_port();
        let host = spec.host.trim();
        let bare = host
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(host);
        let connect_error = |reason: String| RemoteError::Connect {
            host: host.to_string(),
            port,
            reason,
        };

        on_stage(ConnectStage::Resolving);
        let addresses = resolve(bare, port)
            .map_err(|e| connect_error(format!("cannot resolve the host: {e}")))?;
        on_stage(ConnectStage::Connecting);
        let mut socket = None;
        let mut last_error = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, timeout) {
                Ok(stream) => {
                    socket = Some(stream);
                    break;
                }
                Err(error) => last_error = Some(error),
            }
        }
        let socket = match (socket, last_error) {
            (Some(socket), _) => socket,
            (None, Some(error)) => return Err(connect_error(error.to_string())),
            (None, None) => {
                return Err(connect_error(
                    "the host did not resolve to any address".to_string(),
                ));
            }
        };
        let _ = socket.set_read_timeout(Some(timeout));
        let _ = socket.set_write_timeout(Some(timeout));
        let _ = socket.set_nodelay(true);

        let mut session = Session::new()
            .map_err(|e| connect_error(format!("cannot create an ssh session: {e}")))?;
        session.set_tcp_stream(socket);
        let millis = timeout.as_millis().clamp(1, u32::MAX as u128) as u32;
        session.set_timeout(millis);
        on_stage(ConnectStage::Handshake);
        session
            .handshake()
            .map_err(|e| connect_error(format!("ssh handshake failed: {e}")))?;

        // Host-key verification: refuse a changed key even in permissive mode.
        if spec.host_key_policy != rc_core::knownhosts::HostKeyPolicy::Ignore {
            on_stage(ConnectStage::HostKey);
        }
        verify_host_key(&session, spec, host, port)?;

        on_stage(ConnectStage::Authenticating);
        let user = spec.user.trim();
        match &spec.auth {
            crate::spec::AuthMethod::Password => {
                session
                    .userauth_password(user, &spec.password)
                    .map_err(|e| RemoteError::Authentication(format!("{user}@{host}: {e}")))?;
                if !session.authenticated() {
                    return Err(RemoteError::Authentication(format!(
                        "{user}@{host}: the server rejected the password"
                    )));
                }
            }
            crate::spec::AuthMethod::KeyFile { path, passphrase } => {
                if path.as_os_str().is_empty() {
                    return Err(RemoteError::Authentication(format!(
                        "{user}@{host}: the authentication method is “key file” but no file is \
                         set; type the path to your private key or switch to a password"
                    )));
                }
                // Checked before libssh2 sees it: "the key does not open" and "the key is not
                // there" are different problems with different fixes, and the second one is the
                // one a typed path usually has.
                if !path.is_file() {
                    return Err(RemoteError::Authentication(format!(
                        "{user}@{host}: the private key {} does not exist",
                        path.display()
                    )));
                }
                let public = key_public_path(path);
                session
                    .userauth_pubkey_file(user, public.as_deref(), path, passphrase.as_deref())
                    .map_err(|e| {
                        RemoteError::Authentication(format!(
                            "{user}@{host} with key {}: {e}",
                            path.display()
                        ))
                    })?;
                if !session.authenticated() {
                    return Err(RemoteError::Authentication(format!(
                        "{user}@{host}: the server rejected the key {}",
                        path.display()
                    )));
                }
            }
            crate::spec::AuthMethod::Agent => {
                let mut agent = session.agent().map_err(|e| {
                    RemoteError::Authentication(format!("ssh-agent is not available: {e}"))
                })?;
                agent.connect().map_err(|e| {
                    RemoteError::Authentication(format!("ssh-agent refused the connection: {e}"))
                })?;
                agent.list_identities().map_err(|e| {
                    RemoteError::Authentication(format!("ssh-agent cannot list identities: {e}"))
                })?;
                let identities = agent.identities().map_err(|e| {
                    RemoteError::Authentication(format!(
                        "ssh-agent cannot read its identities: {e}"
                    ))
                })?;
                let mut accepted = false;
                for identity in &identities {
                    if agent.userauth(user, identity).is_ok() && session.authenticated() {
                        accepted = true;
                        break;
                    }
                }
                if !accepted {
                    return Err(RemoteError::Authentication(format!(
                        "{user}@{host}: ssh-agent holds no usable identity"
                    )));
                }
            }
        }

        on_stage(ConnectStage::Opening);
        let mut client = SftpClient {
            session,
            cwd: "/".to_string(),
            timeout,
        };
        let home = client.realpath(".").unwrap_or_else(|_| "/".to_string());
        client.cwd = home;
        if !spec.remote_dir.trim().is_empty() {
            let _ = client.cwd(&spec.remote_dir);
        }
        Ok(client)
    }

    /// Open a fresh SFTP channel for one operation.
    fn sftp(&self) -> Result<ssh2::Sftp, RemoteError> {
        self.session
            .sftp()
            .map_err(|e| RemoteError::Protocol(format!("cannot open an sftp channel: {e}")))
    }

    /// Resolve a possibly relative path against the client's working directory.
    fn path(&self, path: &str) -> String {
        absolute_sftp(&self.cwd, path)
    }

    /// `realpath` of a path.
    fn realpath(&self, path: &str) -> Result<String, RemoteError> {
        let sftp = self.sftp()?;
        let resolved = sftp
            .realpath(Path::new(path))
            .map_err(|e| map_error(e, path))?;
        Ok(resolved.to_string_lossy().into_owned())
    }

    /// `lstat` plus symlink resolution for one path.
    fn entry(&self, path: &str) -> Result<RemoteEntry, RemoteError> {
        let sftp = self.sftp()?;
        let absolute = self.path(path);
        let stat = sftp
            .lstat(Path::new(&absolute))
            .map_err(|e| map_error(e, &absolute))?;
        let mut entry = entry_from_stat(&base_name(&absolute), &stat);
        if entry.is_symlink {
            // Follow the link once, so the panel can tell a directory link from
            // a file link — the one thing SFTP reports better than FTP.
            if let Ok(target_stat) = sftp.stat(Path::new(&absolute)) {
                entry.is_dir = kind_of(target_stat.perm.unwrap_or(0)) == S_IFDIR;
            }
            if let Ok(target) = sftp.readlink(Path::new(&absolute)) {
                entry.target = Some(target.to_string_lossy().into_owned());
            }
        }
        Ok(entry)
    }
}

impl RemoteFs for SftpClient {
    fn protocol(&self) -> Protocol {
        Protocol::Sftp
    }

    fn pwd(&mut self) -> Result<String, RemoteError> {
        Ok(self.cwd.clone())
    }

    fn cwd(&mut self, path: &str) -> Result<String, RemoteError> {
        let absolute = self.path(path);
        let sftp = self.sftp()?;
        let stat = sftp
            .stat(Path::new(&absolute))
            .map_err(|e| map_error(e, &absolute))?;
        if kind_of(stat.perm.unwrap_or(0)) != S_IFDIR {
            return Err(RemoteError::Protocol(format!(
                "{absolute}: not a directory"
            )));
        }
        self.cwd = absolute.clone();
        Ok(absolute)
    }

    fn list(&mut self, path: Option<&str>) -> Result<Vec<RemoteEntry>, RemoteError> {
        let directory = match path {
            None => self.cwd.clone(),
            Some(path) => self.path(path),
        };
        let sftp = self.sftp()?;
        let listing = sftp
            .readdir(Path::new(&directory))
            .map_err(|e| map_error(e, &directory))?;
        let mut entries = Vec::with_capacity(listing.len());
        for (entry_path, stat) in listing {
            let name = entry_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            let mut entry = entry_from_stat(&name, &stat);
            if entry.is_symlink {
                if let Ok(target_stat) = sftp.stat(&entry_path) {
                    entry.is_dir = kind_of(target_stat.perm.unwrap_or(0)) == S_IFDIR;
                }
                if let Ok(target) = sftp.readlink(&entry_path) {
                    entry.target = Some(target.to_string_lossy().into_owned());
                }
            }
            entries.push(entry);
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn stat(&mut self, path: &str) -> Result<RemoteEntry, RemoteError> {
        self.entry(path)
    }

    fn mkdir(&mut self, path: &str) -> Result<(), RemoteError> {
        let absolute = self.path(path);
        let sftp = self.sftp()?;
        sftp.mkdir(Path::new(&absolute), 0o755_i32)
            .map_err(|e| map_error(e, &absolute))
    }

    fn rmdir(&mut self, path: &str) -> Result<(), RemoteError> {
        let absolute = self.path(path);
        let sftp = self.sftp()?;
        sftp.rmdir(Path::new(&absolute))
            .map_err(|e| map_error(e, &absolute))
    }

    fn remove_file(&mut self, path: &str) -> Result<(), RemoteError> {
        let absolute = self.path(path);
        let sftp = self.sftp()?;
        sftp.unlink(Path::new(&absolute))
            .map_err(|e| map_error(e, &absolute))
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), RemoteError> {
        let from = self.path(from);
        let to = self.path(to);
        let sftp = self.sftp()?;
        sftp.rename(Path::new(&from), Path::new(&to), None)
            .map_err(|e| map_error(e, &from))
    }

    fn size(&mut self, path: &str) -> Result<u64, RemoteError> {
        Ok(self.entry(path)?.size)
    }

    fn mdtm(&mut self, path: &str) -> Result<Option<i64>, RemoteError> {
        Ok(self.entry(path)?.mtime)
    }

    fn get(
        &mut self,
        remote: &str,
        local: &Path,
        cancel: &CancelToken,
    ) -> Result<u64, RemoteError> {
        if cancel.is_cancelled() {
            return Err(RemoteError::Cancelled);
        }
        let absolute = self.path(remote);
        let sftp = self.sftp()?;
        let mut remote_file: ssh2::File = sftp
            .open(Path::new(&absolute))
            .map_err(|e| map_error(e, &absolute))?;
        if let Some(parent) = local.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(RemoteError::Io)?;
        }
        let mut local_file = LocalFile::create(local).map_err(RemoteError::Io)?;
        let mut buffer = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            if cancel.is_cancelled() {
                drop(local_file);
                let _ = std::fs::remove_file(local);
                return Err(RemoteError::Cancelled);
            }
            let read = remote_file
                .read(&mut buffer)
                .map_err(|e| RemoteError::Protocol(format!("{absolute}: {e}")))?;
            if read == 0 {
                break;
            }
            local_file
                .write_all(&buffer[..read])
                .map_err(RemoteError::Io)?;
            total += read as u64;
        }
        local_file.flush().map_err(RemoteError::Io)?;
        Ok(total)
    }

    fn put(
        &mut self,
        local: &Path,
        remote: &str,
        cancel: &CancelToken,
    ) -> Result<u64, RemoteError> {
        if cancel.is_cancelled() {
            return Err(RemoteError::Cancelled);
        }
        let absolute = self.path(remote);
        let mut local_file = LocalFile::open(local).map_err(RemoteError::Io)?;
        let sftp = self.sftp()?;
        let mut remote_file: ssh2::File = sftp
            .create(Path::new(&absolute))
            .map_err(|e| map_error(e, &absolute))?;
        let mut buffer = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            if cancel.is_cancelled() {
                return Err(RemoteError::Cancelled);
            }
            let read = local_file.read(&mut buffer).map_err(RemoteError::Io)?;
            if read == 0 {
                break;
            }
            remote_file
                .write_all(&buffer[..read])
                .map_err(|e| RemoteError::Protocol(format!("{absolute}: {e}")))?;
            total += read as u64;
        }
        remote_file
            .flush()
            .map_err(|e| RemoteError::Protocol(format!("{absolute}: {e}")))?;
        Ok(total)
    }

    fn quit(&mut self) -> Result<(), RemoteError> {
        // Best effort: the session is unusable afterwards either way.
        let _ = self.session.disconnect(None, "bye", None);
        Ok(())
    }
}

/// Name of a libssh2 host-key type as the known-hosts file spells it.
fn host_key_algorithm(key_type: ssh2::HostKeyType) -> &'static str {
    use ssh2::HostKeyType;
    match key_type {
        HostKeyType::Rsa => "ssh-rsa",
        HostKeyType::Dss => "ssh-dss",
        HostKeyType::Ecdsa256 => "ecdsa-sha2-nistp256",
        HostKeyType::Ecdsa384 => "ecdsa-sha2-nistp384",
        HostKeyType::Ecdsa521 => "ecdsa-sha2-nistp521",
        HostKeyType::Ed25519 => "ssh-ed25519",
        _ => "unknown",
    }
}

/// The matching `.pub` file next to a private key, when it exists.
fn key_public_path(private: &Path) -> Option<std::path::PathBuf> {
    let mut public = private.as_os_str().to_os_string();
    public.push(".pub");
    let path = std::path::PathBuf::from(public);
    path.is_file().then_some(path)
}

/// Check the server's host key against the known-hosts store.
///
/// libssh2 hands us the raw public key; it is stored base64 encoded, exactly as
/// the file format expects. `TrustOnFirstUse` remembers a new key, `Strict`
/// refuses it and a changed key is refused in every policy except `Ignore`.
fn verify_host_key(
    session: &Session,
    spec: &ConnectionSpec,
    host: &str,
    port: u16,
) -> Result<(), RemoteError> {
    use base64::Engine as _;
    let policy = spec.host_key_policy;
    if policy == rc_core::knownhosts::HostKeyPolicy::Ignore {
        return Ok(());
    }
    let Some((key, key_type)) = session.host_key() else {
        return Err(RemoteError::Protocol(
            "the server did not present a host key".to_string(),
        ));
    };
    let algorithm = host_key_algorithm(key_type);
    let encoded = base64::engine::general_purpose::STANDARD.encode(key);
    let path = spec
        .known_hosts_path
        .clone()
        .unwrap_or_else(rc_core::knownhosts::default_path);
    let mut store = rc_core::knownhosts::KnownHosts::load(&path).unwrap_or_default();
    let verdict = store.verdict(host, port, algorithm, &encoded);
    match rc_core::knownhosts::check_and_update(&mut store, policy, host, port, algorithm, &encoded)
    {
        Ok(_) => {
            if verdict == rc_core::knownhosts::HostKeyVerdict::Unknown {
                // Remember a first-use key so a later change is detected.
                let _ = store.save(&path);
            }
            Ok(())
        }
        Err(error) => Err(RemoteError::Protocol(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `FileStat` with the given mode, size and mtime.
    fn stat_with(mode: u32, size: u64, mtime: u64) -> FileStat {
        FileStat {
            size: Some(size),
            uid: None,
            gid: None,
            perm: Some(mode),
            atime: None,
            mtime: Some(mtime),
        }
    }

    #[test]
    fn paths_are_normalised_and_resolved() {
        assert_eq!(normalize_sftp("/a/b/../c"), "/a/c");
        assert_eq!(normalize_sftp("//a//b//"), "/a/b");
        assert_eq!(normalize_sftp("/"), "/");
        assert_eq!(absolute_sftp("/home/u", "docs"), "/home/u/docs");
        assert_eq!(absolute_sftp("/home/u", "/etc"), "/etc");
        assert_eq!(absolute_sftp("/home/u", ".."), "/home");
        assert_eq!(absolute_sftp("/home/u", "."), "/home/u");
        assert_eq!(absolute_sftp("/home/u", ""), "/home/u");
    }

    #[test]
    fn stats_become_entries() {
        let directory = entry_from_stat("docs", &stat_with(S_IFDIR | 0o755, 4096, 1_700_000_000));
        assert!(directory.is_dir && !directory.is_symlink);
        assert_eq!(directory.size, 0);
        assert_eq!(directory.permissions, Some(0o755));
        assert_eq!(directory.mtime, Some(1_700_000_000));

        let file = entry_from_stat("a.txt", &stat_with(0o100644, 12, 1_700_000_001));
        assert!(!file.is_dir && !file.is_symlink);
        assert_eq!(file.size, 12);
        assert_eq!(file.permissions, Some(0o644));

        let link = entry_from_stat("current", &stat_with(S_IFLNK | 0o777, 8, 1_700_000_002));
        assert!(link.is_symlink && !link.is_dir);
        assert_eq!(link.permissions, Some(0o777));
    }

    #[test]
    fn base_names_are_extracted() {
        assert_eq!(base_name("/home/u/a.txt"), "a.txt");
        assert_eq!(base_name("/home/u/dir/"), "dir");
        assert_eq!(base_name("/"), "");
    }

    #[test]
    fn host_resolution_reports_a_failure_for_a_bad_host() {
        // An empty host cannot resolve, and must come back as an error rather
        // than a panic.
        assert!(resolve("", 22).is_err());
        assert!(resolve("127.0.0.1", 22).is_ok());
    }

    #[test]
    fn sftp_error_codes_map_to_the_protocol_neutral_variants() {
        let not_found = ssh2::Error::from_errno(ssh2::ErrorCode::SFTP(2));
        assert!(matches!(
            map_error(not_found, "/x"),
            RemoteError::NotFound(_)
        ));
        let denied = ssh2::Error::from_errno(ssh2::ErrorCode::SFTP(3));
        assert!(matches!(
            map_error(denied, "/x"),
            RemoteError::PermissionDenied(_)
        ));
        let unsupported = ssh2::Error::from_errno(ssh2::ErrorCode::SFTP(8));
        assert!(matches!(
            map_error(unsupported, "/x"),
            RemoteError::Unsupported(_)
        ));
        let full = ssh2::Error::from_errno(ssh2::ErrorCode::SFTP(11));
        assert!(matches!(map_error(full, "/x"), RemoteError::Protocol(_)));
        let other = ssh2::Error::from_errno(ssh2::ErrorCode::SFTP(99));
        assert!(matches!(map_error(other, "/x"), RemoteError::Protocol(_)));
    }
}

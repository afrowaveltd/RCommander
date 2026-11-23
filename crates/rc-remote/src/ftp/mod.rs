//! FTP (RFC 959) and FTPS client.
//!
//! # Protocol usage
//!
//! * **Login**: `USER` (+`PASS` when the server answers `331`). An empty user
//!   name or `anonymous` logs in anonymously; an empty password then becomes
//!   `anonymous@`, which every anonymous server accepts.
//! * **Listings**: `MLSD` (RFC 3659) first, `LIST` as the fallback. `MLSD` gives
//!   sizes, UTC timestamps and `unix.mode` permission bits; the fallback parses
//!   the unix `LIST` format.
//! * **Metadata**: `SIZE`, `MDTM` and `MLST` for [`RemoteFs::stat`],
//!   [`RemoteFs::size`] and [`RemoteFs::mdtm`].
//! * **Transfers**: passive (`PASV`, `EPSV` over IPv6) or active (`PORT`,
//!   `EPRT`), always in binary mode (`TYPE I`).
//!
//! # The `MLSD` fallback rule
//!
//! `list` tries `MLSD` and falls back to `LIST` when
//!
//! 1. the command fails at the protocol level (`5xx`, refused data connection,
//!    an error after the transfer), or
//! 2. the transfer succeeds but none of the lines carries a usable `type=` fact
//!    (some servers answer `MLSD` with the `LIST` format).
//!
//! The fallback is remembered for the rest of the session as soon as `LIST`
//! succeeds, so a server without `MLSD` costs one failed command, not one per
//! listing. If both formats fail, the error from `LIST` is reported and `MLSD`
//! stays enabled — a missing directory or a temporary failure must not degrade
//! the session. A genuine empty directory is *not* a failure and does not
//! trigger the fallback.
//!
//! # Cancellation
//!
//! `get` and `put` check the [`CancelToken`] before every 64 KiB chunk. On
//! cancellation the data connection is closed, `ABOR` is sent, and any reply the
//! server still owes is drained so the control connection stays usable. A
//! partially downloaded file is removed; a partially uploaded file stays on the
//! server (the caller decides whether to delete it).

mod control;
mod data;
#[cfg(test)]
mod mock;
mod reply;
#[cfg(test)]
mod tests;

use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use rc_core::ops::CancelToken;

use crate::encoding::Charset;
use crate::entry::RemoteEntry;
use crate::error::RemoteError;
use crate::parse;
use crate::remote_fs::RemoteFs;
use crate::spec::{ConnectionSpec, DEFAULT_TIMEOUT, Protocol};

use control::{BoxStream, Control, map_io};
use data::{Channel, TlsState};
use reply::Reply;

/// Data read/written per transfer step; also the cancellation granularity.
const CHUNK: usize = 64 * 1024;

/// Upper bound for one directory listing, so a hostile server cannot exhaust
/// the client's memory.
const MAX_LISTING: usize = 64 * 1024 * 1024;

/// Short timeout used while draining replies after an aborted transfer.
const RESYNC_TIMEOUT: Duration = Duration::from_millis(400);

/// A live FTP or FTPS session.
pub(crate) struct FtpClient {
    /// Buffered control connection.
    ctl: Control,
    /// Character set of the control channel and of remote names.
    charset: Charset,
    /// Whether data connections are opened in passive mode.
    passive: bool,
    /// Socket timeout for every read, write, connect and accept.
    timeout: Duration,
    /// Protocol this session was opened with (`Ftp` or `Ftps`).
    protocol: Protocol,
    /// Set once the server refused `MLSD`; later listings use `LIST` directly.
    ml_unsupported: bool,
    /// TLS settings for data connections (empty without the `ftps` feature).
    tls: TlsState,
}

impl std::fmt::Debug for FtpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FtpClient")
            .field("protocol", &self.protocol)
            .field("passive", &self.passive)
            .field("timeout", &self.timeout)
            .field("ml_unsupported", &self.ml_unsupported)
            .finish_non_exhaustive()
    }
}

/// The timeout a spec asks for, with `0` replaced by the default.
fn timeout_of(spec: &ConnectionSpec) -> Duration {
    if spec.timeout.is_zero() {
        DEFAULT_TIMEOUT
    } else {
        spec.timeout
    }
}

impl FtpClient {
    /// Connect, log in and enter binary mode (plain FTP).
    pub(crate) fn connect(spec: &ConnectionSpec) -> Result<Self, RemoteError> {
        let timeout = timeout_of(spec);
        let socket = data::connect_tcp(&spec.host, spec.effective_port(), timeout)?;
        let mut client = Self::with_stream(Box::new(socket), spec, timeout);
        client.read_greeting()?;
        client.login(spec)?;
        client.post_login(spec)?;
        Ok(client)
    }

    /// Connect through TLS: implicit on port 990, explicit (`AUTH TLS`) otherwise.
    #[cfg(feature = "ftps")]
    pub(crate) fn connect_ftps(spec: &ConnectionSpec) -> Result<Self, RemoteError> {
        let timeout = timeout_of(spec);
        let port = spec.effective_port();
        let connector = native_tls::TlsConnector::builder()
            .build()
            .map_err(|e| RemoteError::Protocol(format!("cannot initialise TLS: {e}")))?;
        let domain = spec
            .host
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let socket = data::connect_tcp(&spec.host, port, timeout)?;

        if port == 990 {
            let tls = connector
                .connect(&domain, socket)
                .map_err(|e| RemoteError::Protocol(format!("tls handshake failed: {e}")))?;
            let mut client = Self::with_stream(Box::new(tls), spec, timeout);
            client.tls.session = Some((connector, domain));
            client.read_greeting()?;
            client.login(spec)?;
            client.post_login(spec)?;
            return Ok(client);
        }

        // Explicit FTPS: the greeting is still in the clear, TLS starts with
        // AUTH TLS, and the login must happen *after* the upgrade.
        let mut client = Self::with_stream(Box::new(socket), spec, timeout);
        client.read_greeting()?;
        let reply = client.command("AUTH TLS")?;
        if !reply.is_ok() {
            return Err(reply::error(&reply));
        }
        let socket = client.take_tcp()?;
        let tls = connector
            .connect(&domain, socket)
            .map_err(|e| RemoteError::Protocol(format!("tls handshake failed: {e}")))?;
        client.ctl = Control::new(Box::new(tls));
        client.tls.session = Some((connector, domain));
        // RFC 4217: PBSZ 0 then PROT P, so the data connection is encrypted too.
        let _ = client.command("PBSZ 0");
        let reply = client.command("PROT P")?;
        if !reply.is_ok() {
            return Err(RemoteError::Protocol(format!(
                "the server refused to encrypt the data connection: {}",
                reply.detail()
            )));
        }
        client.login(spec)?;
        client.post_login(spec)?;
        Ok(client)
    }

    /// Build a client around an already connected stream.
    fn with_stream(stream: BoxStream, spec: &ConnectionSpec, timeout: Duration) -> Self {
        FtpClient {
            ctl: Control::new(stream),
            charset: Charset::for_label(&spec.encoding),
            passive: spec.passive,
            timeout,
            protocol: spec.protocol,
            ml_unsupported: false,
            tls: TlsState::default(),
        }
    }

    /// Give up the raw TCP socket (needed before the TLS upgrade).
    #[cfg(feature = "ftps")]
    fn take_tcp(&mut self) -> Result<std::net::TcpStream, RemoteError> {
        let ctl = std::mem::replace(&mut self.ctl, Control::new(disconnected()));
        let stream = ctl.into_stream()?;
        stream.take_tcp().ok_or_else(|| {
            RemoteError::Protocol("the control connection is already encrypted".to_string())
        })
    }

    /// Read (and validate) the `220` greeting.
    fn read_greeting(&mut self) -> Result<(), RemoteError> {
        let mut greeting = self.read_reply()?;
        if greeting.code == 120 {
            // "Service ready in N minutes" — the real greeting follows.
            greeting = self.read_reply()?;
        }
        if greeting.code != 220 {
            return Err(reply::error(&greeting));
        }
        Ok(())
    }

    /// Log in, handling anonymous access and the two-step `USER`/`PASS` dance.
    fn login(&mut self, spec: &ConnectionSpec) -> Result<(), RemoteError> {
        let anonymous = spec.is_anonymous();
        let user = if anonymous {
            "anonymous"
        } else {
            spec.user.trim()
        };
        let reply = self.command_with("USER", user)?;
        match reply.code {
            230 | 231 => return Ok(()),
            331 => {}
            332 => {
                return Err(RemoteError::Authentication(format!(
                    "the server requires an account: {}",
                    reply.detail()
                )));
            }
            _ => return Err(reply::error(&reply)),
        }
        let password = if anonymous && spec.password.is_empty() {
            "anonymous@"
        } else {
            spec.password.as_str()
        };
        let reply = self.command_with("PASS", password)?;
        if reply.is_ok() {
            Ok(())
        } else {
            Err(reply::error(&reply))
        }
    }

    /// Finish the session setup: binary mode and the initial directory.
    fn post_login(&mut self, spec: &ConnectionSpec) -> Result<(), RemoteError> {
        // SYST is optional; a server that does not know it is not a problem.
        let _ = self.command("SYST");
        let reply = self.command("TYPE I")?;
        if !reply.is_ok() {
            return Err(reply::error(&reply));
        }
        if !spec.remote_dir.trim().is_empty() {
            // A missing initial directory must not fail the login: the panel
            // simply starts in whatever directory the server chose.
            let _ = self.cwd(&spec.remote_dir);
        }
        Ok(())
    }

    // -- control channel -------------------------------------------------

    /// Send a command, guarding against CRLF injection from remote names.
    fn send(&mut self, verb: &str, argument: Option<&[u8]>) -> Result<(), RemoteError> {
        let mut command = Vec::with_capacity(verb.len() + argument.map_or(0, <[u8]>::len) + 2);
        command.extend_from_slice(verb.as_bytes());
        if let Some(argument) = argument {
            command.push(b' ');
            command.extend_from_slice(argument);
        }
        if command.iter().any(|&b| b == b'\r' || b == b'\n' || b == 0) {
            return Err(RemoteError::Protocol(
                "refusing to send a name containing a line break".to_string(),
            ));
        }
        command.extend_from_slice(b"\r\n");
        self.ctl.write_all(&command)
    }

    /// Send a command without an argument and read the reply.
    fn command(&mut self, verb: &str) -> Result<Reply, RemoteError> {
        self.send(verb, None)?;
        self.read_reply()
    }

    /// Send a command with a path argument encoded in the connection charset.
    fn command_with(&mut self, verb: &str, argument: &str) -> Result<Reply, RemoteError> {
        let encoded = self.charset.encode(argument);
        self.send(verb, Some(&encoded))?;
        self.read_reply()
    }

    /// Read one reply, including its continuation lines.
    fn read_reply(&mut self) -> Result<Reply, RemoteError> {
        let raw = self.ctl.read_line()?;
        let line = self.charset.decode(&raw);
        let (code, separator, first) = reply::parse_first_line(&line)?;
        if separator != b'-' {
            return Ok(Reply { code, text: first });
        }
        let mut continuation = Vec::new();
        loop {
            let raw = self.ctl.read_line()?;
            let line = self.charset.decode(&raw);
            if reply::closes_reply(&line, code) {
                break;
            }
            if continuation.len() >= 256 {
                return Err(RemoteError::Protocol(
                    "the server sent a reply with too many lines".to_string(),
                ));
            }
            continuation.push(line);
        }
        let text = reply::join_lines(first, &continuation)?;
        Ok(Reply { code, text })
    }

    /// Drain whatever the server still owes us after an aborted transfer.
    ///
    /// A server replies to the aborted data transfer (`426`) and to our `ABOR`
    /// (`226`), in either order, and sometimes not at all. Reading replies until
    /// one read times out leaves the connection synchronised in all of those
    /// cases — a later command then reads its own reply, not a stale one.
    fn resync(&mut self) {
        self.ctl.set_timeouts(Some(RESYNC_TIMEOUT));
        for _ in 0..6 {
            if self.read_reply().is_err() {
                break;
            }
        }
        self.ctl.set_timeouts(Some(self.timeout));
    }

    /// Abandon a transfer: close the data connection, tell the server, resync.
    fn cancel_transfer(&mut self, stream: BoxStream) {
        drop(stream);
        let _ = self.send("ABOR", None);
        self.resync();
    }

    // -- data channel ----------------------------------------------------

    /// Announce (passive) or prepare (active) the data connection.
    fn prepare_channel(&mut self) -> Result<Channel, RemoteError> {
        if self.passive {
            if let Some(SocketAddr::V6(peer)) = self.ctl.stream().peer_addr() {
                let reply = self.command("EPSV")?;
                if reply.is_ok()
                    && let Some(port) = data::parse_epsv(&reply.text)
                {
                    return Ok(Channel::Passive(SocketAddr::new(
                        IpAddr::V6(*peer.ip()),
                        port,
                    )));
                }
            }
            let reply = self.command("PASV")?;
            if !reply.is_ok() {
                return Err(reply::error(&reply));
            }
            let announced = data::parse_pasv(&reply.text).ok_or_else(|| {
                RemoteError::Protocol(format!(
                    "cannot parse the passive address: {}",
                    reply.detail()
                ))
            })?;
            let peer = self.ctl.stream().peer_addr();
            Ok(Channel::Passive(data::passive_address(announced, peer)))
        } else {
            let local_ip = self
                .ctl
                .stream()
                .local_addr()
                .map(|address| address.ip())
                .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
            let listener = data::listen(local_ip)?;
            let address = listener.local_addr().map_err(RemoteError::Io)?;
            let reply = if address.is_ipv4() {
                let argument = data::port_argument(address).ok_or_else(|| {
                    RemoteError::Protocol("cannot describe the local listener".to_string())
                })?;
                self.command_with("PORT", &argument)?
            } else {
                self.command_with("EPRT", &data::eprt_argument(address))?
            };
            if !reply.is_ok() {
                return Err(reply::error(&reply));
            }
            Ok(Channel::Active(listener))
        }
    }

    /// Prepare the data connection, start a transfer and open the socket.
    fn start_transfer(
        &mut self,
        verb: &str,
        argument: Option<&[u8]>,
    ) -> Result<BoxStream, RemoteError> {
        let channel = self.prepare_channel()?;
        self.send(verb, argument)?;
        let reply = self.read_reply()?;
        if reply.is_preliminary() {
            channel.open(self.timeout, &self.tls)
        } else if reply.is_ok() {
            Err(RemoteError::Protocol(format!(
                "the server opened no data connection: {}",
                reply.detail()
            )))
        } else {
            Err(reply::error(&reply))
        }
    }

    /// Close a finished data connection and check the completion reply.
    fn finish_transfer(&mut self, stream: BoxStream) -> Result<(), RemoteError> {
        drop(stream);
        self.expect_completion()
    }

    /// Read the reply that concludes a data transfer.
    fn expect_completion(&mut self) -> Result<(), RemoteError> {
        let reply = self.read_reply()?;
        if reply.is_ok() {
            Ok(())
        } else {
            Err(reply::error(&reply))
        }
    }

    /// Run a listing command and return the raw lines of the data connection.
    fn transfer_lines(
        &mut self,
        verb: &str,
        path: Option<&str>,
    ) -> Result<Vec<Vec<u8>>, RemoteError> {
        let argument = path.map(|path| self.charset.encode(path));
        let mut stream = self.start_transfer(verb, argument.as_deref())?;
        let bytes = match data::read_all(&mut stream, MAX_LISTING) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.cancel_transfer(stream);
                return Err(error);
            }
        };
        self.finish_transfer(stream)?;
        Ok(data::split_lines(&bytes))
    }

    /// `MLSD` listing; fails when the server has no usable `MLSD`.
    fn list_mlsd(&mut self, path: Option<&str>) -> Result<Vec<RemoteEntry>, RemoteError> {
        let lines = self.transfer_lines("MLSD", path)?;
        if lines.is_empty() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::with_capacity(lines.len());
        let mut usable = 0usize;
        for line in &lines {
            let text = self.charset.decode(line);
            let Some(entry) = parse::parse_mlsd_line(&text) else {
                continue;
            };
            usable += 1;
            if entry.name == "." || entry.name == ".." {
                continue;
            }
            entries.push(entry);
        }
        if usable == 0 {
            return Err(RemoteError::Unsupported(
                "the server's MLSD output contained no usable entry".to_string(),
            ));
        }
        Ok(entries)
    }

    /// Classic `LIST` listing parsed with the unix format.
    fn list_textual(&mut self, path: Option<&str>) -> Result<Vec<RemoteEntry>, RemoteError> {
        let lines = self.transfer_lines("LIST", path)?;
        let mut entries = Vec::with_capacity(lines.len());
        for line in lines {
            let text = self.charset.decode(&line);
            if let Some(entry) = parse::parse_list_line(&text) {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    /// `MLST` metadata for one path, or `None` when the server cannot do it.
    fn mlst_entry(&mut self, path: &str) -> Result<Option<RemoteEntry>, RemoteError> {
        let reply = self.command_with("MLST", path)?;
        if reply.code != 250 {
            // Unsupported, refused or missing: let the caller fall back to
            // SIZE/CWD, which produce the authoritative answer.
            return Ok(None);
        }
        for line in reply.text.lines() {
            let Some(mut entry) = parse::parse_mlsd_line(line) else {
                continue;
            };
            entry.name = parse::base_name(&entry.name);
            if entry.name.is_empty() {
                continue;
            }
            return Ok(Some(entry));
        }
        Ok(None)
    }

    /// `true` when the working directory could be changed to `path`.
    fn probe_directory(&mut self, path: &str) -> Result<bool, RemoteError> {
        let original = self.pwd().unwrap_or_else(|_| "/".to_string());
        let reply = self.command_with("CWD", path)?;
        if !reply.is_ok() {
            return Ok(false);
        }
        // Best effort: leave the session where it was.
        let _ = self.command_with("CWD", &original);
        Ok(true)
    }
}

#[cfg(feature = "ftps")]
/// A placeholder stream that is never read from, used while the real stream is
/// moved out for the TLS upgrade. Reading it fails, which is the desired
/// behaviour if the upgrade does not complete.
fn disconnected() -> BoxStream {
    struct Placeholder;
    impl Read for Placeholder {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "connection replaced for TLS",
            ))
        }
    }
    impl Write for Placeholder {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "connection replaced for TLS",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl control::Stream for Placeholder {
        fn set_timeouts(&self, _timeout: Option<Duration>) {}
        fn shutdown_write(&self) {}
        fn local_addr(&self) -> Option<SocketAddr> {
            None
        }
        fn peer_addr(&self) -> Option<SocketAddr> {
            None
        }
        fn take_tcp(self: Box<Self>) -> Option<std::net::TcpStream> {
            None
        }
    }
    Box::new(Placeholder)
}

impl RemoteFs for FtpClient {
    fn protocol(&self) -> Protocol {
        self.protocol
    }

    fn pwd(&mut self) -> Result<String, RemoteError> {
        let reply = self.command("PWD")?;
        if reply.code != 257 && !reply.is_ok() {
            return Err(reply::error(&reply));
        }
        Ok(parse::parse_pwd(&reply.text)
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| "/".to_string()))
    }

    fn cwd(&mut self, path: &str) -> Result<String, RemoteError> {
        let reply = self.command_with("CWD", path)?;
        if !reply.is_ok() {
            return Err(reply::error(&reply));
        }
        self.pwd()
    }

    fn list(&mut self, path: Option<&str>) -> Result<Vec<RemoteEntry>, RemoteError> {
        if !self.ml_unsupported {
            match self.list_mlsd(path) {
                Ok(entries) => return Ok(entries),
                Err(RemoteError::Cancelled) => return Err(RemoteError::Cancelled),
                Err(_) => {
                    // Fall back to the textual format. The fallback is only
                    // remembered when it works, so a missing directory or a
                    // transient server error keeps `MLSD` in use next time.
                    return match self.list_textual(path) {
                        Ok(entries) => {
                            self.ml_unsupported = true;
                            Ok(entries)
                        }
                        Err(error) => Err(error),
                    };
                }
            }
        }
        self.list_textual(path)
    }

    fn stat(&mut self, path: &str) -> Result<RemoteEntry, RemoteError> {
        if let Some(entry) = self.mlst_entry(path)? {
            return Ok(entry);
        }
        // `SIZE` proves the path is a regular file and gives its length.
        match self.size(path) {
            Ok(size) => {
                let mtime = self.mdtm(path).unwrap_or(None);
                return Ok(RemoteEntry {
                    name: parse::base_name(path),
                    size,
                    mtime,
                    ..RemoteEntry::default()
                });
            }
            Err(RemoteError::NotFound(_))
            | Err(RemoteError::PermissionDenied(_))
            | Err(RemoteError::Unsupported(_)) => {}
            Err(error) => return Err(error),
        }
        // A successful `CWD` proves it is a directory.
        if self.probe_directory(path)? {
            return Ok(RemoteEntry {
                name: parse::base_name(path),
                is_dir: true,
                ..RemoteEntry::default()
            });
        }
        Err(RemoteError::NotFound(format!(
            "{path}: the server reports no such file or directory"
        )))
    }

    fn mkdir(&mut self, path: &str) -> Result<(), RemoteError> {
        let reply = self.command_with("MKD", path)?;
        if reply.is_ok() {
            Ok(())
        } else {
            Err(reply::error(&reply))
        }
    }

    fn rmdir(&mut self, path: &str) -> Result<(), RemoteError> {
        let reply = self.command_with("RMD", path)?;
        if reply.is_ok() {
            Ok(())
        } else {
            Err(reply::error(&reply))
        }
    }

    fn remove_file(&mut self, path: &str) -> Result<(), RemoteError> {
        let reply = self.command_with("DELE", path)?;
        if reply.is_ok() {
            Ok(())
        } else {
            Err(reply::error(&reply))
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), RemoteError> {
        let reply = self.command_with("RNFR", from)?;
        if reply.code != 350 && !reply.is_ok() {
            return Err(reply::error(&reply));
        }
        let reply = self.command_with("RNTO", to)?;
        if reply.is_ok() {
            Ok(())
        } else {
            Err(reply::error(&reply))
        }
    }

    fn size(&mut self, path: &str) -> Result<u64, RemoteError> {
        let reply = self.command_with("SIZE", path)?;
        if reply.code != 213 && !reply.is_ok() {
            return Err(reply::error(&reply));
        }
        parse::last_u64(&reply.text).ok_or_else(|| {
            RemoteError::Protocol(format!("cannot parse the size reply: {}", reply.detail()))
        })
    }

    fn mdtm(&mut self, path: &str) -> Result<Option<i64>, RemoteError> {
        let reply = self.command_with("MDTM", path)?;
        match reply.code {
            213 => Ok(parse::parse_mdtm_timestamp(&reply.text)),
            202 | 500 | 501 | 502 | 504 => Ok(None),
            _ if reply.is_ok() => Ok(parse::parse_mdtm_timestamp(&reply.text)),
            _ => Err(reply::error(&reply)),
        }
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
        if let Some(parent) = local.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(RemoteError::Io)?;
        }
        let argument = self.charset.encode(remote);
        let mut stream = self.start_transfer("RETR", Some(&argument))?;
        let mut file = match File::create(local) {
            Ok(file) => file,
            Err(error) => {
                self.cancel_transfer(stream);
                return Err(RemoteError::Io(error));
            }
        };
        let mut buffer = [0u8; CHUNK];
        let mut total = 0u64;
        loop {
            if cancel.is_cancelled() {
                drop(file);
                let _ = fs::remove_file(local);
                self.cancel_transfer(stream);
                return Err(RemoteError::Cancelled);
            }
            let read = match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) => {
                    let error = map_io(error, "downloading");
                    drop(file);
                    let _ = fs::remove_file(local);
                    self.cancel_transfer(stream);
                    return Err(error);
                }
            };
            if let Err(error) = file.write_all(&buffer[..read]) {
                self.cancel_transfer(stream);
                return Err(RemoteError::Io(error));
            }
            total += read as u64;
        }
        drop(stream);
        file.flush().map_err(RemoteError::Io)?;
        if let Err(error) = self.expect_completion() {
            // The server rejected the transfer after the fact: do not leave a
            // truncated file behind, the caller would treat it as complete.
            let _ = fs::remove_file(local);
            return Err(error);
        }
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
        let mut file = File::open(local).map_err(RemoteError::Io)?;
        let argument = self.charset.encode(remote);
        let mut stream = self.start_transfer("STOR", Some(&argument))?;
        let mut buffer = [0u8; CHUNK];
        let mut total = 0u64;
        loop {
            if cancel.is_cancelled() {
                self.cancel_transfer(stream);
                return Err(RemoteError::Cancelled);
            }
            let read = match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) => {
                    self.cancel_transfer(stream);
                    return Err(RemoteError::Io(error));
                }
            };
            if let Err(error) = stream.write_all(&buffer[..read]) {
                // A server that rejects the upload closes the data connection;
                // its completion reply carries the real reason.
                drop(stream);
                let reply = self.read_reply()?;
                if reply.is_ok() {
                    return Err(map_io(error, "uploading"));
                }
                return Err(reply::error(&reply));
            }
            total += read as u64;
        }
        let _ = stream.flush();
        stream.shutdown_write();
        drop(stream);
        self.expect_completion()?;
        Ok(total)
    }

    fn quit(&mut self) -> Result<(), RemoteError> {
        // Best effort: a server that hangs up on QUIT has still done what we
        // asked, and the GUI must not report that as a failure.
        if self.send("QUIT", None).is_ok() {
            let _ = self.read_reply();
        }
        Ok(())
    }
}

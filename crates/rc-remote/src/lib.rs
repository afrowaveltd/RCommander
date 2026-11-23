//! `rc-remote` — the network file-system client of **RCommander**.
//!
//! One interface, three protocols. The GUI panels, the background transfer
//! workers and the *synchronise directories* feature only ever see
//! [`RemoteFs`], a synchronous, cancellable view of a remote tree:
//!
//! ```
//! use rc_remote::{ConnectionSpec, Protocol, describe, parse_list_line};
//!
//! let spec = ConnectionSpec {
//!     host: "ftp.example.com".into(),
//!     protocol: Protocol::Ftp,
//!     ..ConnectionSpec::default()
//! };
//! assert!(spec.is_anonymous());
//! assert_eq!(spec.effective_port(), 21);
//! assert_eq!(describe(&spec), "FTP (passive)");
//!
//! let entry = parse_list_line("drwxr-xr-x 1 ftp ftp 4096 Jan 12  2024 pub").unwrap();
//! assert_eq!(entry.name, "pub");
//! ```
//!
//! # Design rules
//!
//! * **Listing metadata is as exact as the protocol allows.** FTP uses `MLSD`
//!   when the server offers it and falls back to `LIST`; `MDTM`/`SIZE` cover the
//!   rest, so the directory synchroniser gets real sizes and timestamps instead
//!   of guesses. Unknown values stay `None` — never a made-up zero.
//! * **None of `parse_list_line`/`parse_mlsd_line` panics on hostile input.** A
//!   garbage line, a missing or negative size, a year-vs-time date or a name
//!   with spaces all produce `None` or a partially filled entry, never a panic
//!   and never a huge allocation.
//! * **Transfers are cancellable from another thread** through
//!   [`rc_core::ops::CancelToken`], checked between chunks of a file, so a
//!   10 GB download stops within one 64 KiB chunk.
//! * **Encrypted protocols are optional.** `ftps` and `sftp` are cargo features;
//!   without them [`connect`] returns [`RemoteError::Unsupported`] with a clear
//!   message and the panel degrades to a text notice.
//!
//! # Protocols
//!
//! | Protocol | Status | Notes |
//! |----------|--------|-------|
//! | [`Protocol::Ftp`] | implemented, tested | passive + active mode, `MLSD`/`LIST`, `MDTM`, `SIZE`, `MLST`; covered end to end by an in-process mock server |
//! | [`Protocol::Ftps`] | feature `ftps` | explicit `AUTH TLS` (`PBSZ`/`PROT P`) and implicit TLS on port 990; compiles and is unit-covered, but **not tested against a real TLS server**; without the feature: [`RemoteError::Unsupported`] |
//! | [`Protocol::Sftp`] | feature `sftp` | libssh2, password authentication; compiles and its path/stat helpers are unit-tested, but **not tested against a real SSH server**; without the feature: [`RemoteError::Unsupported`]`"sftp feature disabled"` |
//!
//! # Documented limitations
//!
//! * **TLS**: FTP only implements `AUTH TLS`/implicit TLS and `PROT P`; there is
//!   no certificate pinning UI. Without the `ftps` feature FTPS is simply
//!   [`RemoteError::Unsupported`].
//! * **`MLSD` fallback rule**: [`RemoteFs::list`] tries `MLSD` first. If the
//!   command fails at the protocol level (a `5xx` reply such as
//!   `500 Unknown command`, a refused data connection) *or* if the server sends
//!   lines that contain no usable `type=` fact, the client re-lists with `LIST`.
//!   The fallback is remembered for the rest of the session once `LIST` works;
//!   if `LIST` fails as well its error is reported and `MLSD` stays enabled.
//!   A genuine empty directory does not trigger the fallback.
//! * **Encoding**: [`ConnectionSpec::encoding`] is a WHATWG label
//!   (`utf-8`, `latin-1`, `windows-1250`, …) used for both directions. An
//!   unknown label means UTF-8. FTP has no encoding negotiation, so a wrong
//!   label produces mangled names but never a failed listing.
//! * **`LIST` timestamps**: months are English; a date without a year is
//!   resolved against the current year (a date more than a day in the future
//!   belongs to the previous year). A `LIST` line whose date cannot be parsed
//!   still yields an entry, just with `mtime == None`. `LIST` also prints only
//!   the year for older files, so it loses the time of day — `MLSD`/`MDTM` keep
//!   it.
//! * **Passive mode behind NAT**: when a server announces a private or loopback
//!   address in `227`, the port is kept but the address of the control
//!   connection is dialled instead. A public announcement is taken literally.
//! * **Symlinks over FTP** are reported as links without a direction, because
//!   `LIST` does not say what they point at; [`RemoteFs::stat`] resolves them.
//! * **Not implemented**: `REST`/`APPE` (no resume or append), ASCII mode
//!   (transfers are always `TYPE I`), `SITE` commands, FXP server-to-server
//!   copies, and any form of parallel transfer. One session moves one file at a
//!   time.
//! * **SFTP host keys are not verified** (see the `sftp` module documentation);
//!   FTPS uses the platform trust store with the default hostname check, and has
//!   no pinning. Neither encrypted protocol is covered by the offline test
//!   suite beyond compile-and-unit level.

#![deny(missing_docs)]
#![warn(missing_debug_implementations)]

mod encoding;
mod entry;
mod error;
mod ftp;
mod parse;
mod remote_fs;
#[cfg(feature = "sftp")]
mod sftp;
mod spec;
mod time;
mod tree;

pub use entry::RemoteEntry;
pub use error::RemoteError;
pub use parse::{parse_list_line, parse_mlsd_line};
pub use remote_fs::{RemoteFs, TransferStats};
pub use spec::{ConnectionSpec, Protocol};

/// Open a connection for the requested protocol.
///
/// The spec is validated first (see [`ConnectionSpec::validate`]); the returned
/// client owns a live control connection, so the caller is expected to call
/// [`RemoteFs::quit`] (dropping it also closes the socket).
///
/// # Errors
///
/// * a validation failure (empty host, SFTP without a user, …);
/// * [`RemoteError::Connect`] when the TCP connection cannot be established;
/// * [`RemoteError::Authentication`] when the server refuses the credentials;
/// * [`RemoteError::Unsupported`] when the protocol needs a cargo feature that
///   this build does not have (the message says which one).
pub fn connect(spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs>, RemoteError> {
    spec.validate()?;
    match spec.protocol {
        Protocol::Ftp => Ok(Box::new(ftp::FtpClient::connect(spec)?)),
        Protocol::Ftps => connect_ftps(spec),
        Protocol::Sftp => connect_sftp(spec),
    }
}

/// [`connect`], but the returned client can be moved to another thread.
///
/// Every implementation is `Send`, so a panel can hand the session to a
/// background transfer worker. This exists as a separate entry point because
/// the required [`RemoteFs`] trait deliberately carries no `Send` bound.
pub fn connect_send(spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs + Send>, RemoteError> {
    spec.validate()?;
    match spec.protocol {
        Protocol::Ftp => {
            let client = ftp::FtpClient::connect(spec)?;
            Ok(Box::new(client))
        }
        Protocol::Ftps => connect_ftps_send(spec),
        Protocol::Sftp => connect_sftp_send(spec),
    }
}

#[cfg(feature = "ftps")]
fn connect_ftps(spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs>, RemoteError> {
    Ok(Box::new(ftp::FtpClient::connect_ftps(spec)?))
}

/// Message used when FTPS support was compiled out.
#[cfg(not(feature = "ftps"))]
const FTPS_DISABLED: &str =
    "ftps support is disabled in this build; rebuild rc-remote with the `ftps` cargo feature";

#[cfg(not(feature = "ftps"))]
fn connect_ftps(_spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs>, RemoteError> {
    Err(RemoteError::Unsupported(FTPS_DISABLED.to_string()))
}

#[cfg(feature = "ftps")]
fn connect_ftps_send(spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs + Send>, RemoteError> {
    Ok(Box::new(ftp::FtpClient::connect_ftps(spec)?))
}

#[cfg(not(feature = "ftps"))]
fn connect_ftps_send(_spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs + Send>, RemoteError> {
    Err(RemoteError::Unsupported(FTPS_DISABLED.to_string()))
}

#[cfg(feature = "sftp")]
fn connect_sftp(spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs>, RemoteError> {
    Ok(Box::new(sftp::SftpClient::connect(spec)?))
}

#[cfg(not(feature = "sftp"))]
fn connect_sftp(_spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs>, RemoteError> {
    Err(RemoteError::Unsupported(
        "sftp feature disabled".to_string(),
    ))
}

#[cfg(feature = "sftp")]
fn connect_sftp_send(spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs + Send>, RemoteError> {
    Ok(Box::new(sftp::SftpClient::connect(spec)?))
}

#[cfg(not(feature = "sftp"))]
fn connect_sftp_send(_spec: &ConnectionSpec) -> Result<Box<dyn RemoteFs + Send>, RemoteError> {
    Err(RemoteError::Unsupported(
        "sftp feature disabled".to_string(),
    ))
}

/// Short human readable protocol label for a status bar or a panel title,
/// e.g. `"FTP (passive)"`, `"FTPS (active)"` or `"SFTP"`.
pub fn describe(spec: &ConnectionSpec) -> String {
    match spec.protocol {
        Protocol::Ftp => format!("FTP ({})", if spec.passive { "passive" } else { "active" }),
        Protocol::Ftps => format!("FTPS ({})", if spec.passive { "passive" } else { "active" }),
        Protocol::Sftp => "SFTP".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(protocol: Protocol, user: &str) -> ConnectionSpec {
        ConnectionSpec {
            host: "127.0.0.1".into(),
            protocol,
            user: user.into(),
            port: 0,
            ..ConnectionSpec::default()
        }
    }

    #[test]
    fn describe_labels_the_protocol_and_ftp_mode() {
        let mut s = spec(Protocol::Ftp, "anonymous");
        assert_eq!(describe(&s), "FTP (passive)");
        s.passive = false;
        assert_eq!(describe(&s), "FTP (active)");
        s.protocol = Protocol::Ftps;
        s.passive = true;
        assert_eq!(describe(&s), "FTPS (passive)");
        s.passive = false;
        assert_eq!(describe(&s), "FTPS (active)");
        s.protocol = Protocol::Sftp;
        assert_eq!(describe(&s), "SFTP");
    }

    /// `unwrap_err` without requiring `Debug` on the client box.
    fn error_of(result: Result<Box<dyn RemoteFs>, RemoteError>) -> RemoteError {
        match result {
            Ok(_) => panic!("expected the connection to fail"),
            Err(error) => error,
        }
    }

    #[test]
    fn connect_rejects_an_invalid_spec_before_any_network_traffic() {
        let mut s = spec(Protocol::Ftp, "anonymous");
        s.host = String::new();
        let error = error_of(connect(&s));
        assert!(matches!(error, RemoteError::Connect { .. }), "{error:?}");
    }

    #[cfg(not(feature = "sftp"))]
    #[test]
    fn sftp_without_the_feature_reports_unsupported() {
        let s = spec(Protocol::Sftp, "alice");
        let error = error_of(connect(&s));
        assert!(matches!(error, RemoteError::Unsupported(_)), "{error:?}");
        assert!(
            error.to_string().contains("sftp feature disabled"),
            "{error}"
        );
        // The Send-flavoured entry point behaves the same way.
        let error = match connect_send(&s) {
            Ok(_) => panic!("expected the connection to fail"),
            Err(error) => error,
        };
        assert!(error.is_unsupported());
    }

    #[cfg(not(feature = "ftps"))]
    #[test]
    fn ftps_without_the_feature_reports_unsupported() {
        let s = spec(Protocol::Ftps, "anonymous");
        let error = error_of(connect(&s));
        assert!(error.is_unsupported(), "{error:?}");
        assert!(error.to_string().contains("ftps"), "{error}");
    }

    #[test]
    fn parses_are_reachable_from_the_crate_root() {
        assert!(parse_list_line("-rw-r--r-- 1 o g 1 Jan  1  2024 f").is_some());
        assert!(parse_mlsd_line("type=file;size=1; f").is_some());
    }
}

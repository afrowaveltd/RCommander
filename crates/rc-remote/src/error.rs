//! The single error type of the remote file-system client.

use std::time::Duration;

/// Everything that can go wrong while talking to an FTP, FTPS or SFTP server.
///
/// The variants are chosen so that the GUI can decide what to show without
/// knowing the protocol: [`RemoteError::Cancelled`] is not a failure (it ends a
/// transfer the user aborted), [`RemoteError::Unsupported`] means the feature
/// is simply not available here (the panel degrades to a message), and the rest
/// carry enough text to put into a log or a status bar.
#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    /// A local file-system or socket error that is not protocol specific.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// The TCP connection to the server could not be established.
    #[error("cannot connect to {host}:{port}: {reason}")]
    Connect {
        /// Host that was contacted.
        host: String,
        /// Port that was contacted.
        port: u16,
        /// Why the connection failed.
        reason: String,
    },

    /// The server refused the credentials (FTP `530`, SFTP authentication error).
    #[error("authentication failed: {0}")]
    Authentication(String),

    /// The server sent something the client could not make sense of, or refused
    /// a command in a way that is not covered by a more specific variant.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// The remote path does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// The server refused the operation for permission reasons.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// The operation is not available — either the server does not implement it
    /// or this build of `rc-remote` has the required cargo feature disabled.
    #[error("unsupported: {0}")]
    Unsupported(String),

    /// The operation was cancelled through [`rc_core::ops::CancelToken`].
    #[error("operation cancelled")]
    Cancelled,

    /// The server did not answer within the configured timeout.
    #[error("timed out: {0}")]
    Timeout(String),
}

impl RemoteError {
    /// True when the failure is a user cancellation rather than a real error.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, RemoteError::Cancelled)
    }

    /// True when the failure means "this build cannot do that", which the GUI
    /// should present as a capability message instead of an error dialog.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, RemoteError::Unsupported(_))
    }

    /// Build a [`RemoteError::Timeout`] from the configured timeout.
    pub(crate) fn timeout(what: impl std::fmt::Display, after: Duration) -> Self {
        RemoteError::Timeout(format!("{what} (no answer within {:.1?})", after))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_implement_display_and_error() {
        let errors: Vec<RemoteError> = vec![
            RemoteError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "nope")),
            RemoteError::Connect {
                host: "h".into(),
                port: 21,
                reason: "refused".into(),
            },
            RemoteError::Authentication("bad password".into()),
            RemoteError::Protocol("garbage".into()),
            RemoteError::NotFound("/x".into()),
            RemoteError::PermissionDenied("/y".into()),
            RemoteError::Unsupported("nope".into()),
            RemoteError::Cancelled,
            RemoteError::Timeout("too slow".into()),
        ];
        for error in &errors {
            let text = error.to_string();
            assert!(!text.is_empty(), "empty Display for {error:?}");
            // `Error` is implemented: `source()` must not panic.
            let _ = std::error::Error::source(error);
        }
        assert!(errors[7].is_cancelled());
        assert!(!errors[6].is_cancelled());
        assert!(errors[6].is_unsupported());
        assert!(errors[1].to_string().contains("h:21"));
    }

    #[test]
    fn io_errors_convert_through_from() {
        let source = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "gone");
        let error: RemoteError = source.into();
        assert!(matches!(error, RemoteError::Io(_)));
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn timeouts_mention_the_duration() {
        let error = RemoteError::timeout("reading a reply", Duration::from_secs(3));
        assert!(matches!(error, RemoteError::Timeout(_)));
        assert!(error.to_string().contains("3"));
    }
}

/// The step a connection attempt has reached.
///
/// Reported while connecting so the status line can say *what* is happening: a window that shows
/// "authenticating" for ten seconds is honest, one that shows nothing is not. It is also how a
/// failure explains itself — "the host key is not trusted" and "the socket was refused" are very
/// different problems and the step is what makes the difference visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectStage {
    /// Looking the host name up.
    Resolving,
    /// Opening the socket.
    Connecting,
    /// Exchanging keys with the server (SSH) or waiting for the greeting (FTP).
    Handshake,
    /// Checking the server's identity.
    HostKey,
    /// Presenting the password, the key or the agent's identities.
    Authenticating,
    /// Opening the file-transfer channel and reading the first directory.
    Opening,
}

impl ConnectStage {
    /// Localisation key of the stage.
    pub fn key(self) -> &'static str {
        match self {
            ConnectStage::Resolving => "connect.stage.resolving",
            ConnectStage::Connecting => "connect.stage.connecting",
            ConnectStage::Handshake => "connect.stage.handshake",
            ConnectStage::HostKey => "connect.stage.hostkey",
            ConnectStage::Authenticating => "connect.stage.authenticating",
            ConnectStage::Opening => "connect.stage.opening",
        }
    }

    /// Every stage, in order, for the tests and the documentation.
    pub const ALL: [ConnectStage; 6] = [
        ConnectStage::Resolving,
        ConnectStage::Connecting,
        ConnectStage::Handshake,
        ConnectStage::HostKey,
        ConnectStage::Authenticating,
        ConnectStage::Opening,
    ];
}

/// A cheap way to report stages without forcing every caller to care.
pub type StageReporter<'a> = &'a mut dyn FnMut(ConnectStage);

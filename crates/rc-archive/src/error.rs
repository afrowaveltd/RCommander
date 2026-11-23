//! The crate-wide error type.

use std::path::{Path, PathBuf};

/// Result alias used throughout `rc-archive`.
pub type Result<T> = std::result::Result<T, ArchiveError>;

/// Everything that can go wrong while listing, reading, extracting, creating,
/// modifying or testing an archive.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// An I/O failure, carrying the path that was being touched.
    #[error("i/o error on {}: {source}", path.display())]
    Io {
        /// The file or directory the operation was working on.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The container, the compression method or the requested operation is not
    /// supported by this crate.
    #[error("unsupported: {0}")]
    Unsupported(String),

    /// The archive is damaged, truncated or structurally invalid.
    #[error("malformed archive: {0}")]
    Malformed(String),

    /// One member uses a format the backend cannot decode while the rest of the
    /// archive is fine.
    #[error("unsupported member in {archive} ({format}): {reason}")]
    UnsupportedMember {
        /// Archive that contains the member.
        archive: String,
        /// Container format.
        format: String,
        /// Which member, and why it could not be decoded.
        reason: String,
    },

    /// The archive — or a single member — is encrypted and no password was
    /// supplied.
    #[error("password required")]
    PasswordRequired,

    /// A member name would escape the extraction root (absolute path, drive
    /// letter or `..` component).
    #[error("unsafe path in archive: {0}")]
    UnsafePath(String),

    /// The caller set the cancellation token.
    #[error("operation cancelled")]
    Cancelled,
}

impl ArchiveError {
    /// Wrap an [`std::io::Error`] with the path that produced it.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        ArchiveError::Io {
            path: path.into(),
            source,
        }
    }

    /// Build a [`ArchiveError::Malformed`] from any displayable value.
    pub fn malformed(message: impl Into<String>) -> Self {
        ArchiveError::Malformed(message.into())
    }

    /// Build an [`ArchiveError::Unsupported`] from any displayable value.
    pub fn unsupported(message: impl Into<String>) -> Self {
        ArchiveError::Unsupported(message.into())
    }

    /// Build an [`ArchiveError::UnsupportedMember`].
    pub fn unsupported_member(
        archive: impl Into<String>,
        format: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        ArchiveError::UnsupportedMember {
            archive: archive.into(),
            format: format.into(),
            reason: reason.into(),
        }
    }

    /// Build an [`ArchiveError::UnsafePath`] that names the offending member.
    pub fn unsafe_path(member: &str, reason: &str) -> Self {
        ArchiveError::UnsafePath(format!("{reason} (member {member:?})"))
    }

    /// True when the failure is a cancellation rather than a real error.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, ArchiveError::Cancelled)
    }

    /// The path involved, when the error carries one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            ArchiveError::Io { path, .. } => Some(path.as_path()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_carries_path_and_source() {
        let err = ArchiveError::io(
            "/tmp/broken.zip",
            std::io::Error::from(std::io::ErrorKind::NotFound),
        );
        assert_eq!(err.path(), Some(Path::new("/tmp/broken.zip")));
        let text = err.to_string();
        assert!(text.contains("i/o error"), "{text}");
        assert!(text.contains("broken.zip"), "{text}");
        // std::error::Error must be implemented (via thiserror) with a source.
        let as_err: &dyn std::error::Error = &err;
        assert!(as_err.source().is_some());
    }

    #[test]
    fn cancellation_is_detected() {
        assert!(ArchiveError::Cancelled.is_cancelled());
        assert!(!ArchiveError::Malformed("x".into()).is_cancelled());
        assert_eq!(ArchiveError::Cancelled.to_string(), "operation cancelled");
    }

    #[test]
    fn unsafe_path_names_the_member() {
        let err = ArchiveError::unsafe_path("../evil.txt", "parent-directory component");
        let text = err.to_string();
        assert!(text.contains("../evil.txt"), "{text}");
        assert!(text.contains("parent-directory"), "{text}");
        assert!(matches!(err, ArchiveError::UnsafePath(_)));
    }

    #[test]
    fn unsupported_member_mentions_all_three_parts() {
        let err = ArchiveError::unsupported_member("a.zip", "zip", "file.7z: bzip2 method");
        let text = err.to_string();
        assert!(
            text.contains("a.zip") && text.contains("zip") && text.contains("bzip2"),
            "{text}"
        );
        assert!(matches!(err, ArchiveError::UnsupportedMember { .. }));
    }

    #[test]
    fn password_and_unsupported_messages_are_stable() {
        assert_eq!(
            ArchiveError::PasswordRequired.to_string(),
            "password required"
        );
        assert!(
            ArchiveError::unsupported("RAR")
                .to_string()
                .starts_with("unsupported")
        );
        assert!(
            ArchiveError::malformed("bad header")
                .to_string()
                .contains("bad header")
        );
        assert_eq!(ArchiveError::PasswordRequired.path(), None);
    }
}

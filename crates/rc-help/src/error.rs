//! Error type of the help subsystem.

use std::path::PathBuf;

/// Result alias for help operations.
pub type Result<T> = std::result::Result<T, HelpError>;

/// Everything that can go wrong while loading the help tree.
///
/// The help system is deliberately forgiving: a missing root directory, a
/// missing translation, an unreadable topic folder or a file that is not valid
/// UTF-8 are all *not* errors — they degrade to an empty index, a fallback
/// language or lossy-decoded text. Only a hard failure to even stat the
/// configured root is reported here.
#[derive(Debug, thiserror::Error)]
pub enum HelpError {
    /// An I/O error together with the path that produced it.
    #[error("i/o error on {path}: {source}")]
    Io {
        /// Path that could not be accessed.
        path: PathBuf,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },
}

impl HelpError {
    /// Wrap an [`std::io::Error`] together with the path that produced it.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        HelpError::Io {
            path: path.into(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_keeps_path_and_kind() {
        let err = HelpError::io(
            "/tmp/docs",
            std::io::Error::from(std::io::ErrorKind::NotFound),
        );
        let text = err.to_string();
        assert!(text.contains("/tmp/docs"), "message: {text}");
        assert!(text.contains("i/o error"), "message: {text}");
        match err {
            HelpError::Io { ref path, .. } => assert_eq!(path.to_string_lossy(), "/tmp/docs"),
        }
    }
}

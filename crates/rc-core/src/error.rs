//! Error type shared by `rc-core`.

use std::path::PathBuf;

/// Result alias for core operations.
pub type CoreResult<T> = Result<T, CoreError>;

/// Everything that can go wrong in core engine operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid pattern {pattern:?}: {reason}")]
    Pattern { pattern: String, reason: String },

    #[error("invalid location {0:?}")]
    Location(String),

    #[error("operation cancelled")]
    Cancelled,

    #[error("{0}")]
    Invalid(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("unsupported on this platform: {0}")]
    Unsupported(String),
}

impl CoreError {
    /// Wrap an [`std::io::Error`] together with the path that produced it.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.into(),
            source,
        }
    }

    /// True when the failure is a cancellation rather than a real error.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, CoreError::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_keeps_path_and_message() {
        let e = CoreError::io("/tmp/x", std::io::Error::from(std::io::ErrorKind::NotFound));
        let text = e.to_string();
        assert!(text.contains("/tmp/x"), "message: {text}");
        assert!(text.contains("i/o error"), "message: {text}");
        assert!(!e.is_cancelled());
    }

    #[test]
    fn cancellation_is_detected() {
        assert!(CoreError::Cancelled.is_cancelled());
    }
}

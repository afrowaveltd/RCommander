//! Error type for the language-pack engine.
//!
//! The crate deliberately owns its error type instead of reusing `rc-core`'s
//! `CoreError` so that localisation can be tested, embedded and reused without
//! pulling in the rest of the engine.

use std::path::PathBuf;

/// Result alias for localisation operations.
pub type I18nResult<T> = Result<T, I18nError>;

/// Everything that can go wrong while loading, parsing or validating a pack.
#[derive(Debug, thiserror::Error)]
pub enum I18nError {
    /// The pack file could not be read (missing, unreadable, not UTF-8).
    #[error("cannot read language pack {path}: {source}")]
    Io {
        /// File that failed.
        path: PathBuf,
        /// Underlying i/o error.
        #[source]
        source: std::io::Error,
    },

    /// The file is not valid TOML. The inner error carries line and column.
    #[error("language pack {code:?} is not valid TOML: {source}")]
    Parse {
        /// Language code of the pack, or `"<unknown>"` when it is not known yet.
        code: String,
        /// TOML syntax or type error.
        #[source]
        source: toml::de::Error,
    },

    /// The TOML is well formed but does not describe a usable pack.
    #[error("language pack {code:?} is invalid: {reason}")]
    Invalid {
        /// Language code of the pack, or `"<unknown>"` when it is not known yet.
        code: String,
        /// Human readable explanation, suitable for a message dialog.
        reason: String,
    },

    /// A key in `[strings]` breaks the key syntax.
    #[error("language pack {code:?} has an invalid key {key:?}: {reason}")]
    InvalidKey {
        /// Language code of the pack, or `"<unknown>"` when it is not known yet.
        code: String,
        /// The offending key, as written in the file.
        key: String,
        /// Human readable explanation, suitable for a message dialog.
        reason: String,
    },
}

impl I18nError {
    /// Wrap an [`std::io::Error`] together with the path that produced it.
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        I18nError::Io {
            path: path.into(),
            source,
        }
    }

    /// Build an [`I18nError::Invalid`] from any displayable reason.
    pub fn invalid(code: impl Into<String>, reason: impl Into<String>) -> Self {
        I18nError::Invalid {
            code: code_param(code),
            reason: reason.into(),
        }
    }

    /// Build an [`I18nError::InvalidKey`] from any displayable reason.
    pub fn invalid_key(
        code: impl Into<String>,
        key: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        I18nError::InvalidKey {
            code: code_param(code),
            key: key.into(),
            reason: reason.into(),
        }
    }

    /// Language code the failure belongs to.
    ///
    /// Returns `"<unknown>"` when the pack could not be identified, for example
    /// because the TOML is broken before `[meta]` could be read.
    pub fn code(&self) -> &str {
        match self {
            I18nError::Io { .. } => "<unknown>",
            I18nError::Parse { code, .. }
            | I18nError::Invalid { code, .. }
            | I18nError::InvalidKey { code, .. } => code,
        }
    }

    /// True for a TOML syntax/type error.
    pub fn is_parse_error(&self) -> bool {
        matches!(self, I18nError::Parse { .. })
    }

    /// True for an i/o failure.
    pub fn is_io_error(&self) -> bool {
        matches!(self, I18nError::Io { .. })
    }

    /// A one-line problem description for the "check language pack" dialog.
    ///
    /// Unlike [`Display`](std::fmt::Display) this never repeats the language
    /// code, which is what a user looking at a single file wants to read.
    pub fn to_problem(&self) -> String {
        match self {
            I18nError::Io { path, source } => {
                format!("cannot read {}: {source}", path.display())
            }
            I18nError::Parse { source, .. } => format!("not valid TOML: {source}"),
            I18nError::Invalid { reason, .. } => reason.clone(),
            I18nError::InvalidKey { key, reason, .. } => {
                format!("invalid key {key:?}: {reason}")
            }
        }
    }
}

/// Turn a caller supplied code into the label stored in the error.
///
/// Blank or whitespace-only codes become `"<unknown>"` so that error messages
/// always read sensibly.
fn code_param(code: impl Into<String>) -> String {
    let code = code.into();
    if code.trim().is_empty() {
        "<unknown>".to_string()
    } else {
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_keeps_path_and_mentions_it() {
        let e = I18nError::io(
            "/tmp/locales/cs.toml",
            std::io::Error::from(std::io::ErrorKind::NotFound),
        );
        assert!(e.is_io_error());
        assert!(!e.is_parse_error());
        let text = e.to_string();
        assert!(text.contains("cs.toml"), "message: {text}");
        assert!(e.to_problem().contains("cs.toml"));
    }

    #[test]
    fn blank_code_becomes_unknown_placeholder() {
        let e = I18nError::invalid("   ", "no code");
        assert_eq!(e.code(), "<unknown>");
        assert!(!e.to_string().contains("\"\""), "message: {}", e);
        assert_eq!(e.to_problem(), "no code");
    }

    #[test]
    fn invalid_key_error_names_the_key() {
        let e = I18nError::invalid_key("cs", "App.Title", "keys must be lowercase");
        assert_eq!(e.code(), "cs");
        let text = e.to_string();
        assert!(text.contains("App.Title"), "message: {text}");
        assert!(
            e.to_problem().starts_with("invalid key"),
            "problem: {}",
            e.to_problem()
        );
    }

    #[test]
    fn parse_error_is_recognised_and_reports_problem() {
        let source: toml::de::Error = "x = [".parse::<toml::Table>().expect_err("must fail");
        let e = I18nError::Parse {
            code: "cs".to_string(),
            source,
        };
        assert!(e.is_parse_error());
        assert!(!e.is_io_error());
        assert!(
            e.to_problem().starts_with("not valid TOML"),
            "problem: {}",
            e.to_problem()
        );
        assert!(e.to_string().contains("cs"), "message: {e}");
    }
}

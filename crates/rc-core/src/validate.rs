//! Checking a file *before* it is written back (Phase 16.4).
//!
//! An agent that "fixed" a JSON file by removing a comma has produced a change that looks plausible
//! in a diff and breaks the program that reads it. A reviewer can catch that by reading the diff
//! carefully; a parser catches it every time.
//!
//! Two rules shape this module:
//!
//! * **Only formats we can check without running anything.** JSON and TOML are parsed in-process;
//!   anything else returns [`Verdict::Unknown`], which is honest rather than reassuring. Running a
//!   compiler or a test suite is a different, much heavier feature and belongs behind an approval.
//! * **Never a red light by accident.** A file with an unknown extension, or a format we cannot
//!   check, is `Unknown`, not `Broken` — a false alarm on a valid file would train the user to
//!   ignore the warning, which is worse than not having one.

use std::path::Path;

/// What a validator concluded about a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The file parses.
    Valid,
    /// The file does not parse, with the parser's own message.
    Broken(String),
    /// There is no validator for this kind of file.
    Unknown,
}

impl Verdict {
    pub fn is_broken(&self) -> bool {
        matches!(self, Verdict::Broken(_))
    }

    /// Short text for a review row.
    pub fn label(&self) -> &'static str {
        match self {
            Verdict::Valid => "syntax ok",
            Verdict::Broken(_) => "syntax error",
            Verdict::Unknown => "",
        }
    }
}

/// Which validator a path gets, from its extension.
pub fn kind_of(path: &Path) -> Option<&'static str> {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "json" => Some("json"),
        "toml" => Some("toml"),
        "ini" | "cfg" | "conf" => Some("toml"),
        _ => None,
    }
}

/// Validate `bytes` as the kind of file `path` looks like.
pub fn check(path: &Path, bytes: &[u8]) -> Verdict {
    let Some(kind) = kind_of(path) else {
        return Verdict::Unknown;
    };
    // A file that is not text cannot be parsed as either format; saying so beats a parser error
    // about a byte range.
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Verdict::Broken("the file is not valid UTF-8".to_string());
    };
    match kind {
        "json" => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(_) => Verdict::Valid,
            Err(error) => Verdict::Broken(format!("JSON: {error}")),
        },
        "toml" => match toml::from_str::<toml::Value>(text) {
            Ok(_) => Verdict::Valid,
            Err(error) => Verdict::Broken(format!("TOML: {error}")),
        },
        _ => Verdict::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn check_text(name: &str, text: &str) -> Verdict {
        check(&PathBuf::from(name), text.as_bytes())
    }

    #[test]
    fn valid_json_and_toml_pass() {
        assert_eq!(check_text("a.json", "{\"a\": 1}"), Verdict::Valid);
        assert_eq!(check_text("a.toml", "a = 1\n"), Verdict::Valid);
        // The usual configuration extensions are treated as TOML, which is what we write.
        assert_eq!(check_text("rc.conf", "a = 1\n"), Verdict::Valid);
        assert_eq!(check_text("settings.ini", "a = 1\n"), Verdict::Valid);
    }

    #[test]
    fn a_broken_file_is_reported_with_the_parsers_own_words() {
        let verdict = check_text("broken.json", "{\"a\": 1,}");
        match verdict {
            Verdict::Broken(message) => {
                assert!(message.starts_with("JSON:"), "{message}");
                assert!(!message.is_empty());
            }
            other => panic!("expected a broken verdict, got {other:?}"),
        }
        // TOML and JSON are told apart, so the caller can say which format failed.
        match check_text("broken.toml", "a = ") {
            Verdict::Broken(message) => assert!(message.starts_with("TOML:"), "{message}"),
            other => panic!("expected a broken verdict, got {other:?}"),
        }
    }

    #[test]
    fn a_kind_of_file_we_cannot_check_is_unknown_not_broken() {
        // Silence beats a false alarm: an unvalidated report must not train the user to ignore it.
        assert_eq!(check_text("main.rs", "fn main() {}"), Verdict::Unknown);
        assert_eq!(check_text("notes.txt", "hello"), Verdict::Unknown);
        assert_eq!(check_text("no-extension", "hello"), Verdict::Unknown);
        assert!(kind_of(&PathBuf::from("a.json")).is_some());
        assert!(kind_of(&PathBuf::from("a.rs")).is_none());
    }

    #[test]
    fn a_binary_file_with_a_json_extension_is_broken_not_a_panic() {
        let verdict = check(&PathBuf::from("weird.json"), &[0xff, 0xfe, 0x00]);
        match verdict {
            Verdict::Broken(message) => assert!(message.contains("UTF-8"), "{message}"),
            other => panic!("expected a broken verdict, got {other:?}"),
        }
    }

    #[test]
    fn the_labels_fit_a_review_row() {
        assert_eq!(Verdict::Valid.label(), "syntax ok");
        assert_eq!(Verdict::Broken("x".to_string()).label(), "syntax error");
        assert_eq!(Verdict::Unknown.label(), "");
        assert!(Verdict::Broken("x".to_string()).is_broken());
        assert!(!Verdict::Valid.is_broken());
    }
}

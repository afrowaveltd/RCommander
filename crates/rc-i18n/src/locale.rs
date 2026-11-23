//! One language pack: `[meta]` plus the `[strings]` table.
//!
//! Packs are plain TOML files that a translator can edit in any text editor —
//! no compilation step, no external tooling:
//!
//! ```toml
//! [meta]
//! code = "cs"
//! name = "Czech"
//! native_name = "Čeština"
//! direction = "ltr"          # optional, auto-detected from the code
//! version = 1
//! authors = ["Somebody"]
//!
//! [strings]
//! "menu.file" = "Soubor"
//! "status.selected" = "{count} vybráno z {total}"
//! ```
//!
//! Parsing is deliberately forgiving about *what* a pack contains but strict
//! about *shape*: a typo in a key or an empty translation is reported as an
//! error instead of silently producing a half-translated interface.

use std::collections::BTreeMap;
use std::path::Path;

use crate::direction::{Direction, direction_for_code};
use crate::error::{I18nError, I18nResult};

/// Sections that may appear at the top level of a pack.
const KNOWN_SECTIONS: [&str; 2] = ["meta", "strings"];

/// Static facts about one language pack.
///
/// These are what a language selector needs; the strings themselves live in
/// [`Locale`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleMeta {
    /// Normalised lowercase code, e.g. `"cs"` or `"pt-br"`.
    pub code: String,
    /// English name of the language, e.g. `"Czech"`.
    pub name: String,
    /// Name in the language itself, e.g. `"Čeština"`.
    pub native_name: String,
    /// Writing direction of the interface for this language.
    pub direction: Direction,
    /// Pack format version, `1` when the pack does not say.
    pub version: u32,
    /// Translators credited in `[meta] authors`.
    pub authors: Vec<String>,
    /// Number of translated keys in the pack.
    pub string_count: usize,
}

impl LocaleMeta {
    /// True when this language is written right to left.
    pub fn is_rtl(&self) -> bool {
        self.direction.is_rtl()
    }

    /// Label for a language selector: `"Čeština (Czech)"`.
    ///
    /// The English name is added in brackets only when it differs from the
    /// native name, so `"English"` does not become `"English (English)"`.
    pub fn selector_label(&self) -> String {
        if self.native_name == self.name {
            self.native_name.clone()
        } else {
            format!("{} ({})", self.native_name, self.name)
        }
    }
}

/// A parsed language pack.
///
/// `Locale` owns its strings in a sorted map, so iteration is deterministic and
/// lookups are `O(log n)`. Keys are validated on construction, which lets
/// [`get`](Locale::get) stay a plain lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locale {
    meta: LocaleMeta,
    strings: BTreeMap<String, String>,
}

impl Locale {
    /// Parse pack text coming from a file or the clipboard.
    ///
    /// `code_hint` is used when `[meta] code` is absent; pass the file stem
    /// (`cs.toml` → `"cs"`) or an empty string. The hint is normalised the same
    /// way as a declared code: trimmed, lowercased, `_` folded to `-`.
    ///
    /// # Errors
    ///
    /// * [`I18nError::Parse`] — the text is not valid TOML.
    /// * [`I18nError::Invalid`] — unknown section, wrong value type, no code,
    ///   unknown `direction`, out-of-range `version`.
    /// * [`I18nError::InvalidKey`] — a key that is not lowercase dotted ASCII,
    ///   a non-string value, or an empty translation.
    pub fn parse(code_hint: &str, text: &str) -> I18nResult<Locale> {
        let hint = normalize_code(code_hint);
        // Editors on Windows love to prepend a byte order mark; TOML does not.
        let body = text.strip_prefix('\u{feff}').unwrap_or(text);
        let table: toml::Table = body.parse().map_err(|source| I18nError::Parse {
            code: label(&hint),
            source,
        })?;

        // Shape first, so a typo like `[string]` is reported before "no code".
        for key in table.keys() {
            if !KNOWN_SECTIONS.contains(&key.as_str()) {
                return Err(I18nError::invalid(
                    label(&hint),
                    format!("unknown section [{key}]: only [meta] and [strings] are allowed"),
                ));
            }
        }

        // A pack without [meta] is legal: everything is filled in from the hint.
        let empty_meta = toml::Table::new();
        let meta_table = match table.get("meta") {
            Some(value) => value.as_table().ok_or_else(|| {
                I18nError::invalid(
                    label(&hint),
                    format!("[meta] must be a table, found {}", type_name(value)),
                )
            })?,
            None => &empty_meta,
        };

        let code = declared_code(meta_table, &hint)?;

        let name_field = opt_string(meta_table, "name", &code)?;
        let native_field = opt_string(meta_table, "native_name", &code)?;
        let name = non_empty(name_field)
            .or_else(|| non_empty(native_field.clone()))
            .unwrap_or_else(|| code.clone());
        let native_name = non_empty(native_field).unwrap_or_else(|| name.clone());

        let direction = match meta_table.get("direction") {
            None => direction_for_code(&code),
            Some(toml::Value::String(tag)) => Direction::from_tag(tag).ok_or_else(|| {
                I18nError::invalid(
                    code.clone(),
                    format!("meta.direction must be \"ltr\" or \"rtl\", found {tag:?}"),
                )
            })?,
            Some(other) => {
                return Err(I18nError::invalid(
                    code,
                    format!(
                        "meta.direction must be a string, found {}",
                        type_name(other)
                    ),
                ));
            }
        };

        let version = match meta_table.get("version") {
            None => 1,
            Some(toml::Value::Integer(value)) => u32::try_from(*value).map_err(|_| {
                I18nError::invalid(
                    code.clone(),
                    format!(
                        "meta.version must be between 0 and {}, found {value}",
                        u32::MAX
                    ),
                )
            })?,
            Some(other) => {
                return Err(I18nError::invalid(
                    code,
                    format!(
                        "meta.version must be an integer, found {}",
                        type_name(other)
                    ),
                ));
            }
        };

        let authors = match meta_table.get("authors") {
            None => Vec::new(),
            Some(toml::Value::Array(items)) => {
                let mut authors = Vec::with_capacity(items.len());
                for (index, item) in items.iter().enumerate() {
                    match item {
                        toml::Value::String(author) => authors.push(author.trim().to_string()),
                        other => {
                            return Err(I18nError::invalid(
                                code,
                                format!(
                                    "meta.authors[{index}] must be a string, found {}",
                                    type_name(other)
                                ),
                            ));
                        }
                    }
                }
                authors
            }
            Some(other) => {
                return Err(I18nError::invalid(
                    code,
                    format!(
                        "meta.authors must be an array of strings, found {}",
                        type_name(other)
                    ),
                ));
            }
        };

        let mut strings = BTreeMap::new();
        if let Some(value) = table.get("strings") {
            let Some(table) = value.as_table() else {
                return Err(I18nError::invalid(
                    code,
                    format!("[strings] must be a table, found {}", type_name(value)),
                ));
            };
            for (key, value) in table {
                if let Err(reason) = check_key(key) {
                    return Err(I18nError::InvalidKey {
                        code: code.clone(),
                        key: key.clone(),
                        reason,
                    });
                }
                let toml::Value::String(text) = value else {
                    return Err(I18nError::InvalidKey {
                        code: code.clone(),
                        key: key.clone(),
                        reason: format!("the value must be a string, found {}", type_name(value)),
                    });
                };
                let text = text.trim();
                if text.is_empty() {
                    return Err(I18nError::InvalidKey {
                        code: code.clone(),
                        key: key.clone(),
                        reason: "the value is empty; translate it or remove the key".to_string(),
                    });
                }
                strings.insert(key.clone(), text.to_string());
            }
        }

        let string_count = strings.len();
        Ok(Locale {
            meta: LocaleMeta {
                code,
                name,
                native_name,
                direction,
                version,
                authors,
                string_count,
            },
            strings,
        })
    }

    /// Load a pack from disk.
    ///
    /// The file stem is the code hint, so `locales/cs.toml` may omit
    /// `[meta] code` entirely.
    ///
    /// # Errors
    ///
    /// [`I18nError::Io`] when the file cannot be read (including non-UTF-8
    /// content), plus everything [`parse`](Locale::parse) reports.
    pub fn load(path: &Path) -> I18nResult<Locale> {
        let hint = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string();
        let text = std::fs::read_to_string(path).map_err(|error| I18nError::io(path, error))?;
        Locale::parse(&hint, &text)
    }

    /// Build an empty locale with nothing but a code.
    ///
    /// Used as the last-resort fallback for the built-in pack; every lookup
    /// misses, so the translator falls through to the key itself.
    pub(crate) fn empty(code: &str) -> Locale {
        let code = normalize_code(code);
        let meta = LocaleMeta {
            name: code.clone(),
            native_name: code.clone(),
            direction: direction_for_code(&code),
            version: 1,
            authors: Vec::new(),
            string_count: 0,
            code,
        };
        Locale {
            meta,
            strings: BTreeMap::new(),
        }
    }

    /// Description of the pack: code, names, direction, version, authors.
    pub fn meta(&self) -> &LocaleMeta {
        &self.meta
    }

    /// Normalised lowercase language code, e.g. `"cs"`.
    pub fn code(&self) -> &str {
        &self.meta.code
    }

    /// Interface direction of this language.
    pub fn direction(&self) -> Direction {
        self.meta.direction
    }

    /// Translated string for `key`, if this pack has one.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(String::as_str)
    }

    /// Every key in the pack, sorted alphabetically.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.strings.keys()
    }

    /// Every `key = value` pair, sorted by key.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.strings.iter()
    }

    /// Number of translated keys.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// True when the pack has no strings at all.
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// True when this pack translated `key`.
    pub fn contains(&self, key: &str) -> bool {
        self.strings.contains_key(key)
    }
}

/// Resolve the code of a pack: `[meta] code`, else the hint, else an error.
fn declared_code(meta_table: &toml::Table, hint: &str) -> I18nResult<String> {
    let declared = match meta_table.get("code") {
        None => None,
        Some(toml::Value::String(code)) => Some(code.as_str()),
        Some(other) => {
            return Err(I18nError::invalid(
                label(hint),
                format!("meta.code must be a string, found {}", type_name(other)),
            ));
        }
    };
    let code = match declared {
        Some(code) => normalize_code(code),
        None => hint.to_string(),
    };
    if code.is_empty() {
        return Err(I18nError::invalid(
            label(hint),
            "missing language code: add `code = \"cs\"` to [meta], or name the file cs.toml",
        ));
    }
    Ok(code)
}

/// Read an optional string field of `[meta]`, trimmed.
fn opt_string(table: &toml::Table, key: &str, code: &str) -> I18nResult<Option<String>> {
    match table.get(key) {
        None => Ok(None),
        Some(toml::Value::String(value)) => Ok(Some(value.trim().to_string())),
        Some(other) => Err(I18nError::invalid(
            code,
            format!("meta.{key} must be a string, found {}", type_name(other)),
        )),
    }
}

/// Drop an empty string so that a chain of fallbacks can take over.
fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

/// Normalise a language code: trimmed, lowercased, `_` folded to `-`.
///
/// A trailing `.toml` is tolerated because callers often pass a file name where
/// a code is expected.
pub(crate) fn normalize_code(raw: &str) -> String {
    let trimmed = raw.trim();
    let bytes = trimmed.as_bytes();
    let stem = if bytes.len() > 5 && bytes[bytes.len() - 5..].eq_ignore_ascii_case(b".toml") {
        &trimmed[..bytes.len() - 5]
    } else {
        trimmed
    };
    stem.to_lowercase().replace('_', "-")
}

/// Validate the syntax of a key inside `[strings]`.
///
/// Keys are lowercase ASCII words joined with dots, e.g. `dialog.copy.title`.
/// Digits and underscores are allowed; uppercase, spaces, dashes and empty
/// segments are not.
pub(crate) fn check_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("the key is empty".to_string());
    }
    if !key.is_ascii() {
        return Err("keys must be plain ASCII".to_string());
    }
    let is_word_char =
        |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.';
    if let Some(bad) = key.chars().find(|c| !is_word_char(*c)) {
        let lowered = key.to_ascii_lowercase();
        if lowered.chars().all(is_word_char) {
            return Err(format!("keys must be lowercase ASCII; write {lowered:?}"));
        }
        return Err(format!(
            "unexpected character {bad:?}: keys are lowercase words joined with dots"
        ));
    }
    if key.starts_with('.') || key.ends_with('.') || key.contains("..") {
        return Err("the key contains an empty part between dots".to_string());
    }
    Ok(())
}

/// Name of a TOML value type, for error messages.
fn type_name(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::String(_) => "a string",
        toml::Value::Integer(_) => "an integer",
        toml::Value::Float(_) => "a float",
        toml::Value::Boolean(_) => "a boolean",
        toml::Value::Datetime(_) => "a date/time",
        toml::Value::Array(_) => "an array",
        toml::Value::Table(_) => "a table",
    }
}

/// Label used in errors when the pack cannot be identified yet.
fn label(code: &str) -> String {
    if code.trim().is_empty() {
        "<unknown>".to_string()
    } else {
        code.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a pack that is expected to be valid.
    fn parse_ok(hint: &str, text: &str) -> Locale {
        match Locale::parse(hint, text) {
            Ok(locale) => locale,
            Err(error) => panic!("pack must parse: {error}"),
        }
    }

    /// Parse a pack that is expected to be rejected, returning the error text.
    fn parse_err(hint: &str, text: &str) -> String {
        match Locale::parse(hint, text) {
            Ok(locale) => panic!("pack must be rejected, got code {:?}", locale.code()),
            Err(error) => error.to_string(),
        }
    }

    const MINIMAL: &str = r#"
[meta]
code = "cs"

[strings]
"menu.file" = "Soubor"
"#;

    #[test]
    fn parses_a_minimal_pack() {
        let locale = parse_ok("ignored", MINIMAL);
        assert_eq!(locale.code(), "cs");
        assert_eq!(locale.get("menu.file"), Some("Soubor"));
        assert_eq!(locale.len(), 1);
        assert_eq!(locale.meta().string_count, 1);
        // name and native_name default to the code, version defaults to 1
        assert_eq!(locale.meta().name, "cs");
        assert_eq!(locale.meta().native_name, "cs");
        assert_eq!(locale.meta().version, 1);
        assert!(locale.meta().authors.is_empty());
        assert_eq!(locale.direction(), Direction::Ltr);
    }

    #[test]
    fn parses_a_full_pack() {
        let text = r#"
[meta]
code = "CS_cz"
name = "Czech"
native_name = "Čeština"
direction = "LTR"
version = 3
authors = ["Somebody", "  Someone else  "]

[strings]
"app.title" = "RCommander"
"dialog.copy.title" = "Kopírovat"
"status.selected" = "{count} vybráno z {total}"
"#;
        let locale = parse_ok("", text);
        // code is normalised, direction tag is case-insensitive, authors trimmed
        assert_eq!(locale.code(), "cs-cz");
        assert_eq!(locale.meta().name, "Czech");
        assert_eq!(locale.meta().native_name, "Čeština");
        assert_eq!(locale.meta().direction, Direction::Ltr);
        assert_eq!(locale.meta().version, 3);
        assert_eq!(locale.meta().authors, vec!["Somebody", "Someone else"]);
        assert_eq!(locale.len(), 3);
        assert_eq!(locale.meta().selector_label(), "Čeština (Czech)");
    }

    #[test]
    fn missing_meta_defaults_to_the_file_stem_hint() {
        let text = r#"
[strings]
"menu.file" = "Datei"
"#;
        let locale = parse_ok("de.toml", text);
        assert_eq!(locale.code(), "de");
        assert_eq!(locale.meta().name, "de");
        assert_eq!(locale.meta().native_name, "de");
        assert_eq!(locale.meta().version, 1);
        assert_eq!(locale.get("menu.file"), Some("Datei"));
    }

    #[test]
    fn hint_is_normalised_and_lowercase_wins_over_hint() {
        // a declared code always beats the hint
        assert_eq!(parse_ok("xx", MINIMAL).code(), "cs");
        // hint normalisation: blanks, case, underscore, file extension
        let body = "[strings]\n\"a.b\" = \"x\"\n";
        assert_eq!(parse_ok("  DE_de ", body).code(), "de-de");
        assert_eq!(parse_ok("pt_BR.TOML", body).code(), "pt-br");
        // auto-detected direction follows the normalised code
        assert_eq!(parse_ok("AR", body).direction(), Direction::Rtl);
    }

    #[test]
    fn code_is_required_from_hint_or_meta() {
        let message = parse_err("   ", "[strings]\n\"a.b\" = \"x\"\n");
        assert!(
            message.contains("missing language code"),
            "message: {message}"
        );
        assert!(parse_err("", "").contains("missing language code"));
    }

    #[test]
    fn empty_strings_table_is_allowed_and_empty() {
        let locale = parse_ok("cs", "[meta]\ncode = \"cs\"\n\n[strings]\n");
        assert!(locale.is_empty());
        assert_eq!(locale.len(), 0);
        assert_eq!(locale.meta().string_count, 0);
        assert_eq!(locale.keys().count(), 0);
        assert_eq!(locale.get("menu.file"), None);
        assert!(!locale.contains("menu.file"));
        // a pack with no [strings] section at all behaves the same
        let bare = parse_ok("cs", "[meta]\ncode = \"cs\"\n");
        assert!(bare.is_empty());
    }

    #[test]
    fn invalid_toml_is_reported_with_position() {
        let message = parse_err("cs", "[meta]\ncode = \"cs\n");
        assert!(message.contains("not valid TOML"), "message: {message}");
        // duplicate keys are a TOML error too
        let dup = parse_err(
            "cs",
            "[strings]\n\"menu.file\" = \"a\"\n\"menu.file\" = \"b\"\n",
        );
        assert!(dup.contains("not valid TOML"), "message: {dup}");
    }

    #[test]
    fn byte_order_mark_is_tolerated() {
        let locale = parse_ok("cs", "\u{feff}[meta]\ncode = \"cs\"\n");
        assert_eq!(locale.code(), "cs");
    }

    #[test]
    fn unknown_section_is_rejected() {
        let message = parse_err("cs", "[meta]\ncode = \"cs\"\n\n[string]\n\"a.b\" = \"x\"\n");
        assert!(message.contains("unknown section"), "message: {message}");
        assert!(message.contains("string"), "message: {message}");
    }

    #[test]
    fn sections_must_be_tables() {
        assert!(parse_err("cs", "strings = 5\n[meta]\ncode = \"cs\"\n").contains("[strings]"));
        assert!(parse_err("cs", "meta = 1\n").contains("[meta]"));
    }

    #[test]
    fn invalid_keys_are_rejected_with_a_suggestion() {
        let uppercase = parse_err("cs", "[strings]\n\"Menu.File\" = \"x\"\n");
        assert!(uppercase.contains("invalid key"), "message: {uppercase}");
        assert!(uppercase.contains("menu.file"), "message: {uppercase}");
        for bad_key in [
            "Menu.File",
            "menu file",
            "menu-file",
            ".hidden",
            "trailing.",
            "a..b",
            "ménu.file",
        ] {
            let message = parse_err("cs", &format!("[strings]\n\"{bad_key}\" = \"x\"\n"));
            assert!(
                message.contains("invalid key"),
                "key {bad_key:?}: {message}"
            );
        }
        // an empty key is not allowed either
        let empty = parse_err("cs", "[strings]\n\"\" = \"x\"\n");
        assert!(empty.contains("invalid key"), "message: {empty}");
    }

    #[test]
    fn underscore_and_digit_keys_are_accepted() {
        let text = "[strings]\n\"dialog.search.in_files\" = \"Hledat v souborech\"\n\"cmd_deselect_all\" = \"Odznačit vše\"\n\"col.size2\" = \"Velikost\"\n";
        let locale = parse_ok("cs", text);
        assert_eq!(locale.len(), 3);
        assert!(locale.contains("cmd_deselect_all"));
    }

    #[test]
    fn non_string_and_empty_values_are_rejected() {
        assert!(parse_err("cs", "[strings]\n\"a.b\" = 5\n").contains("must be a string"));
        assert!(parse_err("cs", "[strings]\n\"a.b\" = \"\"\n").contains("empty"));
        assert!(parse_err("cs", "[strings]\n\"a.b\" = \"   \"\n").contains("empty"));
        // nested tables are not translations either
        assert!(parse_err("cs", "[strings.a]\nb = \"x\"\n").contains("must be a string"));
    }

    #[test]
    fn values_are_trimmed() {
        let locale = parse_ok("cs", "[strings]\n\"a.b\" = \"  Soubor  \"\n");
        assert_eq!(locale.get("a.b"), Some("Soubor"));
    }

    #[test]
    fn comments_and_multiline_strings_work() {
        let text = r#"
# the whole file may be commented
[meta]
code = "cs"   # trailing comment

[strings]
"menu.file" = "Soubor"   # inline comment
"dialog.help.contents" = """
Obsah
"""
"#;
        let locale = parse_ok("", text);
        assert_eq!(locale.get("menu.file"), Some("Soubor"));
        assert_eq!(locale.get("dialog.help.contents"), Some("Obsah"));
        assert_eq!(locale.meta().version, 1);
    }

    #[test]
    fn direction_falls_back_to_the_code_and_can_be_overridden() {
        assert_eq!(
            parse_ok("", "[meta]\ncode = \"he\"\n").direction(),
            Direction::Rtl
        );
        assert_eq!(
            parse_ok("", "[meta]\ncode = \"fa-IR\"\n").direction(),
            Direction::Rtl
        );
        assert_eq!(
            parse_ok("", "[meta]\ncode = \"cs\"\n").direction(),
            Direction::Ltr
        );
        // explicit tag wins, even when it contradicts the code
        assert_eq!(
            parse_ok("", "[meta]\ncode = \"en\"\ndirection = \"rtl\"\n").direction(),
            Direction::Rtl
        );
        let bad = parse_err("cs", "[meta]\ncode = \"cs\"\ndirection = \"up\"\n");
        assert!(bad.contains("direction"), "message: {bad}");
        assert!(parse_err("cs", "[meta]\ncode = \"cs\"\ndirection = 7\n").contains("direction"));
    }

    #[test]
    fn version_bounds_are_checked() {
        assert_eq!(
            parse_ok("cs", "[meta]\ncode = \"cs\"\nversion = 0\n")
                .meta()
                .version,
            0
        );
        assert_eq!(
            parse_ok("cs", "[meta]\ncode = \"cs\"\nversion = 4294967295\n")
                .meta()
                .version,
            u32::MAX
        );
        assert!(parse_err("cs", "[meta]\ncode = \"cs\"\nversion = -1\n").contains("version"));
        assert!(
            parse_err("cs", "[meta]\ncode = \"cs\"\nversion = 4294967296\n").contains("version")
        );
        assert!(parse_err("cs", "[meta]\ncode = \"cs\"\nversion = \"1\"\n").contains("version"));
    }

    #[test]
    fn name_falls_back_to_the_other_name_and_then_to_the_code() {
        let native_only = parse_ok("cs", "[meta]\ncode = \"cs\"\nnative_name = \"Čeština\"\n");
        assert_eq!(native_only.meta().name, "Čeština");
        assert_eq!(native_only.meta().native_name, "Čeština");
        assert_eq!(native_only.meta().selector_label(), "Čeština");

        let name_only = parse_ok("cs", "[meta]\ncode = \"cs\"\nname = \"Czech\"\n");
        assert_eq!(name_only.meta().name, "Czech");
        assert_eq!(name_only.meta().native_name, "Czech");

        // blank values are ignored, not stored as empty strings
        let blank = parse_ok("cs", "[meta]\ncode = \"cs\"\nname = \"   \"\n");
        assert_eq!(blank.meta().name, "cs");
    }

    #[test]
    fn authors_are_validated() {
        let message = parse_err("cs", "[meta]\ncode = \"cs\"\nauthors = [1]\n");
        assert!(message.contains("authors[0]"), "message: {message}");
        let message = parse_err("cs", "[meta]\ncode = \"cs\"\nauthors = \"me\"\n");
        assert!(message.contains("authors"), "message: {message}");
    }

    #[test]
    fn unknown_meta_keys_are_ignored_for_forward_compatibility() {
        let locale = parse_ok("cs", "[meta]\ncode = \"cs\"\nhomepage = \"https://x\"\n");
        assert_eq!(locale.code(), "cs");
    }

    #[test]
    fn keys_are_sorted_and_match_the_reported_count() {
        let text = "[strings]\n\"c.d\" = \"3\"\n\"a.b\" = \"1\"\n\"b.c\" = \"2\"\n";
        let locale = parse_ok("cs", text);
        let keys: Vec<&str> = locale.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["a.b", "b.c", "c.d"]);
        assert_eq!(locale.meta().string_count, keys.len());
        let pairs: Vec<(&str, &str)> = locale
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(pairs, vec![("a.b", "1"), ("b.c", "2"), ("c.d", "3")]);
    }

    #[test]
    fn load_uses_the_file_stem_as_code() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("cs.toml");
        std::fs::write(&path, "[strings]\n\"menu.file\" = \"Soubor\"\n").expect("write pack");
        let locale = Locale::load(&path).expect("pack loads");
        assert_eq!(locale.code(), "cs");
        assert_eq!(locale.get("menu.file"), Some("Soubor"));
    }

    #[test]
    fn load_reports_missing_files_and_bad_encoding() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("nope.toml");
        let error = Locale::load(&missing).expect_err("missing file must fail");
        assert!(error.is_io_error(), "error: {error}");
        assert!(error.to_string().contains("nope.toml"), "error: {error}");

        let binary = dir.path().join("bin.toml");
        std::fs::write(&binary, [0xffu8, 0xfe, 0x00]).expect("write binary");
        let error = Locale::load(&binary).expect_err("non-utf8 must fail");
        assert!(error.is_io_error(), "error: {error}");
    }

    #[test]
    fn empty_locale_helper_is_empty_and_never_panics() {
        let locale = Locale::empty("HE");
        assert_eq!(locale.code(), "he");
        assert_eq!(locale.direction(), Direction::Rtl);
        assert!(locale.is_empty());
        assert_eq!(locale.get("anything"), None);
    }
}

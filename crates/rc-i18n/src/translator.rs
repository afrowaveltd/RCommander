//! The translator: a fallback locale, a set of loaded packs and the current
//! language.
//!
//! Lookup order for every key is *current language → fallback language → the
//! key itself*, so the interface can never show an empty label:
//!
//! ```
//! use rc_i18n::{Locale, Translator, english};
//!
//! let czech = Locale::parse("cs", r#"
//! [meta]
//! code = "cs"
//! native_name = "Čeština"
//!
//! [strings]
//! "cmd.copy" = "Kopírovat"
//! "#).expect("valid pack");
//!
//! let mut translator = Translator::new(english(), vec![czech]);
//! assert!(translator.set_language("cs"));
//! assert_eq!(translator.t("cmd.copy"), "Kopírovat");
//! // not translated yet — the English fallback answers
//! assert_eq!(translator.t("cmd.quit"), "Quit");
//! // unknown everywhere — the key is shown so the gap is obvious
//! assert_eq!(translator.t("no.such.key"), "no.such.key");
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::direction::{Direction, base_code};
use crate::error::{I18nError, I18nResult};
use crate::locale::{Locale, LocaleMeta, normalize_code};

/// The built-in English pack, compiled into the binary.
///
/// It is always available, which is what makes the fallback chain total.
pub const BUILTIN_EN: &str = include_str!("../locales/en.toml");

/// The built-in English pack as a [`Locale`].
///
/// Parsing cannot fail for the compiled-in pack — the crate's tests assert it —
/// but if the file were ever damaged this degrades to an empty English locale
/// instead of panicking, and every lookup then falls through to the key.
pub fn english() -> Locale {
    match Locale::parse("en", BUILTIN_EN) {
        Ok(locale) => locale,
        Err(_) => Locale::empty("en"),
    }
}

/// Substitutes `{name}` placeholders in `template`.
///
/// * a placeholder whose name is in `args` is replaced by the value;
/// * `{0}`, `{1}`, … are replaced by the value at that position in `args`;
/// * an unknown placeholder is left verbatim, so a half-translated pack shows
///   `{count}` rather than swallowing it;
/// * text without placeholders is returned unchanged;
/// * values are inserted once and never re-scanned, so `args` values may
///   safely contain braces themselves.
///
/// Blanks inside the braces are ignored for the lookup but preserved for an
/// unknown placeholder.
///
/// ```
/// use rc_i18n::substitute;
/// let args = [("count", "3"), ("total", "7")];
/// assert_eq!(substitute("{count} of {total}", &args), "3 of 7");
/// assert_eq!(substitute("{count} {count} {nope}", &args), "3 3 {nope}");
/// assert_eq!(substitute("plain", &args), "plain");
/// ```
pub fn substitute(template: &str, args: &[(&str, &str)]) -> String {
    // Nothing to do for the overwhelming majority of strings.
    if !template.contains('{') {
        return template.to_string();
    }
    let mut out = String::with_capacity(template.len() + 8);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            // A `{` inside the braces means the first one was literal.
            Some(close) if !after[..close].contains('{') => {
                let name = &after[..close];
                match lookup(name.trim(), args) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            // Unterminated placeholder: emit the brace, keep scanning.
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Resolve one placeholder: by name first, then by numeric index.
fn lookup<'a>(name: &str, args: &[(&str, &'a str)]) -> Option<&'a str> {
    if let Some((_, value)) = args.iter().find(|(key, _)| *key == name) {
        return Some(value);
    }
    let index: usize = name.parse().ok()?;
    args.get(index).map(|(_, value)| *value)
}

/// Fallback locale plus every loaded language pack.
///
/// Construct one at start-up with [`Translator::load_dir`] and
/// [`Translator::new`], then hand out `&Translator` to the UI.
#[derive(Debug, Clone)]
pub struct Translator {
    /// Code of the language currently shown.
    current: String,
    /// Language used when the current one lacks a key — English in practice.
    fallback: Locale,
    /// Every known pack, keyed by normalised code.
    locales: BTreeMap<String, Locale>,
}

impl Translator {
    /// Build a translator with English (built-in) as fallback plus the given locales.
    ///
    /// The current language starts as the fallback's language. A pack whose code
    /// equals the fallback code is kept as an ordinary locale: it then overrides
    /// individual fallback strings while still falling back for the rest.
    pub fn new(fallback: Locale, locales: Vec<Locale>) -> Self {
        let current = fallback.code().to_string();
        let mut translator = Translator {
            current,
            fallback,
            locales: BTreeMap::new(),
        };
        for locale in locales {
            translator.insert(locale);
        }
        translator
    }

    /// A translator with the built-in English pack and no other languages.
    pub fn from_builtin() -> Self {
        Translator::new(english(), Vec::new())
    }

    /// Loads every `*.toml` in `dir` (missing dir is not an error: returns an
    /// empty list) and returns the discovered locales sorted by code.
    ///
    /// A fresh installation simply has no packs yet, so a missing directory is
    /// `Ok(vec![])`. Subdirectories, hidden files and files whose extension is
    /// not `.toml` are ignored; if two packs claim the same code the
    /// alphabetically first file wins.
    ///
    /// # Errors
    ///
    /// [`I18nError::Io`] when the directory exists but cannot be read, and any
    /// error from [`Locale::load`] — one broken pack fails the whole load so the
    /// user learns about it instead of silently getting a partial language list.
    /// The GUI can check a file with [`validate_pack`](crate::validate_pack)
    /// first and offer to skip it.
    pub fn load_dir(dir: &Path) -> I18nResult<Vec<Locale>> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(I18nError::io(dir, error)),
        };

        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| I18nError::io(dir, error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| I18nError::io(entry.path(), error))?;
            if !file_type.is_file() || !is_pack_file(&entry.path()) {
                continue;
            }
            paths.push(entry.path());
        }
        paths.sort();

        let mut seen = BTreeSet::new();
        let mut locales = Vec::with_capacity(paths.len());
        for path in paths {
            let locale = Locale::load(&path)?;
            if seen.insert(locale.code().to_string()) {
                locales.push(locale);
            }
        }
        locales.sort_by(|a, b| a.code().cmp(b.code()));
        Ok(locales)
    }

    /// Convenience wrapper around [`Translator::load_dir`] that builds the
    /// translator directly, using the built-in English pack as fallback.
    pub fn discover(dir: &Path) -> I18nResult<Translator> {
        Ok(Translator::new(english(), Translator::load_dir(dir)?))
    }

    /// Add or replace a locale (used when the user drops in a new file).
    ///
    /// Replacing a pack updates every lookup immediately; the current language
    /// is kept.
    pub fn insert(&mut self, locale: Locale) {
        let code = locale.code().to_string();
        self.locales.insert(code, locale);
    }

    /// Drop a locale, e.g. when the user deletes a pack file.
    ///
    /// Returns `false` when the code was unknown. If the removed language was
    /// the current one, the translator switches back to the fallback language.
    pub fn remove(&mut self, code: &str) -> bool {
        let Some(code) = self.resolve_code(code).map(str::to_string) else {
            return false;
        };
        let removed = self.locales.remove(&code).is_some();
        if removed && self.current == code {
            self.current = self.fallback.code().to_string();
        }
        removed
    }

    /// Every known language, sorted by code, for the language selector.
    ///
    /// The fallback language is always part of the list, even when no `en.toml`
    /// was dropped in, because English is always usable. A dropped-in pack with
    /// the fallback's code replaces the built-in entry in this list.
    pub fn available(&self) -> Vec<&LocaleMeta> {
        let mut metas: Vec<&LocaleMeta> = self.locales.values().map(Locale::meta).collect();
        if !self.locales.contains_key(self.fallback.code()) {
            metas.push(self.fallback.meta());
        }
        metas.sort_by(|a, b| a.code.cmp(&b.code));
        metas
    }

    /// Switch the current language.
    ///
    /// The code is normalised first (case, `_` vs `-`), and a regional code
    /// falls back to its base language, so `"cs_CZ"` selects the `cs` pack.
    /// Returns `false` and keeps the current language when nothing matches.
    pub fn set_language(&mut self, code: &str) -> bool {
        match self.resolve_code(code).map(str::to_string) {
            Some(resolved) => {
                self.current = resolved;
                true
            }
            None => false,
        }
    }

    /// Code of the language currently shown.
    pub fn language(&self) -> &str {
        &self.current
    }

    /// Interface direction of the current language.
    pub fn direction(&self) -> Direction {
        self.current_locale().direction()
    }

    /// Translated string: current language → fallback language → `key` itself.
    ///
    /// The result is never empty for a key that exists, and a key that exists
    /// nowhere comes back unchanged, which makes missing translations visible
    /// in the UI without a special "missing" placeholder.
    ///
    /// The returned slice borrows from the translator *or* from `key` — the key
    /// is one of the possible answers — so both borrows share one lifetime.
    /// UI keys are normally `&'static str`, which always satisfies that.
    ///
    /// ```
    /// use rc_i18n::Translator;
    /// let translator = Translator::from_builtin();
    /// const KEY: &str = "cmd.copy";
    /// assert_eq!(translator.t(KEY), "Copy");
    /// ```
    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        if let Some(value) = self.current_locale().get(key) {
            return value;
        }
        if let Some(value) = self.fallback.get(key) {
            return value;
        }
        key
    }

    /// Translated string with `{name}` placeholders filled in.
    ///
    /// Unknown placeholders stay verbatim; see [`substitute`] for the details.
    ///
    /// ```
    /// use rc_i18n::Translator;
    /// let translator = Translator::from_builtin();
    /// assert_eq!(
    ///     translator.t_args("panel.selection_of", &[("count", "3"), ("total", "7")]),
    ///     "3 of 7 selected"
    /// );
    /// ```
    pub fn t_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        substitute(self.t(key), args)
    }

    /// True when the current or the fallback language can translate `key`.
    pub fn has(&self, key: &str) -> bool {
        self.current_locale().contains(key) || self.fallback.contains(key)
    }

    /// Keys present in the fallback but missing in `code` (for a "translation
    /// completeness" report in the UI).
    ///
    /// Sorted alphabetically because the pack iterates in key order. An unknown
    /// code reports nothing missing — use [`Translator::locale`] to tell an
    /// unknown language from a complete one.
    pub fn missing_keys(&self, code: &str) -> Vec<String> {
        let Some(locale) = self.locale(code) else {
            return Vec::new();
        };
        self.fallback
            .keys()
            .filter(|key| !locale.contains(key))
            .cloned()
            .collect()
    }

    /// Number of fallback keys the pack does not translate yet.
    ///
    /// The count behind [`Translator::completeness`], for a "… of 155 strings
    /// translated" line in the language settings.
    pub fn untranslated(&self, code: &str) -> usize {
        self.missing_keys(code).len()
    }

    /// Keys the pack translates but the fallback does not know, e.g. after a
    /// key was renamed in English. Sorted alphabetically.
    pub fn extra_keys(&self, code: &str) -> Vec<String> {
        let Some(locale) = self.locale(code) else {
            return Vec::new();
        };
        locale
            .keys()
            .filter(|key| !self.fallback.contains(key))
            .cloned()
            .collect()
    }

    /// Share of fallback keys the pack translates, `1.0` when complete.
    ///
    /// `None` for an unknown language.
    pub fn completeness(&self, code: &str) -> Option<f32> {
        self.locale(code)?;
        let total = self.fallback.len();
        if total == 0 {
            return Some(1.0);
        }
        let missing = self.missing_keys(code).len();
        Some((total - missing.min(total)) as f32 / total as f32)
    }

    /// The pack for `code`, if any.
    ///
    /// Resolution matches [`Translator::set_language`]: normalised code first,
    /// then the base language, then the fallback language.
    pub fn locale(&self, code: &str) -> Option<&Locale> {
        match self.resolve_code(code) {
            Some(code) if code == self.fallback.code() => {
                Some(self.locales.get(code).unwrap_or(&self.fallback))
            }
            Some(code) => self.locales.get(code),
            None => None,
        }
    }

    /// The fallback language pack (English by default).
    pub fn fallback(&self) -> &Locale {
        &self.fallback
    }

    /// The pack of the current language, falling back to the fallback pack.
    pub fn current_locale(&self) -> &Locale {
        self.locales.get(&self.current).unwrap_or(&self.fallback)
    }

    /// Map a user supplied code onto a code this translator actually knows.
    fn resolve_code(&self, code: &str) -> Option<&str> {
        let normalized = normalize_code(code);
        if normalized.is_empty() {
            return None;
        }
        if let Some((key, _)) = self.locales.get_key_value(&normalized) {
            return Some(key);
        }
        if normalized == self.fallback.code() {
            return Some(self.fallback.code());
        }
        let base = base_code(&normalized).to_ascii_lowercase();
        if base != normalized {
            if let Some((key, _)) = self.locales.get_key_value(&base) {
                return Some(key);
            }
            if base == self.fallback.code() {
                return Some(self.fallback.code());
            }
        }
        None
    }
}

/// True for a file this crate would try to load as a pack.
fn is_pack_file(path: &Path) -> bool {
    let is_hidden = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'));
    let is_toml = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"));
    !is_hidden && is_toml
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled Czech pack, used as the second language in most tests.
    const BUILTIN_CS: &str = include_str!("../locales/cs.toml");

    /// A tiny pack with just the keys a test needs.
    fn pack(code: &str, strings: &str) -> Locale {
        let text = format!("[meta]\ncode = \"{code}\"\n\n[strings]\n{strings}\n");
        match Locale::parse(code, &text) {
            Ok(locale) => locale,
            Err(error) => panic!("test pack {code} must parse: {error}"),
        }
    }

    fn czech() -> Locale {
        match Locale::parse("cs", BUILTIN_CS) {
            Ok(locale) => locale,
            Err(error) => panic!("bundled Czech pack must parse: {error}"),
        }
    }

    #[test]
    fn built_in_english_is_complete_and_usable() {
        let locale = english();
        assert_eq!(locale.code(), "en");
        assert_eq!(locale.direction(), Direction::Ltr);
        assert!(locale.len() >= 120, "only {} keys", locale.len());
        assert!(locale.iter().all(|(_, value)| !value.trim().is_empty()));
        assert_eq!(locale.get("cmd.copy"), Some("Copy"));
        assert_eq!(locale.get("app.title"), Some("RCommander"));
    }

    #[test]
    fn bundled_packs_have_identical_key_sets() {
        let en = english();
        let cs = czech();
        assert_eq!(cs.code(), "cs");
        assert_eq!(cs.meta().native_name, "Čeština");
        assert_eq!(cs.meta().name, "Czech");
        assert_eq!(cs.direction(), Direction::Ltr);
        let translator = Translator::new(en.clone(), vec![cs.clone()]);
        assert!(
            translator.missing_keys("cs").is_empty(),
            "missing: {:?}",
            translator.missing_keys("cs")
        );
        assert!(
            translator.extra_keys("cs").is_empty(),
            "extra: {:?}",
            translator.extra_keys("cs")
        );
        assert_eq!(translator.completeness("cs"), Some(1.0));
        // every required GUI key is translated, not empty, in both packs
        for key in [
            "app.title",
            "cmd.copy",
            "dialog.search.title",
            "status.ready",
            "error.unknown",
        ] {
            let english_value = en.get(key).unwrap_or_else(|| panic!("en lacks {key}"));
            let czech_value = cs.get(key).unwrap_or_else(|| panic!("cs lacks {key}"));
            assert!(!english_value.trim().is_empty());
            assert!(!czech_value.trim().is_empty());
        }
        assert_eq!(cs.len(), en.len());
    }

    #[test]
    fn fallback_chain_reaches_english_and_then_the_key() {
        let english = english();
        let partial = pack("cs", "\"cmd.copy\" = \"Kopírovat\"");
        let mut translator = Translator::new(english, vec![partial]);
        assert!(translator.set_language("cs"));
        // from the current language
        assert_eq!(translator.t("cmd.copy"), "Kopírovat");
        // not in Czech — English answers
        assert_eq!(translator.t("cmd.quit"), "Quit");
        // nowhere — the key itself so the gap is visible
        assert_eq!(translator.t("no.such.key"), "no.such.key");
        assert!(translator.has("cmd.copy"));
        assert!(translator.has("cmd.quit"));
        assert!(!translator.has("no.such.key"));
    }

    #[test]
    fn switching_languages_changes_lookups() {
        let mut translator = Translator::new(english(), vec![czech()]);
        assert_eq!(translator.language(), "en");
        assert_eq!(translator.t("cmd.copy"), "Copy");
        assert!(translator.set_language("cs"));
        assert_eq!(translator.language(), "cs");
        assert_eq!(translator.t("cmd.copy"), "Kopírovat");
        // case, blanks and regional codes are normalised
        assert!(translator.set_language("  EN  "));
        assert_eq!(translator.language(), "en");
        assert_eq!(translator.t("cmd.copy"), "Copy");
        assert!(translator.set_language("cs-CZ"));
        assert_eq!(translator.language(), "cs");
        assert!(translator.set_language("CS_cz"));
        assert_eq!(translator.language(), "cs");
    }

    #[test]
    fn unknown_language_is_refused_and_kept() {
        let mut translator = Translator::new(english(), vec![czech()]);
        assert!(translator.set_language("cs"));
        for code in ["xx", "", "   ", "kl"] {
            assert!(
                !translator.set_language(code),
                "code {code:?} must be refused"
            );
            assert_eq!(translator.language(), "cs", "language must not change");
        }
        assert_eq!(translator.t("cmd.copy"), "Kopírovat");
    }

    #[test]
    fn direction_follows_the_current_language() {
        let arabic = pack("ar", "\"cmd.copy\" = \"نسخ\"");
        let mut translator = Translator::new(english(), vec![arabic, czech()]);
        assert_eq!(translator.direction(), Direction::Ltr);
        assert!(translator.set_language("ar"));
        assert_eq!(translator.direction(), Direction::Rtl);
        assert!(translator.direction().is_rtl());
        assert!(translator.set_language("cs"));
        assert_eq!(translator.direction(), Direction::Ltr);
    }

    #[test]
    fn available_lists_the_fallback_and_is_sorted() {
        let translator = Translator::new(
            english(),
            vec![pack("sk", "\"cmd.copy\" = \"Kopírovať\""), czech()],
        );
        let codes: Vec<&str> = translator
            .available()
            .iter()
            .map(|meta| meta.code.as_str())
            .collect();
        assert_eq!(codes, vec!["cs", "en", "sk"]);
        // a dropped-in en.toml replaces the built-in entry, it does not duplicate
        let custom_en = pack("en", "\"cmd.copy\" = \"Duplicate\"");
        let translator = Translator::new(english(), vec![custom_en]);
        let codes: Vec<&str> = translator
            .available()
            .iter()
            .map(|meta| meta.code.as_str())
            .collect();
        assert_eq!(codes, vec!["en"]);
    }

    #[test]
    fn insert_replaces_and_remove_deletes() {
        let mut translator = Translator::new(english(), vec![czech()]);
        assert!(translator.set_language("cs"));
        assert_eq!(translator.t("cmd.copy"), "Kopírovat");
        // a re-loaded pack with a new wording wins immediately
        translator.insert(pack("cs", "\"cmd.copy\" = \"Duplikovat\""));
        assert_eq!(translator.t("cmd.copy"), "Duplikovat");
        assert_eq!(translator.language(), "cs");
        // removing the current language falls back to English
        assert!(translator.remove("cs"));
        assert_eq!(translator.language(), "en");
        assert_eq!(translator.t("cmd.copy"), "Copy");
        assert!(!translator.remove("cs"));
        assert!(!translator.remove("xx"));
        // removing a non-current language keeps the current one
        translator.insert(czech());
        assert!(translator.set_language("cs"));
        assert!(translator.remove("cs"));
        assert_eq!(translator.language(), "en");
    }

    #[test]
    fn placeholders_are_substituted_from_the_current_language() {
        let mut translator = Translator::new(english(), vec![czech()]);
        assert_eq!(
            translator.t_args("panel.selection_of", &[("count", "3"), ("total", "7")]),
            "3 of 7 selected"
        );
        assert!(translator.set_language("cs"));
        let czech_value =
            translator.t_args("panel.selection_of", &[("count", "3"), ("total", "7")]);
        assert!(
            czech_value.contains('3') && czech_value.contains('7'),
            "value: {czech_value}"
        );
        assert!(
            !czech_value.contains('{'),
            "placeholders must be filled: {czech_value}"
        );
    }

    #[test]
    fn substitution_handles_unknown_and_repeated_placeholders() {
        let args = [("count", "12")];
        assert_eq!(substitute("{count}", &args), "12");
        assert_eq!(substitute("{count} / {count}", &args), "12 / 12");
        // unknown stays verbatim, including its inner blanks
        assert_eq!(substitute("{other}", &args), "{other}");
        assert_eq!(substitute("{ other }", &args), "{ other }");
        assert_eq!(substitute("{count} {other}", &args), "12 {other}");
        // no placeholders at all
        assert_eq!(substitute("", &args), "");
        assert_eq!(substitute("plain text", &args), "plain text");
        assert_eq!(substitute("{count}", &[]), "{count}");
    }

    #[test]
    fn substitution_survives_malformed_templates() {
        // an unterminated brace is literal
        assert_eq!(substitute("a { b", &[("b", "X")]), "a { b");
        assert_eq!(substitute("{", &[("a", "X")]), "{");
        // an empty placeholder and nested braces stay as written
        assert_eq!(substitute("{}", &[("a", "X")]), "{}");
        assert_eq!(substitute("a {b {c}}", &[]), "a {b {c}}");
        // values may contain braces and are not re-scanned
        assert_eq!(substitute("{a}", &[("a", "{b}")]), "{b}");
        // numeric placeholders index into args
        assert_eq!(substitute("{0}-{1}", &[("a", "x"), ("b", "y")]), "x-y");
        assert_eq!(substitute("{2}", &[("a", "x")]), "{2}");
        // unicode values and templates
        assert_eq!(substitute("Čeština: {n}", &[("n", "ano")]), "Čeština: ano");
    }

    #[test]
    fn missing_keys_reports_the_real_difference() {
        let english = english();
        let partial = pack(
            "cs",
            "\"cmd.copy\" = \"Kopírovat\"\n\"cmd.move\" = \"Přesunout\"",
        );
        let translator = Translator::new(english.clone(), vec![partial]);
        let missing = translator.missing_keys("cs");
        assert!(!missing.is_empty());
        assert_eq!(missing.len(), english.len() - 2);
        assert!(!missing.contains(&"cmd.copy".to_string()));
        assert!(!missing.contains(&"cmd.move".to_string()));
        assert!(missing.contains(&"cmd.quit".to_string()));
        // sorted, because the fallback iterates in key order
        let mut sorted = missing.clone();
        sorted.sort();
        assert_eq!(missing, sorted);
        // a complete pack reports nothing
        assert!(translator.missing_keys("en").is_empty());
        // an unknown language reports nothing missing, it is simply not there
        assert!(translator.missing_keys("xx").is_empty());
        assert!(translator.locale("xx").is_none());
        assert!(translator.completeness("xx").is_none());
        assert!(
            translator
                .completeness("cs")
                .is_some_and(|value| value < 0.1)
        );
    }

    #[test]
    fn extra_keys_reports_keys_unknown_to_english() {
        let english = english();
        let stale = pack(
            "cs",
            "\"cmd.copy\" = \"Kopírovat\"\n\"cmd.removed_in_v2\" = \"Pryč\"",
        );
        let translator = Translator::new(english, vec![stale]);
        assert_eq!(
            translator.extra_keys("cs"),
            vec!["cmd.removed_in_v2".to_string()]
        );
        assert!(translator.extra_keys("en").is_empty());
        assert!(translator.extra_keys("xx").is_empty());
    }

    #[test]
    fn locale_lookup_accepts_regional_codes() {
        let translator = Translator::new(english(), vec![czech()]);
        assert_eq!(translator.locale("cs").map(Locale::code), Some("cs"));
        assert_eq!(translator.locale("cs-CZ").map(Locale::code), Some("cs"));
        assert_eq!(translator.locale("CS_cz ").map(Locale::code), Some("cs"));
        // the fallback answers for English even without a dropped-in en.toml
        assert_eq!(translator.locale("en").map(Locale::code), Some("en"));
        assert_eq!(translator.locale("en-GB").map(Locale::code), Some("en"));
        assert!(translator.locale("xx").is_none());
        assert_eq!(translator.fallback().code(), "en");
        assert_eq!(translator.current_locale().code(), "en");
    }

    #[test]
    fn a_pack_with_the_fallback_code_overrides_single_strings() {
        let custom_en = pack("en", "\"cmd.copy\" = \"Duplicate\"");
        let translator = Translator::new(english(), vec![custom_en]);
        assert_eq!(translator.t("cmd.copy"), "Duplicate");
        // the rest still comes from the built-in fallback
        assert_eq!(translator.t("cmd.quit"), "Quit");
        assert!(!translator.missing_keys("en").is_empty());
        assert!(
            translator
                .missing_keys("en")
                .contains(&"cmd.quit".to_string())
        );
    }

    #[test]
    fn from_builtin_starts_in_english() {
        let translator = Translator::from_builtin();
        assert_eq!(translator.language(), "en");
        assert_eq!(translator.t("menu.file"), "&File");
        assert_eq!(translator.direction(), Direction::Ltr);
        assert_eq!(translator.available().len(), 1);
        assert!(!Translator::from_builtin().set_language("cs"));
    }

    #[test]
    fn load_dir_reads_packs_sorted_by_code() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            dir.path().join("sk.toml"),
            "[meta]\ncode = \"sk\"\nnative_name = \"Slovenčina\"\n\n[strings]\n\"cmd.copy\" = \"Kopírovať\"\n",
        )
        .expect("write sk");
        std::fs::write(
            dir.path().join("cs.toml"),
            "[strings]\n\"cmd.copy\" = \"Kopírovat\"\n",
        )
        .expect("write cs");
        let locales = Translator::load_dir(dir.path()).expect("dir loads");
        let codes: Vec<&str> = locales.iter().map(Locale::code).collect();
        assert_eq!(codes, vec!["cs", "sk"]);
        // a pack that only states native_name uses it as its English name too
        assert_eq!(locales[1].meta().name, "Slovenčina");
        assert_eq!(locales[1].meta().native_name, "Slovenčina");
        assert_eq!(locales[0].get("cmd.copy"), Some("Kopírovat"));
    }

    #[test]
    fn load_dir_ignores_irrelevant_entries() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("cs.toml"), "[strings]\n\"a.b\" = \"x\"\n").expect("pack");
        std::fs::write(dir.path().join("notes.txt"), "not a pack").expect("txt");
        std::fs::write(dir.path().join(".hidden.toml"), "junk").expect("hidden");
        std::fs::create_dir(dir.path().join("nested.toml")).expect("dir named like a pack");
        std::fs::create_dir(dir.path().join("sub")).expect("subdir");
        std::fs::write(
            dir.path().join("sub/de.toml"),
            "[strings]\n\"a.b\" = \"y\"\n",
        )
        .expect("nested pack");
        let locales = Translator::load_dir(dir.path()).expect("dir loads");
        let codes: Vec<&str> = locales.iter().map(Locale::code).collect();
        assert_eq!(codes, vec!["cs"]);
    }

    #[test]
    fn load_dir_accepts_an_empty_and_a_missing_directory() {
        let empty = tempfile::tempdir().expect("temp dir");
        assert!(
            Translator::load_dir(empty.path())
                .expect("empty dir")
                .is_empty()
        );
        let missing = empty.path().join("does/not/exist");
        assert!(
            Translator::load_dir(&missing)
                .expect("missing dir is fine")
                .is_empty()
        );
        // and a translator built from a missing directory still works in English
        let mut translator = Translator::discover(&missing).expect("discover");
        assert_eq!(translator.language(), "en");
        assert_eq!(translator.t("cmd.copy"), "Copy");
        assert!(!translator.set_language("cs"));
    }

    #[test]
    fn load_dir_reports_a_broken_pack() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("cs.toml"), "[meta]\ncode = \"cs\n").expect("broken pack");
        let error = Translator::load_dir(dir.path()).expect_err("broken pack must fail");
        assert!(error.is_parse_error(), "error: {error}");
        std::fs::write(
            dir.path().join("cs.toml"),
            "[strings]\n\"Bad.Key\" = \"x\"\n",
        )
        .expect("bad key");
        let error = Translator::load_dir(dir.path()).expect_err("bad key must fail");
        assert!(error.to_string().contains("Bad.Key"), "error: {error}");
    }

    #[test]
    fn load_dir_prefers_the_first_file_for_a_duplicate_code() {
        let dir = tempfile::tempdir().expect("temp dir");
        // both files claim code "cs"; a.toml sorts first and wins
        std::fs::write(
            dir.path().join("a.toml"),
            "[meta]\ncode = \"cs\"\n\n[strings]\n\"a.b\" = \"first\"\n",
        )
        .expect("write a");
        std::fs::write(
            dir.path().join("b.toml"),
            "[meta]\ncode = \"cs\"\n\n[strings]\n\"a.b\" = \"second\"\n",
        )
        .expect("write b");
        let locales = Translator::load_dir(dir.path()).expect("dir loads");
        assert_eq!(locales.len(), 1);
        assert_eq!(locales[0].get("a.b"), Some("first"));
    }

    #[test]
    fn discover_round_trip_with_real_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("cs.toml"), BUILTIN_CS).expect("write cs");
        let mut translator = Translator::discover(dir.path()).expect("discover");
        let codes: Vec<String> = translator
            .available()
            .iter()
            .map(|meta| meta.code.clone())
            .collect();
        assert_eq!(codes, vec!["cs".to_string(), "en".to_string()]);
        assert!(translator.set_language("cs"));
        assert_eq!(
            translator.t("menu.file"),
            translator
                .current_locale()
                .get("menu.file")
                .unwrap_or_default()
        );
        assert!(translator.missing_keys("cs").is_empty());
        // the built-in English fallback is still reachable through the key
        assert_eq!(translator.t("no.such.key"), "no.such.key");
    }
}

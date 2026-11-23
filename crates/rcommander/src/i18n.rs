//! Localisation façade.
//!
//! The `rc-i18n` crate owns language-pack parsing. The model only needs a tiny
//! interface, and keeping it behind a trait lets the model tests run with a
//! deterministic fake translator.

/// Base text direction of a language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

impl Direction {
    pub fn is_rtl(self) -> bool {
        matches!(self, Direction::Rtl)
    }

    /// `true` for layouts that must be mirrored horizontally.
    pub fn layout_rtl(self) -> bool {
        self.is_rtl()
    }
}

/// Minimal text interface used by the model.
pub trait Strings: Send + Sync {
    /// Text for `key`; never empty (falls back to the key itself).
    fn get(&self, key: &str) -> String;
    /// Text with `{name}` placeholders substituted.
    fn format(&self, key: &str, args: &[(&str, &str)]) -> String;
    /// Direction of the current language.
    fn direction(&self) -> Direction;
    /// Current language code, e.g. `en`.
    fn language(&self) -> String;
}

/// Deterministic translator used by tests and as a last-resort fallback: it
/// returns the key with its last dotted segment removed, so UI tests can assert
/// on stable text.
pub struct KeyEchoStrings {
    language: String,
    direction: Direction,
}

impl Default for KeyEchoStrings {
    fn default() -> Self {
        KeyEchoStrings {
            language: "en".to_string(),
            direction: Direction::Ltr,
        }
    }
}

impl KeyEchoStrings {
    pub fn new(language: impl Into<String>, direction: Direction) -> Self {
        KeyEchoStrings {
            language: language.into(),
            direction,
        }
    }
}

impl Strings for KeyEchoStrings {
    fn get(&self, key: &str) -> String {
        key.to_string()
    }

    fn format(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut out = key.to_string();
        for (k, v) in args {
            out = out.replace(&format!("{{{k}}}"), v);
        }
        out
    }

    fn direction(&self) -> Direction {
        self.direction
    }

    fn language(&self) -> String {
        self.language.clone()
    }
}

/// Adapter that exposes an `rc-i18n` translator through the model's tiny
/// `Strings` interface.
pub struct TranslatorStrings {
    translator: rc_i18n::Translator,
}

impl TranslatorStrings {
    pub fn new(translator: rc_i18n::Translator) -> Self {
        TranslatorStrings { translator }
    }

    /// The wrapped translator (used by the options dialog).
    pub fn inner(&self) -> &rc_i18n::Translator {
        &self.translator
    }

    /// Build a translator from the `locales` directory, always with English as
    /// fallback so the application never runs without strings.
    pub fn discover(dir: &std::path::Path, language: &str) -> Self {
        let fallback = rc_i18n::english();
        let locales = rc_i18n::Translator::load_dir(dir).unwrap_or_default();
        let mut translator = rc_i18n::Translator::new(fallback, locales);
        if !translator.set_language(language) {
            translator.set_language("en");
        }
        TranslatorStrings { translator }
    }
}

impl Strings for TranslatorStrings {
    fn get(&self, key: &str) -> String {
        self.translator.t(key).to_string()
    }

    fn format(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.translator.t_args(key, args)
    }

    fn direction(&self) -> Direction {
        match self.translator.direction() {
            rc_i18n::Direction::Rtl => Direction::Rtl,
            rc_i18n::Direction::Ltr => Direction::Ltr,
        }
    }

    fn language(&self) -> String {
        self.translator.language().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translator_adapter_exposes_text_direction_and_language() {
        let mut translator =
            TranslatorStrings::discover(std::path::Path::new("/nonexistent-locales"), "cs");
        assert_eq!(
            translator.language(),
            "en",
            "unknown language falls back to English"
        );
        assert_eq!(translator.direction(), Direction::Ltr);
        assert!(!translator.get("app.title").is_empty());
        let formatted = translator.format("panel.files", &[("count", "3")]);
        assert!(!formatted.is_empty());
        // An unknown language must not change the direction.
        assert!(
            !translator
                .translator
                .set_language("definitely-not-installed")
        );
        assert_eq!(translator.direction(), Direction::Ltr);
        assert_eq!(
            KeyEchoStrings::new("ar", Direction::Rtl).direction(),
            Direction::Rtl
        );
    }

    #[test]
    fn key_echo_returns_the_key_and_substitutes_placeholders() {
        let s = KeyEchoStrings::default();
        assert_eq!(s.get("dialog.copy.title"), "dialog.copy.title");
        assert_eq!(
            s.format("status.of", &[("count", "3"), ("total", "9")]),
            "status.of"
        );
        assert_eq!(
            s.format("{count} of {total}", &[("count", "3"), ("total", "9")]),
            "3 of 9"
        );
        assert_eq!(s.format("{missing}", &[("other", "1")]), "{missing}");
        assert_eq!(s.language(), "en");
        assert!(!s.direction().is_rtl());
        assert!(
            KeyEchoStrings::new("ar", Direction::Rtl)
                .direction()
                .layout_rtl()
        );
    }
}

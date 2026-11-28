//! Validation of a language pack before it is used.
//!
//! The GUI offers a "check language pack" button: the user points it at a file
//! (or pastes its text) and gets either the pack's metadata or one readable
//! sentence explaining what is wrong. This module turns the crate's structured
//! errors into exactly that.

use crate::locale::{Locale, LocaleMeta};

/// Longest accepted language subtag, matching BCP 47's 8 character limit.
const MAX_SUBTAG_LEN: usize = 8;

/// Validate a pack and report a human readable problem, e.g. for the
/// "check language pack" dialog.
///
/// On success the pack's [`LocaleMeta`] is returned, so the caller can show
/// "Czech — Čeština, 155 strings, version 1" right away.
///
/// On failure the message is one sentence without a trailing newline and
/// without the file name: the caller knows which file it asked about. TOML
/// syntax problems keep their line and column information.
///
/// Validation is stricter than [`Locale::parse`] in one respect: the language
/// code must look like a language tag (`cs`, `eng`, `pt-br`), because a pack
/// called `czech.toml` would never be picked up by a language selector.
///
/// ```
/// use rc_i18n::validate_pack;
///
/// let meta = validate_pack("[meta]\ncode = \"cs\"\n\n[strings]\n\"cmd.copy\" = \"Kopírovat\"\n")
///     .expect("valid pack");
/// assert_eq!(meta.code, "cs");
/// assert_eq!(meta.string_count, 1);
///
/// // a typo in the section name is reported, not ignored
/// let problem = validate_pack("[string]\n\"cmd.copy\" = \"Kopírovat\"\n").unwrap_err();
/// assert!(problem.contains("unknown section"), "problem: {problem}");
/// ```
pub fn validate_pack(text: &str) -> Result<LocaleMeta, String> {
    let locale = Locale::parse("", text).map_err(|error| error.to_problem())?;
    check_code_shape(locale.code())?;
    Ok(locale.meta().clone())
}

/// Check that a code looks like a language tag, e.g. `cs`, `eng` or `pt-br`.
fn check_code_shape(code: &str) -> Result<(), String> {
    let mut subtags = code.split('-');
    let base = subtags.next().unwrap_or_default();
    let base_ok = (2..=3).contains(&base.len()) && base.chars().all(|c| c.is_ascii_lowercase());
    if !base_ok {
        return Err(format!(
            "unusual language code {code:?}: use an ISO 639 code such as \"cs\", \"eng\" or \"pt-br\""
        ));
    }
    for subtag in subtags {
        let ok = !subtag.is_empty()
            && subtag.len() <= MAX_SUBTAG_LEN
            && subtag.chars().all(|c| c.is_ascii_alphanumeric());
        if !ok {
            return Err(format!(
                "unusual language code {code:?}: the subtag {subtag:?} is not a valid language subtag"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translator::{BUILTIN_EN, english};

    const BUILTIN_CS: &str = include_str!("../locales/cs.toml");

    #[test]
    fn a_good_pack_validates_and_reports_metadata() {
        let text = r#"
[meta]
code = "cs"
name = "Czech"
native_name = "Čeština"
version = 2
authors = ["Somebody"]

[strings]
"cmd.copy" = "Kopírovat"
"cmd.move" = "Přesunout"
"#;
        let meta = validate_pack(text).expect("pack is valid");
        assert_eq!(meta.code, "cs");
        assert_eq!(meta.name, "Czech");
        assert_eq!(meta.native_name, "Čeština");
        assert_eq!(meta.version, 2);
        assert_eq!(meta.string_count, 2);
        assert_eq!(meta.authors, vec!["Somebody"]);
    }

    #[test]
    fn syntax_errors_keep_their_position() {
        let problem = validate_pack("[meta]\ncode = \"cs\n").expect_err("must fail");
        assert!(problem.starts_with("not valid TOML"), "problem: {problem}");
        assert!(problem.contains("line"), "problem: {problem}");
    }

    #[test]
    fn a_pack_without_a_code_is_rejected() {
        let problem =
            validate_pack("[strings]\n\"cmd.copy\" = \"Kopírovat\"\n").expect_err("must fail");
        assert!(
            problem.contains("missing language code"),
            "problem: {problem}"
        );
        // the message tells the user how to fix it
        assert!(problem.contains("cs.toml"), "problem: {problem}");
    }

    #[test]
    fn structural_problems_are_described_plainly() {
        let cases = [
            (
                "[meta]\ncode = \"cs\"\ndirection = \"sideways\"\n",
                "direction",
            ),
            ("[meta]\ncode = \"cs\"\nversion = -2\n", "version"),
            ("[meta]\ncode = 5\n", "meta.code"),
            ("[string]\n\"a.b\" = \"x\"\n", "unknown section"),
            (
                "[meta]\ncode = \"cs\"\n\n[strings]\n\"Menu.File\" = \"x\"\n",
                "invalid key",
            ),
            (
                "[meta]\ncode = \"cs\"\n\n[strings]\n\"menu.file\" = \"\"\n",
                "empty",
            ),
            (
                "[meta]\ncode = \"cs\"\n\n[strings]\n\"menu.file\" = 1\n",
                "must be a string",
            ),
        ];
        for (text, needle) in cases {
            let problem = validate_pack(text).expect_err(&format!("must reject: {text}"));
            assert!(problem.contains(needle), "text {text:?} gave {problem:?}");
            // no language code is repeated in the message: the caller has the file
            assert!(!problem.contains("\"<unknown>\""), "problem: {problem}");
        }
    }

    #[test]
    fn unusual_language_codes_are_flagged() {
        for code in ["czech", "c", "cs-CZECHREPUBLIC", "cs-", "1c"] {
            let text = format!("[meta]\ncode = \"{code}\"\n");
            let problem =
                validate_pack(&text).expect_err(&format!("code {code:?} must be flagged"));
            assert!(
                problem.contains("unusual language code"),
                "code {code:?}: {problem}"
            );
        }
        // normalised codes are accepted: "CS" becomes "cs"
        for code in ["cs", "CS", "en", "eng", "pt-br", "zh-hans", "sr-latn-rs"] {
            let text = format!("[meta]\ncode = \"{code}\"\n");
            assert!(
                validate_pack(&text).is_ok(),
                "code {code:?} must be accepted"
            );
        }
    }

    #[test]
    fn arabic_pack_covers_reference_keys_and_preserves_placeholders() {
        const AR: &str = include_str!("../locales/ar.toml");
        assert_eq!(AR, include_str!("../../../locales/ar.toml"));
        let ar = Locale::parse("ar", AR).expect("Arabic TOML");
        let en = english();
        assert_eq!(ar.code(), "ar");
        assert_eq!(ar.meta().native_name, "العربية");
        assert!(ar.meta().is_rtl());
        assert_eq!(ar.len(), en.len());
        fn placeholders(text: &str) -> Vec<&str> {
            let mut tokens: Vec<&str> = text
                .split('{')
                .skip(1)
                .filter_map(|tail| tail.split_once('}').map(|(key, _)| key))
                .collect();
            tokens.sort_unstable();
            tokens
        }
        for (key, original) in en.iter() {
            let translated = ar
                .get(key)
                .unwrap_or_else(|| panic!("missing Arabic key: {key}"));
            assert_eq!(placeholders(original), placeholders(translated), "{key}");
        }
        let mut translator = crate::Translator::new(en, vec![ar]);
        assert!(translator.set_language("ar"));
        assert!(translator.direction().is_rtl());
        assert_eq!(translator.t("cmd.copy"), "نسخ");
        let status = translator.t_args("panel.selection_of", &[("count", "3"), ("total", "10")]);
        assert_eq!(status, "تم تحديد 3 من 10");
        assert!(translator.set_language("en"));
        assert!(!translator.direction().is_rtl());
    }

    #[test]
    fn czech_pack_matches_reference_keys_and_placeholders() {
        assert_eq!(BUILTIN_CS, include_str!("../../../locales/cs.toml"));
        let cs = Locale::parse("cs", BUILTIN_CS).expect("Czech TOML");
        let en = english();
        assert_eq!(cs.len(), en.len());
        fn placeholders(text: &str) -> Vec<&str> {
            let mut tokens: Vec<&str> = text
                .split('{')
                .skip(1)
                .filter_map(|tail| tail.split_once('}').map(|(key, _)| key))
                .collect();
            tokens.sort_unstable();
            tokens
        }
        for (key, original) in en.iter() {
            let translated = cs
                .get(key)
                .unwrap_or_else(|| panic!("missing Czech key: {key}"));
            assert_eq!(placeholders(original), placeholders(translated), "{key}");
        }
    }

    #[test]
    fn bundled_packs_validate() {
        let english_meta = validate_pack(BUILTIN_EN).expect("built-in English is valid");
        assert_eq!(english_meta.code, "en");
        assert!(english_meta.string_count >= 120);
        let czech_meta = validate_pack(BUILTIN_CS).expect("bundled Czech is valid");
        assert_eq!(czech_meta.code, "cs");
        assert_eq!(czech_meta.string_count, english_meta.string_count);
        // the helpers agree, so a validated pack is also a loadable one
        let locale = Locale::parse("cs", BUILTIN_CS).expect("parses");
        assert_eq!(locale.meta(), &czech_meta);
        assert_eq!(english().meta().code, english_meta.code);
    }
}

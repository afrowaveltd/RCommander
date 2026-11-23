//! Text direction: locale metadata, script detection and per-string heuristics.
//!
//! The GUI needs three related answers:
//!
//! * [`Direction`] — how the whole interface of the current language is laid
//!   out (mirrored panels, right-aligned widgets).
//! * [`direction_for_code`] — the same question answered from a bare language
//!   code, used when a pack does not state `direction` in `[meta]`.
//! * [`text_is_rtl`] and [`detect_text_direction`] — how one particular string
//!   (a file name, a network path, an error message) should be rendered. A
//!   Hebrew file name in an English listing still has to be drawn right to left.

use std::fmt;

use crate::error::I18nError;

/// Writing direction of a language or of one string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Direction {
    /// Left to right — the default for most languages.
    #[default]
    Ltr,
    /// Right to left — Arabic, Hebrew, Persian, Urdu and friends.
    Rtl,
}

impl Direction {
    /// The wire/`TOML` spelling, `"ltr"` or `"rtl"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Ltr => "ltr",
            Direction::Rtl => "rtl",
        }
    }

    /// True for [`Direction::Rtl`].
    pub fn is_rtl(self) -> bool {
        matches!(self, Direction::Rtl)
    }

    /// Parse `"ltr"`/`"rtl"` case-insensitively, ignoring surrounding blanks.
    ///
    /// Returns `None` for anything else, so callers can decide whether an
    /// unknown tag is an error or simply "not stated".
    pub fn from_tag(tag: &str) -> Option<Direction> {
        match tag.trim().to_ascii_lowercase().as_str() {
            "ltr" => Some(Direction::Ltr),
            "rtl" => Some(Direction::Rtl),
            _ => None,
        }
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Direction {
    type Err = I18nError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Direction::from_tag(s).ok_or_else(|| {
            I18nError::invalid(
                "",
                format!("unknown direction {s:?}: expected \"ltr\" or \"rtl\""),
            )
        })
    }
}

/// Language codes (base ISO 639-1/2 subtags) written right to left.
///
/// The list is intentionally conservative: every entry is a language whose
/// primary script is right to left.
const RTL_CODES: &[&str] = &[
    "ar",  // Arabic
    "arc", // Aramaic
    "ckb", // Central Kurdish (Sorani)
    "dv",  // Divehi (Thaana)
    "fa",  // Persian
    "glk", // Gilaki
    "he",  // Hebrew
    "iw",  // Hebrew, legacy code
    "ji",  // Yiddish, legacy code
    "khw", // Khowar
    "ks",  // Kashmiri
    "lrc", // Northern Luri
    "mzn", // Mazanderani
    "nqo", // N'Ko
    "pnb", // Western Panjabi (Shahmukhi)
    "prs", // Dari
    "ps",  // Pashto
    "sd",  // Sindhi
    "skr", // Saraiki
    "syr", // Syriac
    "ug",  // Uyghur
    "ur",  // Urdu
    "yi",  // Yiddish
];

/// The base language subtag of a code: `"cs-CZ"` and `"cs_CZ"` both give `"cs"`.
///
/// Surrounding blanks are ignored and nothing is lowercased, so this is a plain
/// slice of the input.
///
/// ```
/// use rc_i18n::base_code;
/// assert_eq!(base_code("pt-BR"), "pt");
/// assert_eq!(base_code(" cs_CZ "), "cs");
/// assert_eq!(base_code("he"), "he");
/// ```
pub fn base_code(code: &str) -> &str {
    let trimmed = code.trim();
    match trimmed.find(['-', '_']) {
        Some(cut) => &trimmed[..cut],
        None => trimmed,
    }
}

/// Right-to-left script languages known by code prefix.
///
/// Region and script subtags are ignored, so `ar-EG`, `AR` and `ar_SA` all
/// answer [`Direction::Rtl`]. Anything unrecognised is [`Direction::Ltr`].
///
/// ```
/// use rc_i18n::{Direction, direction_for_code};
/// assert_eq!(direction_for_code("he-IL"), Direction::Rtl);
/// assert_eq!(direction_for_code("cs"), Direction::Ltr);
/// assert_eq!(direction_for_code("xx-YY"), Direction::Ltr);
/// ```
pub fn direction_for_code(code: &str) -> Direction {
    let base = base_code(code).to_ascii_lowercase();
    if RTL_CODES.contains(&base.as_str()) {
        Direction::Rtl
    } else {
        Direction::Ltr
    }
}

/// True when `c` belongs to a right-to-left script.
///
/// Covers Hebrew, Arabic (all blocks), Syriac, Thaana, N'Ko, Samaritan,
/// Mandaic, the Aramaic/Phoenician/Kharoshthi families, the historic Iberian
/// and Siyaq numeral blocks and Adlam, plus the explicit right-to-left mark.
pub fn is_rtl_char(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x05FF // Hebrew
        | 0x0600..=0x06FF // Arabic (includes U+061C ALM)
        | 0x0700..=0x074F // Syriac
        | 0x0750..=0x077F // Arabic Supplement
        | 0x0780..=0x07BF // Thaana
        | 0x07C0..=0x07FF // N'Ko
        | 0x0800..=0x083F // Samaritan
        | 0x0840..=0x085F // Mandaic
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0x200F // RIGHT-TO-LEFT MARK
        | 0xFB1D..=0xFB4F // Hebrew presentation forms
        | 0xFB50..=0xFDFF // Arabic presentation forms A
        | 0xFE70..=0xFEFF // Arabic presentation forms B
        | 0x10840..=0x108FF // Imperial Aramaic, Palmyrene, Nabataean, Hatran
        | 0x10900..=0x1093F // Phoenician, Lydian
        | 0x10A00..=0x10A5F // Kharoshthi
        | 0x10D00..=0x10D3F // Hanifi Rohingya
        | 0x10F30..=0x10FAF // Sogdian, Old Uyghur
        | 0x1E800..=0x1E8DF // Mende Kikakui
        | 0x1E900..=0x1E95F // Adlam
        | 0x1EC70..=0x1ECBF // Indic Siyaq numbers
        | 0x1ED00..=0x1ED4F // Ottoman Siyaq numbers
        | 0x1EE00..=0x1EEFF // Arabic mathematical alphabetic symbols
    )
}

/// True when `c` is a left-to-right *strong* character.
///
/// Digits, punctuation, symbols and whitespace are direction-neutral and are
/// deliberately excluded, so they never tip a decision on their own.
pub fn is_ltr_char(c: char) -> bool {
    c == '\u{200E}' || (c.is_alphabetic() && !is_rtl_char(c))
}

/// True when the given text contains a majority of RTL characters.
///
/// Only *strong* characters count, so numbers, punctuation and whitespace in a
/// mixed string never decide the outcome. A tie is reported as `false`, i.e.
/// left to right, which is the safe default for a mostly Latin interface.
///
/// ```
/// use rc_i18n::text_is_rtl;
/// assert!(text_is_rtl("שלום עולם"));
/// assert!(!text_is_rtl("Hello world"));
/// assert!(!text_is_rtl("123"));
/// ```
pub fn text_is_rtl(text: &str) -> bool {
    let mut rtl = 0usize;
    let mut ltr = 0usize;
    for c in text.chars() {
        if is_rtl_char(c) {
            rtl += 1;
        } else if is_ltr_char(c) {
            ltr += 1;
        }
    }
    rtl > ltr
}

/// Suggest a base text direction for a mixed string (first strong character wins).
///
/// This is the Unicode bidi paragraph rule P2/P3 in miniature: skip neutral
/// characters, then let the first character that actually has a direction
/// decide. Text without any strong character is [`Direction::Ltr`].
///
/// ```
/// use rc_i18n::{Direction, detect_text_direction};
/// assert_eq!(detect_text_direction("Hello שלום"), Direction::Ltr);
/// assert_eq!(detect_text_direction("שלום Hello"), Direction::Rtl);
/// assert_eq!(detect_text_direction("42 مرحبا"), Direction::Rtl);
/// ```
pub fn detect_text_direction(text: &str) -> Direction {
    for c in text.chars() {
        if is_rtl_char(c) {
            return Direction::Rtl;
        }
        if is_ltr_char(c) {
            return Direction::Ltr;
        }
    }
    Direction::Ltr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtl_language_codes_are_recognised() {
        for code in ["ar", "he", "fa", "ur"] {
            assert_eq!(direction_for_code(code), Direction::Rtl, "code {code}");
        }
        // case, whitespace, region and script subtags do not matter
        for code in [
            "AR", " he ", "fa-IR", "ur_PK", "ar-EG", "he-IL", "iw", "yi", "ckb", "ps",
        ] {
            assert_eq!(direction_for_code(code), Direction::Rtl, "code {code}");
        }
    }

    #[test]
    fn ltr_and_unknown_codes_answer_ltr() {
        for code in [
            "en", "cs", "de", "en-US", "cs_CZ", "zh-Hans", "", "  ", "xx", "klingon",
        ] {
            assert_eq!(direction_for_code(code), Direction::Ltr, "code {code:?}");
        }
    }

    #[test]
    fn arabic_text_reads_right_to_left() {
        assert!(text_is_rtl("مرحبا بالعالم"));
        assert!(text_is_rtl("שלום"));
        assert!(!text_is_rtl("Hello world"));
        assert!(!text_is_rtl("Příliš žluťoučký kůň"));
    }

    #[test]
    fn neutral_text_is_not_rtl() {
        assert!(!text_is_rtl(""));
        assert!(!text_is_rtl("123 456"));
        assert!(!text_is_rtl("... --- !!!"));
        assert!(!text_is_rtl("\t\n "));
    }

    #[test]
    fn majority_of_strong_characters_decides() {
        // 12 Arabic letters against 5 Latin ones
        assert!(text_is_rtl("مرحبا بالعالم hello"));
        // 5 Latin letters against 3 Arabic ones
        assert!(!text_is_rtl("hello مرحبا"));
        // an exact tie is not RTL
        assert!(!text_is_rtl("ab اب"));
    }

    #[test]
    fn first_strong_character_wins_for_mixed_strings() {
        assert_eq!(detect_text_direction("Hello שלום"), Direction::Ltr);
        assert_eq!(detect_text_direction("שלום Hello"), Direction::Rtl);
        assert_eq!(detect_text_direction("مرحبا 42 en"), Direction::Rtl);
        // neutral leading characters are skipped
        assert_eq!(detect_text_direction("42 - مرحبا"), Direction::Rtl);
        assert_eq!(detect_text_direction("\"Příliš\" 11"), Direction::Ltr);
        // digits and punctuation alone carry no direction
        assert_eq!(detect_text_direction("1.2.3"), Direction::Ltr);
        assert_eq!(detect_text_direction(""), Direction::Ltr);
    }

    #[test]
    fn explicit_marks_count_as_strong() {
        // U+200F RIGHT-TO-LEFT MARK still selects RTL
        assert_eq!(detect_text_direction("\u{200F}1"), Direction::Rtl);
        // U+200E LEFT-TO-RIGHT MARK selects LTR
        assert_eq!(detect_text_direction("\u{200E}مرحبا"), Direction::Ltr);
    }

    #[test]
    fn cjk_is_treated_as_left_to_right() {
        assert_eq!(detect_text_direction("日本語"), Direction::Ltr);
        assert!(!text_is_rtl("Кириллица"));
    }

    #[test]
    fn direction_tags_round_trip() {
        for d in [Direction::Ltr, Direction::Rtl] {
            assert_eq!(Direction::from_tag(d.as_str()), Some(d));
            assert_eq!(d.to_string(), d.as_str());
            assert_eq!(d.as_str().parse::<Direction>().ok(), Some(d));
        }
        assert_eq!(Direction::from_tag(" RTL "), Some(Direction::Rtl));
        assert_eq!(Direction::from_tag("right-to-left"), None);
        assert!("sideways".parse::<Direction>().is_err());
    }

    #[test]
    fn direction_defaults_and_predicates() {
        assert_eq!(Direction::default(), Direction::Ltr);
        assert!(Direction::Rtl.is_rtl());
        assert!(!Direction::Ltr.is_rtl());
        assert!(Direction::Rtl > Direction::Ltr);
    }
}

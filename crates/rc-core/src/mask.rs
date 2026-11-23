//! Wildcard masks and mask sets.
//!
//! Supported syntax: `*` any sequence, `?` a single character, `[abc]` and
//! `[a-z]` character classes, `[!abc]` negated classes, `\` escapes the next
//! character. Several masks can be combined in a [`MaskSet`] using `;`, `,` or
//! `|` as separators. `*` and `*.*` match everything, including names without
//! an extension.

use crate::error::{CoreError, CoreResult};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Char(char),
    AnySeq,
    AnyOne,
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
}

/// A single compiled wildcard mask.
#[derive(Clone, Debug)]
pub struct Pattern {
    tokens: Vec<Token>,
    raw: String,
    case_sensitive: bool,
    match_all: bool,
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw && self.case_sensitive == other.case_sensitive
    }
}

impl Eq for Pattern {}

impl Pattern {
    /// Compile `spec` as a case-insensitive mask.
    pub fn new(spec: &str) -> CoreResult<Self> {
        Self::with_case_sensitivity(spec, false)
    }

    /// Compile `spec`, optionally honouring character case.
    pub fn with_case_sensitivity(spec: &str, case_sensitive: bool) -> CoreResult<Self> {
        let trimmed = spec.trim();
        let match_all = matches!(trimmed, "*" | "*.*");
        let mut tokens = Vec::new();
        let mut chars = trimmed.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '*' => {
                    if !matches!(tokens.last(), Some(Token::AnySeq)) {
                        tokens.push(Token::AnySeq);
                    }
                }
                '?' => tokens.push(Token::AnyOne),
                '\\' => {
                    let escaped = chars.next().ok_or_else(|| CoreError::Pattern {
                        pattern: spec.to_string(),
                        reason: "trailing escape character".to_string(),
                    })?;
                    tokens.push(Token::Char(escaped));
                }
                '[' => {
                    let mut negated = false;
                    if matches!(chars.peek(), Some('!')) {
                        chars.next();
                        negated = true;
                    }
                    let mut ranges = Vec::new();
                    let mut closed = false;
                    let mut first = true;
                    while let Some(ch) = chars.next() {
                        if ch == ']' && !first {
                            closed = true;
                            break;
                        }
                        first = false;
                        let start = ch;
                        if matches!(chars.peek(), Some('-')) {
                            let mut probe = chars.clone();
                            probe.next();
                            if let Some(&end) = probe.peek()
                                && end != ']'
                            {
                                chars.next();
                                chars.next();
                                ranges.push((start, end));
                                continue;
                            }
                        }
                        ranges.push((start, start));
                    }
                    if !closed {
                        return Err(CoreError::Pattern {
                            pattern: spec.to_string(),
                            reason: "unterminated character class".to_string(),
                        });
                    }
                    tokens.push(Token::Class { negated, ranges });
                }
                other => tokens.push(Token::Char(other)),
            }
        }
        Ok(Pattern {
            tokens,
            raw: trimmed.to_string(),
            case_sensitive,
            match_all,
        })
    }

    /// The mask text this pattern was compiled from.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    /// Does `name` match this mask?
    pub fn is_match(&self, name: &str) -> bool {
        if self.match_all {
            return true;
        }
        let hay: Vec<char> = if self.case_sensitive {
            name.chars().collect()
        } else {
            name.chars().flat_map(|c| c.to_lowercase()).collect()
        };
        self.match_from(0, &hay, 0)
    }

    fn match_from(&self, ti: usize, hay: &[char], hi: usize) -> bool {
        if ti == self.tokens.len() {
            return hi == hay.len();
        }
        match &self.tokens[ti] {
            Token::AnySeq => {
                for start in hi..=hay.len() {
                    if self.match_from(ti + 1, hay, start) {
                        return true;
                    }
                }
                false
            }
            Token::AnyOne => hi < hay.len() && self.match_from(ti + 1, hay, hi + 1),
            Token::Char(c) => {
                let c = if self.case_sensitive {
                    *c
                } else {
                    c.to_ascii_lowercase()
                };
                hi < hay.len() && hay[hi] == c && self.match_from(ti + 1, hay, hi + 1)
            }
            Token::Class { negated, ranges } => {
                if hi >= hay.len() {
                    return false;
                }
                let ch = hay[hi];
                let ch = if self.case_sensitive {
                    ch
                } else {
                    ch.to_ascii_lowercase()
                };
                let inside = ranges.iter().any(|(a, b)| {
                    let a = if self.case_sensitive {
                        *a
                    } else {
                        a.to_ascii_lowercase()
                    };
                    let b = if self.case_sensitive {
                        *b
                    } else {
                        b.to_ascii_lowercase()
                    };
                    ch >= a && ch <= b
                });
                (inside != *negated) && self.match_from(ti + 1, hay, hi + 1)
            }
        }
    }
}

/// A set of include masks combined with `;`, `,` or `|`.
#[derive(Clone, Debug, Default)]
pub struct MaskSet {
    patterns: Vec<Pattern>,
}

impl MaskSet {
    /// A set that matches everything (equivalent to an unspecified filter).
    pub fn all() -> Self {
        MaskSet {
            patterns: vec![Pattern::new("*.*").expect("static mask compiles")],
        }
    }

    /// Parse a mask specification such as `*.txt;*.MD,README*`.
    pub fn parse(spec: &str) -> CoreResult<Self> {
        let mut patterns = Vec::new();
        for part in spec.split([';', ',', '|']) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            patterns.push(Pattern::new(part)?);
        }
        Ok(MaskSet { patterns })
    }

    /// Parse a specification, treating an empty result as "match everything".
    pub fn parse_or_all(spec: &str) -> CoreResult<Self> {
        let set = Self::parse(spec)?;
        if set.is_empty() {
            Ok(Self::all())
        } else {
            Ok(set)
        }
    }

    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// True when any mask in the set matches `name`; an empty set matches all.
    pub fn is_match(&self, name: &str) -> bool {
        self.patterns.is_empty() || self.patterns.iter().any(|p| p.is_match(name))
    }

    /// True when the set is guaranteed to match everything.
    pub fn matches_all(&self) -> bool {
        self.patterns
            .iter()
            .any(|p| p.is_match("") && p.is_match("x") && p.is_match("x.y"))
    }

    /// Joined mask text, e.g. `*.txt;*.md`.
    pub fn to_string_joined(&self) -> String {
        self.patterns
            .iter()
            .map(|p| p.raw().to_string())
            .collect::<Vec<_>>()
            .join(";")
    }
}

/// Convenience: does `name` match the single mask `spec`?
pub fn matches(name: &str, spec: &str) -> bool {
    Pattern::new(spec)
        .map(|p| p.is_match(name))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_matches_everything_including_names_without_extension() {
        let p = Pattern::new("*.*").unwrap();
        assert!(p.is_match("readme"));
        assert!(p.is_match("readme.txt"));
        assert!(p.is_match(".hidden"));
        assert!(Pattern::new("*").unwrap().is_match("anything"));
    }

    #[test]
    fn extension_masks_are_anchored_at_the_end() {
        let p = Pattern::new("*.txt").unwrap();
        assert!(p.is_match("a.txt"));
        assert!(p.is_match("A.TXT"), "default matching is case-insensitive");
        assert!(p.is_match("archive.tar.txt"));
        assert!(!p.is_match("a.txt2"));
        assert!(!p.is_match("txt"));
        assert!(!p.is_match("a.txt.bak"));
    }

    #[test]
    fn exact_names_and_prefixes_work() {
        assert!(Pattern::new("readme").unwrap().is_match("README"));
        assert!(!Pattern::new("readme").unwrap().is_match("readme2"));
        let p = Pattern::new("log*").unwrap();
        assert!(p.is_match("log"));
        assert!(p.is_match("logfile.2024"));
        assert!(!p.is_match("catalog"));
    }

    #[test]
    fn question_mark_matches_exactly_one_character() {
        let p = Pattern::new("a?c").unwrap();
        assert!(p.is_match("abc"));
        assert!(p.is_match("ačc"));
        assert!(!p.is_match("ac"));
        assert!(!p.is_match("abbc"));
    }

    #[test]
    fn character_classes_and_ranges_match() {
        let p = Pattern::new("file[0-9].txt").unwrap();
        assert!(p.is_match("file7.txt"));
        assert!(!p.is_match("filex.txt"));
        let neg = Pattern::new("file[!0-9].txt").unwrap();
        assert!(neg.is_match("filex.txt"));
        assert!(!neg.is_match("file7.txt"));
    }

    #[test]
    fn escapes_and_errors_are_handled() {
        assert!(Pattern::new("a\\*b").unwrap().is_match("a*b"));
        assert!(!Pattern::new("a\\*b").unwrap().is_match("axb"));
        assert!(Pattern::new("a[").is_err());
        assert!(Pattern::new("a\\").is_err());
    }

    #[test]
    fn case_sensitive_matching_is_available() {
        let p = Pattern::with_case_sensitivity("*.TXT", true).unwrap();
        assert!(p.is_match("A.TXT"));
        assert!(!p.is_match("a.txt"));
    }

    #[test]
    fn mask_sets_split_on_several_separators() {
        let set = MaskSet::parse("*.txt;*.md,README|*.rst").unwrap();
        assert_eq!(set.patterns().len(), 4);
        for name in ["a.txt", "b.md", "readme", "c.rst"] {
            assert!(set.is_match(name), "{name} should match");
        }
        assert!(!set.is_match("d.zip"));
    }

    #[test]
    fn empty_mask_set_matches_everything() {
        let set = MaskSet::parse("").unwrap();
        assert!(set.is_empty());
        assert!(set.is_match("whatever"));
        assert!(MaskSet::all().matches_all());
        assert!(!MaskSet::parse("*.txt").unwrap().matches_all());
        assert_eq!(
            MaskSet::parse("*.txt; *.md").unwrap().to_string_joined(),
            "*.txt;*.md"
        );
    }

    #[test]
    fn masks_with_single_star_inside_extension_match_dotfiles() {
        let p = Pattern::new("*rc").unwrap();
        assert!(p.is_match(".bashrc"));
        assert!(p.is_match("myrc"));
    }

    #[test]
    fn matches_helper_is_convenient() {
        assert!(matches("x.log", "*.log"));
        assert!(!matches("x.log", "*.txt"));
        assert!(!matches("x", "["), "invalid masks never match");
    }
}

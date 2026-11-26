//! A line diff, small enough to keep the review honest.
//!
//! The workspace feature needs to show *what changed* in a copy before anything is written back, and
//! a review that says "this file differs" is not a review. This is a classic longest-common-
//! subsequence diff over lines: correct, dependency-free, and honest about its own limit — past a
//! size budget it stops guessing and reports the file as wholly replaced, which the caller shows as
//! such instead of pretending to have read it.

/// One line of a diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffLine {
    /// A line both versions share, kept as context.
    Same(String),
    /// A line only the new version has.
    Added(String),
    /// A line only the old version had.
    Removed(String),
    /// Lines that were left out of the output (`n` of them).
    Skipped(usize),
}

/// A diff with its own counts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diff {
    pub lines: Vec<DiffLine>,
    pub added: usize,
    pub removed: usize,
    /// True when the diff was too large to compute line by line.
    pub approximate: bool,
}

impl Diff {
    /// True when the two versions are the same.
    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }

    /// A one-line summary for a list row: `+3 −1`, or `unchanged`.
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "unchanged".to_string();
        }
        let mut parts = Vec::new();
        if self.added > 0 {
            parts.push(format!("+{}", self.added));
        }
        if self.removed > 0 {
            parts.push(format!("-{}", self.removed));
        }
        if self.approximate {
            parts.push("(large file)".to_string());
        }
        parts.join(" ")
    }
}

/// How many context lines to keep around a change.
const CONTEXT: usize = 2;

/// Above this many cells the LCS table is not built; the diff degrades to whole-file.
const MAX_CELLS: usize = 4_000_000;

/// Diff two texts line by line.
pub fn diff_lines(old: &str, new: &str) -> Diff {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let cells = (old_lines.len() + 1).saturating_mul(new_lines.len() + 1);
    if cells > MAX_CELLS {
        // Honest degradation: say the file was replaced rather than show a wrong diff.
        let mut lines: Vec<DiffLine> = old_lines
            .iter()
            .map(|line| DiffLine::Removed((*line).to_string()))
            .collect();
        lines.extend(
            new_lines
                .iter()
                .map(|line| DiffLine::Added((*line).to_string())),
        );
        return Diff {
            lines,
            added: new_lines.len(),
            removed: old_lines.len(),
            approximate: true,
        };
    }
    // LCS lengths, bottom-up: `table[i][j]` is the LCS of the two suffixes.
    let mut table = vec![vec![0u32; new_lines.len() + 1]; old_lines.len() + 1];
    for i in (0..old_lines.len()).rev() {
        for j in (0..new_lines.len()).rev() {
            table[i][j] = if old_lines[i] == new_lines[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut raw: Vec<DiffLine> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut added = 0;
    let mut removed = 0;
    while i < old_lines.len() && j < new_lines.len() {
        if old_lines[i] == new_lines[j] {
            raw.push(DiffLine::Same(old_lines[i].to_string()));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            raw.push(DiffLine::Removed(old_lines[i].to_string()));
            removed += 1;
            i += 1;
        } else {
            raw.push(DiffLine::Added(new_lines[j].to_string()));
            added += 1;
            j += 1;
        }
    }
    while i < old_lines.len() {
        raw.push(DiffLine::Removed(old_lines[i].to_string()));
        removed += 1;
        i += 1;
    }
    while j < new_lines.len() {
        raw.push(DiffLine::Added(new_lines[j].to_string()));
        added += 1;
        j += 1;
    }
    Diff {
        lines: collapse_context(raw),
        added,
        removed,
        approximate: false,
    }
}

/// Keep the changes and a little context, and mark what was left out.
fn collapse_context(raw: Vec<DiffLine>) -> Vec<DiffLine> {
    let keep: Vec<bool> = (0..raw.len())
        .map(|index| {
            let changed = !matches!(raw[index], DiffLine::Same(_));
            // Kept when it is a change, or when a change is within the context window on either
            // side of it.
            changed
                || (1..=CONTEXT).any(|offset| {
                    index >= offset && !matches!(raw[index - offset], DiffLine::Same(_))
                })
                || (1..=CONTEXT).any(|offset| {
                    index + offset < raw.len() && !matches!(raw[index + offset], DiffLine::Same(_))
                })
        })
        .collect();
    let mut out: Vec<DiffLine> = Vec::new();
    let mut skipped = 0;
    for (index, line) in raw.into_iter().enumerate() {
        if keep[index] {
            if skipped > 0 {
                out.push(DiffLine::Skipped(skipped));
                skipped = 0;
            }
            out.push(line);
        } else {
            skipped += 1;
        }
    }
    if skipped > 0 {
        out.push(DiffLine::Skipped(skipped));
    }
    out
}

/// True when these bytes look like text: no NUL and valid UTF-8 in the prefix we check.
pub fn is_text(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(8192)];
    !head.contains(&0) && (head.is_ascii() || std::str::from_utf8(head).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(diff: &Diff) -> Vec<String> {
        diff.lines
            .iter()
            .map(|line| match line {
                DiffLine::Same(text) => format!("  {text}"),
                DiffLine::Added(text) => format!("+ {text}"),
                DiffLine::Removed(text) => format!("- {text}"),
                DiffLine::Skipped(count) => format!("… {count}"),
            })
            .collect()
    }

    #[test]
    fn identical_texts_have_no_difference() {
        let diff = diff_lines("a\nb\n", "a\nb\n");
        assert!(diff.is_empty());
        assert_eq!(diff.summary(), "unchanged");
    }

    #[test]
    fn a_changed_line_is_shown_as_one_removal_and_one_addition() {
        let diff = diff_lines("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(diff.added, 1);
        assert_eq!(diff.removed, 1);
        let text = texts(&diff).join("\n");
        assert!(text.contains("+ B"), "{text}");
        assert!(text.contains("- b"), "{text}");
        // The unchanged neighbours are context, not noise.
        assert!(text.contains("  a"), "{text}");
        assert!(text.contains("  c"), "{text}");
        assert_eq!(diff.summary(), "+1 -1");
    }

    #[test]
    fn an_added_line_is_only_an_addition() {
        let diff = diff_lines("one\ntwo\n", "one\ntwo\nthree\n");
        assert_eq!((diff.added, diff.removed), (1, 0));
        assert!(texts(&diff).iter().any(|line| line == "+ three"));
    }

    #[test]
    fn a_removed_line_is_only_a_removal() {
        let diff = diff_lines("one\ntwo\nthree\n", "one\nthree\n");
        assert_eq!((diff.added, diff.removed), (0, 1));
        assert!(texts(&diff).iter().any(|line| line == "- two"));
    }

    #[test]
    fn long_unchanged_stretches_are_collapsed_but_counted() {
        let mut old = String::new();
        let mut new = String::new();
        for index in 0..40 {
            old.push_str(&format!("line {index}\n"));
            if index == 20 {
                new.push_str("CHANGED\n");
            } else {
                new.push_str(&format!("line {index}\n"));
            }
        }
        let diff = diff_lines(&old, &new);
        assert_eq!((diff.added, diff.removed), (1, 1));
        let skipped: usize = diff
            .lines
            .iter()
            .filter_map(|line| match line {
                DiffLine::Skipped(count) => Some(*count),
                _ => None,
            })
            .sum();
        assert!(skipped > 20, "the far context is collapsed: {skipped}");
        // The change itself is still there.
        assert!(texts(&diff).iter().any(|line| line == "+ CHANGED"));
    }

    #[test]
    fn a_file_beyond_the_budget_is_reported_as_wholesale() {
        // 3000 × 3000 lines is past the cell budget; the diff must say so rather than lie.
        let old: String = (0..3000).map(|index| format!("old {index}\n")).collect();
        let new: String = (0..3000).map(|index| format!("new {index}\n")).collect();
        let diff = diff_lines(&old, &new);
        assert!(diff.approximate);
        assert_eq!(diff.added, 3000);
        assert_eq!(diff.removed, 3000);
        assert!(diff.summary().contains("large file"), "{}", diff.summary());
    }

    #[test]
    fn text_and_binary_are_told_apart() {
        assert!(is_text(b"hello\nworld\n"));
        assert!(is_text("ěščřžýáíé".as_bytes()));
        assert!(!is_text(b"PK\x03\x04\x00\x00"));
        assert!(!is_text(&[0xff, 0xfe, 0x00, 0x01]));
        assert!(is_text(b""));
    }
}

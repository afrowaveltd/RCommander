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
    /// The view a reader sees: changes with a little context, long gaps collapsed.
    pub lines: Vec<DiffLine>,
    /// The same changes without any collapsing, which is what hunks are built from.
    pub raw: Vec<DiffLine>,
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
            raw: lines.clone(),
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
        lines: collapse_context(raw.clone()),
        raw,
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

/// One contiguous run of changes, with the context lines around it.
///
/// A hunk is the unit a careful reviewer actually decides about: "this change yes, that one no" is a
/// real answer, and a file-level tick cannot express it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// Index in [`Diff::hunks`], which is what a caller stores when a user ticks one.
    pub index: usize,
    /// The lines of this hunk, context included.
    pub lines: Vec<DiffLine>,
    /// Lines the hunk adds.
    pub added: usize,
    /// Lines the hunk removes.
    pub removed: usize,
    /// One-based line number in the old file where the hunk starts.
    pub old_start: usize,
    /// Indices into [`Diff::raw`] of the added and removed lines in this hunk.
    ///
    /// Reconstruction needs the association between a change and its hunk; keeping it here means
    /// the merge never has to re-derive it (and cannot get it wrong).
    pub changes: Vec<usize>,
}

impl Hunk {
    /// A one-line description: `@@ -12 +12 @@ +1 -1`.
    pub fn header(&self) -> String {
        format!(
            "@@ -{} @@ +{} -{}",
            self.old_start, self.added, self.removed
        )
    }
}

impl Diff {
    /// The changes split into hunks, context included.
    ///
    /// Context lines between two changes belong to the hunk they touch; a run of changes closer
    /// than six lines stays one hunk, because splitting it would ask the user to approve half a
    /// thought.
    pub fn hunks(&self) -> Vec<Hunk> {
        let mut hunks: Vec<Hunk> = Vec::new();
        let mut current: Vec<DiffLine> = Vec::new();
        let mut old_line = 1usize;
        let mut current_start = 1usize;
        let mut added = 0usize;
        let mut removed = 0usize;
        // Lines since the last change, so a hunk can be closed once the gap is wide enough.
        let mut context_run = 0usize;
        let mut change_indices: Vec<usize> = Vec::new();
        for (raw_index, line) in self.raw.iter().enumerate() {
            match line {
                DiffLine::Same(_) => {
                    if !current.is_empty() {
                        context_run += 1;
                        if context_run > CONTEXT * 2 {
                            // Close the hunk: the context after it is another hunk's.
                            while current.len() > 1
                                && matches!(current.last(), Some(DiffLine::Same(_)))
                                && current
                                    .iter()
                                    .rev()
                                    .take_while(|line| matches!(line, DiffLine::Same(_)))
                                    .count()
                                    > CONTEXT
                            {
                                current.pop();
                            }
                            hunks.push(Hunk {
                                index: hunks.len(),
                                lines: std::mem::take(&mut current),
                                added,
                                removed,
                                old_start: current_start,
                                changes: std::mem::take(&mut change_indices),
                            });
                            added = 0;
                            removed = 0;
                            context_run = 0;
                            // The context that was left becomes the next hunk's opening.
                            current_start = old_line.saturating_sub(CONTEXT);
                        }
                    }
                    if !current.is_empty() {
                        current.push(line.clone());
                    }
                    old_line += 1;
                }
                DiffLine::Added(_) => {
                    if current.is_empty() {
                        current_start = old_line;
                    }
                    context_run = 0;
                    current.push(line.clone());
                    change_indices.push(raw_index);
                    added += 1;
                }
                DiffLine::Removed(_) => {
                    if current.is_empty() {
                        current_start = old_line;
                    }
                    context_run = 0;
                    current.push(line.clone());
                    change_indices.push(raw_index);
                    removed += 1;
                    old_line += 1;
                }
                DiffLine::Skipped(_) => {}
            }
        }
        if !current.is_empty() {
            hunks.push(Hunk {
                index: hunks.len(),
                lines: current,
                added,
                removed,
                old_start: current_start,
                changes: change_indices,
            });
        }
        hunks
    }

    /// Rebuild the new text from the hunks whose indices are in `selected`.
    ///
    /// Selecting every hunk gives the new file exactly; selecting none gives the old file exactly.
    /// Anything in between is a file the user assembled on purpose, and that is what gets written.
    pub fn apply_selected(&self, selected: &[usize]) -> String {
        let chosen: std::collections::BTreeSet<usize> = selected.iter().copied().collect();
        // Which hunk owns each change line of the raw diff.
        let mut owner: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
        for hunk in self.hunks() {
            for index in hunk.changes {
                owner.insert(index, hunk.index);
            }
        }
        let mut text = String::new();
        for (raw_index, line) in self.raw.iter().enumerate() {
            let selected_hunk = owner
                .get(&raw_index)
                .map(|hunk| chosen.contains(hunk))
                .unwrap_or(true);
            match line {
                // Context is in both versions.
                DiffLine::Same(content) => {
                    text.push_str(content);
                    text.push('\n');
                }
                // A removal survives only when its hunk was not taken.
                DiffLine::Removed(content) if !selected_hunk => {
                    text.push_str(content);
                    text.push('\n');
                }
                // An addition appears only when its hunk was taken.
                DiffLine::Added(content) if selected_hunk => {
                    text.push_str(content);
                    text.push('\n');
                }
                DiffLine::Skipped(_) => {}
                _ => {}
            }
        }
        text
    }
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

#[cfg(test)]
mod hunk_tests {
    use super::*;

    #[test]
    fn selecting_every_hunk_rebuilds_the_new_file() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nB\nc\nd\nE\n";
        let diff = diff_lines(old, new);
        let all: Vec<usize> = diff.hunks().iter().map(|hunk| hunk.index).collect();
        assert_eq!(diff.apply_selected(&all), new);
    }

    #[test]
    fn selecting_no_hunk_rebuilds_the_old_file() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nB\nc\nd\nE\n";
        let diff = diff_lines(old, new);
        assert_eq!(diff.apply_selected(&[]), old);
    }

    #[test]
    fn one_hunk_can_be_taken_and_another_left_alone() {
        // Two changes far enough apart to be two hunks.
        let old = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
        let new = "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nNINE\nten\n";
        let diff = diff_lines(old, new);
        let hunks = diff.hunks();
        assert_eq!(hunks.len(), 2, "{hunks:?}");
        // Take the first, leave the second.
        let mixed = diff.apply_selected(&[0]);
        assert!(mixed.contains("TWO"), "{mixed}");
        assert!(
            mixed.contains("nine"),
            "the second change is not applied: {mixed}"
        );
        assert!(!mixed.contains("NINE"), "{mixed}");
        // Take the second, leave the first.
        let other = diff.apply_selected(&[1]);
        assert!(other.contains("two"), "{other}");
        assert!(other.contains("NINE"), "{other}");
    }

    #[test]
    fn a_hunk_says_where_it_starts_and_how_big_it_is() {
        let diff = diff_lines("a\nb\nc\n", "a\nB\nc\n");
        let hunks = diff.hunks();
        assert_eq!(hunks.len(), 1);
        let hunk = &hunks[0];
        assert_eq!(hunk.added, 1);
        assert_eq!(hunk.removed, 1);
        assert!(hunk.old_start >= 1);
        assert!(hunk.header().starts_with("@@ -"), "{}", hunk.header());
        assert!(hunk.header().contains("+1 -1"), "{}", hunk.header());
    }

    #[test]
    fn an_added_file_is_one_hunk_of_additions() {
        let diff = diff_lines("", "x\ny\n");
        let hunks = diff.hunks();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].added, 2);
        assert_eq!(hunks[0].removed, 0);
        assert_eq!(diff.apply_selected(&[0]), "x\ny\n");
    }

    #[test]
    fn the_view_and_the_raw_list_agree_on_what_changed() {
        // A long file with one change in the middle: the view collapses context, the raw list does
        // not, and both must report the same numbers.
        let mut old = String::new();
        let mut new = String::new();
        for index in 0..40 {
            old.push_str(&format!("line {index}\n"));
            new.push_str(&format!("line {index}\n"));
        }
        let changed = new.replace("line 20\n", "line TWENTY\n");
        let diff = diff_lines(&old, &changed);
        assert_eq!(diff.hunks().len(), 1);
        let hunk = &diff.hunks()[0];
        assert_eq!((hunk.added, hunk.removed), (1, 1));
        let all = vec![0usize];
        let rebuilt = diff.apply_selected(&all);
        assert_eq!(rebuilt.lines().count(), 40);
        assert!(rebuilt.contains("line TWENTY"));
    }
}

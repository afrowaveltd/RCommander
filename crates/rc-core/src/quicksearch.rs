//! Incremental "type to find" search inside one panel.

/// Tracks the keystrokes of a quick search and resolves them against a list of
/// entry names.
#[derive(Clone, Debug, Default)]
pub struct QuickSearch {
    buffer: String,
    /// Index the search started at, so Esc can jump back.
    origin: Option<usize>,
}

impl QuickSearch {
    pub fn new() -> Self {
        QuickSearch::default()
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn is_active(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Remember where the search began, the first time a character arrives.
    pub fn begin_at(&mut self, index: usize) {
        if self.origin.is_none() {
            self.origin = Some(index);
        }
    }

    pub fn origin(&self) -> Option<usize> {
        self.origin
    }

    /// Reset the buffer (Esc or after the search was accepted).
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.origin = None;
    }

    /// Remove the last typed character.
    pub fn backspace(&mut self) {
        self.buffer.pop();
    }

    /// Append a character and return the matching entry index, if any.
    ///
    /// When the buffer consists of one repeated character and the same
    /// character is typed again, the search jumps to the *next* entry that
    /// starts with it — the classic cycle-through behaviour.
    pub fn push(&mut self, ch: char, names: &[String], cursor: usize) -> Option<usize> {
        let repeated = !self.buffer.is_empty() && self.buffer.chars().all(|c| c == ch);
        if repeated {
            self.buffer.push(ch);
            let cyc = self.buffer.chars().next()?;
            return cycle_from(names, cursor, cyc);
        }
        self.buffer.push(ch);
        let needle = self.buffer.to_lowercase();
        if let Some(i) = names
            .iter()
            .position(|n| n.to_lowercase().starts_with(&needle))
        {
            return Some(i);
        }
        // No prefix hit: fall back to a "contains" search so that typing a
        // middle word still finds something.
        names
            .iter()
            .position(|n| n.to_lowercase().contains(&needle))
    }

    /// Find `buffer` starting at `from`, wrapping around.
    pub fn find_from(&self, names: &[String], from: usize) -> Option<usize> {
        if self.buffer.is_empty() {
            return None;
        }
        let needle = self.buffer.to_lowercase();
        let n = names.len();
        for step in 0..n {
            let idx = (from + step) % n;
            if names[idx].to_lowercase().starts_with(&needle) {
                return Some(idx);
            }
        }
        None
    }
}

fn cycle_from(names: &[String], cursor: usize, ch: char) -> Option<usize> {
    let n = names.len();
    if n == 0 {
        return None;
    }
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    for step in 1..=n {
        let idx = (cursor + step) % n;
        if names[idx].to_lowercase().starts_with(lower) {
            return Some(idx);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["alpha.txt", "beta.txt", "bravo.md", "charlie", "delta"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn typing_a_prefix_jumps_to_the_first_match() {
        let mut q = QuickSearch::new();
        let n = names();
        assert_eq!(q.push('b', &n, 0), Some(1));
        assert_eq!(q.buffer(), "b");
        assert_eq!(q.push('r', &n, 1), Some(2));
        assert_eq!(q.buffer(), "br");
    }

    #[test]
    fn a_single_prefix_skips_to_the_first_entry_with_that_letter() {
        let mut q = QuickSearch::new();
        let n = names();
        assert_eq!(q.push('c', &n, 0), Some(3));
    }

    #[test]
    fn repeating_a_character_cycles_through_matches() {
        let mut q = QuickSearch::new();
        let n = names();
        q.push('a', &n, 0);
        assert_eq!(
            q.push('a', &n, 0),
            Some(0),
            "only alpha starts with a, wraps to itself"
        );
        let mut q2 = QuickSearch::new();
        let n2: Vec<String> = ["file1", "file2", "file3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        q2.push('f', &n2, 0);
        assert_eq!(q2.push('f', &n2, 0), Some(1));
        assert_eq!(q2.push('f', &n2, 1), Some(2));
    }

    #[test]
    fn unresolvable_prefix_falls_back_to_contains() {
        let mut q = QuickSearch::new();
        let n = names();
        q.push('z', &n, 0);
        assert_eq!(q.push('z', &n, 0), None);
        let mut q2 = QuickSearch::new();
        let n2: Vec<String> = ["readme.md", "notable"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(q2.push('t', &n2, 0), Some(1));
    }

    #[test]
    fn reset_and_backspace_clear_state() {
        let mut q = QuickSearch::new();
        let n = names();
        q.begin_at(2);
        q.push('b', &n, 2);
        q.backspace();
        assert_eq!(q.buffer(), "");
        q.push('d', &n, 2);
        q.reset();
        assert!(!q.is_active());
        assert_eq!(q.origin(), None);
    }

    #[test]
    fn origin_is_remembered_once() {
        let mut q = QuickSearch::new();
        q.begin_at(3);
        q.begin_at(0);
        assert_eq!(q.origin(), Some(3));
    }

    #[test]
    fn find_from_wraps_around() {
        let q = QuickSearch {
            buffer: "a".to_string(),
            origin: None,
        };
        let n = names();
        assert_eq!(q.find_from(&n, 2), Some(0));
        assert_eq!(QuickSearch::new().find_from(&n, 0), None);
    }
}

//! File search: name masks, content matching and archive-free recursive walks.
//!
//! The search runs on a worker thread so a big directory tree never blocks the
//! window. The model polls it every frame, exactly like a transfer.

use crate::model::dialogs::{SearchHit, SearchState};
use rc_core::mask::MaskSet;
use rc_core::ops::CancelToken;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::JoinHandle;

/// Messages the search thread sends to the model.
enum SearchEvent {
    Progress { scanned: u64 },
    Hit(SearchHit),
    Finished { scanned: u64, truncated: bool },
}

/// A running search.
pub struct SearchJob {
    receiver: Receiver<SearchEvent>,
    cancel: CancelToken,
    join: Option<JoinHandle<()>>,
    pub scanned: u64,
    pub hits: Vec<SearchHit>,
    pub finished: bool,
    pub truncated: bool,
}

impl SearchJob {
    /// Start a search described by `state`.
    pub fn spawn(state: &SearchState) -> Option<SearchJob> {
        if state.validate().is_err() {
            return None;
        }
        let (tx, receiver) = channel();
        let cancel = CancelToken::new();
        let thread_cancel = cancel.clone();
        let state = state.clone();
        let join = std::thread::spawn(move || {
            let mask = MaskSet::parse_or_all(&state.name_mask).unwrap_or_default();
            let mut scanned = 0u64;
            let mut hits = 0usize;
            let mut truncated = false;
            let mut stack: Vec<PathBuf> = vec![PathBuf::from(&state.root)];
            while let Some(dir) = stack.pop() {
                if thread_cancel.is_cancelled() {
                    break;
                }
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                let mut children: Vec<PathBuf> = Vec::new();
                for entry in entries.flatten() {
                    if thread_cancel.is_cancelled() {
                        break;
                    }
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if is_dir {
                        if !state.search_hidden && name.starts_with('.') {
                            continue;
                        }
                        if state.subdirs {
                            children.push(path);
                        }
                        continue;
                    }
                    scanned += 1;
                    if !state.search_hidden && name.starts_with('.') {
                        continue;
                    }
                    if !mask.is_match(&name) {
                        continue;
                    }
                    let mut line = None;
                    if state.is_content_search() {
                        match match_content(&path, &state) {
                            Some(found) => line = Some(found),
                            None => continue,
                        }
                    }
                    if hits >= state.max_results {
                        truncated = true;
                        break;
                    }
                    hits += 1;
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    let hit = SearchHit {
                        path: path.to_string_lossy().to_string(),
                        line,
                        size,
                    };
                    if tx.send(SearchEvent::Hit(hit)).is_err() {
                        return;
                    }
                    let _ = tx.send(SearchEvent::Progress { scanned });
                }
                if truncated {
                    break;
                }
                children.sort();
                // Depth-first, in name order, so results are predictable.
                while let Some(child) = children.pop() {
                    stack.push(child);
                }
            }
            let _ = tx.send(SearchEvent::Finished { scanned, truncated });
        });
        Some(SearchJob {
            receiver,
            cancel,
            join: Some(join),
            scanned: 0,
            hits: Vec::new(),
            finished: false,
            truncated: false,
        })
    }

    /// Drain pending events. Returns true while the search is still running.
    pub fn poll(&mut self) -> bool {
        if self.finished {
            return false;
        }
        loop {
            match self.receiver.try_recv() {
                Ok(SearchEvent::Progress { scanned }) => self.scanned = scanned,
                Ok(SearchEvent::Hit(hit)) => self.hits.push(hit),
                Ok(SearchEvent::Finished { scanned, truncated }) => {
                    self.scanned = scanned;
                    self.truncated = truncated;
                    self.finished = true;
                    return false;
                }
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => {
                    self.finished = true;
                    return false;
                }
            }
        }
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Block until the search finishes (tests and the CLI).
    pub fn wait(mut self) -> (Vec<SearchHit>, u64) {
        while self.poll() {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        (self.hits, self.scanned)
    }
}

/// Look for `state.content` inside `path`; returns the matching line.
fn match_content(path: &Path, state: &SearchState) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let needle = state.content.as_str();
    if state.regex {
        let pattern = if state.case_sensitive {
            needle.to_string()
        } else {
            format!("(?i){needle}")
        };
        let regex = regex::Regex::new(&pattern).ok()?;
        for line in text.lines() {
            if regex.is_match(line) {
                return Some(line.trim().chars().take(200).collect());
            }
        }
        return None;
    }
    let (haystack, needle) = if state.case_sensitive {
        (text.to_string(), needle.to_string())
    } else {
        (text.to_lowercase(), needle.to_lowercase())
    };
    let index = haystack.find(&needle)?;
    // Report the whole line that contains the match.
    let line_start = haystack[..index].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = haystack[index..]
        .find('\n')
        .map(|i| index + i)
        .unwrap_or(haystack.len());
    let original = &text[line_start.min(text.len())..line_end.min(text.len())];
    Some(original.trim().chars().take(200).collect())
}

/// Search without a background thread (used by the CLI and tests).
pub fn search_blocking(state: &SearchState) -> (Vec<SearchHit>, u64, bool) {
    match SearchJob::spawn(state) {
        Some(job) => {
            let (hits, scanned) = job.wait();
            (hits, scanned, false)
        }
        None => (Vec::new(), 0, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::create_dir_all(dir.path().join("skip")).unwrap();
        fs::write(dir.path().join("alpha.txt"), "hello world\nsecond line\n").unwrap();
        fs::write(dir.path().join("beta.log"), "HELLO there\n").unwrap();
        fs::write(dir.path().join("a/b/gamma.txt"), "third\nneedle here\n").unwrap();
        fs::write(dir.path().join("a/b/hidden.txt"), "hidden needle\n").unwrap();
        fs::write(dir.path().join(".dotfile.txt"), "dot needle\n").unwrap();
        dir
    }

    fn state(root: &Path) -> SearchState {
        SearchState::new(root.to_string_lossy().to_string())
    }

    #[test]
    fn name_search_finds_matching_files_recursively() {
        let dir = tree();
        let mut s = state(dir.path());
        s.name_mask = "*.txt".to_string();
        let (hits, scanned, _) = search_blocking(&s);
        let names: Vec<String> = hits
            .iter()
            .map(|h| {
                Path::new(&h.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert!(names.contains(&"alpha.txt".to_string()));
        assert!(names.contains(&"gamma.txt".to_string()));
        assert!(!names.contains(&"beta.log".to_string()));
        assert!(scanned >= 4);
    }

    #[test]
    fn non_recursive_search_stays_in_the_start_directory() {
        let dir = tree();
        let mut s = state(dir.path());
        s.name_mask = "*.txt".to_string();
        s.subdirs = false;
        let (hits, _, _) = search_blocking(&s);
        let names: Vec<String> = hits
            .iter()
            .map(|h| {
                Path::new(&h.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert!(names.contains(&"alpha.txt".to_string()));
        assert!(!names.contains(&"gamma.txt".to_string()));
    }

    #[test]
    fn hidden_files_are_skipped_unless_requested() {
        let dir = tree();
        let mut s = state(dir.path());
        s.name_mask = "*.txt".to_string();
        let (hits, _, _) = search_blocking(&s);
        assert!(!hits.iter().any(|h| h.path.contains(".dotfile")));
        s.search_hidden = true;
        let (hits, _, _) = search_blocking(&s);
        assert!(hits.iter().any(|h| h.path.contains(".dotfile")));
    }

    #[test]
    fn content_search_reports_the_matching_line() {
        let dir = tree();
        let mut s = state(dir.path());
        s.name_mask = "*.*".to_string();
        s.content = "needle".to_string();
        let (hits, _, _) = search_blocking(&s);
        assert_eq!(hits.len(), 2, "hits: {hits:?}");
        assert!(
            hits.iter()
                .any(|h| h.line.as_deref() == Some("needle here"))
        );
        assert!(hits.iter().any(|h| h.path.contains("hidden.txt")));
    }

    #[test]
    fn content_search_is_case_insensitive_by_default() {
        let dir = tree();
        let mut s = state(dir.path());
        s.content = "hello".to_string();
        let (hits, _, _) = search_blocking(&s);
        assert_eq!(hits.len(), 2, "both hello and HELLO should match: {hits:?}");
        s.case_sensitive = true;
        let (hits, _, _) = search_blocking(&s);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("alpha.txt"));
    }

    #[test]
    fn regex_search_works_when_asked_for() {
        let dir = tree();
        let mut s = state(dir.path());
        s.content = "^thi.d".to_string();
        s.regex = true;
        let (hits, _, _) = search_blocking(&s);
        assert_eq!(hits.len(), 1, "hits: {hits:?}");
        assert!(hits[0].path.ends_with("gamma.txt"));
        // An invalid expression must not panic, it just finds nothing.
        s.content = "([".to_string();
        let (hits, _, _) = search_blocking(&s);
        assert!(hits.is_empty());
    }

    #[test]
    fn the_result_limit_truncates_the_search() {
        let dir = tree();
        let mut s = state(dir.path());
        s.name_mask = "*.txt".to_string();
        s.max_results = 1;
        let (hits, _, _) = search_blocking(&s);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn an_invalid_search_state_produces_no_job() {
        let mut s = SearchState::new("");
        assert!(SearchJob::spawn(&s).is_none());
        s.root = "/tmp".to_string();
        s.name_mask = " ".to_string();
        assert!(SearchJob::spawn(&s).is_none());
    }

    #[test]
    fn a_cancelled_search_stops_quickly() {
        let dir = tree();
        let mut s = state(dir.path());
        s.name_mask = "*.txt".to_string();
        let job = SearchJob::spawn(&s).expect("job");
        job.cancel();
        let (hits, _) = job.wait();
        assert!(
            hits.len() <= 4,
            "at most the files of the fixture: {hits:?}"
        );
    }

    #[test]
    fn polling_reports_progress_and_completion() {
        let dir = tree();
        let mut s = state(dir.path());
        s.name_mask = "*.txt".to_string();
        let mut job = SearchJob::spawn(&s).expect("job");
        let mut guard = 0;
        while job.poll() {
            guard += 1;
            assert!(guard < 2000, "poll did not finish");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(job.finished);
        assert!(!job.hits.is_empty());
        assert!(job.scanned > 0);
    }
}

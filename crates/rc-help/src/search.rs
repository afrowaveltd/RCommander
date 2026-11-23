//! Full-text search across a [`HelpIndex`].

use crate::plain::to_plain_text;
use crate::topic::HelpIndex;

/// Score of a hit on the topic title; always larger than [`BODY_SCORE`].
const TITLE_SCORE: u32 = 1000;

/// Score of a hit on a body line.
const BODY_SCORE: u32 = 500;

/// Upper bound on the number of hits returned by [`search`].
const MAX_HITS: usize = 100;

/// One search result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    /// Id of the topic the match was found in.
    pub topic_id: String,
    /// Topic title in the language that was actually used.
    pub title: String,
    /// The matching line of text (the title itself for a title match).
    pub line: String,
    /// Relevance: title matches score higher than body matches, and repeated
    /// occurrences raise the score slightly.
    pub score: u32,
    /// Language the match was found in — the requested one, or the fallback
    /// language when the topic is not translated.
    pub language: String,
}

/// Case-insensitive search across the whole index.
///
/// Matching is done on lower-cased text (`to_lowercase`, so non-ASCII case
/// folding works for Latin-1/Cyrillic/Greek scripts). An empty or
/// whitespace-only `query` returns no hits.
///
/// Every topic is loaded with [`HelpIndex::body`], so the language fallback
/// rules apply: a topic that has no `lang` translation is searched in
/// `fallback_lang`, and reported with that language. A single result is emitted
/// per matching line, plus one for the title; results are sorted by descending
/// score, then by topic id and line, and truncated to 100 hits.
///
/// # Examples
///
/// ```no_run
/// # use std::path::Path;
/// let index = rc_help::HelpIndex::load(Path::new("Docs")).unwrap();
/// for hit in rc_help::search(&index, "preview", "cs", "en") {
///     println!("{}: {}", hit.topic_id, hit.line);
/// }
/// ```
pub fn search(index: &HelpIndex, query: &str, lang: &str, fallback_lang: &str) -> Vec<SearchHit> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<SearchHit> = Vec::new();
    for topic in index.topics() {
        let Some(document) = index.body(&topic.id, lang, fallback_lang) else {
            continue;
        };
        let title_lower = document.title.to_lowercase();
        let title_matched = title_lower.contains(&needle);
        if title_matched {
            hits.push(SearchHit {
                topic_id: topic.id.clone(),
                title: document.title.clone(),
                line: document.title.clone(),
                score: TITLE_SCORE + occurrences(&title_lower, &needle),
                language: document.language.clone(),
            });
        }
        let text = to_plain_text(&document.blocks);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let lowered = line.to_lowercase();
            if !lowered.contains(&needle) {
                continue;
            }
            // The title usually appears again as the first heading line; do not
            // report the same match twice.
            if title_matched && lowered == title_lower {
                continue;
            }
            hits.push(SearchHit {
                topic_id: topic.id.clone(),
                title: document.title.clone(),
                line: line.to_string(),
                score: BODY_SCORE + occurrences(&lowered, &needle),
                language: document.language.clone(),
            });
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.topic_id.cmp(&b.topic_id))
            .then_with(|| a.line.cmp(&b.line))
    });
    hits.truncate(MAX_HITS);
    hits
}

/// Number of non-overlapping occurrences, capped so scores stay small.
fn occurrences(haystack: &str, needle: &str) -> u32 {
    if needle.is_empty() {
        return 0;
    }
    (haystack.matches(needle).count().min(9)) as u32
}

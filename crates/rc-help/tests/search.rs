//! Search behaviour: ranking, language fallback and degenerate queries.

mod common;

use common::Fixture;
use rc_help::search;

#[test]
fn finds_a_phrase_in_the_body() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let hits = search(&index, "archive engine", "en", "en");
    assert_eq!(hits.len(), 1, "hits: {hits:#?}");
    let hit = &hits[0];
    assert_eq!(hit.topic_id, "Packing");
    assert_eq!(hit.title, "Packing");
    assert_eq!(hit.line, "Packing uses the archive engine.");
    assert_eq!(hit.language, "en");
    assert!(hit.score >= 500);
}

#[test]
fn search_is_case_insensitive() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let lower = search(&index, "remote servers", "en", "en");
    let upper = search(&index, "REMOTE SERVERS", "en", "en");
    assert_eq!(lower.len(), 1);
    assert_eq!(lower[0].topic_id, "FTP connections");
    assert_eq!(
        lower.iter().map(|hit| hit.line.clone()).collect::<Vec<_>>(),
        upper.iter().map(|hit| hit.line.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn title_match_ranks_above_body_match() {
    let fixture = Fixture::new();
    let index = fixture.index();

    // "previewer" appears in the title of the Previewer topic and in the body
    // of the Keyboard shortcuts topic.
    let hits = search(&index, "previewer", "en", "en");
    assert!(hits.len() >= 2, "hits: {hits:#?}");
    assert_eq!(hits[0].topic_id, "Previewer");
    assert_eq!(hits[0].line, "Previewer");

    let body_hit = hits
        .iter()
        .find(|hit| hit.topic_id == "Keyboard shortcuts")
        .expect("body match in Keyboard shortcuts");
    assert_eq!(body_hit.line, "Press F3 to open the previewer.");
    assert!(
        hits[0].score > body_hit.score,
        "title {} vs body {}",
        hits[0].score,
        body_hit.score
    );

    // Sorted by descending score.
    assert!(hits.windows(2).all(|pair| pair[0].score >= pair[1].score));
}

#[test]
fn search_uses_the_fallback_language_and_reports_it() {
    let fixture = Fixture::new();
    let index = fixture.index();

    // Requested Czech: only Previewer and Settings have cs.
    let hits = search(&index, "remote", "cs", "en");
    assert_eq!(hits.len(), 1, "hits: {hits:#?}");
    assert_eq!(hits[0].topic_id, "FTP connections");
    assert_eq!(hits[0].language, "en", "fallback language must be reported");

    let czech = search(&index, "Soubor", "cs", "en");
    assert_eq!(czech.len(), 1);
    assert_eq!(czech[0].topic_id, "Previewer");
    assert_eq!(czech[0].language, "cs");
    // The Czech body is searched, not the English one.
    assert!(search(&index, "Soubor", "en", "en").is_empty());
}

#[test]
fn search_without_fallback_available_still_finds_a_language() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let hits = search(&index, "Deutsch", "en", "en");
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|hit| hit.topic_id == "Only German"));
    assert!(hits.iter().all(|hit| hit.language == "de"));
    // The German title matches first.
    assert_eq!(hits[0].line, "Nur Deutsch");
    assert!(hits[0].score >= 1000);
}

#[test]
fn empty_query_returns_no_hits() {
    let fixture = Fixture::new();
    let index = fixture.index();

    assert!(search(&index, "", "en", "en").is_empty());
    assert!(search(&index, "   ", "en", "en").is_empty());
    assert!(search(&index, "\t\n ", "cs", "en").is_empty());
}

#[test]
fn unknown_query_returns_no_hits() {
    let fixture = Fixture::new();
    let index = fixture.index();

    assert!(search(&index, "zzzz-not-present", "en", "en").is_empty());
}

#[test]
fn hits_are_not_duplicated_between_title_and_body() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let hits = search(&index, "Packing", "en", "en");
    let packing: Vec<_> = hits
        .iter()
        .filter(|hit| hit.topic_id == "Packing")
        .collect();
    // Exactly one title hit: the heading line of the body is not repeated.
    assert_eq!(packing.iter().filter(|hit| hit.score >= 1000).count(), 1);
    // The body line is still reported.
    assert!(
        packing
            .iter()
            .any(|hit| hit.line == "Packing uses the archive engine.")
    );
}

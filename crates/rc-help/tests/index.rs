//! Index discovery, titles, language fallback, link resolution and the
//! translation-completeness report, all on real `tempfile` trees.

mod common;

use std::fs;

use common::{EXPECTED_ORDER, Fixture, write, write_bytes};
use rc_help::HelpIndex;

#[test]
fn discovers_topics_with_spaces_and_mixed_case_ids() {
    let fixture = Fixture::new();
    let index = fixture.index();

    assert_eq!(fixture.ids(), EXPECTED_ORDER);
    assert_eq!(index.root(), fixture.docs());

    let ftp = index
        .topic("FTP connections")
        .expect("FTP connections topic");
    assert_eq!(ftp.id, "FTP connections");
    assert_eq!(ftp.relative_path, "FTP connections");
    assert_eq!(ftp.languages, vec!["en".to_string()]);

    // A stray markdown file in the root must not become a topic.
    assert!(index.topic("README").is_none());
    assert!(index.topic("Docs").is_none());
    // Hidden folders are ignored.
    assert!(index.topic("hidden").is_none());
    assert!(index.topic("Hidden").is_none());
    // The stray non-markdown file inside a topic folder is harmless.
    assert!(index.topic("notes").is_none());
    assert_eq!(index.topics().len(), EXPECTED_ORDER.len());
}

#[test]
fn order_prefix_sorts_first_and_is_stripped_from_the_id() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let first = &index.topics()[0];
    assert_eq!(first.id, "Keyboard shortcuts");
    assert_eq!(first.order, 1);
    // relative_path carries the stripped folder name as well
    assert_eq!(first.relative_path, "Keyboard shortcuts");
    assert!(
        index.topics()[1..]
            .iter()
            .all(|topic| topic.order == u32::MAX)
    );

    // The raw folder name still resolves to the topic.
    assert_eq!(
        index
            .topic("01 - Keyboard shortcuts")
            .map(|topic| topic.id.as_str()),
        Some("Keyboard shortcuts")
    );
}

#[test]
fn deep_nesting_keeps_id_and_relative_path() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let nested = index
        .topic("Advanced/Packing")
        .expect("nested by relative path");
    assert_eq!(nested.id, "Packing");
    assert_eq!(nested.relative_path, "Advanced/Packing");
    assert_eq!(nested.title_for("en", "en"), Some("Packing"));
    assert_eq!(
        index.topic("Packing").map(|t| t.relative_path.as_str()),
        Some("Advanced/Packing")
    );
}

#[test]
fn languages_are_a_sorted_union() {
    let fixture = Fixture::new();
    let index = fixture.index();

    assert_eq!(
        index.languages(),
        vec!["cs".to_string(), "de".to_string(), "en".to_string()]
    );
    let previewer = index.topic("Previewer").expect("previewer");
    assert_eq!(
        previewer.languages,
        vec!["cs".to_string(), "en".to_string()]
    );
}

#[test]
fn titles_come_from_the_first_heading_and_fall_back_to_the_folder_name() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let previewer = index.topic("Previewer").expect("previewer");
    assert_eq!(previewer.title_for("en", "en"), Some("Previewer"));
    assert_eq!(previewer.title_for("cs", "en"), Some("Prohlížeč"));
    // Unknown language falls through to the fallback language.
    assert_eq!(previewer.title_for("de", "en"), Some("Previewer"));
    assert_eq!(previewer.title_for("de", "cs"), Some("Prohlížeč"));

    // A file without any heading uses the topic id (the folder name).
    let no_heading = index.topic("No heading").expect("no heading");
    assert_eq!(
        no_heading.title.get("en").map(String::as_str),
        Some("No heading")
    );
    assert_eq!(no_heading.title_for("en", "en"), Some("No heading"));

    // The default fallback chain of title_for still finds something.
    assert_eq!(no_heading.title_for("zz", "yy"), Some("No heading"));
}

#[test]
fn body_uses_the_requested_language_when_available() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let document = index.body("Previewer", "cs", "en").expect("body");
    assert_eq!(document.topic_id, "Previewer");
    assert_eq!(document.language, "cs");
    assert_eq!(document.requested_language, "cs");
    assert!(!document.used_fallback);
    assert_eq!(document.title, "Prohlížeč");
    assert!(
        document.source_path.ends_with("cs.md"),
        "path: {:?}",
        document.source_path
    );
    assert!(
        fs::read_to_string(&document.source_path)
            .expect("source readable")
            .contains("Soubor")
    );
    assert!(!document.blocks.is_empty());
    assert_eq!(document.link_targets, vec!["../Settings/cs.md".to_string()]);
}

#[test]
fn body_falls_back_and_records_the_requested_language() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let document = index.body("FTP connections", "cs", "en").expect("body");
    assert_eq!(document.language, "en");
    assert_eq!(document.requested_language, "cs");
    assert!(document.used_fallback);
    assert_eq!(document.title, "FTP connections");
    assert!(document.source_path.ends_with("en.md"));
}

#[test]
fn body_falls_back_to_any_language_when_neither_is_available() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let document = index.body("Only German", "cs", "en").expect("body");
    assert_eq!(document.language, "de");
    assert_eq!(document.requested_language, "cs");
    assert!(document.used_fallback);
    assert_eq!(document.title, "Nur Deutsch");
}

#[test]
fn body_of_a_missing_topic_is_none() {
    let fixture = Fixture::new();
    let index = fixture.index();

    assert!(index.body("Does not exist", "en", "en").is_none());
    assert!(index.body("", "en", "en").is_none());
    assert!(index.body("   ", "en", "en").is_none());
}

#[test]
fn body_reports_link_targets_and_link_resolution() {
    let fixture = Fixture::new();
    let index = fixture.index();

    let document = index.body("Previewer", "en", "en").expect("body");
    assert_eq!(
        document.link_targets,
        vec![
            "../Settings/en.md".to_string(),
            "../01 - Keyboard shortcuts/en.md".to_string(),
        ]
    );
    assert_eq!(
        index
            .resolve_link("Previewer", &document.link_targets[0])
            .as_deref(),
        Some("Settings")
    );
    assert_eq!(
        index
            .resolve_link("Previewer", &document.link_targets[1])
            .as_deref(),
        Some("Keyboard shortcuts")
    );
}

#[test]
fn resolve_link_accepts_relative_names_and_root_absolute_paths() {
    let fixture = Fixture::new();
    let index = fixture.index();

    // `./` relative to the topic folder.
    assert_eq!(
        index
            .resolve_link("FTP connections", "./Previewer/en.md")
            .as_deref(),
        Some("Previewer")
    );
    // Bare topic name.
    assert_eq!(
        index
            .resolve_link("FTP connections", "Previewer")
            .as_deref(),
        Some("Previewer")
    );
    // `../` back to the root.
    assert_eq!(
        index
            .resolve_link("Previewer", "../Settings/en.md")
            .as_deref(),
        Some("Settings")
    );
    // Repo-absolute style.
    assert_eq!(
        index
            .resolve_link("Previewer", "/Docs/Settings/en.md")
            .as_deref(),
        Some("Settings")
    );
    // Nested target with and without the language file.
    assert_eq!(
        index
            .resolve_link("Previewer", "./Advanced/Packing/en.md")
            .as_deref(),
        Some("Packing")
    );
    assert_eq!(
        index
            .resolve_link("Previewer", "Advanced/Packing")
            .as_deref(),
        Some("Packing")
    );
    // A link to the topic's own language file stays on the topic.
    assert_eq!(
        index.resolve_link("Previewer", "./en.md").as_deref(),
        Some("Previewer")
    );
    // Anchors and angle brackets.
    assert_eq!(
        index
            .resolve_link("Previewer", "<../Settings/en.md#top>")
            .as_deref(),
        Some("Settings")
    );
    // Windows separators are tolerated.
    assert_eq!(
        index
            .resolve_link("Previewer", "..\\Settings\\en.md")
            .as_deref(),
        Some("Settings")
    );
}

#[test]
fn unresolvable_and_external_links_yield_none() {
    let fixture = Fixture::new();
    let index = fixture.index();

    assert_eq!(index.resolve_link("Previewer", "../Nope/en.md"), None);
    assert_eq!(index.resolve_link("Previewer", "Nope"), None);
    assert_eq!(index.resolve_link("Previewer", ""), None);
    assert_eq!(index.resolve_link("Previewer", "#section"), None);
    assert_eq!(
        index.resolve_link("Previewer", "../Settings/en.md#anchor"),
        Some("Settings".to_string())
    );
    assert_eq!(
        index.resolve_link("Previewer", "https://example.invalid/x.md"),
        None
    );
    assert_eq!(
        index.resolve_link("Previewer", "http://example.invalid"),
        None
    );
    assert_eq!(index.resolve_link("Previewer", "mailto:a@b.c"), None);
    // Excess `..` is clamped at the root instead of failing.
    assert_eq!(
        index
            .resolve_link("Previewer", "../../../../Settings/en.md")
            .as_deref(),
        Some("Settings")
    );
}

#[test]
fn missing_translations_lists_expected_languages() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let expected = vec!["en".to_string(), "cs".to_string()];

    let report = index.missing_translations(&expected);
    assert_eq!(
        report,
        vec![
            ("Keyboard shortcuts".to_string(), vec!["cs".to_string()]),
            ("FTP connections".to_string(), vec!["cs".to_string()]),
            ("No heading".to_string(), vec!["cs".to_string()]),
            (
                "Only German".to_string(),
                vec!["en".to_string(), "cs".to_string()]
            ),
            ("Packing".to_string(), vec!["cs".to_string()]),
        ]
    );

    // Case-insensitive, de-duplicated, empty-safe expectations.
    assert_eq!(
        index.missing_translations(&["EN".to_string(), " en ".to_string(), String::new()]),
        vec![("Only German".to_string(), vec!["en".to_string()])]
    );
    assert!(index.missing_translations(&[]).is_empty());
    // A language nobody provides is reported for every topic.
    assert_eq!(
        index.missing_translations(&["fr".to_string()]).len(),
        index.topics().len()
    );
}

#[test]
fn missing_root_yields_an_empty_index() {
    let fixture = Fixture::new();
    let missing = fixture.temp().join("no-such-docs");

    let index = HelpIndex::load(&missing).expect("missing root is not an error");
    assert_eq!(index.root(), missing.as_path());
    assert!(index.topics().is_empty());
    assert!(index.languages().is_empty());
    assert!(index.topic("Previewer").is_none());
    assert!(index.body("Previewer", "en", "en").is_none());
    assert!(index.resolve_link("Previewer", "Previewer").is_none());
    assert!(index.missing_translations(&["en".to_string()]).is_empty());
}

#[test]
fn root_that_is_a_file_yields_an_empty_index() {
    let fixture = Fixture::new();
    let file = fixture.temp().join("not-a-directory");
    fs::write(&file, "hello").expect("write");

    let index = HelpIndex::load(&file).expect("file root is not an error");
    assert!(index.topics().is_empty());
}

#[test]
fn empty_docs_root_is_tolerated() {
    let temp = tempfile::tempdir().expect("tempdir");
    let docs = temp.path().join("Docs");
    fs::create_dir_all(&docs).expect("mkdir");

    let index = HelpIndex::load(&docs).expect("empty root");
    assert!(index.topics().is_empty());
    assert!(index.languages().is_empty());
}

#[test]
fn non_utf8_and_odd_content_is_tolerated() {
    let temp = tempfile::tempdir().expect("tempdir");
    let docs = temp.path().join("Docs");

    // Invalid UTF-8 bytes around a valid heading.
    write_bytes(
        &docs,
        "Weird/en.md",
        b"# Weird \xff\xfe title\n\nBody \xff text.\n",
    );
    // CRLF, a BOM and tabs.
    write(
        &docs,
        "Windows/en.md",
        "\u{feff}# Windows\r\n\r\n\t- item one\r\n\t- item two\r\n",
    );
    // A file that is not markdown at all.
    write(&docs, "Plain/en.md", "Just a sentence, no markup.\n");

    let index = HelpIndex::load(&docs).expect("load odd tree");
    assert_eq!(index.topics().len(), 3);

    let weird = index.body("Weird", "en", "en").expect("lossy body");
    assert!(weird.title.starts_with("Weird"), "title: {}", weird.title);
    assert!(!weird.blocks.is_empty());

    let windows = index.body("Windows", "en", "en").expect("crlf body");
    assert_eq!(windows.title, "Windows");
    assert!(
        matches!(windows.blocks[1], rc_help::Block::BulletList(_)),
        "{:?}",
        windows.blocks
    );

    let plain = index.body("Plain", "en", "en").expect("plain body");
    assert_eq!(
        rc_help::to_plain_text(&plain.blocks),
        "Just a sentence, no markup."
    );
}

#[test]
fn topic_lookup_is_forgiving() {
    let fixture = Fixture::new();
    let index = fixture.index();

    assert_eq!(
        index.topic("previewer").map(|t| t.id.as_str()),
        Some("Previewer")
    );
    assert_eq!(
        index.topic("  Previewer  ").map(|t| t.id.as_str()),
        Some("Previewer")
    );
    assert_eq!(
        index.topic("Advanced/Packing").map(|t| t.id.as_str()),
        Some("Packing")
    );
    assert_eq!(
        index.topic("advanced/packing").map(|t| t.id.as_str()),
        Some("Packing")
    );
    assert_eq!(
        index.topic("Advanced\\Packing").map(|t| t.id.as_str()),
        Some("Packing")
    );
    assert_eq!(
        index.topic("no heading").map(|t| t.id.as_str()),
        Some("No heading")
    );
}

#[cfg(unix)]
#[test]
fn symlink_cycle_does_not_hang_or_duplicate_topics() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    // Docs/Previewer/loop -> Docs  (a cycle)
    symlink(fixture.docs(), fixture.docs().join("Previewer/loop")).expect("symlink");

    let index = HelpIndex::load(fixture.docs()).expect("load with cycle");
    assert_eq!(index.topics().len(), EXPECTED_ORDER.len());
    assert_eq!(fixture.ids(), EXPECTED_ORDER);

    // A self-referential folder is also fine.
    symlink(fixture.docs(), fixture.docs().join("Self")).expect("self symlink");
    let index = HelpIndex::load(fixture.docs()).expect("load with self link");
    assert_eq!(fixture.ids(), EXPECTED_ORDER);
    assert!(index.topic("Self").is_none());
}

#[test]
fn same_id_in_two_folders_is_deduplicated_deterministically() {
    let temp = tempfile::tempdir().expect("tempdir");
    let docs = temp.path().join("Docs");
    write(&docs, "Previewer/en.md", "# From plain folder\n");
    write(&docs, "Advanced/Previewer/en.md", "# From nested folder\n");

    let index = HelpIndex::load(&docs).expect("load");
    assert_eq!(index.topics().len(), 1);
    let topic = index.topic("Previewer").expect("one topic");
    // The lexicographically first relative path wins.
    assert_eq!(topic.relative_path, "Advanced/Previewer");
    assert_eq!(topic.title_for("en", "en"), Some("From nested folder"));
    assert!(index.body("Previewer", "en", "en").is_some());
}

#[test]
fn uppercase_language_extension_and_case_insensitive_languages() {
    let temp = tempfile::tempdir().expect("tempdir");
    let docs = temp.path().join("Docs");
    write(&docs, "Topic/EN.md", "# English title\n");
    write(&docs, "Topic/CS.MD", "# Český titulek\n");

    let index = HelpIndex::load(&docs).expect("load");
    let topic = index.topic("Topic").expect("topic");
    assert_eq!(topic.languages, vec!["cs".to_string(), "en".to_string()]);

    let requested = index.body("Topic", "CS", "en").expect("body");
    assert_eq!(requested.language, "cs");
    assert!(!requested.used_fallback);
    assert_eq!(requested.title, "Český titulek");
}

#[test]
fn a_file_deleted_after_loading_does_not_panic() {
    let temp = tempfile::tempdir().expect("tempdir");
    let docs = temp.path().join("Docs");
    write(&docs, "Gone/en.md", "# Gone\n\nSoon removed.\n");

    let index = HelpIndex::load(&docs).expect("load");
    assert!(index.body("Gone", "en", "en").is_some());

    fs::remove_file(docs.join("Gone/en.md")).expect("remove");
    // The topic is still indexed but reading it now degrades to `None`.
    assert!(index.topic("Gone").is_some());
    assert!(index.body("Gone", "en", "en").is_none());
}

#[test]
fn rich_document_plain_text_has_no_markdown_syntax() {
    let temp = tempfile::tempdir().expect("tempdir");
    let docs = temp.path().join("Docs");
    write(
        &docs,
        "Rich/en.md",
        "# Rich page\n\
         \n\
         Open with **F3** or `Enter`, see [Previewer](../Previewer/en.md).\n\
         \n\
         - first *item*\n\
         - second `item`\n\
         \n\
         1. one\n\
         2. two\n\
         \n\
         > a quote\n\
         \n\
         | Key | Action |\n\
         | :-- | -----: |\n\
         | F3 | view |\n\
         \n\
         ```\n\
         plain code line\n\
         ```\n\
         \n\
         ---\n",
    );
    write(&docs, "Previewer/en.md", "# Previewer\n\nTarget page.\n");

    let index = HelpIndex::load(&docs).expect("load");
    let document = index.body("Rich", "en", "en").expect("body");
    let text = rc_help::to_plain_text(&document.blocks);

    assert!(text.contains("Open with F3 or Enter, see Previewer."));
    assert!(text.contains("plain code line"));
    assert!(text.contains("view"));
    for marker in ["#", "**", "`", "](", "](", "|", "> "] {
        assert!(
            !text.contains(marker),
            "plain text must not contain {marker:?}:\n{text}"
        );
    }
    assert_eq!(
        index
            .resolve_link("Rich", &document.link_targets[0])
            .as_deref(),
        Some("Previewer")
    );
}

#[test]
fn utf16_files_saved_by_notepad_are_decoded() {
    let temp = tempfile::tempdir().expect("tempdir");
    let docs = temp.path().join("Docs");

    // "Unicode" in Windows Notepad: UTF-16LE with a BOM.
    let body = "# Windows Unicode\n\nUloženo v UTF-16.\n";
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    for unit in body.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    write_bytes(&docs, "Notepad/en.md", &bytes);

    let index = HelpIndex::load(&docs).expect("load utf16 tree");
    let document = index.body("Notepad", "en", "en").expect("utf16 body");
    assert_eq!(document.title, "Windows Unicode");
    assert_eq!(
        rc_help::to_plain_text(&document.blocks),
        "Windows Unicode\n\nUloženo v UTF-16."
    );
}

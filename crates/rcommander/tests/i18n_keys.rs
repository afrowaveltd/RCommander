//! Guard against untranslated UI keys.
//!
//! The UI asks the translator for a key; a key that is missing from a language
//! pack silently falls back to the key itself, which is how labels such as
//! `cmd.config` ended up in the menus. This test reads the actual UI sources (they
//! are compiled into the test binary with `include_str!`) and fails when a key the
//! interface uses is absent from a pack, or when the two packs drift apart.

use std::collections::BTreeSet;

/// UI sources whose `t("…")`/`tf("…")` keys must exist in every pack.
const SOURCES: &[&str] = &[
    include_str!("../src/ui/app.rs"),
    include_str!("../src/ui/dialogs.rs"),
    include_str!("../src/ui/panel_view.rs"),
    include_str!("../src/ui/help_window.rs"),
    include_str!("../src/ui/viewer_window.rs"),
    include_str!("../src/ui/transfer_window.rs"),
    include_str!("../src/ui/agent_panel.rs"),
    include_str!("../src/model/commands.rs"),
    include_str!("../src/model/mod.rs"),
    include_str!("../src/model/dialogs.rs"),
    include_str!("../src/agent_host.rs"),
];

/// Keys that appear as static strings in the sources but are not translation keys.
const NOT_KEYS: &[&str] = &[
    "a.tar.gz",
    "a.txt",
    "a.zip",
    "f.txt",
    "report.txt",
    "status.of",
    "x.txt",
];

fn packs() -> Vec<(String, BTreeSet<String>)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    ["locales/en.toml", "locales/cs.toml"]
        .iter()
        .map(|relative| {
            let path = root.join(relative);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let value: toml::Value =
                toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let strings = value
                .get("strings")
                .and_then(|s| s.as_table())
                .unwrap_or_else(|| panic!("{} has no [strings]", path.display()));
            (relative.to_string(), strings.keys().cloned().collect())
        })
        .collect()
}

/// Extract dotted keys passed to `t("…")`, `tf("…")` and the label tables.
fn used_keys() -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for source in SOURCES {
        for pattern in [".t(\"", ".tf(\"", "label_key: \"", "title_key: \""] {
            let mut rest = *source;
            while let Some(position) = rest.find(pattern) {
                rest = &rest[position + pattern.len()..];
                let end = match rest.find('"') {
                    Some(end) => end,
                    None => break,
                };
                let key = &rest[..end];
                if key.contains('.')
                    && key.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                    && !NOT_KEYS.contains(&key)
                {
                    keys.insert(key.to_string());
                }
            }
        }
    }
    keys
}

#[test]
fn every_ui_key_exists_in_both_language_packs() {
    let keys = used_keys();
    assert!(
        keys.len() > 100,
        "expected to find the UI vocabulary, found {} keys",
        keys.len()
    );
    for (name, available) in packs() {
        let missing: Vec<&String> = keys
            .iter()
            .filter(|key| !available.contains(*key))
            .collect();
        assert!(
            missing.is_empty(),
            "{name} is missing {} UI keys: {missing:#?}",
            missing.len()
        );
    }
}

#[test]
fn the_two_packs_offer_the_same_vocabulary() {
    let packs = packs();
    let (english_name, english) = &packs[0];
    let (czech_name, czech) = &packs[1];
    let only_english: Vec<&String> = english.difference(czech).collect();
    let only_czech: Vec<&String> = czech.difference(english).collect();
    assert!(
        only_english.is_empty(),
        "{english_name} has keys {czech_name} lacks: {only_english:#?}"
    );
    assert!(
        only_czech.is_empty(),
        "{czech_name} has keys {english_name} lacks: {only_czech:#?}"
    );
}

#[test]
fn no_pack_value_is_empty_or_still_a_key() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for relative in ["locales/en.toml", "locales/cs.toml"] {
        let text = std::fs::read_to_string(root.join(relative)).unwrap();
        let value: toml::Value = toml::from_str(&text).unwrap();
        let strings = value.get("strings").and_then(|s| s.as_table()).unwrap();
        for (key, value) in strings {
            let text = value
                .as_str()
                .unwrap_or_else(|| panic!("{relative}: {key} is not a string"));
            assert!(!text.trim().is_empty(), "{relative}: {key} is empty");
            assert_ne!(text, key, "{relative}: {key} is not translated");
        }
    }
}

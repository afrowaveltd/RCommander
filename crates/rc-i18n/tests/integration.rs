//! End-to-end tests of the published API, exercising exactly the calls the GUI
//! makes: discover packs on disk, pick a language, render strings with
//! placeholders and report translation completeness.

use std::path::Path;

use rc_i18n::{
    Direction, Locale, Translator, base_code, detect_text_direction, direction_for_code, english,
    text_is_rtl, validate_pack,
};

/// The pack a user might drop into `locales/` by hand.
const HEBREW: &str = r#"
# A pack written by a contributor, not by the program.
[meta]
code = "he"
native_name = "עברית"
name = "Hebrew"

[strings]
"app.title" = "RCommander"
"cmd.copy" = "העתק"
"cmd.quit" = "יציאה"
"cmd.unknown_to_english" = "לא בשימוש"
"panel.selection_of" = "{count} מתוך {total} נבחרו"
"#;

#[test]
fn a_user_supplied_pack_dir_drives_the_whole_interface() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("he.toml"), HEBREW).expect("write hebrew pack");
    // the bundled Czech pack, as the installer would ship it
    std::fs::write(
        dir.path().join("cs.toml"),
        include_str!("../locales/cs.toml"),
    )
    .expect("write czech pack");

    // ---- start-up ---------------------------------------------------------
    let locales = Translator::load_dir(dir.path()).expect("packs load");
    assert_eq!(locales.len(), 2);
    let mut translator = Translator::new(english(), locales);
    assert_eq!(translator.language(), "en");
    let offered: Vec<String> = translator
        .available()
        .iter()
        .map(|meta| meta.selector_label())
        .collect();
    assert_eq!(
        offered,
        vec!["Čeština (Czech)", "English", "עברית (Hebrew)"]
    );

    // ---- English baseline -------------------------------------------------
    assert_eq!(translator.direction(), Direction::Ltr);
    assert_eq!(translator.t("menu.file"), "&File");
    assert_eq!(
        translator.t_args("panel.selection_of", &[("count", "2"), ("total", "5")]),
        "2 of 5 selected"
    );
    assert!(translator.has("cmd.copy"));
    assert!(!translator.has("cmd.not_a_key"));

    // ---- a complete translation ------------------------------------------
    assert!(translator.set_language("CS"));
    assert_eq!(translator.language(), "cs");
    // menu captions carry the keyboard mnemonic marker in both languages
    assert_eq!(translator.t("menu.file"), "&Soubor");
    assert_eq!(
        translator.t_args("panel.free_space", &[("size", "1,2 GB")]),
        "Volné místo: 1,2 GB"
    );
    assert!(translator.missing_keys("cs").is_empty());
    assert_eq!(translator.completeness("cs"), Some(1.0));
    assert!(translator.extra_keys("cs").is_empty());

    // ---- an incomplete right-to-left translation -------------------------
    assert!(translator.set_language("he"));
    assert_eq!(translator.direction(), Direction::Rtl);
    assert_eq!(translator.t("cmd.copy"), "העתק");
    assert_eq!(
        translator.t_args("panel.selection_of", &[("count", "2"), ("total", "5")]),
        "2 מתוך 5 נבחרו"
    );
    // untranslated keys come from English, never from an empty label
    assert_eq!(translator.t("menu.file"), "&File");
    let missing = translator.missing_keys("he");
    assert!(missing.contains(&"dialog.copy.title".to_string()));
    assert!(!missing.contains(&"cmd.copy".to_string()));
    assert_eq!(missing.len(), english().len() - 4);
    // a key the English pack dropped is reported the other way round
    assert_eq!(
        translator.extra_keys("he"),
        vec!["cmd.unknown_to_english".to_string()]
    );
    // a key unknown everywhere is shown verbatim so the gap is visible
    assert_eq!(translator.t("cmd.not_a_key"), "cmd.not_a_key");
    let completeness = translator.completeness("he").expect("known language");
    assert!(
        completeness > 0.0 && completeness < 0.1,
        "completeness: {completeness}"
    );

    // ---- per-string direction for mixed content --------------------------
    assert_eq!(detect_text_direction("report-2024.pdf"), Direction::Ltr);
    assert!(text_is_rtl("דוח שנתי"));
    assert!(!text_is_rtl("report.pdf"));
    assert_eq!(direction_for_code("he-IL"), Direction::Rtl);
    assert_eq!(base_code("he-IL"), "he");

    // ---- the "check language pack" dialog --------------------------------
    let meta = validate_pack(HEBREW).expect("hand written pack is valid");
    assert_eq!(meta.code, "he");
    assert_eq!(meta.direction, Direction::Rtl);
    assert_eq!(meta.string_count, 5);
    assert_eq!(meta.version, 1);

    let problem = validate_pack("[meta]\ncode = \"he\"\n\n[strings]\n\"Cmd.Copy\" = \"העתק\"\n")
        .expect_err("typo in key must be reported");
    assert!(problem.contains("invalid key"), "problem: {problem}");
}

#[test]
fn a_refreshed_pack_replaces_the_running_translation() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut translator = Translator::discover(dir.path()).expect("empty dir is fine");
    assert_eq!(translator.available().len(), 1);

    // the user drops in a fresh file while the application is running
    std::fs::write(
        dir.path().join("de.toml"),
        "[meta]\ncode = \"de\"\nnative_name = \"Deutsch\"\n\n[strings]\n\"cmd.copy\" = \"Kopieren\"\n",
    )
    .expect("write german pack");
    for locale in Translator::load_dir(dir.path()).expect("packs load") {
        translator.insert(locale);
    }
    assert!(translator.set_language("de"));
    assert_eq!(translator.t("cmd.copy"), "Kopieren");
    assert_eq!(translator.language(), "de");
    assert_eq!(translator.untranslated("de"), english().len() - 1);
}

#[test]
fn the_built_in_pack_covers_the_documented_gui_vocabulary() {
    let english = english();
    // grouped prefixes that the GUI addresses as concrete widgets
    for prefix in [
        "app.", "menu.", "cmd.", "panel.", "col.", "dialog.", "viewer.", "status.", "ftp.",
        "agent.", "error.",
    ] {
        assert!(
            english.keys().any(|key| key.starts_with(prefix)),
            "no key for prefix {prefix}"
        );
    }
    // the exact key the status bar asks for, with both spellings of deselect
    assert_eq!(english.get("cmd.select_all"), Some("Select all"));
    assert_eq!(
        english.get("cmd.deselect_all"),
        english.get("cmd_deselect_all")
    );
    // nothing is a placeholder-only or whitespace-only string
    for (key, value) in english.iter() {
        assert!(!value.trim().is_empty(), "key {key} is empty");
    }
}

#[test]
fn loading_a_missing_directory_is_not_an_error() {
    let translator = Translator::discover(Path::new("/definitely/not/here")).expect("missing dir");
    assert_eq!(translator.language(), "en");
    assert_eq!(translator.t("app.title"), "RCommander");
}

#[test]
fn a_broken_pack_can_be_skipped_by_the_caller() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("broken.toml"), "[meta]\ncode = ").expect("write broken pack");
    // load_dir reports the broken file ...
    assert!(Translator::load_dir(dir.path()).is_err());
    // ... and the caller can validate each file and carry on with the good ones
    let mut good = Vec::new();
    for entry in std::fs::read_dir(dir.path()).expect("read dir") {
        let path = entry.expect("entry").path();
        let text = std::fs::read_to_string(&path).expect("read pack");
        if validate_pack(&text).is_ok() {
            good.push(Locale::load(&path).expect("validated pack loads"));
        }
    }
    assert!(good.is_empty());
    let translator = Translator::new(english(), good);
    assert_eq!(translator.t("cmd.copy"), "Copy");
}

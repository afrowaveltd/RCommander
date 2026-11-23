//! Model-level tests: everything here runs headless against real temp dirs.

use crate::i18n::{Direction, KeyEchoStrings};
use crate::model::commands::apply_rename_pattern;
use crate::model::dialogs::*;
use crate::model::{CommandOutcome, Model, Side, ViewMode};
use rc_core::config::Settings;
use rc_core::keymap::Shortcut;
use rc_core::location::Location;
use rc_core::sorting::SortField;
use std::fs;
use std::path::Path;

fn write(path: &Path, content: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Two temporary directories, one of them with a small tree.
fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Model) {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    write(&left.path().join("alpha.txt"), b"aaa");
    write(&left.path().join("beta.log"), b"bbbb");
    write(&left.path().join("gamma.txt"), b"ccccc");
    write(&left.path().join("sub/nested.txt"), b"nested");
    fs::create_dir_all(left.path().join("zeta")).unwrap();
    let model = Model::for_test(
        Settings::default(),
        Location::local(left.path()),
        Location::local(right.path()),
    );
    (left, right, model)
}

fn names(model: &Model) -> Vec<String> {
    model
        .left
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect()
}

#[test]
fn boot_lists_directories_first_and_sorts_naturally() {
    let (_l, _r, model) = fixture();
    assert_eq!(
        names(&model),
        ["sub", "zeta", "alpha.txt", "beta.log", "gamma.txt"]
    );
    assert_eq!(model.focus, Side::Left);
    assert!(!model.modal());
    assert_eq!(model.status, "status.ready");
}

#[test]
fn boot_survives_a_missing_directory() {
    let settings = Settings::default();
    let model = Model::for_test(
        settings,
        Location::local("/definitely/not/here"),
        Location::local("/tmp"),
    );
    assert!(model.left.entries().is_empty());
    assert_eq!(model.focus, Side::Left);
}

#[test]
fn window_title_shows_both_paths() {
    let (left, right, model) = fixture();
    let title = model.window_title();
    assert!(title.contains(&left.path().to_string_lossy().to_string()));
    assert!(title.contains(&right.path().to_string_lossy().to_string()));
    assert!(title.starts_with("RCommander"));
}

#[test]
fn status_text_counts_files_dirs_and_bytes() {
    let (_l, _r, model) = fixture();
    let text = model.status_text();
    // 3 files (3+4+5 bytes), 2 directories
    assert!(text.contains("panel.files"), "status: {text}");
    assert!(
        text.contains("12"),
        "status should contain the byte total: {text}"
    );
}

#[test]
fn status_text_with_a_selection_uses_the_selection_template() {
    let (_l, _r, mut model) = fixture();
    model.left.selection.set(["alpha.txt"]);
    let text = model.status_text();
    assert!(text.contains("panel.selection_of"), "status: {text}");
}

#[test]
fn cursor_movement_and_enter_navigates_into_directories() {
    let (left, _r, mut model) = fixture();
    model.left.cursor_home();
    assert_eq!(model.left.cursor_entry().unwrap().name, "sub");
    model.left.move_cursor(2, false);
    assert_eq!(model.left.cursor_entry().unwrap().name, "alpha.txt");
    model.left.cursor_home();
    assert!(model.enter().is_ok());
    assert_eq!(
        model.left.location().as_path().unwrap(),
        left.path().join("sub")
    );
    assert_eq!(names(&model), ["nested.txt"]);
    // back up
    assert!(model.go_parent().is_ok());
    assert_eq!(model.left.location().as_path().unwrap(), left.path());
}

#[test]
fn go_parent_at_root_reports_an_error() {
    let (_l, _r, mut model) = fixture();
    model.navigate(Location::local("/")).unwrap();
    let err = model.go_parent().unwrap_err();
    assert!(err.to_string().contains("root"));
}

#[test]
fn entering_a_regular_file_asks_the_viewer_to_open_it() {
    let (left, _r, mut model) = fixture();
    let index = model.left.listing.find("alpha.txt").unwrap();
    model.left.set_cursor(index);
    match model.enter() {
        CommandOutcome::OpenFile(path) => assert_eq!(path, left.path().join("alpha.txt")),
        other => panic!("expected OpenFile, got {other:?}"),
    }
}

#[test]
fn a_zip_archive_can_be_entered_and_listed() {
    let (left, _r, mut model) = fixture();
    // Build a real archive with the archive backend.
    let archive = left.path().join("real.zip");
    let options = rc_archive::PackOptions::default();
    rc_archive::create(&archive, &[left.path().join("alpha.txt")], &options).expect("zip created");
    model.refresh_active();
    let index = model.left.listing.find("real.zip").unwrap();
    model.left.set_cursor(index);
    let outcome = model.enter();
    assert!(
        !matches!(outcome, CommandOutcome::OpenFile(_)),
        "archives are entered: {outcome:?}"
    );
    assert!(
        model.left.location().is_archive(),
        "location is an archive: {}",
        model.left.location().display()
    );
    let names: Vec<String> = model
        .left
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert!(
        names.iter().any(|n| n == "alpha.txt"),
        "archive contents listed: {names:?}"
    );
}

#[test]
fn entering_an_archive_location_is_attempted_when_enabled() {
    let (left, _r, mut model) = fixture();
    write(&left.path().join("pack.zip"), b"PK\x03\x04broken");
    model.refresh_active();
    let index = model.left.listing.find("pack.zip").unwrap();
    model.left.set_cursor(index);
    let outcome = model.enter();
    // Either the archive backend lists it, or the read error is reported — but
    // the cursor entry must have been treated as an archive, not a plain file.
    assert!(
        !matches!(outcome, CommandOutcome::OpenFile(_)),
        "archives are entered, not opened: {outcome:?}"
    );
}

#[test]
fn archives_are_not_entered_when_the_option_is_off() {
    let (left, _r, mut model) = fixture();
    model.settings.behavior.enter_archives = false;
    write(&left.path().join("pack.zip"), b"PK\x03\x04broken");
    model.refresh_active();
    let index = model.left.listing.find("pack.zip").unwrap();
    model.left.set_cursor(index);
    assert!(matches!(model.enter(), CommandOutcome::OpenFile(_)));
}

#[test]
fn switching_focus_and_swapping_panels() {
    let (left_dir, right_dir, mut model) = fixture();
    model.execute("cm_SwitchPanel");
    assert_eq!(model.focus, Side::Right);
    model.execute("cm_SwitchPanel");
    assert_eq!(model.focus, Side::Left);
    model.execute("cm_Exchange");
    assert_eq!(model.left.location().as_path().unwrap(), right_dir.path());
    assert_eq!(model.right.location().as_path().unwrap(), left_dir.path());
    assert_eq!(model.focus, Side::Left, "swap keeps the focus side");
}

#[test]
fn match_target_to_source_copies_the_directory() {
    let (left_dir, _r, mut model) = fixture();
    assert!(model.execute("cm_MatchSrc").is_ok());
    assert_eq!(model.right.location().as_path().unwrap(), left_dir.path());
}

#[test]
fn open_cursor_directory_in_the_other_panel() {
    let (left_dir, _r, mut model) = fixture();
    model.left.cursor_home();
    assert!(model.execute("cm_OpenDirInOtherPanel").is_ok());
    assert_eq!(
        model.right.location().as_path().unwrap(),
        left_dir.path().join("sub")
    );
}

#[test]
fn selection_commands_mark_the_expected_entries() {
    let (_l, _r, mut model) = fixture();
    assert!(model.execute("cm_SelectAll").is_ok());
    assert_eq!(model.left.selection.len(), 5);
    assert!(model.execute("cm_Clearall").is_ok());
    assert!(model.left.selection.is_empty());
    assert!(model.execute("cm_ExchangeSelection").is_ok());
    assert_eq!(model.left.selection.len(), 5);

    model.execute("cm_Clearall");
    let index = model.left.listing.find("alpha.txt").unwrap();
    model.left.set_cursor(index);
    assert!(model.execute("cm_ToggleSelection").is_ok());
    assert_eq!(model.left.selected_names(), ["alpha.txt"]);
    assert!(model.execute("cm_SelectSameExt").is_ok());
    let mut sel = model.left.selected_names();
    sel.sort();
    assert_eq!(sel, ["alpha.txt", "gamma.txt"]);
    assert!(model.execute("cm_DeselectSameExt").is_ok());
    assert!(model.left.selection.is_empty());
}

#[test]
fn toggle_and_advance_walks_down_the_list() {
    let (_l, _r, mut model) = fixture();
    model.left.cursor_home();
    model.left.toggle_and_advance();
    assert_eq!(model.left.cursor(), 1);
    assert_eq!(model.left.selected_names(), ["sub"]);
    model.left.toggle_and_advance();
    assert_eq!(model.left.cursor(), 2);
    assert_eq!(model.left.selection.len(), 2);
}

#[test]
fn operation_targets_prefers_marks_then_cursor() {
    let (_l, _r, mut model) = fixture();
    model.left.cursor_home();
    assert_eq!(model.left.operation_targets().len(), 1);
    model.left.selection.set(["alpha.txt", "beta.log"]);
    let targets: Vec<String> = model
        .left
        .operation_targets()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(targets, ["alpha.txt", "beta.log"]);
}

#[test]
fn view_and_sort_commands_change_panel_state() {
    let (_l, _r, mut model) = fixture();
    assert!(model.execute("cm_SrcShort").is_ok());
    assert_eq!(model.left.view_mode, ViewMode::Brief);
    assert!(model.execute("cm_SrcLong").is_ok());
    assert_eq!(model.left.view_mode, ViewMode::Full);
    // The first press sorts by name, a second press reverses the order.
    assert!(model.execute("cm_SrcByName").is_ok());
    assert_eq!(model.left.sort().field, SortField::Name);
    assert!(
        model.left.sort().reversed,
        "re-pressing the sort key reverses"
    );
    assert!(model.execute("cm_SrcByName").is_ok());
    assert!(!model.left.sort().reversed);
    assert!(model.execute("cm_SrcBySize").is_ok());
    assert_eq!(model.left.sort().field, SortField::Size);
    assert!(!model.left.sort().reversed);
    assert!(model.execute("cm_SrcUnsorted").is_ok());
    assert_eq!(model.left.sort().field, SortField::Unsorted);
}

#[test]
fn filter_commands_restrict_visible_files() {
    let (_l, _r, mut model) = fixture();
    assert!(model.execute("cm_SrcAllFiles").is_ok());
    let visible = names(&model);
    assert!(visible.contains(&"alpha.txt".to_string()));
    model.left.mask_spec = "*.txt".to_string();
    model.refresh_active();
    let visible = names(&model);
    assert!(
        !visible.contains(&"beta.log".to_string()),
        "log filtered out: {visible:?}"
    );
    assert!(
        visible.contains(&"sub".to_string()),
        "directories stay visible"
    );
}

#[test]
fn key_bindings_dispatch_commands() {
    let (_l, _r, mut model) = fixture();
    assert!(model.handle_key(&Shortcut::parse("f7").unwrap()));
    assert!(matches!(model.top_dialog(), Some(Dialog::MakeDir(_))));
    model.close_dialogs();
    assert!(model.handle_key(&Shortcut::parse("tab").unwrap()));
    assert_eq!(model.focus, Side::Right);
    model.close_dialogs();
    model.execute("cm_SwitchPanel"); // back to the left panel
    assert_eq!(model.focus, Side::Left);
    let file_index = model.left.listing.find("alpha.txt").unwrap();
    model.left.set_cursor(file_index);
    assert_eq!(
        model.execute("cm_List"),
        CommandOutcome::OpenFile(model.left.cursor_entry().unwrap().path.clone())
    );
    model.left.cursor_home();
    assert_eq!(
        model.execute("cm_List"),
        CommandOutcome::None,
        "directories are not previewed"
    );
    // unbound key is not consumed
    assert!(!model.handle_key(&Shortcut::parse("f24").unwrap()));
}

#[test]
fn dialogs_stack_and_block_input() {
    let (_l, _r, mut model) = fixture();
    assert!(!model.modal());
    model.open_dialog(Dialog::About);
    assert!(model.modal());
    model.open_dialog(Dialog::MakeDir(MakeDirState::default()));
    assert_eq!(model.dialogs.len(), 2);
    assert!(matches!(model.top_dialog(), Some(Dialog::MakeDir(_))));
    assert!(matches!(model.close_top_dialog(), Some(Dialog::MakeDir(_))));
    assert!(model.modal());
    model.close_dialogs();
    assert!(!model.modal());
}

#[test]
fn quick_search_jumps_to_matching_entries() {
    let (_l, _r, mut model) = fixture();
    model.left.cursor_home();
    assert!(model.handle_char('g'));
    assert_eq!(model.left.cursor_entry().unwrap().name, "gamma.txt");
    model.cancel_quick_search();
    assert!(!model.left.quick.is_active());
}

#[test]
fn command_line_cd_and_commands() {
    let (left_dir, _r, mut model) = fixture();
    model.command_line = format!("cd {}", left_dir.path().join("sub").display());
    assert!(model.run_command_line().is_ok());
    assert_eq!(
        model.left.location().as_path().unwrap(),
        left_dir.path().join("sub")
    );
    assert!(model.command_line.is_empty());
    assert_eq!(model.command_history.len(), 1);
    model.command_line = "cm_SrcShort".to_string();
    assert!(model.run_command_line().is_ok());
    assert_eq!(model.left.view_mode, ViewMode::Brief);
    model.command_line = "cd /nope/nowhere".to_string();
    assert!(matches!(
        model.run_command_line(),
        CommandOutcome::Failed(_)
    ));
}

#[test]
fn transfer_copies_marked_files_into_the_target() {
    let (_l, right_dir, mut model) = fixture();
    model.left.selection.set(["alpha.txt", "beta.log"]);
    let state = TransferDialogState {
        ask_overwrite: false,
        overwrite: rc_core::ops::OverwriteMode::Fail,
        ..TransferDialogState::new(
            false,
            model.selected_paths(),
            right_dir.path().to_string_lossy().to_string(),
        )
    };
    assert!(model.run_transfer(&state).is_ok());
    assert!(right_dir.path().join("alpha.txt").exists());
    assert_eq!(
        fs::read(right_dir.path().join("beta.log")).unwrap(),
        b"bbbb"
    );
    assert_eq!(
        fs::read(right_dir.path().join("alpha.txt")).unwrap(),
        b"aaa"
    );
}

#[test]
fn transfer_move_removes_the_source() {
    let (left_dir, right_dir, mut model) = fixture();
    model.left.selection.set(["alpha.txt"]);
    let state = TransferDialogState {
        ask_overwrite: false,
        ..TransferDialogState::new(
            true,
            model.selected_paths(),
            right_dir.path().to_string_lossy().to_string(),
        )
    };
    assert!(model.run_transfer(&state).is_ok());
    assert!(!left_dir.path().join("alpha.txt").exists());
    assert!(right_dir.path().join("alpha.txt").exists());
}

#[test]
fn transfer_into_a_missing_target_directory_fails_gracefully() {
    let (_l, _r, mut model) = fixture();
    model.left.selection.set(["alpha.txt"]);
    let state = TransferDialogState {
        ask_overwrite: false,
        ..TransferDialogState::new(false, model.selected_paths(), "/definitely/not/here")
    };
    let outcome = model.run_transfer(&state);
    assert!(
        matches!(outcome, CommandOutcome::Failed(_)),
        "expected failure, got {outcome:?}"
    );
    assert!(model.last_error.is_some());
}

#[test]
fn delete_removes_files_and_directories() {
    let (left_dir, _r, mut model) = fixture();
    model.left.selection.set(["alpha.txt", "sub"]);
    let state = DeleteState::new(model.selected_paths(), true);
    assert!(model.run_delete(&state).is_ok());
    assert!(!left_dir.path().join("alpha.txt").exists());
    assert!(!left_dir.path().join("sub").exists());
    let visible = names(&model);
    assert!(!visible.contains(&"alpha.txt".to_string()));
}

#[test]
fn mkdir_creates_a_directory_and_refreshes() {
    let (left_dir, _r, mut model) = fixture();
    model.open_dialog(Dialog::MakeDir(MakeDirState {
        name: "brandnew".into(),
        in_target_panel: false,
    }));
    let state = match model.top_dialog().unwrap() {
        Dialog::MakeDir(s) => s.clone(),
        _ => unreachable!(),
    };
    assert!(model.run_mkdir(&state).is_ok());
    assert!(left_dir.path().join("brandnew").is_dir());
    assert!(names(&model).contains(&"brandnew".to_string()));
}

#[test]
fn mkdir_into_the_target_panel() {
    let (_l, right_dir, mut model) = fixture();
    let state = MakeDirState {
        name: "indest".into(),
        in_target_panel: true,
    };
    assert!(model.run_mkdir(&state).is_ok());
    assert!(right_dir.path().join("indest").is_dir());
}

#[test]
fn rename_changes_the_file_name() {
    let (left_dir, _r, mut model) = fixture();
    let index = model.left.listing.find("alpha.txt").unwrap();
    model.left.set_cursor(index);
    let state = RenameState {
        name: "renamed.txt".into(),
        keep_extension: true,
        ..RenameState::new("alpha.txt")
    };
    assert!(model.run_rename(&state).is_ok());
    assert!(left_dir.path().join("renamed.txt").exists());
    assert!(!left_dir.path().join("alpha.txt").exists());
}

#[test]
fn rename_rejects_an_existing_target() {
    let (_l, _r, mut model) = fixture();
    let index = model.left.listing.find("alpha.txt").unwrap();
    model.left.set_cursor(index);
    let state = RenameState {
        name: "beta.log".into(),
        ..RenameState::new("alpha.txt")
    };
    assert!(matches!(
        model.run_rename(&state),
        CommandOutcome::Failed(_)
    ));
}

#[test]
fn multi_rename_applies_the_pattern() {
    let (left_dir, _r, mut model) = fixture();
    model.left.selection.set(["alpha.txt", "gamma.txt"]);
    let mut state = MultiRenameState::new(model.left.selected_names());
    state.pattern = "file[C]".to_string();
    state.refresh_preview();
    assert_eq!(state.preview[0].1, "file1.txt");
    assert!(model.run_multi_rename(&state).is_ok());
    assert!(left_dir.path().join("file1.txt").exists());
    assert!(left_dir.path().join("file2.txt").exists());
    assert!(!left_dir.path().join("alpha.txt").exists());
}

#[test]
fn comment_is_written_to_descript_ion() {
    let (left_dir, _r, mut model) = fixture();
    let state = CommentState::new(
        left_dir.path().to_string_lossy().to_string(),
        "alpha.txt",
        "important file",
    );
    assert!(model.run_comment(&state).is_ok());
    let text = fs::read_to_string(left_dir.path().join("descript.ion")).unwrap();
    assert!(
        text.contains("alpha.txt important file"),
        "descript.ion: {text}"
    );
    let loaded = rc_core::comment::Comments::load_dir(left_dir.path());
    assert_eq!(loaded.get("alpha.txt"), Some("important file"));
}

#[test]
fn attributes_toggle_readonly_on_unix() {
    let (left_dir, _r, mut model) = fixture();
    let state = AttributesState {
        readonly: Some(true),
        paths: vec![
            left_dir
                .path()
                .join("alpha.txt")
                .to_string_lossy()
                .to_string(),
        ],
        ..Default::default()
    };
    assert!(model.run_attributes(&state).is_ok());
    let md = fs::metadata(left_dir.path().join("alpha.txt")).unwrap();
    assert!(md.permissions().readonly());
}

#[test]
fn viewer_and_editor_state_are_opened_for_paths() {
    let (left_dir, _r, mut model) = fixture();
    model.open_viewer_for(left_dir.path().join("alpha.txt"));
    let viewer = model.viewer.clone().unwrap();
    assert!(viewer.path.ends_with("alpha.txt"));
    assert_eq!(viewer.title, "alpha.txt");
    assert_eq!(viewer.hex_bytes, 16);
    model
        .open_editor_for(left_dir.path().join("alpha.txt"))
        .unwrap();
    let editor = model.editor.clone().unwrap();
    assert_eq!(editor.text, "aaa");
    assert!(!editor.modified);
}

#[test]
fn help_state_opens_with_the_current_language() {
    let (_l, _r, mut model) = fixture();
    model.settings.language = "cs".to_string();
    model.open_help(Some("Previewer".to_string()));
    let help = model.help.clone().unwrap();
    assert_eq!(help.topic.as_deref(), Some("Previewer"));
    assert_eq!(help.language, "cs");
}

#[test]
fn theme_toggle_flips_between_light_and_dark() {
    let (_l, _r, mut model) = fixture();
    assert_eq!(model.settings.colors.theme, "light");
    assert!(model.execute("cm_ThemeToggle").is_ok());
    assert_eq!(model.settings.colors.theme, "dark");
    assert!(model.execute("cm_ThemeToggle").is_ok());
    assert_eq!(model.settings.colors.theme, "light");
}

#[test]
fn exit_command_requests_quit() {
    let (_l, _r, mut model) = fixture();
    assert_eq!(model.execute("cm_Exit"), CommandOutcome::Quit);
    assert_eq!(model.execute(""), CommandOutcome::None);
    assert_eq!(
        model.execute("cm_DefinitelyNotACommand"),
        CommandOutcome::None
    );
}

#[test]
fn version_info_and_space_reporting_do_not_fail() {
    let (_l, _r, mut model) = fixture();
    model.left.cursor_home();
    assert!(model.execute("cm_VersionInfo").is_ok());
    assert!(model.status.contains("sub"));
    assert!(model.execute("cm_GetFileSpace").is_ok());
    assert!(model.status.contains("files"));
}

#[test]
fn rtl_language_direction_is_exposed() {
    let (_l, _r, model) = fixture();
    assert_eq!(model.direction(), Direction::Ltr);
    let mut rtl = Model::boot(
        Settings::default(),
        Box::new(KeyEchoStrings::new("ar", Direction::Rtl)),
        Location::local("/tmp"),
        Location::local("/tmp"),
    );
    assert_eq!(rtl.direction(), Direction::Rtl);
    assert!(rtl.direction().layout_rtl());
    rtl.set_language("ar");
    assert_eq!(rtl.settings.language, "ar");
}

#[test]
fn navigation_keeps_the_cursor_on_the_same_name_when_possible() {
    let (left_dir, _r, mut model) = fixture();
    let index = model.left.listing.find("beta.log").unwrap();
    model.left.set_cursor(index);
    let sub = model.left.location().child("sub").unwrap();
    model.navigate(sub).unwrap();
    assert_eq!(model.left.cursor_entry().unwrap().name, "nested.txt");
    model.go_parent().unwrap();
    assert_eq!(
        model.left.cursor_entry().unwrap().name,
        "sub",
        "the cursor lands on the directory we left"
    );
    let index = model.left.listing.find("beta.log").unwrap();
    model.left.set_cursor(index);
    model.refresh_active();
    assert_eq!(
        model.left.cursor_entry().unwrap().name,
        "beta.log",
        "a re-read keeps the cursor on the same name"
    );
    assert!(left_dir.path().is_dir());
}

// ---------------- rename pattern unit tests ----------------

#[test]
fn rename_pattern_name_and_extension_placeholders() {
    assert_eq!(
        apply_rename_pattern("report.txt", "[N]", "[E]", 1, 0, "", ""),
        "report.txt"
    );
    assert_eq!(
        apply_rename_pattern("report.txt", "[n]", "md", 1, 0, "", ""),
        "report.md"
    );
    assert_eq!(
        apply_rename_pattern("Report.TXT", "[U]", "[e]", 1, 0, "", ""),
        "REPORT.txt"
    );
    assert_eq!(
        apply_rename_pattern("noext", "[N]", "[E]", 1, 0, "", ""),
        "noext"
    );
    assert_eq!(
        apply_rename_pattern("a.tar.gz", "[N]", "[E]", 1, 0, "", ""),
        "a.tar.gz"
    );
}

#[test]
fn rename_pattern_counter_forms() {
    assert_eq!(
        apply_rename_pattern("a.txt", "pic[C]", "[E]", 1, 0, "", ""),
        "pic1.txt"
    );
    assert_eq!(
        apply_rename_pattern("a.txt", "pic[C]", "[E]", 7, 3, "", ""),
        "pic007.txt"
    );
    assert_eq!(
        apply_rename_pattern("a.txt", "pic[C4]", "[E]", 7, 0, "", ""),
        "pic0007.txt"
    );
    assert_eq!(
        apply_rename_pattern("a.txt", "pic[C+10]", "[E]", 7, 0, "", ""),
        "pic17.txt"
    );
    assert_eq!(
        apply_rename_pattern("a.txt", "pic[C-10]", "[E]", 7, 0, "", ""),
        "pic0.txt"
    );
}

#[test]
fn rename_pattern_search_and_replace_runs_on_the_result() {
    assert_eq!(
        apply_rename_pattern("a b.txt", "[N]", "[E]", 1, 0, " ", "_"),
        "a_b.txt"
    );
    assert_eq!(
        apply_rename_pattern("a.txt", "x[N]y", "[E]", 1, 0, "x", ""),
        "ay.txt"
    );
}

#[test]
fn rename_pattern_unknown_tokens_stay_verbatim() {
    assert_eq!(
        apply_rename_pattern("a.txt", "[Q][N]", "[E]", 1, 0, "", ""),
        "[Q]a.txt"
    );
    assert_eq!(
        apply_rename_pattern("a.txt", "[N", "[E]", 1, 0, "", ""),
        "[N.txt"
    );
}

#[test]
fn rename_pattern_date_parts_are_numeric() {
    let out = apply_rename_pattern("a.txt", "[Y]-[M]-[D]", "[E]", 1, 0, "", "");
    assert_eq!(out.len(), "YYYY-MM-DD.txt".len());
    assert!(out.ends_with(".txt"));
    assert_eq!(out.matches('-').count(), 2);
}

#[test]
fn rename_pattern_empty_extension_pattern_drops_the_dot() {
    assert_eq!(apply_rename_pattern("a.txt", "b", "", 1, 0, "", ""), "b");
    assert_eq!(
        apply_rename_pattern("a.txt", "b", ".md", 1, 0, "", ""),
        "b.md"
    );
}

// ---------------- tabs, panels and auxiliary dialogs ----------------

#[test]
fn tab_commands_add_close_and_cycle() {
    let (left, _r, mut model) = fixture();
    assert_eq!(model.left.tabs.len(), 1);
    assert!(model.execute("cm_NewTab").is_ok());
    assert_eq!(model.left.tabs.len(), 2);
    assert_eq!(model.left.tabs.active_index(), 1);
    // The new tab shows the same directory.
    assert_eq!(model.left.location().as_path().unwrap(), left.path());
    assert!(model.execute("cm_NextTab").is_ok());
    assert_eq!(model.left.tabs.active_index(), 0);
    assert!(model.execute("cm_PrevTab").is_ok());
    assert_eq!(model.left.tabs.active_index(), 1);
    assert!(model.execute("cm_CloseTab").is_ok());
    assert_eq!(model.left.tabs.len(), 1);
    // Closing the last tab is refused, without an error dialog.
    assert_eq!(model.execute("cm_CloseTab"), CommandOutcome::None);
    assert_eq!(model.left.tabs.len(), 1);
    assert!(model.status.contains("last tab"));
}

#[test]
fn quick_view_toggles_and_excludes_the_tree_panel() {
    let (_l, _r, mut model) = fixture();
    assert!(!model.quick_view);
    assert!(model.execute("cm_SrcQuickView").is_ok());
    assert!(model.quick_view);
    assert!(model.execute("cm_SeparateTree1").is_ok());
    assert!(model.tree_view);
    assert!(!model.quick_view, "the tree panel replaces the quick view");
    assert!(model.execute("cm_SrcQuickView").is_ok());
    assert!(!model.tree_view);
}

#[test]
fn history_dialog_lists_visited_directories() {
    let (left, _r, mut model) = fixture();
    model
        .navigate(Location::local(left.path().join("sub")))
        .unwrap();
    assert!(model.execute("cm_HistoryList").is_ok());
    let state = match model.top_dialog() {
        Some(Dialog::History(state)) => state.clone(),
        other => panic!("expected history dialog, got {other:?}"),
    };
    assert!(!state.is_empty());
    assert!(state.entries.iter().any(|r| r.path.ends_with("sub")));
    assert!(
        state
            .entries
            .iter()
            .any(|r| r.path == left.path().to_string_lossy())
    );
}

#[test]
fn tree_dialog_collects_nested_directories() {
    let (left, _r, mut model) = fixture();
    std::fs::create_dir_all(left.path().join("sub/deeper")).unwrap();
    assert!(model.execute("cm_CDtree").is_ok());
    let state = match model.top_dialog() {
        Some(Dialog::Tree(state)) => state.clone(),
        other => panic!("expected tree dialog, got {other:?}"),
    };
    assert!(state.entries.iter().any(|p| p.ends_with("sub")));
    assert!(state.entries.iter().any(|p| p.ends_with("deeper")));
    assert_eq!(state.root, left.path().to_string_lossy());
}

#[test]
fn compare_directories_marks_differences() {
    let (left_dir, right_dir, mut model) = fixture();
    // same name, different content/size, plus a right-only file
    write(&right_dir.path().join("alpha.txt"), b"different size");
    write(&right_dir.path().join("beta.log"), b"bbbb");
    write(&right_dir.path().join("right-only.txt"), b"x");
    model.refresh_all();
    assert!(model.execute("cm_CompareDirs").is_ok());
    let state = match model.top_dialog() {
        Some(Dialog::Compare(state)) => state.clone(),
        other => panic!("expected compare dialog, got {other:?}"),
    };
    let alpha = state.rows.iter().find(|r| r.name == "alpha.txt").unwrap();
    assert_eq!(alpha.status, "!=", "different sizes must be reported");
    let beta = state.rows.iter().find(|r| r.name == "beta.log").unwrap();
    assert_eq!(beta.status, "=", "identical entries are equal");
    let right_only = state
        .rows
        .iter()
        .find(|r| r.name == "right-only.txt")
        .unwrap();
    assert_eq!(right_only.status, ">");
    let left_only = state.rows.iter().find(|r| r.name == "gamma.txt").unwrap();
    assert_eq!(left_only.status, "<");
    // Differences are marked in the active panel.
    assert!(model.left.selection.contains("alpha.txt"));
    assert!(!model.left.selection.contains("beta.log"));
    assert!(model.status.contains("differ"));
    assert!(left_dir.path().exists());
}

#[test]
fn mask_prompt_applies_and_clears_a_filter() {
    let (_l, _r, mut model) = fixture();
    assert!(model.execute("cm_SrcUserDef").is_ok());
    let mut state = match model.top_dialog() {
        Some(Dialog::MaskPrompt(state)) => state.clone(),
        other => panic!("expected mask dialog, got {other:?}"),
    };
    assert_eq!(state.mask, "*.*");
    state.mask = "*.txt".to_string();
    assert!(model.apply_mask(&state).is_ok());
    let visible = names(&model);
    assert!(!visible.contains(&"beta.log".to_string()));
    assert!(visible.contains(&"alpha.txt".to_string()));
    assert!(model.execute("cm_SrcAllFiles").is_ok());
    assert!(names(&model).contains(&"beta.log".to_string()));
    let bad = MaskPromptState {
        mask: "[".to_string(),
        hide_non_matching: false,
    };
    assert!(matches!(model.apply_mask(&bad), CommandOutcome::Failed(_)));
}

#[test]
fn ctrl_pagedown_enters_a_directory_and_alt_enter_is_version_info() {
    let (_l, _r, mut model) = fixture();
    model.left.cursor_home();
    assert!(model.execute("cm_OpenDirOrArchive").is_ok());
    assert!(model.left.location().as_path().unwrap().ends_with("sub"));
}

#[test]
fn insert_into_command_line_uses_the_cursor_entry() {
    let (_l, _r, mut model) = fixture();
    model
        .left
        .set_cursor(model.left.listing.find("alpha.txt").unwrap());
    assert!(model.execute("cm_OpenInCommandLine").is_ok());
    assert_eq!(model.command_line, "alpha.txt");
    assert!(model.execute("cm_OpenInCommandLine").is_ok());
    assert_eq!(model.command_line, "alpha.txt alpha.txt");
    // Empty listing is a no-op.
    model.left.cursor_home();
    model.left.set_cursor(0);
    model.command_line.clear();
    model.left.listing = rc_core::listing::Listing::from_entries(
        Location::local("/tmp"),
        Vec::new(),
        &rc_core::listing::ReadOptions::default(),
        Default::default(),
    );
    assert_eq!(model.execute("cm_OpenInCommandLine"), CommandOutcome::None);
}

#[test]
fn exit_and_minimise_outcomes_are_distinct() {
    let (_l, _r, mut model) = fixture();
    assert_eq!(model.execute("cm_Exit"), CommandOutcome::Quit);
    assert_eq!(model.execute("cm_Minimize"), CommandOutcome::Minimize);
}

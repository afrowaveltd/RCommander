//! Model-level tests: everything here runs headless against real temp dirs.

use crate::i18n::{Direction, KeyEchoStrings};
use crate::model::commands::apply_rename_pattern;
use crate::model::dialogs::*;
use crate::model::{CommandOutcome, Model, ResetReport, ResetScope, Side, ViewMode};
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
fn entering_a_regular_file_asks_the_system_to_open_it() {
    // Enter and a double-click mean "the default action": the system opens the file in whatever it
    // associates with it, or runs it if it is a program. The built-in viewer stays on F3.
    let (left, _r, mut model) = fixture();
    let index = model.left.listing.find("alpha.txt").unwrap();
    model.left.set_cursor(index);
    match model.enter() {
        CommandOutcome::OpenExternally(path) => {
            assert_eq!(path, left.path().join("alpha.txt"))
        }
        other => panic!("expected OpenExternally, got {other:?}"),
    }
}

#[test]
fn entering_a_folder_still_navigates_instead_of_launching_it() {
    let (_left, _r, mut model) = fixture();
    let index = model.left.listing.find("sub").unwrap();
    model.left.set_cursor(index);
    assert_eq!(model.enter(), CommandOutcome::Ok);
    assert!(
        model
            .left
            .location()
            .as_path()
            .is_some_and(|path| path.ends_with("sub")),
        "the panel entered the folder: {}",
        model.left.location().display()
    );
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
        !matches!(
            outcome,
            CommandOutcome::OpenFile(_) | CommandOutcome::OpenExternally(_)
        ),
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
        !matches!(
            outcome,
            CommandOutcome::OpenFile(_) | CommandOutcome::OpenExternally(_)
        ),
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
    assert!(matches!(model.enter(), CommandOutcome::OpenExternally(_)));
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

// ---------------- background operations ----------------

#[test]
fn a_background_copy_runs_and_reports_completion() {
    let (left_dir, right_dir, mut model) = fixture();
    model.left.selection.set(["alpha.txt", "beta.log"]);
    let state = TransferDialogState {
        ask_overwrite: false,
        background: true,
        overwrite: rc_core::ops::OverwriteMode::Overwrite,
        ..TransferDialogState::new(
            false,
            model.selected_paths(),
            right_dir.path().to_string_lossy().to_string(),
        )
    };
    let job = model.start_transfer(&state).expect("job started");
    assert!(job > 0);
    assert!(
        model.is_busy(),
        "the job must be running right after it was started"
    );
    assert!(model.transfer_progress().is_some());
    // Poll until the worker finishes (bounded wait).
    let mut guard = 0;
    while model.poll_transfers() {
        guard += 1;
        if guard > 2000 {
            panic!("background copy did not finish");
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!model.is_busy());
    assert!(right_dir.path().join("alpha.txt").exists());
    assert!(right_dir.path().join("beta.log").exists());
    assert_eq!(
        fs::read(right_dir.path().join("alpha.txt")).unwrap(),
        b"aaa"
    );
    assert!(
        model.status.contains("status.done"),
        "status: {}",
        model.status
    );
    assert!(left_dir.path().exists());
}

#[test]
fn a_background_move_removes_the_sources() {
    let (left_dir, right_dir, mut model) = fixture();
    model.left.selection.set(["gamma.txt"]);
    let state = TransferDialogState {
        ask_overwrite: false,
        ..TransferDialogState::new(
            true,
            model.selected_paths(),
            right_dir.path().to_string_lossy().to_string(),
        )
    };
    model.start_transfer(&state).expect("job started");
    let mut guard = 0;
    while model.poll_transfers() {
        guard += 1;
        assert!(guard < 2000, "move did not finish");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!left_dir.path().join("gamma.txt").exists());
    assert!(right_dir.path().join("gamma.txt").exists());
}

#[test]
fn a_background_delete_removes_the_targets() {
    let (left_dir, _r, mut model) = fixture();
    let state = DeleteState::new(
        vec![
            left_dir
                .path()
                .join("alpha.txt")
                .to_string_lossy()
                .to_string(),
        ],
        true,
    );
    let job = model.start_delete(&state);
    assert!(job > 0);
    let mut guard = 0;
    while model.poll_transfers() {
        guard += 1;
        assert!(guard < 2000, "delete did not finish");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!left_dir.path().join("alpha.txt").exists());
}

#[test]
fn an_invalid_transfer_is_not_started() {
    let (_l, _r, mut model) = fixture();
    let state = TransferDialogState::new(false, Vec::new(), "/tmp");
    assert!(model.start_transfer(&state).is_none());
    assert!(!model.is_busy());
    assert!(model.transfer_progress().is_none());
}

#[test]
fn a_failing_background_copy_reports_the_error() {
    let (_l, _r, mut model) = fixture();
    model.left.selection.set(["alpha.txt"]);
    let state = TransferDialogState {
        ask_overwrite: false,
        ..TransferDialogState::new(
            false,
            model.selected_paths(),
            "/proc/definitely/not/writable",
        )
    };
    model.start_transfer(&state).expect("job started");
    let mut guard = 0;
    while model.poll_transfers() {
        guard += 1;
        assert!(guard < 2000, "failing copy did not finish");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(model.last_error.is_some(), "a failure must be reported");
}

// ---------------- search through the model ----------------

#[test]
fn a_background_search_fills_the_dialog_and_can_be_navigated() {
    let (left, _r, mut model) = fixture();
    let search_root = left.path().to_string_lossy().to_string();
    let mut state = SearchState::new(search_root);
    state.name_mask = "*.txt".to_string();
    model.open_dialog(Dialog::Search(state.clone()));
    assert!(model.start_search(&state));
    let mut guard = 0;
    while model.poll_search() {
        guard += 1;
        assert!(guard < 3000, "search did not finish");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let results = match model.top_dialog() {
        Some(Dialog::Search(dialog)) => dialog.results.clone(),
        other => panic!("expected the search dialog, got {other:?}"),
    };
    assert!(!results.is_empty(), "the fixture contains .txt files");
    assert!(results.iter().any(|h| h.path.ends_with("alpha.txt")));
    assert!(!results.iter().any(|h| h.path.ends_with("beta.log")));
    assert!(model.status.contains("dialog.search.results_count"));

    // Navigating to a result opens its directory in the other panel.
    let index = results
        .iter()
        .position(|h| h.path.ends_with("nested.txt"))
        .expect("nested result");
    assert_eq!(model.goto_search_hit(index), CommandOutcome::Ok);
    assert_eq!(
        model.right.location().as_path().unwrap(),
        left.path().join("sub")
    );
    assert_eq!(model.right.cursor_entry().unwrap().name, "nested.txt");
}

#[test]
fn a_search_without_matches_reports_not_found() {
    let (left, _r, mut model) = fixture();
    let mut state = SearchState::new(left.path().to_string_lossy().to_string());
    state.name_mask = "*.nothing-here".to_string();
    model.open_dialog(Dialog::Search(state.clone()));
    model.start_search(&state);
    let mut guard = 0;
    while model.poll_search() {
        guard += 1;
        assert!(guard < 3000);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        model.status.contains("dialog.search.not_found"),
        "status: {}",
        model.status
    );
}

#[test]
fn a_started_search_can_be_cancelled() {
    let (left, _r, mut model) = fixture();
    let mut state = SearchState::new(left.path().to_string_lossy().to_string());
    state.name_mask = "*.txt".to_string();
    model.open_dialog(Dialog::Search(state.clone()));
    model.start_search(&state);
    model.cancel_search();
    assert!(model.search_job.is_none());
    let running = match model.top_dialog() {
        Some(Dialog::Search(dialog)) => dialog.running,
        _ => false,
    };
    assert!(!running, "the dialog must stop claiming it is running");
}

#[test]
fn an_invalid_search_is_rejected() {
    let (_l, _r, mut model) = fixture();
    let state = SearchState::new("");
    assert!(!model.start_search(&state));
    assert!(model.search_job.is_none());
    assert!(model.last_error.is_some());
}

// ---------------- synchronise directories ----------------

fn sync_fixture() -> (tempfile::TempDir, tempfile::TempDir, Model) {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    write(&left.path().join("both.txt"), b"same");
    write(&right.path().join("both.txt"), b"same");
    write(&left.path().join("left-only.txt"), b"left");
    write(&right.path().join("right-only.txt"), b"right");
    write(&left.path().join("newer.txt"), b"left version");
    write(&right.path().join("newer.txt"), b"right version");
    let model = Model::for_test(
        Settings::default(),
        Location::local(left.path()),
        Location::local(right.path()),
    );
    (left, right, model)
}

#[test]
fn sync_compare_classifies_differences() {
    let (left, right, mut model) = sync_fixture();
    let mut state = SyncState {
        left: left.path().to_string_lossy().to_string(),
        right: right.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    assert!(model.sync_compare(&mut state).is_ok());
    let status = |name: &str| {
        state
            .rows
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.status.clone())
            .unwrap_or_default()
    };
    assert_eq!(status("both.txt"), "=");
    assert_eq!(status("left-only.txt"), "->");
    assert_eq!(status("right-only.txt"), "<-");
    assert_eq!(status("newer.txt"), "!=");
    assert_eq!(state.pending(), 3);
}

#[test]
fn sync_ignore_dates_can_make_entries_equal() {
    let (left, right, mut model) = sync_fixture();
    // Same size, different timestamps.
    write(&left.path().join("twin.txt"), b"abcd");
    write(&right.path().join("twin.txt"), b"abcd");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    write(&right.path().join("twin.txt"), b"abcd");
    let mut state = SyncState {
        left: left.path().to_string_lossy().to_string(),
        right: right.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    model.sync_compare(&mut state).is_ok();
    let with_dates = state
        .rows
        .iter()
        .find(|r| r.name == "twin.txt")
        .map(|r| r.status.clone());
    assert_eq!(
        with_dates.as_deref(),
        Some("!="),
        "different timestamps differ"
    );
    state.ignore_dates = true;
    model.sync_compare(&mut state).is_ok();
    let without_dates = state
        .rows
        .iter()
        .find(|r| r.name == "twin.txt")
        .map(|r| r.status.clone());
    assert_eq!(without_dates.as_deref(), Some("="));
}

#[test]
fn sync_dry_run_reports_without_copying() {
    let (left, right, mut model) = sync_fixture();
    let mut state = SyncState {
        left: left.path().to_string_lossy().to_string(),
        right: right.path().to_string_lossy().to_string(),
        dry_run: true,
        ..Default::default()
    };
    model.sync_compare(&mut state).is_ok();
    assert!(model.sync_execute(&state).is_ok());
    assert!(
        !right.path().join("left-only.txt").exists(),
        "a dry run must not copy"
    );
    assert!(left.path().join("left-only.txt").exists());
}

#[test]
fn sync_execute_copies_in_both_directions() {
    let (left, right, mut model) = sync_fixture();
    let mut state = SyncState {
        left: left.path().to_string_lossy().to_string(),
        right: right.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    model.open_dialog(Dialog::Synchronize(state.clone()));
    model.sync_compare(&mut state).is_ok();
    assert!(model.sync_execute(&state).is_ok());
    assert!(
        right.path().join("left-only.txt").exists(),
        "left-only travelled right"
    );
    assert!(
        left.path().join("right-only.txt").exists(),
        "right-only travelled left"
    );
    assert!(model.status.contains("status.done") || model.status.contains("count"));
}

#[test]
fn sync_compare_rejects_invalid_directories() {
    let (_l, _r, mut model) = fixture();
    let mut state = SyncState {
        left: "/does/not/exist".into(),
        right: "/tmp".into(),
        ..Default::default()
    };
    assert!(matches!(
        model.sync_compare(&mut state),
        CommandOutcome::Failed(_)
    ));
    let empty = SyncState::default();
    assert!(matches!(
        model.sync_execute(&empty),
        CommandOutcome::Failed(_)
    ));
}

// ---------------- configurable button bar ----------------

#[test]
fn the_button_bar_editor_adds_edits_removes_and_reorders() {
    let (_l, _r, mut model) = fixture();
    let initial = model.settings.button_bar.buttons.len();
    assert!(model.execute("cm_ButtonConfig").is_ok());
    let mut state = match model.top_dialog() {
        Some(Dialog::ButtonBar(state)) => state.clone(),
        other => panic!("expected the button bar dialog, got {other:?}"),
    };
    // Add a new button.
    state.start_add();
    state.edit_caption = "My".to_string();
    state.edit_command = "cm_MultiRenameFiles".to_string();
    assert!(model.apply_button_bar(&mut state).is_ok());
    assert_eq!(model.settings.button_bar.buttons.len(), initial + 1);
    let added = model.settings.button_bar.buttons.last().cloned().unwrap();
    assert_eq!(added.caption, "My");
    assert_eq!(added.command, "cm_MultiRenameFiles");
    // Edit the same button.
    let index = model.settings.button_bar.buttons.len() - 1;
    assert_eq!(state.selected, Some(index));
    state.edit_caption = "Renamed".to_string();
    assert!(model.apply_button_bar(&mut state).is_ok());
    assert_eq!(model.settings.button_bar.buttons[index].caption, "Renamed");
    // Move it up and back.
    assert!(model.move_button_bar(&mut state, true).is_ok());
    assert_eq!(state.selected, Some(index - 1));
    assert!(model.move_button_bar(&mut state, false).is_ok());
    // Remove it again.
    assert!(model.remove_button_bar(&mut state).is_ok());
    assert_eq!(model.settings.button_bar.buttons.len(), initial);
    assert_eq!(state.selected, None);
}

#[test]
fn a_button_without_a_command_is_rejected() {
    let (_l, _r, mut model) = fixture();
    let mut state = ButtonBarState::new();
    state.start_add();
    state.edit_caption = "no command".to_string();
    assert!(matches!(
        model.apply_button_bar(&mut state),
        CommandOutcome::Failed(_)
    ));
    assert_eq!(
        model.settings.button_bar.buttons.len(),
        rc_core::config::ButtonBar::default().buttons.len()
    );
}

#[test]
fn moving_or_removing_without_a_selection_does_nothing() {
    let (_l, _r, mut model) = fixture();
    let mut state = ButtonBarState::new();
    assert_eq!(model.remove_button_bar(&mut state), CommandOutcome::None);
    assert_eq!(
        model.move_button_bar(&mut state, true),
        CommandOutcome::None
    );
}

#[test]
fn toolbar_custom_layout_survives_factory_reset_and_restore() {
    let (_l, _r, mut model) = fixture();
    model.settings.button_bar.remove(0);
    let mut state = ButtonBarState::new();
    state.start_add();
    state.edit_command = "cm_ChatPane".into();
    assert!(model.apply_button_bar(&mut state).is_ok());
    let custom = model.settings.button_bar.clone();
    model.reset_button_bar();
    assert_eq!(
        model.settings.button_bar,
        rc_core::config::ButtonBar::default()
    );
    assert!(model.restore_custom_button_bar());
    assert_eq!(model.settings.button_bar, custom);
    assert_eq!(model.settings.button_bar_custom, Some(custom));
}

#[test]
fn toolbar_directory_shortcut_preserves_case_and_uses_active_panel() {
    let (left, right, mut model) = fixture();
    let mixed = left.path().join("MixedCase Directory");
    std::fs::create_dir(&mixed).unwrap();
    model.open_in(Side::Left, Location::local(&mixed)).unwrap();
    model.focus = Side::Left;
    assert!(model.add_current_directory_button().is_ok());
    let button = model.settings.button_bar.buttons.last().unwrap().clone();
    model
        .open_in(Side::Left, Location::local(left.path()))
        .unwrap();
    assert!(model.execute(&button.command).is_ok());
    assert_eq!(
        model.panel(Side::Left).location().as_path(),
        Some(mixed.as_path())
    );
    assert_eq!(
        model.panel(Side::Right).location().as_path(),
        Some(right.path())
    );
    assert!(matches!(
        model.execute("rc_OpenDirectory:relative"),
        CommandOutcome::Failed(_)
    ));
}

#[cfg(unix)]
#[test]
fn toolbar_directory_shortcut_refuses_a_lossy_non_utf8_path() {
    use std::os::unix::ffi::OsStringExt;
    let (left, _right, mut model) = fixture();
    let unusual = left
        .path()
        .join(std::ffi::OsString::from_vec(vec![b'd', 0xff]));
    std::fs::create_dir(&unusual).unwrap();
    model
        .open_in(Side::Left, Location::local(&unusual))
        .unwrap();
    model.focus = Side::Left;
    let before = model.settings.button_bar.buttons.len();
    assert!(matches!(
        model.add_current_directory_button(),
        CommandOutcome::Failed(_)
    ));
    assert_eq!(model.settings.button_bar.buttons.len(), before);
}

// ---------------- drag & drop ----------------

#[test]
fn dropping_files_copies_them_into_the_panel_directory() {
    let (left_dir, right_dir, mut model) = fixture();
    let external = tempfile::tempdir().unwrap();
    write(&external.path().join("dropped.txt"), b"from outside");
    write(&external.path().join("other.bin"), b"0123456789");
    let paths = vec![
        external.path().join("dropped.txt"),
        external.path().join("other.bin"),
    ];
    assert!(model.drop_files_into(Side::Right, paths).is_ok());
    assert_eq!(
        fs::read(right_dir.path().join("dropped.txt")).unwrap(),
        b"from outside"
    );
    assert_eq!(
        fs::metadata(right_dir.path().join("other.bin"))
            .unwrap()
            .len(),
        10
    );
    assert!(model.status.contains("status.done"));
    assert!(left_dir.path().exists());
}

#[test]
fn dropping_onto_an_existing_name_renames_instead_of_overwriting() {
    let (_l, right_dir, mut model) = fixture();
    write(&right_dir.path().join("target.txt"), b"existing");
    let external = tempfile::tempdir().unwrap();
    write(&external.path().join("target.txt"), b"new content");
    assert!(
        model
            .drop_files_into(Side::Right, vec![external.path().join("target.txt")])
            .is_ok()
    );
    assert_eq!(
        fs::read(right_dir.path().join("target.txt")).unwrap(),
        b"existing",
        "the original is untouched"
    );
    assert_eq!(
        fs::read(right_dir.path().join("target (2).txt")).unwrap(),
        b"new content"
    );
}

#[test]
fn dropping_nothing_is_a_no_op_and_non_local_panels_are_refused() {
    let (_l, _r, mut model) = fixture();
    assert_eq!(
        model.drop_files_into(Side::Left, Vec::new()),
        CommandOutcome::None
    );
    let remote = Location::remote("ftp", "example.com", None, "/pub");
    model.left.listing = rc_core::listing::Listing::from_entries(
        remote,
        Vec::new(),
        &rc_core::listing::ReadOptions::default(),
        Default::default(),
    );
    let external = tempfile::tempdir().unwrap();
    write(&external.path().join("x.txt"), b"x");
    assert!(matches!(
        model.drop_files_into(Side::Left, vec![external.path().join("x.txt")]),
        CommandOutcome::Failed(_)
    ));
}

// ---------------- custom columns and view modes ----------------

#[test]
fn columns_can_be_reordered_hidden_and_restored() {
    let (_l, _r, mut model) = fixture();
    let ids = |m: &Model| -> Vec<String> {
        m.settings
            .display
            .columns
            .iter()
            .map(|c| c.id.clone())
            .collect()
    };
    assert_eq!(ids(&model), ["name", "size", "date", "attrs"]);

    // Hide the attributes column.
    assert!(model.remove_column("attrs").is_ok());
    assert_eq!(ids(&model), ["name", "size", "date"]);

    // Re-adding it puts it at the end.
    assert!(model.add_column("attrs").is_ok());
    assert_eq!(ids(&model), ["name", "size", "date", "attrs"]);

    // Move the date column up once, then once more into the first position.
    assert!(model.move_column("date", true).is_ok());
    assert_eq!(ids(&model), ["name", "date", "size", "attrs"]);
    assert!(model.move_column("date", true).is_ok());
    assert_eq!(ids(&model), ["date", "name", "size", "attrs"]);
    assert!(model.move_column("date", false).is_ok());
    assert_eq!(ids(&model), ["name", "date", "size", "attrs"]);

    // Set a width (the date column is second again after the moves above).
    assert!(model.set_column_width("date", 200.0).is_ok());
    assert_eq!(model.settings.display.columns[1].width, 200.0);

    // Unknown columns and no-ops are rejected.
    assert!(matches!(
        model.add_column("nonsense"),
        CommandOutcome::Failed(_)
    ));
    assert!(matches!(
        model.set_column_width("nonsense", 10.0),
        CommandOutcome::Failed(_)
    ));
    assert_eq!(
        model.move_column("name", true),
        CommandOutcome::None,
        "the first column cannot move up"
    );
    assert_eq!(
        model.remove_column("name"),
        CommandOutcome::Failed("the name column is required".to_string())
    );
    assert_eq!(model.remove_column("absurd"), CommandOutcome::None);
}

#[test]
fn a_custom_column_list_is_remembered_per_settings() {
    let (_l, _r, mut model) = fixture();
    model.settings.display.columns = vec![
        rc_core::config::ColumnSpec::new("name", 0.0),
        rc_core::config::ColumnSpec::new("comment", 240.0),
    ];
    let text = model.settings.to_toml().unwrap();
    let restored = Settings::from_toml(&text).unwrap();
    assert_eq!(restored.display.columns.len(), 2);
    assert_eq!(restored.display.columns[1].id, "comment");
}

#[test]
fn view_modes_can_be_saved_applied_and_removed() {
    let (left, _r, mut model) = fixture();
    let mut state = ViewModeDialogState {
        name: "Text files".to_string(),
        kind: "full".to_string(),
        mask: "*.txt".to_string(),
        sort_field: "date".to_string(),
        sort_reversed: true,
        ..ViewModeDialogState::default()
    };
    assert!(model.save_view_mode(&state).is_ok());
    assert_eq!(model.settings.display.view_modes.len(), 1);

    assert!(model.apply_view_mode("Text files").is_ok());
    assert_eq!(model.left.view_mode, ViewMode::Full);
    assert_eq!(model.left.mask_spec, "*.txt");
    assert_eq!(
        model.left.sort().field,
        rc_core::sorting::SortField::ModifiedTime
    );
    assert!(model.left.sort().reversed);
    let names = names(&model);
    assert!(
        !names.contains(&"beta.log".to_string()),
        "the mode filter applies: {names:?}"
    );
    assert!(left.path().exists());

    // A second mode with the same name replaces the first.
    state.mask = "*.*".to_string();
    assert!(model.save_view_mode(&state).is_ok());
    assert_eq!(model.settings.display.view_modes.len(), 1);

    assert!(model.remove_view_mode("Text files").is_ok());
    assert!(model.settings.display.view_modes.is_empty());
    assert_eq!(model.apply_view_mode("Text files"), CommandOutcome::None);
}

#[test]
fn invalid_view_modes_are_rejected() {
    let (_l, _r, mut model) = fixture();
    let mut state = ViewModeDialogState::default();
    assert!(matches!(
        model.save_view_mode(&state),
        CommandOutcome::Failed(_)
    ));
    state.name = "Broken".to_string();
    state.kind = "carousel".to_string();
    assert!(matches!(
        model.save_view_mode(&state),
        CommandOutcome::Failed(_)
    ));
    assert!(model.settings.display.view_modes.is_empty());
}

// ---------------- file utilities ----------------

#[test]
fn the_split_command_opens_and_produces_parts() {
    let (left, _r, mut model) = fixture();
    write(&left.path().join("payload.bin"), &vec![9u8; 5000]);
    model.refresh_active();
    model
        .left
        .set_cursor(model.left.listing.find("payload.bin").unwrap());
    assert!(model.execute("cm_Split").is_ok());
    let mut state = match model.top_dialog() {
        Some(Dialog::Split(state)) => state.clone(),
        other => panic!("expected the split dialog, got {other:?}"),
    };
    state.part_size = 2000;
    assert!(model.run_split(&state).is_ok());
    assert!(left.path().join("payload.001").exists());
    assert!(left.path().join("payload.002").exists());
    assert!(left.path().join("payload.003").exists());
    assert_eq!(
        fs::metadata(left.path().join("payload.001")).unwrap().len(),
        2000
    );
    assert!(model.status.contains("status.done"));
}

#[test]
fn combine_restores_the_original_content() {
    let (left, _r, mut model) = fixture();
    let content: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
    write(&left.path().join("payload.bin"), &content);
    model.refresh_active();
    model
        .left
        .set_cursor(model.left.listing.find("payload.bin").unwrap());
    let split = SplitState {
        part_size: 1000,
        ..SplitState::new(
            left.path()
                .join("payload.bin")
                .to_string_lossy()
                .to_string(),
            left.path().to_string_lossy().to_string(),
        )
    };
    assert!(model.run_split(&split).is_ok());
    let first = left.path().join("payload.001");
    let joined = left.path().join("joined.bin");
    let state = CombineState::new(
        first.to_string_lossy().to_string(),
        joined.to_string_lossy().to_string(),
    );
    assert!(model.run_combine(&state).is_ok());
    assert_eq!(fs::read(&joined).unwrap(), content);
}

#[test]
fn creating_and_verifying_checksums_through_the_model() {
    let (left, _r, mut model) = fixture();
    model.left.selection.set(["alpha.txt", "gamma.txt"]);
    assert!(model.execute("cm_CRCcreate").is_ok());
    let mut state = match model.top_dialog() {
        Some(Dialog::Checksum(state)) => state.clone(),
        other => panic!("expected the checksum dialog, got {other:?}"),
    };
    state.algorithm = "sha256".to_string();
    state.file_name = "check.sha256".to_string();
    assert!(model.run_create_checksums(&state).is_ok());
    let written = left.path().join("check.sha256");
    assert!(written.exists());
    let text = fs::read_to_string(&written).unwrap();
    assert!(text.contains("alpha.txt") && text.contains("gamma.txt"));

    // Verify passes, then fails after a file changes.
    let mut verify = VerifyChecksumState::new(
        written.to_string_lossy().to_string(),
        left.path().to_string_lossy().to_string(),
    );
    assert!(model.run_verify_checksums(&mut verify).is_ok());
    assert_eq!(verify.checked, 2);
    assert_eq!(verify.ok, 2);
    assert!(verify.failures.is_empty());
    write(&left.path().join("alpha.txt"), b"tampered");
    let mut verify2 = VerifyChecksumState::new(
        written.to_string_lossy().to_string(),
        left.path().to_string_lossy().to_string(),
    );
    assert!(model.run_verify_checksums(&mut verify2).is_ok());
    assert_eq!(verify2.ok, 1);
    assert_eq!(verify2.failures.len(), 1);
    assert!(
        model.last_error.is_some(),
        "a failed verification is reported"
    );
}

#[test]
fn compare_files_by_content_reports_identity_and_offsets() {
    let (left, _r, mut model) = fixture();
    let left_file = left.path().join("alpha.txt");
    let right_file = left.path().join("gamma.txt");
    model.open_dialog(Dialog::CompareFiles(CompareFilesState::new(
        left_file.to_string_lossy().to_string(),
        right_file.to_string_lossy().to_string(),
    )));
    let mut state = match model.top_dialog() {
        Some(Dialog::CompareFiles(state)) => state.clone(),
        other => panic!("expected the compare dialog, got {other:?}"),
    };
    assert!(model.run_compare_files(&mut state).is_ok());
    assert!(
        matches!(state.result, Some(Some(_))),
        "the files differ: {:?}",
        state.result
    );
    assert!(state.result_text().contains("differ at byte"));

    // Identical files report identity.
    let copy = left.path().join("alpha-copy.txt");
    fs::copy(&left_file, &copy).unwrap();
    let mut same = CompareFilesState::new(
        left_file.to_string_lossy().to_string(),
        copy.to_string_lossy().to_string(),
    );
    assert!(model.run_compare_files(&mut same).is_ok());
    assert_eq!(same.result, Some(None));
    assert!(same.result_text().contains("identical"));
}

#[test]
fn the_duplicate_finder_reports_groups() {
    let (left, _r, mut model) = fixture();
    write(&left.path().join("dup1.txt"), b"duplicate content");
    write(&left.path().join("dup2.txt"), b"duplicate content");
    model.refresh_active();
    assert!(model.execute("cm_FindDuplicates").is_ok());
    let mut state = match model.top_dialog() {
        Some(Dialog::Duplicates(state)) => state.clone(),
        other => panic!("expected the duplicates dialog, got {other:?}"),
    };
    assert!(model.run_duplicates(&mut state).is_ok());
    assert_eq!(state.groups.len(), 1);
    assert_eq!(state.groups[0].paths.len(), 2);
    assert!(state.summary().contains("group"));
    assert!(model.status.contains("group"));
    // An invalid algorithm is rejected.
    state.algorithm = "magic".into();
    assert!(matches!(
        model.run_duplicates(&mut state),
        CommandOutcome::Failed(_)
    ));
}

#[test]
fn utility_dialogs_validate_their_input() {
    let (_l, _r, mut model) = fixture();
    let split = SplitState {
        part_size: 0,
        ..SplitState::new("/tmp/a", "/tmp")
    };
    assert!(matches!(model.run_split(&split), CommandOutcome::Failed(_)));
    let combine = CombineState::new("", "");
    assert!(matches!(
        model.run_combine(&combine),
        CommandOutcome::Failed(_)
    ));
    let checksum = ChecksumState {
        algorithm: "magic".into(),
        ..ChecksumState::new(vec!["/tmp/a".into()], "/tmp")
    };
    assert!(matches!(
        model.run_create_checksums(&checksum),
        CommandOutcome::Failed(_)
    ));
    let empty = ChecksumState::new(Vec::new(), "/tmp");
    assert!(matches!(
        model.run_create_checksums(&empty),
        CommandOutcome::Failed(_)
    ));
    let mut verify = VerifyChecksumState::default();
    assert!(matches!(
        model.run_verify_checksums(&mut verify),
        CommandOutcome::Failed(_)
    ));
    let mut compare = CompareFilesState::default();
    assert!(matches!(
        model.run_compare_files(&mut compare),
        CommandOutcome::Failed(_)
    ));
}

// ---------------- encoding, selections and the branch view ----------------

#[test]
fn encoding_and_decoding_a_file_round_trips() {
    let (left, _r, mut model) = fixture();
    let content: Vec<u8> = (0..200u32).map(|i| (i % 251) as u8).collect();
    write(&left.path().join("blob.bin"), &content);
    model.refresh_active();
    model.left.selection.set(["blob.bin"]);
    for format in ["MIME", "UUE", "XXE"] {
        assert!(model.execute("cm_Encode").is_ok());
        let mut state = match model.top_dialog() {
            Some(Dialog::Encode(state)) => state.clone(),
            other => panic!("expected the encode dialog, got {other:?}"),
        };
        state.encoding = format.to_string();
        assert!(
            model.run_encode(&state).is_ok(),
            "{format} encode failed: {}",
            model.status
        );
        let extension = format.to_ascii_lowercase().replace("mime", "b64");
        let encoded = left.path().join(format!("blob.bin.{extension}"));
        assert!(encoded.exists(), "{format}: {} missing", encoded.display());
        // Decode it back and compare.
        model.open_dialog(Dialog::Decode(DecodeState::new(
            encoded.to_string_lossy().to_string(),
        )));
        let mut decode = DecodeState::new(encoded.to_string_lossy().to_string());
        decode.target_name = format!("decoded-{format}.bin");
        assert!(
            model.run_decode(&mut decode).is_ok(),
            "{format} decode failed: {}",
            model.status
        );
        assert_eq!(
            fs::read(left.path().join(format!("decoded-{format}.bin"))).unwrap(),
            content
        );
    }
}

#[test]
fn a_selection_can_be_saved_and_restored_from_a_file() {
    let (left, _r, mut model) = fixture();
    model.left.selection.set(["alpha.txt", "gamma.txt"]);
    let save = SaveSelectionState::new(model.left.selection.len());
    assert!(model.run_save_selection(&save).is_ok());
    let written = left.path().join("selection.txt");
    assert!(written.exists());
    let text = fs::read_to_string(&written).unwrap();
    assert!(text.contains("alpha.txt"));
    // Change the selection, then restore it.
    model.left.selection.clear();
    let mut load = LoadSelectionState::new();
    assert!(model.run_load_selection(&mut load).is_ok());
    assert_eq!(load.loaded, 2);
    let mut names = model.left.selected_names();
    names.sort();
    assert_eq!(names, ["alpha.txt", "gamma.txt"]);
}

#[test]
fn an_in_memory_selection_can_be_saved_and_restored() {
    let (_l, _r, mut model) = fixture();
    model.left.selection.set(["beta.log"]);
    assert!(model.execute("cm_SaveSelection").is_ok());
    model.left.selection.clear();
    assert!(model.execute("cm_RestoreSelection").is_ok());
    assert_eq!(model.left.selected_names(), ["beta.log"]);
    // Restoring without a saved selection is inert.
    let mut fresh = Model::for_test(
        Settings::default(),
        Location::local("/tmp"),
        Location::local("/tmp"),
    );
    assert_eq!(fresh.execute("cm_RestoreSelection"), CommandOutcome::None);
}

#[test]
fn the_branch_view_lists_the_whole_subtree_and_can_be_left() {
    let (left, _r, mut model) = fixture();
    std::fs::create_dir_all(left.path().join("sub/deep")).unwrap();
    write(&left.path().join("sub/deep/leaf.txt"), b"leaf");
    model.refresh_active();
    assert!(model.execute("cm_DirBranch").is_ok());
    assert_eq!(model.left.view_mode, ViewMode::Tree);
    let flat = names(&model);
    assert!(
        flat.contains(&"sub/deep/leaf.txt".to_string()),
        "flat names: {flat:?}"
    );
    assert!(flat.contains(&"alpha.txt".to_string()));
    // Toggling again restores the normal listing.
    assert!(model.execute("cm_DirBranch").is_ok());
    assert_ne!(model.left.view_mode, ViewMode::Tree);
    assert!(!names(&model).contains(&"sub/deep/leaf.txt".to_string()));
}

#[test]
fn utility_dialog_validation_for_encoding_and_selections() {
    let (_l, _r, mut model) = fixture();
    let empty = EncodeState::new(Vec::new());
    assert!(matches!(
        model.run_encode(&empty),
        CommandOutcome::Failed(_)
    ));
    let bad = EncodeState {
        encoding: "rot13".into(),
        ..EncodeState::new(vec!["/tmp/a".into()])
    };
    assert!(matches!(model.run_encode(&bad), CommandOutcome::Failed(_)));
    let mut decode = DecodeState::new("");
    assert!(matches!(
        model.run_decode(&mut decode),
        CommandOutcome::Failed(_)
    ));
    let save = SaveSelectionState::new(0);
    assert!(matches!(
        model.run_save_selection(&save),
        CommandOutcome::Failed(_)
    ));
    let mut load = LoadSelectionState {
        file_name: String::new(),
        loaded: 0,
    };
    assert!(matches!(
        model.run_load_selection(&mut load),
        CommandOutcome::Failed(_)
    ));
    let mut missing = LoadSelectionState::new();
    missing.file_name = "not-there.txt".into();
    assert!(matches!(
        model.run_load_selection(&mut missing),
        CommandOutcome::Failed(_)
    ));
}

// ---------------- per-panel command families ----------------

#[test]
fn left_and_right_command_variants_act_on_their_panel() {
    let (_l, _r, mut model) = fixture();
    assert_eq!(model.focus, Side::Left);
    // cm_RightLong makes the right panel full detail and activates it.
    assert!(model.execute("cm_RightLong").is_ok());
    assert_eq!(model.focus, Side::Right);
    assert_eq!(model.right.view_mode, ViewMode::Full);
    // cm_LeftShort is the brief view of the left panel.
    assert!(model.execute("cm_LeftShort").is_ok());
    assert_eq!(model.focus, Side::Left);
    assert_eq!(model.left.view_mode, ViewMode::Brief);
    // Sorting families work per panel too.
    assert!(model.execute("cm_RightBySize").is_ok());
    assert_eq!(model.focus, Side::Right);
    assert_eq!(model.right.sort().field, SortField::Size);
    assert_eq!(
        model.left.sort().field,
        SortField::Name,
        "the other panel is untouched"
    );
    // Filters and quick view as well.
    assert!(model.execute("cm_LeftComments").is_ok());
    assert_eq!(model.left.view_mode, ViewMode::Comments);
    assert!(model.execute("cm_LeftQuickView").is_ok());
    assert!(model.quick_view);
    assert!(model.execute("cm_DirBranch").is_ok());
    assert_eq!(model.left.view_mode, ViewMode::Tree);
}

#[test]
fn numbered_command_families_work() {
    let (_l, _r, mut model) = fixture();
    // Tabs: cm_ActivateTab2 opens/activates the second tab.
    assert!(model.execute("cm_ActivateTab2").is_ok());
    assert_eq!(model.left.tabs.len(), 2);
    assert_eq!(model.left.tabs.active_index(), 1);
    assert!(model.execute("cm_ActivateTab1").is_ok());
    assert_eq!(model.left.tabs.active_index(), 0);
    assert_eq!(model.execute("cm_ActivateTab0"), CommandOutcome::None);

    // Sorting by column position follows the configured columns.
    // Default columns: 1 name, 2 size, 3 date, 4 attrs.
    assert!(model.execute("cm_SortByCol2").is_ok());
    assert_eq!(model.left.sort().field, SortField::Size);
    assert!(model.execute("cm_SrcSortByCol3").is_ok());
    assert_eq!(model.left.sort().field, SortField::ModifiedTime);
    assert_eq!(model.execute("cm_SortByCol99"), CommandOutcome::None);
    // The attributes column has no sort field.
    assert_eq!(model.execute("cm_SortByCol4"), CommandOutcome::None);

    // View modes are addressed by their index.
    let mut state = ViewModeDialogState {
        name: "Shots".into(),
        kind: "thumbnails".into(),
        mask: "*.png".into(),
        ..Default::default()
    };
    assert!(model.save_view_mode(&state).is_ok());
    assert!(model.execute("cm_ViewMode1").is_ok());
    assert_eq!(model.left.view_mode, ViewMode::Thumbnails);
    assert_eq!(model.execute("cm_ViewMode9"), CommandOutcome::None);
    state.name = "Texts".into();
    state.kind = "full".into();
    state.mask = "*.txt".into();
    assert!(model.save_view_mode(&state).is_ok());
    assert!(model.execute("cm_SrcViewMode2").is_ok());
    assert_eq!(model.left.view_mode, ViewMode::Full);
}

// ---------------- clipboard and layout toggles ----------------

#[test]
fn clipboard_commands_queue_text_for_the_view_layer() {
    let (_l, _r, mut model) = fixture();
    model.left.selection.set(["alpha.txt", "beta.log"]);
    assert!(model.execute("cm_CopyNamesToClip").is_ok());
    assert_eq!(model.clipboard.len(), 1);
    let text = model.clipboard[0].clone();
    let mut lines: Vec<&str> = text.lines().collect();
    lines.sort();
    assert_eq!(lines, ["alpha.txt", "beta.log"]);

    model.clipboard.clear();
    assert!(model.execute("cm_CopyFullNamesToClip").is_ok());
    assert!(
        model.clipboard[0].contains('/'),
        "full paths are copied: {}",
        model.clipboard[0]
    );

    model.clipboard.clear();
    assert!(model.execute("cm_CopyTrgPathToClip").is_ok());
    assert!(!model.clipboard[0].is_empty());

    // Nothing selected and no cursor entry: nothing is copied.
    model.clipboard.clear();
    model.left.selection.clear();
    model.left.cursor_home();
    model.left.listing = rc_core::listing::Listing::from_entries(
        Location::local("/tmp"),
        Vec::new(),
        &rc_core::listing::ReadOptions::default(),
        Default::default(),
    );
    assert_eq!(model.execute("cm_CopyNamesToClip"), CommandOutcome::None);
    assert!(model.clipboard.is_empty());
}

#[test]
fn layout_visibility_commands_toggle_the_chrome() {
    let (_l, _r, mut model) = fixture();
    assert!(model.settings.layout.show_command_line);
    assert!(model.execute("cm_VisCmdLine").is_ok());
    assert!(!model.settings.layout.show_command_line);
    assert!(model.execute("cm_VisCmdLine").is_ok());
    assert!(model.settings.layout.show_command_line);

    assert!(model.settings.layout.show_status_bar);
    assert!(model.execute("cm_VisStatusBar").is_ok());
    assert!(!model.settings.layout.show_status_bar);

    assert!(model.execute("cm_VisButtonBar").is_ok());
    assert!(!model.settings.layout.show_button_bar);
    assert!(model.execute("cm_VisKeyButtons").is_ok());
    assert!(!model.settings.layout.show_function_key_bar);
    assert!(model.execute("cm_VisDirTabs").is_ok());
    assert!(!model.settings.layout.show_tabs);
    assert!(model.execute("cm_VisDriveButtons").is_ok());
    assert!(!model.settings.layout.show_drive_bar);
    assert!(model.status.contains("drivebuttons"));
    // An unknown element is inert.
    assert_eq!(model.execute("cm_VisNonsense"), CommandOutcome::None);
}

// ---------------- reported bugs: mnemonics, language, persistence ----------------

#[test]
fn menu_labels_do_not_show_mnemonic_markers() {
    // The Czech pack writes "&Soubor"; the UI must show "Soubor".
    let mut model = Model::boot(
        Settings::default(),
        Box::new(crate::i18n::KeyEchoStrings::default()),
        Location::local("/tmp"),
        Location::local("/tmp"),
    );
    // The deterministic translator echoes keys, so use a real one for this test.
    let translator = crate::i18n::TranslatorStrings::discover(&crate::paths::locales_dir(), "cs");
    model.strings = Box::new(translator);
    assert_eq!(model.t("menu.file"), "Soubor");
    assert_eq!(model.t("menu.show"), "Zobrazit");
    // A literal ampersand must survive as "&&" -> "&".
    assert_eq!(
        crate::i18n::strip_mnemonics("Copy && Paste"),
        "Copy & Paste"
    );
    assert_eq!(crate::i18n::strip_mnemonics("&File"), "File");
    assert_eq!(crate::i18n::strip_mnemonics("no markers"), "no markers");
}

#[test]
fn switching_the_language_swaps_the_translator() {
    let mut model = Model::boot(
        Settings::default(),
        Box::new(crate::i18n::TranslatorStrings::discover(
            &crate::paths::locales_dir(),
            "en",
        )),
        Location::local("/tmp"),
        Location::local("/tmp"),
    );
    let english = model.t("menu.file");
    assert!(!english.is_empty());
    assert!(model.set_language("cs"), "cs is an installed language");
    assert_eq!(model.settings.language, "cs");
    assert_ne!(
        model.t("menu.file"),
        english,
        "the UI text must actually change"
    );
    assert_eq!(model.t("menu.file"), "Soubor");
    assert!(model.set_language("en"));
    assert_eq!(model.t("menu.file"), english);
    // An unknown language is refused and changes nothing.
    assert!(!model.set_language("klingon"));
    assert_eq!(model.settings.language, "en");
    assert!(
        model.last_error.is_some(),
        "the refusal is reported to the user"
    );
}

// ---------------- credential storage (Phase 13.4) ----------------

#[test]
fn passwords_are_migrated_out_of_the_settings_file() {
    let (_l, _r, mut model) = fixture();
    // An older settings file still carries a plain-text password.
    let mut connection = rc_core::config::SavedConnection {
        name: "work".into(),
        host: "example.com".into(),
        port: 21,
        user: "bob".into(),
        protocol: "ftp".into(),
        ..Default::default()
    };
    connection.password = "legacy-secret".to_string();
    model.settings.ftp.connections.push(connection);

    let migrated = model.migrate_plaintext_passwords();
    assert_eq!(migrated, 1);
    assert!(
        model.settings.ftp.connections[0].password.is_empty(),
        "the plain text must be gone"
    );
    assert_eq!(
        model
            .stored_password(&model.settings.ftp.connections[0])
            .as_deref(),
        Some("legacy-secret")
    );
    assert!(
        model.settings_dirty,
        "the changed settings must be persisted"
    );
    // The settings file no longer contains the secret.
    let text = model.settings.to_toml().unwrap();
    assert!(!text.contains("legacy-secret"));
    // A second run migrates nothing.
    assert_eq!(model.migrate_plaintext_passwords(), 0);
}

#[test]
fn remembering_a_password_uses_the_credential_store() {
    let (_l, _r, mut model) = fixture();
    let connection = rc_core::config::SavedConnection {
        name: "git".into(),
        host: "git.example.com".into(),
        port: 22,
        user: "alice".into(),
        protocol: "sftp".into(),
        password: "s3cret".into(),
        ..Default::default()
    };
    assert!(model.remember_password(&connection).is_ok());
    assert_eq!(
        model.stored_password(&connection).as_deref(),
        Some("s3cret")
    );
    // An empty password stores nothing.
    let empty = rc_core::config::SavedConnection {
        ..connection.clone()
    };
    let mut empty = empty;
    empty.password.clear();
    assert_eq!(model.remember_password(&empty), CommandOutcome::None);
}

#[test]
fn the_encrypted_store_can_replace_the_memory_store() {
    let (_l, _r, mut model) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secrets.enc");
    let connection = rc_core::config::SavedConnection {
        host: "example.com".into(),
        user: "bob".into(),
        port: 21,
        protocol: "ftp".into(),
        password: "pw".into(),
        ..Default::default()
    };
    assert!(model.remember_password(&connection).is_ok());
    // Swap in an encrypted store, migrate the in-memory value, persist it.
    let mut store = rc_core::secrets::EncryptedStore::new(path.clone());
    store.insert(connection.account_id(), "pw");
    store.save("master-passphrase").unwrap();
    model.set_secret_store(Box::new(store));
    assert_eq!(model.stored_password(&connection).as_deref(), Some("pw"));
    // Reopening the file with the same passphrase yields the same secret.
    let reopened = rc_core::secrets::EncryptedStore::load(path, "master-passphrase").unwrap();
    assert_eq!(reopened.get_secret(&connection.account_id()), Some("pw"));
    // The file on disk never contains the plain password.
    let bytes = std::fs::read(dir.path().join("secrets.enc")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("pw"));
}

// ---------------- SFTP / SSH connection setup (Phase 13) ----------------

#[test]
fn a_saved_connection_maps_to_a_connection_spec() {
    use rc_core::config::SavedConnection;
    let saved = SavedConnection {
        name: "git".into(),
        host: "git.example.com".into(),
        port: 22,
        user: "alice".into(),
        protocol: "sftp".into(),
        auth_method: "key".into(),
        key_path: "/home/alice/.ssh/id_ed25519".into(),
        host_key_policy: "strict".into(),
        remote_dir: "/srv".into(),
        ..Default::default()
    };
    let spec = crate::model::remote::connection_spec(&saved, Some("pw".to_string()));
    assert_eq!(spec.protocol, rc_remote::Protocol::Sftp);
    assert_eq!(spec.password, "pw");
    assert_eq!(
        spec.host_key_policy,
        rc_core::knownhosts::HostKeyPolicy::Strict
    );
    match &spec.auth {
        rc_remote::AuthMethod::KeyFile { path, .. } => {
            assert_eq!(path.to_string_lossy(), "/home/alice/.ssh/id_ed25519")
        }
        other => panic!("expected key authentication, got {other:?}"),
    }
    // Agent and password map as well.
    let agent = SavedConnection {
        auth_method: "agent".into(),
        ..saved.clone()
    };
    assert_eq!(
        crate::model::remote::connection_spec(&agent, None).auth,
        rc_remote::AuthMethod::Agent
    );
    let password = SavedConnection {
        auth_method: "password".into(),
        ..saved
    };
    assert_eq!(
        crate::model::remote::connection_spec(&password, Some("x".into())).auth,
        rc_remote::AuthMethod::Password
    );
}

#[test]
fn the_connect_dialog_validates_ssh_settings() {
    let mut state = FtpConnectState {
        connection: rc_core::config::SavedConnection {
            host: "h".into(),
            protocol: "sftp".into(),
            user: String::new(),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(state.is_ssh());
    assert!(state.validate().is_err(), "SFTP needs a user");
    state.connection.user = "alice".into();
    assert!(state.validate().is_ok());
    // Key authentication without a key file is refused.
    state.connection.auth_method = "key".into();
    assert!(state.validate().is_err());
    state.connection.key_path = "/home/alice/.ssh/id_ed25519".into();
    assert!(state.validate().is_ok());
    // FTP ignores the SSH checks.
    state.connection.protocol = "ftp".into();
    state.connection.user = String::new();
    assert!(!state.is_ssh());
    assert!(state.validate().is_ok());
}

#[test]
fn a_stored_password_is_reused_for_a_saved_connection() {
    let (_l, _r, mut model) = fixture();
    let connection = rc_core::config::SavedConnection {
        name: "sftp".into(),
        host: "127.0.0.1".into(),
        port: 22,
        user: "alice".into(),
        protocol: "sftp".into(),
        password: "secret".into(),
        ..Default::default()
    };
    assert!(model.remember_password(&connection).is_ok());
    let mut stored = connection.clone();
    stored.password.clear();
    assert_eq!(model.stored_password(&stored).as_deref(), Some("secret"));
    // A missing entry yields nothing, so the dialog has to ask.
    let unknown = rc_core::config::SavedConnection {
        host: "other".into(),
        ..stored
    };
    assert!(model.stored_password(&unknown).is_none());
}

// ---------------- persistent encrypted credential store ----------------

fn connection(host: &str, user: &str, password: &str) -> rc_core::config::SavedConnection {
    rc_core::config::SavedConnection {
        name: host.into(),
        host: host.into(),
        port: 22,
        user: user.into(),
        protocol: "sftp".into(),
        password: password.into(),
        ..Default::default()
    }
}

#[test]
fn the_credential_store_is_persistent_and_needs_its_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secrets.enc");

    // 1. First session: unlock (creates the file), remember a password.
    let mut model = Model::for_test(
        Settings::default(),
        Location::local("/tmp"),
        Location::local("/tmp"),
    );
    model.secrets_path_override = Some(path.clone());
    assert!(!model.secrets_unlocked());
    let mut state = UnlockSecretsState::new(false);
    state.passphrase = "short".into();
    state.confirm = "short".into();
    assert!(
        state.validate().is_err(),
        "a new store needs a longer passphrase"
    );
    assert!(model.unlock_secrets("correct horse battery").is_ok());
    assert!(model.secrets_unlocked());
    let connection = connection("git.example.com", "alice", "s3cret");
    assert!(model.remember_password(&connection).is_ok());
    assert!(path.exists(), "the store is written to disk");

    // 2. A new session starts locked and can reopen the same file.
    let mut second = Model::for_test(
        Settings::default(),
        Location::local("/tmp"),
        Location::local("/tmp"),
    );
    second.secrets_path_override = Some(path.clone());
    assert!(!second.secrets_unlocked());
    assert!(
        second.stored_password(&connection).is_none(),
        "nothing is available before unlocking"
    );
    // The wrong passphrase is refused and changes nothing.
    assert!(second.unlock_secrets("wrong passphrase").is_err());
    assert!(!second.secrets_unlocked());
    // The right one restores the secret.
    assert!(second.unlock_secrets("correct horse battery").is_ok());
    assert_eq!(
        second.stored_password(&connection).as_deref(),
        Some("s3cret")
    );

    // 3. Locking drops the in-memory copy.
    second.lock_secrets();
    assert!(!second.secrets_unlocked());
    assert!(second.stored_password(&connection).is_none());

    // 4. The file never contains the plain password.
    let bytes = std::fs::read(dir.path().join("secrets.enc")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("s3cret"));
}

#[test]
fn a_password_remembered_while_locked_is_written_after_unlocking() {
    let dir = tempfile::tempdir().unwrap();
    let mut model = Model::for_test(
        Settings::default(),
        Location::local("/tmp"),
        Location::local("/tmp"),
    );
    model.secrets_path_override = Some(dir.path().join("secrets.enc"));
    let connection = connection("host.example", "bob", "queued-secret");
    assert!(model.remember_password(&connection).is_ok());
    assert_eq!(
        model.pending_secrets.len(),
        1,
        "the secret waits for the store"
    );
    assert!(model.status.contains("dialog.secrets.pending"));
    // Unlocking writes the queued secret out.
    assert!(model.unlock_secrets("passphrase-123").is_ok());
    assert!(model.pending_secrets.is_empty());
    assert_eq!(
        model.stored_password(&connection).as_deref(),
        Some("queued-secret")
    );
    assert!(dir.path().exists());
}

#[test]
fn the_unlock_dialog_validates_both_fields() {
    let mut create = UnlockSecretsState::new(false);
    assert!(create.validate().is_err(), "an empty passphrase is refused");
    create.passphrase = "long-enough".into();
    create.confirm = "different".into();
    assert!(create.validate().is_err(), "the confirmation must match");
    create.confirm = "long-enough".into();
    assert!(create.validate().is_ok());
    assert_eq!(create.title(), "dialog.secrets.create");
    let mut unlock = UnlockSecretsState::new(true);
    unlock.passphrase = "x".into();
    assert!(
        unlock.validate().is_ok(),
        "an existing store accepts any non-empty passphrase"
    );
    assert_eq!(unlock.title(), "dialog.secrets.unlock");
}

// ----- dangerous page: factory resets --------------------------------

/// A model whose per-user files live in `dir`, so a reset cannot touch the
/// real profile of whoever runs the suite.
fn model_with_config_dir(dir: &std::path::Path) -> Model {
    let mut model = Model::for_test(
        Settings::default(),
        Location::local("/tmp"),
        Location::local("/tmp"),
    );
    model.config_dir_override = Some(dir.to_path_buf());
    model
}

#[test]
fn factory_settings_reset_restores_defaults_and_deletes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut model = model_with_config_dir(dir.path());
    model.settings.display.font_size = 21.0;
    model.settings.colors.theme = "dark".into();
    model.settings.display.show_hidden = true;
    std::fs::write(
        model.config_file("settings.toml"),
        "[display]\nfont_size = 21.0\n",
    )
    .unwrap();

    let report = model.reset(ResetScope::Settings);

    let defaults = Settings::default();
    assert_eq!(model.settings.display.font_size, defaults.display.font_size);
    assert_eq!(model.settings.colors.theme, defaults.colors.theme);
    assert!(!model.settings.display.show_hidden);
    assert_eq!(report.removed_files, 1);
    assert!(!model.config_file("settings.toml").exists());
    assert!(
        model.settings_dirty,
        "the view has to write the defaults out"
    );
}

#[test]
fn layout_reset_forgets_tabs_history_hotlist_and_command_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut model = model_with_config_dir(dir.path());
    std::fs::write(model.config_file("hotlist.toml"), "[[items]]\n").unwrap();
    model.command_history.push("cd /tmp".into());
    model.left.history.record("/tmp/a", None, 1);
    model.left.tabs.duplicate_active();
    assert_eq!(model.left.tabs.len(), 2);

    let report = model.reset(ResetScope::Layout);

    assert!(model.command_history.is_empty());
    assert!(model.left.history.is_empty());
    assert_eq!(model.left.tabs.len(), 1, "one tab on the current directory");
    assert!(!model.config_file("hotlist.toml").exists());
    assert_eq!(report.removed_files, 1);
    // The panels themselves keep their directories: the user did not ask to
    // lose their place, only the memory of where they were before.
    assert_eq!(model.left.location().display(), "/tmp");
}

#[test]
fn known_hosts_reset_deletes_the_trust_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut model = model_with_config_dir(dir.path());
    let path = model.config_file("known_hosts");
    std::fs::write(&path, "example.com:22 ssh-ed25519 AAAA\n").unwrap();

    let report = model.reset(ResetScope::KnownHosts);

    assert!(!path.exists());
    assert_eq!(report.removed_files, 1);
    assert_eq!(report.cleared, vec!["known hosts".to_string()]);
}

#[test]
fn credentials_reset_deletes_the_store_and_locks_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut model = model_with_config_dir(dir.path());
    assert!(model.unlock_secrets("passphrase-123").is_ok());
    let connection = connection("git.example.com", "alice", "s3cret");
    assert!(model.remember_password(&connection).is_ok());
    let store = model.secrets_file();
    assert!(store.is_file());

    let report = model.reset(ResetScope::Credentials);

    assert!(!store.exists(), "the encrypted file is gone");
    assert!(!model.secrets_exist());
    assert!(!model.secrets_unlocked(), "the session is locked again");
    assert_eq!(model.secrets_count(), 0);
    assert_eq!(report.removed_files, 1);
    // With the store gone the dialog must offer to *create* a passphrase again.
    assert!(!model.secrets_exist());
}

#[test]
fn everything_reset_clears_all_per_user_files_and_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut model = model_with_config_dir(dir.path());
    assert!(model.unlock_secrets("passphrase-123").is_ok());
    for name in [
        "settings.toml",
        "hotlist.toml",
        "known_hosts",
        "session.toml",
    ] {
        std::fs::write(model.config_file(name), "x").unwrap();
    }
    model.command_history.push("dir".into());

    let report = model.reset(ResetScope::Everything);

    for name in [
        "settings.toml",
        "hotlist.toml",
        "known_hosts",
        "session.toml",
        "secrets.enc",
    ] {
        assert!(
            !model.config_file(name).exists(),
            "{name} should have been deleted"
        );
    }
    assert!(model.command_history.is_empty());
    assert!(!model.secrets_unlocked());
    assert_eq!(report.removed_files, 5);
    assert!(report.cleared.iter().any(|item| item == "session"));
}

#[test]
fn a_reset_without_any_files_says_there_was_nothing_to_do() {
    let dir = tempfile::tempdir().unwrap();
    let mut model = model_with_config_dir(dir.path());
    let report = model.reset(ResetScope::Everything);
    assert_eq!(report.removed_files, 0);
    assert!(!report.is_empty(), "the settings themselves were reset");
    let empty = ResetReport {
        removed_files: 0,
        cleared: Vec::new(),
    };
    assert!(empty.is_empty());
}

#[test]
fn every_reset_scope_has_a_label_and_a_hint() {
    let mut seen = Vec::new();
    for scope in ResetScope::all() {
        assert!(scope.label_key().starts_with("reset."));
        assert!(scope.hint_key().starts_with("reset."));
        assert!(!seen.contains(&scope.label_key()));
        seen.push(scope.label_key());
    }
    assert_eq!(ResetScope::all().len(), 5);
    assert!(ResetScope::Credentials.destroys_credentials());
    assert!(ResetScope::Everything.destroys_credentials());
    assert!(!ResetScope::Settings.destroys_credentials());
}

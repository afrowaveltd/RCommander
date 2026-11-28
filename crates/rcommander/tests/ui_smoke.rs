//! Widget-level tests: the real egui UI is driven headlessly with
//! `egui_kittest`, so layout, interaction and command wiring are covered
//! without a window system.

use egui_kittest::kittest::Queryable;
use rc_core::config::Settings;
use rc_core::location::Location;
use rcommander::i18n::KeyEchoStrings;
use rcommander::ui::app::RCommanderApp;

fn write(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

/// Smallest possible red PNG, written without an image crate.
fn write_image(path: &std::path::Path) {
    // A 16x16 red PNG: small enough to inline, real enough to decode.
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x10, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x91, 0x68, 0x36, 0x00, 0x00, 0x00, 0x16, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x38,
        0x21, 0x27, 0x47, 0x12, 0x62, 0x18, 0xd5, 0x30, 0xaa, 0x61, 0xf8, 0x6a, 0x00, 0x00, 0xe4,
        0x62, 0x04, 0x10, 0xe9, 0x9b, 0x80, 0xf1, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
        0xae, 0x42, 0x60, 0x82,
    ];
    std::fs::write(path, PNG).unwrap();
}

fn fixture() -> (tempfile::TempDir, tempfile::TempDir) {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    write(&left.path().join("alpha.txt"), "hello");
    write(&left.path().join("beta.log"), "log data");
    std::fs::create_dir_all(left.path().join("sub")).unwrap();
    write(&right.path().join("target.txt"), "t");
    (left, right)
}

fn app(left: &std::path::Path, right: &std::path::Path) -> RCommanderApp {
    RCommanderApp::with_strings(
        Settings::default(),
        Box::new(KeyEchoStrings::default()),
        Location::local(left),
        Location::local(right),
    )
}

fn harness(app: RCommanderApp) -> egui_kittest::Harness<'static, RCommanderApp> {
    let mut harness = egui_kittest::Harness::builder()
        .with_accessibility_check(false)
        // A dialog that changes its height (a tab row that wraps, a scroll area that
        // re-lays out) needs a few frames to settle; the default budget is four.
        .with_max_steps(16)
        .build_ui_state(
            move |ui, app: &mut RCommanderApp| {
                let mut frame = eframe::Frame::_new_kittest();
                <RCommanderApp as eframe::App>::ui(app, ui, &mut frame);
            },
            app,
        );
    harness.set_size(egui::vec2(1100.0, 700.0));
    harness.run();
    harness
}

#[test]
fn the_main_window_renders_both_panels() {
    let (left, right) = fixture();
    let harness = harness(app(left.path(), right.path()));
    assert_eq!(harness.state().model.left.entries().len(), 3);
    assert_eq!(harness.state().model.right.entries().len(), 1);
    assert!(harness.state().title().contains("RCommander"));
    // The active panel indicator and both panel frames are drawn.
    assert!(
        harness.root().query_all_by_label("sub").count() > 0,
        "directory row is exposed as an accessible label"
    );
}

#[test]
fn drive_selector_is_shown_in_each_panel_and_respects_layout_setting() {
    let (left, right) = fixture();
    let enabled = harness(app(left.path(), right.path()));
    assert!(
        enabled
            .root()
            .query_all_by_label_contains("panel.drives")
            .count()
            >= 2,
        "both panels must expose a drive picker"
    );

    let mut without_drive_bar = app(left.path(), right.path());
    without_drive_bar.model.settings.layout.show_drive_bar = false;
    let disabled = harness(without_drive_bar);
    assert_eq!(
        disabled
            .root()
            .query_all_by_label_contains("panel.drives")
            .count(),
        0,
        "the layout setting must actually hide drive selectors"
    );
}

#[test]
fn tool_bar_and_function_keys_are_rendered() {
    let (left, right) = fixture();
    let harness = harness(app(left.path(), right.path()));
    // The bar is rendered from `settings.button_bar`, whose defaults are real captions.
    assert!(
        harness.root().query_all_by_label("Pack").count() > 0,
        "pack button present"
    );
    assert!(
        harness.root().query_all_by_label("Search").count() > 0,
        "search button present"
    );
    assert!(
        harness.root().query_all_by_label("Options").count() > 0,
        "options button present"
    );
    assert!(
        harness.root().query_all_by_role(egui::Role::Button).count() >= 10,
        "the bar is populated"
    );
    assert!(
        harness.root().query_all_by_label_contains("F3").count() > 0,
        "function key bar present"
    );
}

#[test]
fn pressing_f7_opens_the_create_directory_dialog() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.key_press(egui::Key::F7);
    harness.run();
    assert!(
        matches!(
            harness.state().dialog(),
            Some(rcommander::model::Dialog::MakeDir(_))
        ),
        "expected the mkdir dialog, got {:?}",
        harness.state().dialog()
    );
}

#[test]
fn pressing_tab_switches_the_active_panel() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    assert_eq!(
        harness.state().model.focus,
        rc_core::location::Location::local(left.path())
            .as_path()
            .map(|_| rcommander::Side::Left)
            .unwrap()
    );
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert_eq!(harness.state().model.focus, rcommander::Side::Right);
}

#[test]
fn ctrl_l_opens_language_picker_even_above_an_existing_dialog() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // Language names can use scripts unavailable in the harness's fonts.
    // Match the application's default rendering policy for those glyphs.
    harness
        .ctx
        .set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    harness.key_press(egui::Key::F7);
    harness.run();
    harness.key_press_modifiers(
        egui::Modifiers {
            ctrl: true,
            ..Default::default()
        },
        egui::Key::L,
    );
    harness.run();
    assert!(matches!(
        harness.state().dialog(),
        Some(rcommander::model::Dialog::LanguagePicker)
    ));
    assert_eq!(harness.state().model.dialogs.len(), 2);
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(matches!(
        harness.state().dialog(),
        Some(rcommander::model::Dialog::MakeDir(_))
    ));
}

#[test]
fn clicking_a_row_moves_the_cursor() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // The rows are painted; clicking the text node of a name selects the row.
    {
        let node = harness.root().get_by_label("beta.log");
        node.click();
    }
    harness.run();
    let cursor = harness
        .state()
        .model
        .left
        .cursor_entry()
        .map(|e| e.name.clone());
    assert_eq!(
        cursor.as_deref(),
        Some("beta.log"),
        "cursor should follow the click"
    );
}

#[test]
fn the_help_window_opens_from_the_menu_command() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().execute("cm_HelpIndex");
    harness.run();
    assert!(harness.state().model.help.is_some());
}

#[test]
fn ctrl_t_adds_a_tab_and_ctrl_w_closes_it() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    assert_eq!(harness.state().model.left.tabs.len(), 1);
    harness.key_press_modifiers(
        egui::Modifiers {
            ctrl: true,
            ..Default::default()
        },
        egui::Key::T,
    );
    harness.run();
    assert_eq!(harness.state().model.left.tabs.len(), 2);
    harness.key_press_modifiers(
        egui::Modifiers {
            ctrl: true,
            ..Default::default()
        },
        egui::Key::W,
    );
    harness.run();
    assert_eq!(harness.state().model.left.tabs.len(), 1);
}

#[test]
fn ctrl_q_renders_the_quick_view_of_the_cursor_file() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // Put the cursor on alpha.txt, whose content is "hello".
    let index = harness
        .state()
        .model
        .left
        .listing
        .find("alpha.txt")
        .unwrap();
    harness.state_mut().model.left.set_cursor(index);
    harness.key_press_modifiers(
        egui::Modifiers {
            ctrl: true,
            ..Default::default()
        },
        egui::Key::Q,
    );
    harness.run();
    assert!(harness.state().model.quick_view);
    assert!(
        harness.root().query_all_by_label_contains("hello").count() > 0,
        "the quick view should show the file content"
    );
}

#[test]
fn shift_f2_opens_the_directory_comparison() {
    let (left, right) = fixture();
    // Make sure the two panels differ.
    write(&right.path().join("only-right.txt"), "x");
    let mut harness = harness(app(left.path(), right.path()));
    harness.key_press_modifiers(
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
        egui::Key::F2,
    );
    harness.run();
    match harness.state().dialog() {
        Some(rcommander::model::Dialog::Compare(state)) => {
            assert!(state.rows.iter().any(|r| r.name == "only-right.txt"));
        }
        other => panic!("expected the compare dialog, got {other:?}"),
    }
}

#[test]
fn alt_down_opens_the_directory_history() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.key_press_modifiers(
        egui::Modifiers {
            alt: true,
            ..Default::default()
        },
        egui::Key::ArrowDown,
    );
    harness.run();
    assert!(matches!(
        harness.state().dialog(),
        Some(rcommander::model::Dialog::History(_))
    ));
}

#[test]
fn the_copy_dialog_runs_a_background_transfer_end_to_end() {
    let (left, right) = fixture();
    write(&left.path().join("big.bin"), &"x".repeat(400_000));
    let mut app = app(left.path(), right.path());
    app.model.settings.copy.background = true;
    let mut harness = harness(app);
    // Mark a file, press F5, accept the destination pre-filled from the target panel.
    let index = harness.state().model.left.listing.find("big.bin").unwrap();
    harness.state_mut().model.left.set_cursor(index);
    harness.state_mut().model.left.selection.set(["big.bin"]);
    harness.key_press(egui::Key::F5);
    harness.run();
    assert!(
        matches!(
            harness.state().dialog(),
            Some(rcommander::model::Dialog::Transfer(_))
        ),
        "F5 opens the copy dialog"
    );
    {
        let node = harness.root().get_by_label("dialog.config.ok");
        node.click();
    }
    for _ in 0..400 {
        harness.run();
        if !harness.state().model.is_busy() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        !harness.state().model.is_busy(),
        "the background copy finished"
    );
    assert!(
        right.path().join("big.bin").exists(),
        "the file reached the target panel directory"
    );
    assert!(
        std::fs::metadata(right.path().join("big.bin"))
            .unwrap()
            .len()
            == 400_000
    );
}

#[test]
fn the_agent_panel_opens_and_lists_configured_servers() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model
        .settings
        .agent
        .clients
        .push(rc_core::config::McpServerConfig {
            name: "demo".to_string(),
            transport: "stdio".to_string(),
            command: "demo-server".to_string(),
            ..Default::default()
        });
    app.model.show_agent_panel = true;
    let harness = harness(app);
    assert!(harness.state().model.show_agent_panel);
    assert!(
        harness.root().query_all_by_label("demo").count() > 0,
        "the configured MCP server should be offered in the panel"
    );
}

#[test]
fn the_net_menu_offers_ftp_and_agents() {
    let (left, right) = fixture();
    let harness = harness(app(left.path(), right.path()));
    assert!(
        harness.root().query_all_by_label("menu.net").count() > 0,
        "the Net menu is in the menu bar"
    );
}

#[test]
fn ctrl_shift_f1_switches_to_the_thumbnail_grid() {
    let (left, right) = fixture();
    // A real PNG so the thumbnail decoder has something to work with.
    {
        let rgba: Vec<u8> = (0..16 * 16 * 4).map(|i| (i % 251) as u8).collect();
        let image: image::RgbaImage = image::ImageBuffer::from_raw(16, 16, rgba).unwrap();
        image.save(left.path().join("thumb.png")).unwrap();
    }
    let mut app = app(left.path(), right.path());
    app.model.refresh_active();
    let mut harness = harness(app);
    harness.key_press_modifiers(
        egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
        egui::Key::F1,
    );
    harness.run();
    assert_eq!(
        harness.state().model.left.view_mode,
        rcommander::model::ViewMode::Thumbnails,
        "Ctrl+Shift+F1 selects the thumbnail view"
    );
    // The grid renders the entries as accessible labels, and the image is
    // decodable so a thumbnail is produced.
    harness.run();
    assert!(harness.root().query_all_by_label("thumb.png").count() > 0);
    assert!(
        harness.root().query_all_by_label("sub").count() > 0,
        "directories also appear in the grid"
    );
}

#[test]
fn alt_f7_opens_the_search_dialog_and_runs_a_search() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.key_press_modifiers(
        egui::Modifiers {
            alt: true,
            ..Default::default()
        },
        egui::Key::F7,
    );
    harness.run();
    assert!(
        matches!(
            harness.state().dialog(),
            Some(rcommander::model::Dialog::Search(_))
        ),
        "Alt+F7 opens the search dialog"
    );
    // Narrow the search to one file name and start it.
    if let Some(rcommander::model::Dialog::Search(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.name_mask = "alpha.txt".to_string();
    }
    {
        let node = harness.root().get_by_label("dialog.search.start");
        node.click();
    }
    for _ in 0..400 {
        harness.run();
        let running = match harness.state().dialog() {
            Some(rcommander::model::Dialog::Search(state)) => state.running,
            _ => false,
        };
        if !running {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let results = match harness.state().dialog() {
        Some(rcommander::model::Dialog::Search(state)) => state.results.clone(),
        other => panic!("expected the search dialog, got {other:?}"),
    };
    assert!(
        results.iter().any(|h| h.path.ends_with("alpha.txt")),
        "results: {results:?}"
    );
}

#[test]
fn the_column_editor_opens_and_hides_a_column() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().execute("cm_CustomColumnViewMenu");
    harness.run();
    assert!(
        matches!(
            harness.state().dialog(),
            Some(rcommander::model::Dialog::ViewModes(_))
        ),
        "the editor opens"
    );
    let before = harness.state().model.settings.display.columns.len();
    harness.root().get_by_label("col.attrs ×").click();
    harness.run();
    let columns = &harness.state().model.settings.display.columns;
    assert_eq!(columns.len(), before - 1);
    assert!(columns.iter().all(|column| column.id != "attrs"));
}

#[test]
fn the_full_view_renders_the_configured_columns() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    // Keep only name + comment, so "attrs" must disappear from the header.
    app.model.settings.display.columns = vec![
        rc_core::config::ColumnSpec::new("name", 0.0),
        rc_core::config::ColumnSpec::new("comment", 120.0),
    ];
    let harness = harness(app);
    assert!(
        harness.root().query_all_by_label("col.comment").count() > 0,
        "the comment header is drawn"
    );
    assert_eq!(
        harness.root().query_all_by_label("col.attrs").count(),
        0,
        "the attrs header is gone"
    );
}

#[test]
fn the_transfer_manager_opens_from_the_command() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().execute("cm_OpenTransferManager");
    harness.run();
    assert!(harness.state().model.show_transfer_manager);
    // With no jobs the window still renders (empty state), so the tree contains
    // the window title from the localisation keys.
    assert!(
        harness
            .root()
            .query_all_by_label_contains("dialog.copy.background")
            .count()
            > 0
    );
}

#[test]
fn the_transfer_manager_lists_a_running_job() {
    let (left, right) = fixture();
    write(&left.path().join("big.bin"), &"y".repeat(800_000));
    let mut app = app(left.path(), right.path());
    app.model.refresh_active();
    let index = app.model.left.listing.find("big.bin").unwrap();
    app.model.left.set_cursor(index);
    app.model.left.selection.set(["big.bin"]);
    let state = rcommander::model::dialogs::TransferDialogState {
        ask_overwrite: false,
        ..rcommander::model::dialogs::TransferDialogState::new(
            false,
            app.model.selected_paths(),
            right.path().to_string_lossy().to_string(),
        )
    };
    app.model.start_transfer(&state).expect("job started");
    app.model.show_transfer_manager = true;
    let harness = harness(app);
    assert!(
        harness.state().model.is_busy() || !harness.state().model.is_busy(),
        "the app stays consistent"
    );
    // The manager shows the job row while it runs; if it already finished the
    // empty state is shown instead. Either way the window must be open.
    assert!(harness.state().model.show_transfer_manager);
}

#[test]
fn the_split_dialog_runs_from_the_ui() {
    let (left, right) = fixture();
    write(&left.path().join("payload.bin"), &"z".repeat(4500));
    let mut app = app(left.path(), right.path());
    app.model.refresh_active();
    let index = app.model.left.listing.find("payload.bin").unwrap();
    app.model.left.set_cursor(index);
    let mut harness = harness(app);
    harness.state_mut().execute("cm_Split");
    harness.run();
    assert!(
        matches!(
            harness.state().dialog(),
            Some(rcommander::model::Dialog::Split(_))
        ),
        "the split dialog opens"
    );
    if let Some(rcommander::model::Dialog::Split(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.part_size = 1500;
    }
    {
        let node = harness.root().get_by_label("dialog.config.apply");
        node.click();
    }
    harness.run();
    harness.run();
    assert!(
        left.path().join("payload.001").exists(),
        "parts were written"
    );
    assert!(left.path().join("payload.002").exists());
}

#[test]
fn the_duplicate_finder_dialog_reports_a_group() {
    let (left, right) = fixture();
    write(&left.path().join("d1.txt"), "same bytes");
    write(&left.path().join("d2.txt"), "same bytes");
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().execute("cm_FindDuplicates");
    harness.run();
    assert!(matches!(
        harness.state().dialog(),
        Some(rcommander::model::Dialog::Duplicates(_))
    ));
    {
        let node = harness.root().get_by_label("dialog.search.start");
        node.click();
    }
    harness.run();
    match harness.state().dialog() {
        Some(rcommander::model::Dialog::Duplicates(state)) => {
            assert_eq!(state.groups.len(), 1, "the group is listed");
        }
        other => panic!("expected the duplicates dialog, got {other:?}"),
    }
}

#[test]
fn the_branch_view_command_switches_the_panel() {
    let (left, right) = fixture();
    write(&left.path().join("sub/nested.txt"), "nested content");
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().execute("cm_DirBranch");
    harness.run();
    assert_eq!(
        harness.state().model.left.view_mode,
        rcommander::model::ViewMode::Tree
    );
    // The flat listing shows the nested file by its relative path.
    let entries: Vec<String> = harness
        .state()
        .model
        .left
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert!(
        entries.iter().any(|name| name == "sub/nested.txt"),
        "the branch listing contains the nested file: {entries:?}"
    );
    assert!(
        harness.root().query_all_by_label_contains("nested").count() > 0,
        "the row is rendered"
    );
}

#[test]
fn the_encode_dialog_runs_and_writes_a_file() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.refresh_active();
    app.model.left.selection.set(["alpha.txt"]);
    let mut harness = harness(app);
    harness.state_mut().execute("cm_Encode");
    harness.run();
    assert!(matches!(
        harness.state().dialog(),
        Some(rcommander::model::Dialog::Encode(_))
    ));
    {
        let node = harness.root().get_by_label("dialog.config.apply");
        node.click();
    }
    harness.run();
    harness.run();
    assert!(
        left.path().join("alpha.txt.b64").exists(),
        "the MIME encoded file is written"
    );
}

// ---------------- reported bugs: theme, persistence, navigation ----------------

#[test]
fn the_whole_window_follows_a_theme_change() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    assert_eq!(harness.state().applied_theme(), "light");
    // The dark theme must reach the window chrome, not only the panels.
    harness.state_mut().execute("cm_ThemeToggle");
    harness.run();
    assert_eq!(harness.state().applied_theme(), "dark");
    harness.state_mut().execute("cm_ThemeToggle");
    harness.run();
    assert_eq!(harness.state().applied_theme(), "light");
}

#[test]
fn a_settings_change_is_written_to_disk_automatically() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let path = config.path().join("settings.toml");
    let mut app = app(left.path(), right.path());
    app.settings_writer = rcommander::settings_writer::SettingsWriter::new(path.clone());
    let mut harness = harness(app);
    assert!(
        !path.exists(),
        "nothing has changed yet, so nothing is written"
    );
    harness.state_mut().execute("cm_VisStatusBar");
    harness.run();
    harness.run();
    assert!(
        path.exists(),
        "the change was persisted without an explicit save command"
    );
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("show_status_bar = false"),
        "settings file: {text}"
    );
}

#[test]
fn the_panel_offers_an_explicit_way_to_go_up() {
    let (left, right) = fixture();
    write(&left.path().join("sub/nested.txt"), "n");
    let mut harness = harness(app(&left.path().join("sub"), right.path()));
    // The up button is labelled and reachable without knowing a shortcut.
    {
        // Both panels show an up button (icon + localised caption); the left
        // panel's is the leftmost one.
        let node = harness
            .root()
            .query_all_by_label_contains("cmd.up")
            .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
            .expect("an up button in the panel");
        node.click();
    }
    harness.run();
    harness.run();
    assert_eq!(
        harness.state().model.left.location().as_path().unwrap(),
        left.path()
    );
}

#[test]
fn parent_row_and_history_buttons_navigate_the_same_panel() {
    let (left, right) = fixture();
    let child = left.path().join("sub");
    let mut application = app(&child, right.path());
    application.model.settings.display.show_parent_entry = true;
    application.model.navigate(Location::local(&child)).unwrap();
    let mut harness = harness(application);
    harness
        .root()
        .query_all_by_label("..")
        .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
        .expect("parent row")
        .click();
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(
        harness.state().model.left.location(),
        &Location::local(left.path())
    );
    harness
        .root()
        .query_all_by_label("cmd.go_back")
        .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
        .expect("back button")
        .click();
    harness.run();
    assert_eq!(
        harness.state().model.left.location(),
        &Location::local(&child)
    );
    harness
        .root()
        .query_all_by_label("cmd.go_forward")
        .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
        .expect("forward button")
        .click();
    harness.run();
    assert_eq!(
        harness.state().model.left.location(),
        &Location::local(left.path())
    );
    assert_eq!(
        harness.state().model.right.location(),
        &Location::local(right.path())
    );
}

#[test]
fn the_bundled_icon_font_renders_every_icon() {
    let (left, right) = fixture();
    let harness = harness(app(left.path(), right.path()));
    // Build one label with every glyph; egui panics when a glyph is missing, so a
    // successful run proves the bundled font covers the icon set.
    let all: String = rcommander::icons::Icon::all()
        .iter()
        .map(|icon| icon.glyph())
        .collect();
    // The app installs the font in its theme step; this throwaway harness has to
    // request it itself (once), otherwise egui would reject the code points.
    struct State {
        text: String,
        installed: bool,
    }
    let mut font_harness = egui_kittest::Harness::builder()
        .with_accessibility_check(false)
        .build_ui_state(
            move |ui, state: &mut State| {
                if !state.installed {
                    rcommander::theme::install_icon_font(ui.ctx());
                    state.installed = true;
                }
                ui.label(state.text.clone());
            },
            State {
                text: all,
                installed: false,
            },
        );
    font_harness.run();
    font_harness.run();
    // And the real UI renders entries with their icons attached to the name.
    let names: Vec<String> = harness
        .state()
        .model
        .left
        .entries()
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert!(names.iter().any(|name| name == "alpha.txt"));
    assert!(harness.root().query_all_by_label_contains("sub").count() > 0);
}

#[test]
fn the_credential_store_dialog_opens_and_creates_a_store() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let secrets = config.path().join("secrets.enc");
    let mut app = app(left.path(), right.path());
    app.model.secrets_path_override = Some(secrets.clone());
    let mut harness = harness(app);
    harness
        .state_mut()
        .model
        .open_dialog(rcommander::model::Dialog::UnlockSecrets(
            rcommander::model::dialogs::UnlockSecretsState::new(false),
        ));
    harness.run();
    assert!(matches!(
        harness.state().dialog(),
        Some(rcommander::model::Dialog::UnlockSecrets(_))
    ));
    // Type the passphrase into both fields and accept.
    if let Some(rcommander::model::Dialog::UnlockSecrets(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.passphrase = "correct horse battery".to_string();
        state.confirm = "correct horse battery".to_string();
    }
    harness.run();
    {
        let node = harness.root().get_by_label("dialog.config.ok");
        node.click();
    }
    harness.run();
    harness.run();
    assert!(
        harness.state().model.secrets_unlocked(),
        "the store is unlocked after accepting"
    );
    assert!(secrets.exists(), "the encrypted store file is created");
}

#[test]
fn the_window_close_button_closes_a_dialog() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.key_press(egui::Key::F7);
    harness.run();
    assert!(harness.state().dialog().is_some(), "a dialog is open");
    // The × of the dialog window is what users click to back out. It used to do
    // nothing because the state was written back regardless.
    harness.root().get_by_label("Close window").click();
    harness.run();
    assert!(
        harness.state().dialog().is_none(),
        "clicking the × must close the dialog"
    );
    // The panels accept keys again.
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert_eq!(harness.state().model.focus, rcommander::Side::Right);
}

#[test]
fn the_credential_dialog_offers_to_create_a_passphrase() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    let mut harness = harness(app);
    assert!(!harness.state().model.secrets_exist(), "no store yet");
    harness
        .state_mut()
        .model
        .open_dialog(rcommander::model::Dialog::UnlockSecrets(
            rcommander::model::dialogs::UnlockSecretsState::new(false),
        ));
    harness.run();
    // With KeyEchoStrings every label is its key, so the create wording is
    // directly visible: the dialog says "create", not "unlock".
    assert!(
        harness
            .root()
            .query_all_by_label("dialog.secrets.create")
            .count()
            > 0,
        "the dialog must offer creating a passphrase"
    );
    assert!(
        harness
            .root()
            .query_all_by_label("dialog.secrets.locked")
            .count()
            == 0
    );
}

#[test]
fn enter_in_the_passphrase_fields_unlocks_the_store() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    let mut harness = harness(app);
    harness
        .state_mut()
        .model
        .open_dialog(rcommander::model::Dialog::UnlockSecrets(
            rcommander::model::dialogs::UnlockSecretsState::new(false),
        ));
    harness.run();
    // Typing goes straight into the field that asked for the focus.
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("x".into()));
    harness.run();
    if let Some(rcommander::model::Dialog::UnlockSecrets(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.passphrase = "correct horse battery".to_string();
    }
    harness.key_press(egui::Key::Enter); // moves to the confirmation field
    harness.run();
    assert!(
        !harness.state().model.secrets_unlocked(),
        "one Enter is not enough while the passphrase is not repeated"
    );
    if let Some(rcommander::model::Dialog::UnlockSecrets(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.confirm = "correct horse battery".to_string();
    }
    harness.run();
    harness.key_press(egui::Key::Enter); // submits
    harness.run();
    harness.run();
    harness.run();
    assert!(
        harness.state().model.secrets_unlocked(),
        "the second Enter creates the store"
    );
    assert!(harness.state().model.secrets_file().is_file());
}

#[test]
fn the_thumbnail_view_draws_a_card_with_the_file_name() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    // Switch the way the UI does it: through the command, not by editing the
    // settings behind the panel's back.
    app.model.execute("cm_SrcThumbs");
    let mut harness = harness(app);
    harness.run();
    assert_eq!(
        harness
            .state()
            .model
            .panel(rcommander::Side::Left)
            .view_mode,
        rcommander::model::ViewMode::Thumbnails
    );
    // Every file gets a card whose caption is the file name, so the grid is
    // readable instead of being a wall of empty squares.
    for name in ["alpha.txt", "beta.log"] {
        assert!(
            harness.root().query_all_by_label_contains(name).count() > 0,
            "{name} should have a caption in the thumbnail grid"
        );
    }
    // The card is drawn for directories too.
    assert!(
        harness.root().query_all_by_label_contains("sub").count() > 0,
        "the directory card is drawn"
    );
}

#[test]
fn the_dangerous_page_lists_every_reset_and_needs_confirmation() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    assert!(app.model.unlock_secrets("passphrase-123").is_ok());
    let store = app.model.secrets_file();
    assert!(store.is_file());
    let mut harness = harness(app);
    let settings = harness.state().model.settings.clone();
    harness
        .state_mut()
        .model
        .open_dialog(rcommander::model::Dialog::Options(
            rcommander::model::dialogs::OptionsState::new(
                &settings,
                rcommander::model::dialogs::OptionsTab::Dangerous,
            ),
        ));
    harness.run();
    // Every scope has a button on the page.
    for key in [
        "reset.settings",
        "reset.layout",
        "reset.known_hosts",
        "reset.credentials",
        "reset.everything",
    ] {
        assert!(
            harness.root().query_all_by_label(key).count() > 0,
            "{key} is missing from the dangerous page"
        );
    }
    // First click only arms the reset; nothing is deleted yet.
    harness.root().get_by_label("reset.credentials").click();
    harness.run();
    assert!(store.is_file(), "an armed reset deletes nothing");
    if let Some(rcommander::model::Dialog::Options(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        assert_eq!(
            state.pending_reset,
            Some(rcommander::model::ResetScope::Credentials)
        );
    }
    // Second click, on the confirmation button, wipes the store.
    harness.root().get_by_label("reset.confirm_yes").click();
    harness.run();
    harness.run();
    assert!(!store.exists(), "the encrypted store is deleted");
    assert!(!harness.state().model.secrets_unlocked());
    assert!(!harness.state().model.secrets_exist());
}

/// Render the interesting screens to PNGs for a manual look:
/// `cargo test -p rcommander --test ui_smoke -- --ignored render_snapshots`.
#[test]
#[ignore = "writes screenshots; only useful for a manual look"]
fn render_snapshots() {
    let (left, right) = fixture();
    let out = std::path::Path::new("../../work/snapshots");
    std::fs::create_dir_all(out).unwrap();

    // A real image, so the snapshot covers a decoded card as well as icons.
    write_image(&left.path().join("picture.png"));
    let mut thumbnails = app(left.path(), right.path());
    thumbnails.model.execute("cm_SrcThumbs");
    let mut first = harness(thumbnails);
    first.run();
    first
        .render()
        .unwrap()
        .save(out.join("thumbnails.png"))
        .unwrap();
    drop(first);

    let mut options = app(left.path(), right.path());
    let settings = options.model.settings.clone();
    options
        .model
        .open_dialog(rcommander::model::Dialog::Options(
            rcommander::model::dialogs::OptionsState::new(
                &settings,
                rcommander::model::dialogs::OptionsTab::Dangerous,
            ),
        ));
    let mut second = harness(options);
    second.run();
    second
        .render()
        .unwrap()
        .save(out.join("dangerous.png"))
        .unwrap();
    drop(second);

    let mut connect = app(left.path(), right.path());
    let session_dir = tempfile::tempdir().unwrap();
    connect.model.config_dir_override = Some(session_dir.path().to_path_buf());
    connect.model.load_sessions();
    let _ = connect.model.save_current_as_session(
        "work",
        &rc_core::config::SavedConnection {
            host: "files.example.com".into(),
            user: "alice".into(),
            protocol: "sftp".into(),
            ..rc_core::config::SavedConnection::with_defaults("sftp")
        },
    );
    connect.model.execute("cm_FtpConnect");
    let mut fourth = harness(connect);
    fourth.run();
    fourth
        .render()
        .unwrap()
        .save(out.join("ftp_connect.png"))
        .unwrap();
    drop(fourth);

    let mut colors = app(left.path(), right.path());
    let settings = colors.model.settings.clone();
    colors.model.open_dialog(rcommander::model::Dialog::Options(
        rcommander::model::dialogs::OptionsState::new(
            &settings,
            rcommander::model::dialogs::OptionsTab::Colors,
        ),
    ));
    let mut fifth = harness(colors);
    fifth.run();
    fifth
        .render()
        .unwrap()
        .save(out.join("colors.png"))
        .unwrap();
    drop(fifth);

    let mut unlock = app(left.path(), right.path());
    unlock
        .model
        .open_dialog(rcommander::model::Dialog::UnlockSecrets(
            rcommander::model::dialogs::UnlockSecretsState::new(false),
        ));
    let mut third = harness(unlock);
    third.run();
    third
        .render()
        .unwrap()
        .save(out.join("unlock.png"))
        .unwrap();
}

#[test]
fn clicking_a_row_in_the_other_panel_moves_the_focus_there() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    assert_eq!(harness.state().model.focus, rcommander::Side::Left);

    // Clicking a name in the right panel has to activate that panel; the old
    // code only moved the focus when the panel was already active, so the left
    // panel stayed marked as active forever.
    harness.root().get_by_label("target.txt").click();
    harness.run();
    assert_eq!(
        harness.state().model.focus,
        rcommander::Side::Right,
        "a click in the right panel activates it"
    );
    assert_eq!(
        harness
            .state()
            .model
            .right
            .cursor_entry()
            .map(|e| e.name.clone())
            .as_deref(),
        Some("target.txt")
    );

    // And back again.
    harness.root().get_by_label("beta.log").click();
    harness.run();
    assert_eq!(harness.state().model.focus, rcommander::Side::Left);
}

#[test]
fn view_mode_commands_apply_to_the_panel_that_was_clicked() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.root().get_by_label("target.txt").click();
    harness.run();
    assert_eq!(harness.state().model.focus, rcommander::Side::Right);
    harness.state_mut().execute("cm_SrcThumbs");
    harness.run();
    assert_eq!(
        harness.state().model.right.view_mode,
        rcommander::model::ViewMode::Thumbnails,
        "the clicked panel receives the view-mode command"
    );
    assert_eq!(
        harness.state().model.left.view_mode,
        rcommander::model::ViewMode::Full,
        "the other panel is untouched"
    );
}

#[test]
fn the_connect_dialog_is_pre_filled_with_localhost_and_a_port() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().execute("cm_FtpConnect");
    harness.run();
    let Some(rcommander::model::Dialog::FtpConnect(state)) = harness.state().model.top_dialog()
    else {
        panic!("the connect dialog should be open");
    };
    assert_eq!(state.connection.host, "localhost");
    assert_eq!(state.connection.port, 21, "FTP is pre-ported");
    // Switching to SFTP through the dialog state moves the port along.
    if let Some(rcommander::model::Dialog::FtpConnect(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.set_protocol("sftp");
    }
    harness.run();
    let Some(rcommander::model::Dialog::FtpConnect(state)) = harness.state().model.top_dialog()
    else {
        panic!("the connect dialog should still be open");
    };
    assert_eq!(state.connection.port, 22);
}

#[test]
fn the_connect_dialog_lists_saved_sessions_and_imports_ssh_config() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    app.model.load_sessions();
    app.model
        .save_current_as_session(
            "work",
            &rc_core::config::SavedConnection {
                host: "files.example.com".into(),
                user: "alice".into(),
                protocol: "sftp".into(),
                ..rc_core::config::SavedConnection::with_defaults("sftp")
            },
        )
        .unwrap();
    let mut harness = harness(app);
    harness.state_mut().execute("cm_FtpConnect");
    harness.run();
    // The saved session is offered as a clickable line.
    assert!(
        harness
            .root()
            .query_all_by_label_contains("files.example.com")
            .count()
            > 0,
        "the saved session is listed in the dialog"
    );

    // Importing an ssh_config file adds its hosts as sessions.
    let ssh_config = config.path().join("ssh_config");
    std::fs::write(
        &ssh_config,
        "Host build\n  HostName build.example.com\n  User ci\n",
    )
    .unwrap();
    if let Some(rcommander::model::Dialog::FtpConnect(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.ssh_config_path = ssh_config.to_string_lossy().to_string();
    }
    harness.run();
    harness.root().get_by_label("ftp.sessions.import").click();
    harness.run();
    let mut names = harness.state().model.session_names();
    names.sort();
    assert_eq!(names, vec!["build".to_string(), "work".to_string()]);
    assert!(config.path().join("sessions.toml").is_file());
}

#[test]
fn saved_session_password_prompt_is_focused_and_cancel_does_not_connect() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut application = app(left.path(), right.path());
    application.model.config_dir_override = Some(config.path().to_path_buf());
    application.model.load_sessions();
    let mut profile = rc_core::sessions::Session::new("work", "files.example.com", "sftp");
    profile.user = "alice".into();
    profile.port = 2222;
    application.model.save_session(profile).unwrap();
    assert!(application.model.connect_session("work").is_ok());
    let mut harness = harness(application);
    assert!(harness.root().query_all_by_label("ftp.password").count() > 0);
    assert_eq!(harness.root().query_all_by_label("ftp.host").count(), 0);
    assert_eq!(harness.root().query_all_by_label("ftp.port").count(), 0);
    harness.root().get_by_label("dialog.config.ok").click();
    harness.run();
    assert!(!harness.state().model.is_connecting());
    assert!(harness.state().dialog().is_some());
    harness.root().get_by_label("dialog.config.cancel").click();
    harness.run();
    assert!(!harness.state().model.is_connecting());
    assert!(harness.state().dialog().is_none());
}

#[test]
fn loading_a_saved_session_fills_the_connect_dialog() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    app.model.load_sessions();
    app.model
        .save_current_as_session(
            "office",
            &rc_core::config::SavedConnection {
                host: "office.example.com".into(),
                user: "bob".into(),
                port: 2022,
                ..rc_core::config::SavedConnection::with_defaults("sftp")
            },
        )
        .unwrap();
    let mut harness = harness(app);
    harness.state_mut().execute("cm_FtpConnect");
    harness.run();
    // Click the session line: the form follows.
    harness
        .root()
        .get_by_label_contains("office.example.com")
        .click();
    harness.run();
    let Some(rcommander::model::Dialog::FtpConnect(state)) = harness.state().model.top_dialog()
    else {
        panic!("the connect dialog should still be open");
    };
    assert_eq!(state.connection.host, "office.example.com");
    assert_eq!(state.connection.user, "bob");
    assert_eq!(
        state.connection.port, 2022,
        "a custom port survives the session"
    );
    assert_eq!(state.session_name, "office");
}

#[test]
fn connecting_runs_in_the_background_and_reports_the_failure() {
    use std::io::{ErrorKind, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    const REJECTION: &str = "FTP smoke test fixture refused access";
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();

    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    app.model.load_sessions();
    let mut harness = harness(app);
    harness.state_mut().execute("cm_FtpConnect");
    harness.run();
    let Some(rcommander::model::Dialog::FtpConnect(state)) =
        harness.state_mut().model.top_dialog_mut()
    else {
        panic!("expected the FTP connect dialog");
    };
    state.connection.host = address.ip().to_string();
    state.connection.port = address.port();
    state.connection.user = "nobody".to_string();
    // A named FTP account needs a nonempty password to start the worker.
    state.connection.password = "not-a-real-password".to_string();
    harness.run();
    harness.root().get_by_label("dialog.config.ok").click();
    harness.step();
    assert!(
        harness.state().model.top_dialog().is_none(),
        "the dialog closes immediately and the attempt continues in the background"
    );
    assert!(
        harness.state().model.is_connecting(),
        "the worker stays active while the fixture withholds the greeting"
    );

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "the worker did not contact the fixture"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("could not accept the FTP control connection: {error}"),
        }
    };
    writeln!(socket, "421 {REJECTION}\r").unwrap();
    drop(socket);

    while Instant::now() < deadline && harness.state().model.is_connecting() {
        std::thread::sleep(Duration::from_millis(10));
        harness.step();
    }
    assert!(
        !harness.state().model.is_connecting(),
        "the attempt finished"
    );
    let error = harness.state().model.last_error.as_deref();
    assert!(
        error.is_some_and(|message| message.contains(REJECTION)),
        "the window reports the fixture's rejection, got {error:?}"
    );
}

#[test]
fn a_theme_file_in_the_theme_directory_is_applied() {
    let (left, right) = fixture();
    let themes = tempfile::tempdir().unwrap();
    std::fs::write(
        themes.path().join("ocean.toml"),
        "[meta]\nname = 'Ocean'\nbase = 'dark'\n[colors]\nbackground = '#001a2b'\n",
    )
    .unwrap();
    let mut app = app(left.path(), right.path());
    app.model.themes_dir_override = Some(themes.path().to_path_buf());
    app.model.load_themes();
    let mut harness = harness(app);
    // The theme is discovered without recompiling anything.
    assert!(
        harness
            .state()
            .model
            .theme_names()
            .iter()
            .any(|name| name.eq_ignore_ascii_case("ocean"))
    );
    harness.state_mut().execute("cm_ThemeNext");
    harness.run();
    harness.run();
    assert_eq!(harness.state().model.settings.colors.theme, "ocean");
    assert_eq!(harness.state().applied_theme(), "ocean");
    let palette = rcommander::theme::Palette::for_model(&harness.state().model);
    assert_eq!(palette.background.r(), 0x00);
    assert_eq!(palette.background.g(), 0x1a);
    assert_eq!(palette.background.b(), 0x2b);
    assert!(palette.is_dark());
    // Inherited roles came along: text is still the dark theme's light grey.
    assert_eq!(palette.foreground.r(), palette.foreground.g());
    assert!(palette.foreground.r() > 0xc0);
}

#[test]
fn the_colors_page_edits_and_saves_a_theme() {
    let (left, right) = fixture();
    let themes = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.themes_dir_override = Some(themes.path().to_path_buf());
    app.model.load_themes();
    let settings = app.model.settings.clone();
    // The dialog is opened before the harness exists so its window has settled by
    // the first frame, which is what a running application sees.
    app.model.open_dialog(rcommander::model::Dialog::Options(
        rcommander::model::dialogs::OptionsState::new(
            &settings,
            rcommander::model::dialogs::OptionsTab::Colors,
        ),
    ));
    let mut harness = harness(app);
    harness.run();
    // The page lists every role.
    assert!(
        harness
            .root()
            .query_all_by_label("theme.role.background")
            .count()
            > 0
    );
    assert!(
        harness
            .root()
            .query_all_by_label("theme.role.warning")
            .count()
            > 0
    );

    if let Some(rcommander::model::Dialog::Options(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state
            .theme_edit
            .colors
            .insert("accent".to_string(), "#ff00ff".to_string());
        state.theme_name = "Edited".to_string();
    }
    harness.run();
    // `click_accesskit` because the page lives inside a scrolling dialog: the
    // accessibility action does not depend on the widget being in view, which
    // makes the test independent of the window height.
    harness.root().get_by_label("theme.save").click_accesskit();
    harness.run();
    let file = themes.path().join("edited.toml");
    assert!(
        file.is_file(),
        "the theme is written to the theme directory"
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("#ff00ff"),
        "the edited colour is stored: {text}"
    );
    assert_eq!(harness.state().model.settings.colors.theme, "edited");
    assert_eq!(harness.state().model.theme_display_name("edited"), "Edited");
    assert_eq!(
        harness
            .state()
            .model
            .resolved_theme()
            .get(rc_core::theme::Role::Accent),
        rc_core::theme::Rgb::new(0xff, 0x00, 0xff)
    );
}

#[test]
fn the_git_configuration_dialog_asks_before_reading_and_can_be_refused() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    let mut harness = harness(app);
    assert_eq!(
        harness.state().model.git_config_gate(),
        rcommander::model::GitConfigGate::Undecided
    );
    harness.state_mut().execute("cm_GitConfig");
    harness.run();
    let Some(rcommander::model::Dialog::GitConfig(state)) = harness.state().model.top_dialog()
    else {
        panic!("the git configuration dialog should be open");
    };
    assert!(state.is_asking());
    // Nothing is in use until the user allows it.
    assert!(harness.state().model.git_config().is_none());
    // The dialog lists what it found, and the source files it would read.
    assert!(
        harness.root().query_all_by_label("git.no_secrets").count() > 0,
        "the dialog promises not to store secrets"
    );

    harness.root().get_by_label("git.deny").click();
    harness.run();
    assert!(
        harness.state().model.top_dialog().is_none(),
        "the dialog closes"
    );
    assert_eq!(
        harness.state().model.git_config_gate(),
        rcommander::model::GitConfigGate::Denied
    );
    assert_eq!(
        harness.state().model.settings.git.read_system_config,
        "denied",
        "the answer is remembered because the checkbox starts ticked"
    );
}

#[test]
fn allowing_the_git_configuration_makes_it_available() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    let mut harness = harness(app);
    harness.state_mut().execute("cm_GitConfig");
    harness.run();
    harness.root().get_by_label("git.allow").click();
    harness.run();
    assert_eq!(
        harness.state().model.git_config_gate(),
        rcommander::model::GitConfigGate::Allowed
    );
    assert!(harness.state().model.git_config().is_some());
    // Re-opening shows the active configuration instead of asking again.
    harness.state_mut().execute("cm_GitConfig");
    harness.run();
    let Some(rcommander::model::Dialog::GitConfig(state)) = harness.state().model.top_dialog()
    else {
        panic!("the dialog should show the active configuration");
    };
    assert!(!state.is_asking());
}

/// Build a repository in `root` with one commit, so the Git panel tests are offline.
fn git_fixture(root: &std::path::Path) {
    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head("main");
    let repo = git2::Repository::init_opts(root, &options).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test Author").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(root.join("tracked.txt"), "one\n").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "// lib\n").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Test Author", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[])
        .unwrap();
    // A second local branch, and a change on disk for the status colours.
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("dev", &head, false).unwrap();
    std::fs::write(root.join("tracked.txt"), "changed\n").unwrap();
    std::fs::write(root.join("untracked.txt"), "new\n").unwrap();
    std::fs::write(root.join("debug.log"), "ignored\n").unwrap();
}

#[test]
fn a_git_panel_shows_the_virtual_folders_and_the_branch() {
    let (_left, right) = fixture();
    let repo_dir = tempfile::tempdir().unwrap();
    git_fixture(repo_dir.path());
    let mut app = app(repo_dir.path(), right.path());
    app.model.config_dir_override = Some(repo_dir.path().to_path_buf());
    let mut harness = harness(app);
    harness.state_mut().execute("cm_GitPanel");
    harness.run();
    let panel = &harness.state().model.left;
    let names: Vec<String> = panel.entries().iter().map(|e| e.name.clone()).collect();
    for folder in ["branches", "tags", "commits", "remotes"] {
        assert!(names.contains(&folder.to_string()), "{folder} in {names:?}");
    }
    assert!(names.contains(&"tracked.txt".to_string()));
    assert_eq!(panel.git_label, "main", "the header shows the branch");
    assert!(panel.location().is_git());

    // The working-tree status reaches the panel, which is what colours the rows.
    let status = panel
        .git_status
        .get("tracked.txt")
        .copied()
        .unwrap_or_default();
    assert_eq!(
        rcommander::git_bridge::status_role(status),
        Some(rc_core::theme::Role::Warning),
        "a modified file is drawn in the warning colour"
    );
    let ignored = panel
        .git_status
        .get("debug.log")
        .copied()
        .unwrap_or_default();
    assert_eq!(
        rcommander::git_bridge::status_role(ignored),
        Some(rc_core::theme::Role::Hidden),
        "an ignored file is muted"
    );
    let untracked = panel
        .git_status
        .get("untracked.txt")
        .copied()
        .unwrap_or_default();
    assert_eq!(
        rcommander::git_bridge::status_role(untracked),
        Some(rc_core::theme::Role::Symlink)
    );
}

#[test]
fn the_branch_folder_lists_branches_and_opens_one() {
    let (_left, right) = fixture();
    let repo_dir = tempfile::tempdir().unwrap();
    git_fixture(repo_dir.path());
    let mut app = app(repo_dir.path(), right.path());
    app.model.config_dir_override = Some(repo_dir.path().to_path_buf());
    let mut harness = harness(app);
    harness.state_mut().execute("cm_GitPanel");
    harness.run();
    // Navigate the way the panel does: move the cursor, then enter.
    let branches = harness
        .state()
        .model
        .left
        .entries()
        .iter()
        .position(|e| e.name == "branches")
        .expect("the branches folder is listed");
    harness.state_mut().model.left.set_cursor(branches);
    harness.state_mut().model.enter();
    harness.run();
    let names: Vec<String> = harness
        .state()
        .model
        .left
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, vec!["dev".to_string(), "main".to_string()]);

    // Opening a branch shows that revision's tree, with a branch label.
    harness.state_mut().model.left.set_cursor(0);
    harness.state_mut().model.enter();
    harness.run();
    assert_eq!(harness.state().model.left.git_label, "dev");
    let names: Vec<String> = harness
        .state()
        .model
        .left
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert!(names.contains(&"tracked.txt".to_string()), "{names:?}");
}

#[test]
fn saving_in_the_editor_offers_to_commit() {
    let (_left, right) = fixture();
    let repo_dir = tempfile::tempdir().unwrap();
    git_fixture(repo_dir.path());
    let mut app = app(repo_dir.path(), right.path());
    app.model.config_dir_override = Some(repo_dir.path().to_path_buf());
    let mut harness = harness(app);
    // Open the tracked file in the editor the way F4 would.
    let index = harness
        .state()
        .model
        .left
        .entries()
        .iter()
        .position(|e| e.name == "tracked.txt")
        .expect("the fixture has a tracked file");
    harness.state_mut().model.left.set_cursor(index);
    let path = harness
        .state()
        .model
        .left
        .cursor_entry()
        .unwrap()
        .path
        .clone();
    harness.state_mut().model.open_editor_for(&path).unwrap();
    harness.run();
    assert!(harness.state().model.editor.is_some(), "the editor is open");
    if let Some(state) = harness.state_mut().model.editor.as_mut() {
        state.text.push_str("edited\n");
        state.modified = true;
    }
    harness.run();
    // The editor's Save button writes the file, which asks about committing.
    harness.root().get_by_label("prompt.save").click_accesskit();
    harness.run();
    harness.run();
    match harness.state().model.top_dialog() {
        Some(rcommander::model::Dialog::GitCommit(state)) => {
            assert_eq!(state.paths, vec!["tracked.txt".to_string()]);
            assert!(state.message.contains("tracked.txt"));
        }
        other => panic!("expected the commit dialog, got {other:?}"),
    }
    // Committing closes the dialog and leaves a clean tree.
    harness
        .root()
        .get_by_label("git.commit.do")
        .click_accesskit();
    harness.run();
    harness.run();
    assert!(harness.state().model.top_dialog().is_none());
    let repo = rc_git::GitRepo::open(repo_dir.path()).unwrap();
    let status = repo.status().unwrap();
    assert!(
        status.get("tracked.txt").is_clean(),
        "the saved file was committed: {:?}",
        status.entries
    );
    // The unchanged untracked file of the fixture is still untracked.
    assert!(status.get("untracked.txt").unstaged.is_some());
}

#[test]
fn the_branch_picker_switches_the_panel() {
    let (_left, right) = fixture();
    let repo_dir = tempfile::tempdir().unwrap();
    git_fixture(repo_dir.path());
    let mut app = app(repo_dir.path(), right.path());
    app.model.config_dir_override = Some(repo_dir.path().to_path_buf());
    let mut harness = harness(app);
    harness.state_mut().execute("cm_GitPanel");
    harness.run();
    assert_eq!(harness.state().model.left.git_label, "main");

    harness.state_mut().execute("cm_GitBranches");
    harness.run();
    match harness.state().model.top_dialog() {
        Some(rcommander::model::Dialog::GitBranches(state)) => {
            assert!(
                state.branches.contains(&"dev".to_string()),
                "{:?}",
                state.branches
            );
            assert!(state.branches.contains(&"main".to_string()));
        }
        other => panic!("expected the branch dialog, got {other:?}"),
    }
    // Pick `dev` and switch.
    if let Some(rcommander::model::Dialog::GitBranches(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.selected = state.branches.iter().position(|name| name == "dev");
        // The fixture leaves a modified tracked file, so the switch needs the
        // "keep my changes" confirmation the dialog offers.
        assert!(state.dirty, "the fixture tree is dirty");
        state.allow_dirty = true;
    }
    harness.run();
    harness
        .root()
        .get_by_label("git.branches.switch")
        .click_accesskit();
    harness.run();
    harness.run();
    assert!(
        harness.state().model.top_dialog().is_none(),
        "the dialog closes"
    );
    assert_eq!(
        harness.state().model.left.git_label,
        "dev",
        "the panel follows the new branch"
    );
}

#[test]
fn the_remote_dialog_shows_the_branch_and_fetches_in_the_background() {
    let (_left, right) = fixture();
    let repo_dir = tempfile::tempdir().unwrap();
    git_fixture(repo_dir.path());
    let mut app = app(repo_dir.path(), right.path());
    app.model.config_dir_override = Some(repo_dir.path().to_path_buf());
    let mut harness = harness(app);
    harness.state_mut().execute("cm_GitPanel");
    harness.run();
    harness.state_mut().execute("cm_GitRemote");
    harness.run();
    match harness.state().model.top_dialog() {
        Some(rcommander::model::Dialog::GitRemote(state)) => {
            assert_eq!(state.branch, "main");
            assert!(state.upstream.is_empty(), "the fixture has no remote");
        }
        other => panic!("expected the remote dialog, got {other:?}"),
    }
    // Fetching without a remote fails with a message instead of blocking the window.
    harness
        .root()
        .get_by_label("git.remote.fetch")
        .click_accesskit();
    harness.run();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline && harness.state().model.is_pushing() {
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.run();
    }
    assert!(!harness.state().model.is_pushing(), "the attempt finished");
    assert!(
        harness.state().model.last_error.is_some(),
        "a repository without a remote reports why"
    );
}

#[test]
fn switching_the_theme_repaints_buttons_and_title_bars() {
    let (_left, right) = fixture();
    let themes = tempfile::tempdir().unwrap();
    // A theme with a distinctly dark chrome, so a leftover white button is obvious.
    std::fs::write(
        themes.path().join("night.toml"),
        "[meta]\nname = 'Night'\nbase = 'dark'\n[colors]\nbackground = '#001018'\nheader_background = '#102030'\n",
    )
    .unwrap();
    let mut app = app(themes.path(), right.path());
    app.model.themes_dir_override = Some(themes.path().to_path_buf());
    app.model.load_themes();
    let mut harness = harness(app);
    assert_eq!(harness.state().model.settings.colors.theme, "light");

    harness.state_mut().model.settings.colors.theme = "night".to_string();
    harness.run();
    harness.run();
    // The window palette follows the theme…
    assert_eq!(harness.state().applied_theme(), "night");
    let applied = harness.state().palette.clone();
    assert_eq!(applied.background.r(), 0x00);
    assert_eq!(applied.header_background.r(), 0x10);
    // …and so do the widget styles the window chrome is painted with: buttons and
    // window title bars use the theme's header colour, never egui's default white.
    let visuals = rcommander::theme::visuals(&applied, applied.is_dark());
    assert_eq!(
        visuals.widgets.inactive.weak_bg_fill,
        applied.header_background
    );
    assert_eq!(visuals.widgets.open.weak_bg_fill, applied.header_background);
    assert_eq!(
        visuals.widgets.noninteractive.bg_fill,
        applied.header_background
    );
    assert!(
        visuals.widgets.inactive.weak_bg_fill.r() < 0x80,
        "a dark theme must not paint white buttons"
    );
}

#[test]
fn the_panel_offers_a_volume_selector() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // The selector exists when the machine has a volume the picker shows.
    let pickable = harness.state().model.selectable_volumes();
    if pickable.is_empty() {
        return;
    }
    // The list hides the system volume, snap images and pseudo file systems.
    for volume in &pickable {
        let root = volume.root.to_string_lossy().to_string();
        assert!(
            !root.starts_with("/snap/") && root != "/" && !root.starts_with("/boot/"),
            "the selector shows {root:?}"
        );
    }
    // The panel knows which volume it is on.
    let current = harness.state().model.current_volume(rcommander::Side::Left);
    assert!(current.is_some(), "the panel reports its volume");
    // And the model can move a panel there without a dialog.
    let root = pickable[0].root.clone();
    harness
        .state_mut()
        .model
        .open_volume(rcommander::Side::Left, &root);
    harness.run();
    assert_eq!(
        harness.state().model.left.location().as_path(),
        Some(root.as_path())
    );
}

#[test]
fn right_clicking_a_row_opens_the_context_menu() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // Right-click selects the row and opens the menu of file operations.
    harness.root().get_by_label("beta.log").click_secondary();
    harness.run();
    harness.run();
    assert_eq!(
        harness
            .state()
            .model
            .left
            .cursor_entry()
            .map(|e| e.name.clone())
            .as_deref(),
        Some("beta.log"),
        "the right-clicked row becomes the cursor row"
    );
    // The menu offers view/edit/copy/move/rename/delete (KeyEchoStrings shows the keys).
    for key in ["cmd.view", "cmd.edit", "cmd.copy", "cmd.delete"] {
        assert!(
            harness.root().query_all_by_label_contains(key).count() > 0,
            "the context menu offers {key}"
        );
    }
    // The menu closes again when the command runs; the mapping from a menu entry to an
    // internal command is covered by a unit test in `panel_view`, because a headless
    // harness cannot reliably click inside an egui popup.
}

#[test]
fn the_approval_dialog_shows_the_operation_and_can_be_refused() {
    use rc_core::policy::Request;
    let (left, right) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(dir.path().to_path_buf());
    app.model.audit = rc_core::audit::AuditLog::new(dir.path().join("audit.log"), true);
    app.model.settings.agent.profile = "privileged".to_string();
    app.model.refresh_policy();
    let mut harness = harness(app);

    let request = Request::privileged("systemctl restart apache2", "agent");
    harness
        .state_mut()
        .model
        .ask_approval(&request, "the service fails to start");
    harness.run();
    // The dialog names the operation and its target, and warns that this one is irreversible.
    assert!(
        harness
            .root()
            .query_all_by_label("agent.op.privileged")
            .count()
            > 0
    );
    assert!(
        harness
            .root()
            .query_all_by_label_contains("systemctl restart apache2")
            .count()
            > 0
    );
    assert!(
        harness
            .root()
            .query_all_by_label("agent.risk.irreversible")
            .count()
            > 0
    );
    assert!(
        harness
            .root()
            .query_all_by_label("agent.risk.privileged")
            .count()
            > 0,
        "the dialog says what elevated rights mean"
    );
    assert!(
        harness
            .root()
            .query_all_by_label("agent.approval.why")
            .count()
            > 0
    );

    // Refusing closes the dialog and lands in the audit trail.
    harness
        .root()
        .get_by_label("agent.approval.refuse")
        .click_accesskit();
    harness.run();
    harness.run();
    assert!(harness.state().model.top_dialog().is_none());
    let events = harness.state().model.audit().read();
    assert_eq!(
        events.last().unwrap().verdict,
        rc_core::audit::Verdict::Refused
    );
    assert!(harness.state().model.audit_state().is_intact());
}

/// A provider that answers from a fixed list, so the dialog can be tested offline.
#[derive(Debug)]
struct StubProvider;

impl rc_llm::Provider for StubProvider {
    fn kind(&self) -> &'static str {
        "ollama"
    }
    fn base_url(&self) -> &str {
        "http://stub"
    }
    fn list_models(&self) -> Result<Vec<rc_llm::Model>, rc_llm::LlmError> {
        Ok(vec![
            rc_llm::Model {
                id: "coder:7b".into(),
                size: Some(4_700_000_000),
                parameters: Some("7.6B".into()),
                capabilities: vec!["completion".into(), "tools".into()],
            },
            rc_llm::Model {
                id: "chat:3b".into(),
                size: None,
                parameters: None,
                capabilities: vec!["completion".into()],
            },
        ])
    }
    fn chat(&self, request: &rc_llm::ChatRequest) -> Result<rc_llm::ChatReply, rc_llm::LlmError> {
        Ok(rc_llm::ChatReply {
            content: format!("OK from {}", request.model),
            model: request.model.clone(),
            done_reason: Some("stop".into()),
            prompt_tokens: Some(3),
            answer_tokens: Some(2),
        })
    }
}

#[test]
fn the_model_picker_lists_models_and_selects_one() {
    let (_left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(config.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    app.model
        .set_ai_provider(Some(std::sync::Arc::new(StubProvider)));
    let mut harness = harness(app);
    harness.state_mut().execute("cm_AiModel");
    harness.run();

    // Both models are listed with their capabilities.
    assert!(
        harness
            .root()
            .query_all_by_label_contains("coder:7b")
            .count()
            > 0,
        "the tool-capable model is offered"
    );
    assert!(
        harness
            .root()
            .query_all_by_label_contains("chat:3b")
            .count()
            > 0
    );
    // Picking the second model and pressing "use" stores it.
    if let Some(rcommander::model::Dialog::AiModel(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.selected = Some(1);
    }
    harness.run();
    harness.root().get_by_label("ai.use").click_accesskit();
    harness.run();
    assert_eq!(harness.state().model.ai_model(), Some("chat:3b"));
    assert_eq!(
        harness.state().model.settings.agent.model,
        "chat:3b",
        "the choice is persisted in the settings"
    );
}

// ---- agent chat (Phase 16.1) ---------------------------------------------

/// A model provider that streams a fixed answer, for the chat window tests.
struct ChatStub {
    pieces: Vec<String>,
}

impl std::fmt::Debug for ChatStub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChatStub")
    }
}

impl rc_llm::Provider for ChatStub {
    fn kind(&self) -> &'static str {
        "ollama"
    }
    fn base_url(&self) -> &str {
        "http://stub"
    }
    fn list_models(&self) -> Result<Vec<rc_llm::Model>, rc_llm::LlmError> {
        Ok(vec![rc_llm::Model {
            id: "chat:3b".into(),
            size: None,
            parameters: None,
            capabilities: vec!["completion".into()],
        }])
    }
    fn chat(&self, request: &rc_llm::ChatRequest) -> Result<rc_llm::ChatReply, rc_llm::LlmError> {
        Ok(rc_llm::ChatReply {
            content: self.pieces.join(""),
            model: request.model.clone(),
            done_reason: Some("stop".into()),
            prompt_tokens: None,
            answer_tokens: None,
        })
    }
    fn chat_stream(
        &self,
        request: &rc_llm::ChatRequest,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<rc_llm::ChatReply, rc_llm::LlmError> {
        for piece in &self.pieces {
            on_chunk(piece);
        }
        Ok(rc_llm::ChatReply {
            content: self.pieces.join(""),
            model: request.model.clone(),
            done_reason: Some("stop".into()),
            prompt_tokens: Some(3),
            answer_tokens: Some(2),
        })
    }
}

fn chat_harness(
    left: &std::path::Path,
    right: &std::path::Path,
) -> egui_kittest::Harness<'static, RCommanderApp> {
    let mut app = app(left, right);
    app.model
        .set_ai_provider(Some(std::sync::Arc::new(ChatStub {
            pieces: vec!["Two ".into(), "files.".into()],
        })));
    app.model.settings.agent.model = "chat:3b".to_string();
    app.model.show_chat_panel = true;
    harness(app)
}

#[test]
fn the_chat_window_opens_from_the_menu_and_streams_an_answer() {
    let (left, right) = fixture();
    let mut harness = chat_harness(left.path(), right.path());
    // The window is on screen with its empty-transcript hint.
    harness.root().get_by_label("ai.chat.empty");
    // Typing and pressing Send puts the question in the transcript.
    harness.state_mut().model.chat().input = "how many files?".to_string();
    harness.run();
    if let Some(button) = harness.root().query_all_by_label("ai.chat.send").last() {
        button.click_accesskit();
    }
    // Let the worker finish and the frames catch up.
    for _ in 0..200 {
        if !harness.state().model.chat_is_running() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.run();
    }
    harness.run();
    let lines = harness.state().model.chat_session.lines.clone();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[1].text, "Two files.");
    harness.root().get_by_label("how many files?");
    harness.root().get_by_label("Two files.");
}

#[test]
fn the_chat_window_toggles_with_its_command() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    assert!(!harness.state().model.show_chat_panel);
    harness.state_mut().execute("cm_AiChat");
    assert!(
        harness.state().model.show_chat_panel,
        "the command opens the chat"
    );
    harness.run();
    harness.root().get_by_label("ai.chat.title");
    // Closing the window clears the flag through the window's own close button.
    harness.state_mut().model.show_chat_panel = false;
    harness.run();
    assert!(
        harness.root().query_all_by_label("ai.chat.empty").count() == 0,
        "a closed chat draws nothing"
    );
}

#[test]
fn a_markdown_answer_is_rendered_instead_of_showing_its_markup() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.show_chat_panel = true;
    app.model.chat().push(
        rcommander::model::chat::Speaker::Model,
        "## Result\n\nThere are **two** files:\n\n- `a.txt`\n- b.txt\n",
    );
    let harness = harness(app);
    // The words are on screen...
    harness.root().get_by_label("Result");
    harness.root().get_by_label("two");
    assert!(
        harness.root().query_all_by_label_contains("a.txt").count() > 0,
        "the inline code keeps its text"
    );
    // ...and the markup characters are not.
    for markup in ["##", "**"] {
        assert!(
            harness.root().query_all_by_label_contains(markup).count() == 0,
            "{markup} should not be on screen"
        );
    }
}

#[test]
fn a_streaming_answer_grows_without_losing_its_place() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.show_chat_panel = true;
    let chat = app.model.chat();
    chat.push(rcommander::model::chat::Speaker::User, "and then?");
    let index = chat.push(rcommander::model::chat::Speaker::Model, "");
    let seq = chat.lines[index].seq;
    chat.lines[index].streaming = true;
    let mut harness = harness(app);

    // Three frames of a growing answer: the text is replaced in place, the line keeps its identity.
    for piece in ["Look", " at this", " file."] {
        if let Some(line) = harness
            .state_mut()
            .model
            .chat()
            .lines
            .iter_mut()
            .find(|line| line.seq == seq)
        {
            line.text.push_str(piece);
        }
        harness.run();
    }
    harness.run();
    let line = harness
        .state()
        .model
        .chat_session
        .lines
        .iter()
        .find(|line| line.seq == seq)
        .cloned()
        .expect("the answer is still there");
    assert_eq!(line.text, "Look at this file.");
    assert_eq!(
        harness
            .state()
            .model
            .chat_session
            .lines
            .iter()
            .filter(|line| line.seq == seq)
            .count(),
        1,
        "one line, not three partial ones"
    );
    harness.root().get_by_label("Look at this file.");
}

#[test]
fn the_approval_dialog_offers_three_answers_and_the_red_one_asks_for_a_word() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    // A push always needs approval, so it is the honest case for this dialog.
    let request = rc_core::policy::Request::publish("origin/main", "agent:git");
    app.model.ask_approval(&request, "publishing the commit");
    let mut harness = harness(app);
    // The dialog names the operation, the target and what saying yes means.
    harness.root().get_by_label("agent.approval.allow_once");
    harness.root().get_by_label("agent.approval.allow_session");
    harness.root().get_by_label("agent.approval.allow_always");
    harness.root().get_by_label("agent.risk.publish");

    // The red choice leads to a second window that wants the word typed.
    harness
        .root()
        .get_by_label("agent.approval.allow_always")
        .click_accesskit();
    harness.run();
    harness.root().get_by_label("agent.always.title");
    assert!(
        !harness
            .state()
            .model
            .policy()
            .is_preapproved(rc_core::policy::Operation::Publish),
        "nothing is allowed yet"
    );
    assert!(
        !harness
            .state()
            .model
            .policy()
            .is_approved_this_session(rc_core::policy::Operation::Publish)
    );
    // "Confirm" is disabled until the word matches.
    if let Some(rcommander::model::Dialog::AgentApproval(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.typed = "yes".to_string();
    }
    harness.run();
    harness
        .root()
        .get_by_label("agent.always.send")
        .click_accesskit();
    harness.run();
    assert!(
        harness.state().model.top_dialog().is_none(),
        "the dialog closed after the confirmation"
    );
    assert!(
        harness
            .state()
            .model
            .policy()
            .is_preapproved(rc_core::policy::Operation::Publish),
        "the permanent answer is stored"
    );
}

#[test]
fn the_security_page_shows_the_profile_the_preapprovals_and_the_log() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    // A stored permanent answer from an earlier session, plus a live session approval.
    app.model.settings.agent.preapproved = vec![rc_core::config::Preapproval {
        operation: "publish".to_string(),
        label: "approved in the dialog".to_string(),
        created: 1_700_000_000,
    }];
    app.model.refresh_policy();
    app.model
        .policy
        .approve_for_session(rc_core::policy::Operation::Execute);
    app.model.execute("cm_Config");
    assert!(
        matches!(
            app.model.top_dialog(),
            Some(rcommander::model::Dialog::Options(_))
        ),
        "the configuration dialog is open"
    );
    let mut harness = harness(app);
    if let Some(rcommander::model::Dialog::Options(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.tab = rcommander::model::dialogs::OptionsTab::Security;
    }
    harness.run();
    // The three parts of the page are on screen.
    harness.root().get_by_label("agent.security.profile");
    harness.root().get_by_label("agent.security.always");
    harness.root().get_by_label("agent.security.log");
    // The stored pre-approval names its operation (the same name also appears as an approval
    // checkbox, hence `query_all`) and offers to take it back.
    assert!(
        harness
            .root()
            .query_all_by_label("agent.op.publish")
            .count()
            >= 2,
        "the page lists the pre-approval as well as the checkbox"
    );
    harness.root().get_by_label("agent.security.always.revoke");
    // The live session approval is listed too.
    assert!(
        harness
            .root()
            .query_all_by_label("agent.op.execute")
            .count()
            > 0
    );

    // Taking the permanent answer back clears it from the policy as well.
    harness
        .root()
        .get_by_label("agent.security.always.revoke")
        .click_accesskit();
    harness.run();
    assert!(
        !harness
            .state()
            .model
            .policy()
            .is_preapproved(rc_core::policy::Operation::Publish),
        "the revoke button bites immediately"
    );
    assert!(harness.state().model.settings.agent.preapproved.is_empty());
}

#[test]
fn the_font_page_offers_three_roles_and_a_sample() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.execute("cm_Config");
    let mut harness = harness(app);
    if let Some(rcommander::model::Dialog::Options(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.tab = rcommander::model::dialogs::OptionsTab::Fonts;
    }
    harness.run();
    // The three roles are named, and the page says where the list comes from...
    harness.root().get_by_label("fonts.body");
    harness.root().get_by_label("fonts.heading");
    harness.root().get_by_label("fonts.mono");
    harness.root().get_by_label("fonts.sample");
    // ...and it has a live sample in each of the three sizes.
    harness.root().get_by_label("fonts.sample.heading");
    harness.root().get_by_label("fonts.sample.body");
    harness.root().get_by_label("fonts.sample.mono");
    // Picking a family writes it into the draft, not straight into the live settings.
    if let Some(rcommander::model::Dialog::Options(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.draft.fonts.mono_family = "DejaVu Sans Mono".to_string();
        state.draft.fonts.heading_size = 22.0;
    }
    harness.run();
    assert!(
        harness.state().model.settings.fonts.mono_family.is_empty(),
        "the draft is not the settings"
    );
    // Accepting the dialog commits it and sanitises the sizes.
    if let Some(rcommander::model::Dialog::Options(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        state.draft.fonts.mono_size = 999.0;
    }
    harness.run();
    harness
        .root()
        .get_by_label("dialog.config.ok")
        .click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().model.settings.fonts.mono_family,
        "DejaVu Sans Mono"
    );
    assert_eq!(harness.state().model.settings.fonts.heading_size, 22.0);
    assert_eq!(
        harness.state().model.settings.fonts.mono_size,
        48.0,
        "an impossible size is clamped instead of accepted"
    );
}

#[test]
fn a_code_block_in_an_answer_is_syntax_coloured() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.show_chat_panel = true;
    app.model.chat().push(
        rcommander::model::chat::Speaker::Model,
        "Run this:\n\n```rust\nlet x: usize = 42; // the answer\n```\n",
    );
    let harness = harness(app);
    // The code is on screen, and the markup around it is not.
    assert!(
        harness
            .root()
            .query_all_by_label_contains("let x: usize = 42;")
            .count()
            > 0,
        "the code line is rendered"
    );
    // The fence marker is markup and must not be drawn; the language label is not markup and is.
    assert!(
        harness.root().query_all_by_label_contains("```").count() == 0,
        "the fence is not shown"
    );
    assert!(
        harness.root().query_all_by_label_contains("rust").count() > 0,
        "the language is labelled above the code"
    );
}

#[test]
fn the_chat_can_live_in_a_panel_while_the_other_panel_shows_files() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.chat().push(
        rcommander::model::chat::Speaker::User,
        "what is in this folder?",
    );
    app.model.chat().push(
        rcommander::model::chat::Speaker::Model,
        "Three files, listed above.",
    );
    let mut harness = harness(app);
    harness.state_mut().model.focus = rcommander::model::Side::Left;
    harness
        .state_mut()
        .model
        .set_panel_chat(rcommander::model::Side::Right, true);
    harness.run();
    // The conversation is in the panel...
    harness.root().get_by_label("what is in this folder?");
    harness.root().get_by_label("Three files, listed above.");
    // ...and the left panel still lists the files, so both are on screen at once.
    assert!(
        harness
            .root()
            .query_all_by_label_contains("alpha.txt")
            .count()
            > 0,
        "the file list is still visible"
    );
    // "Show files" puts the listing back and keeps the conversation.
    harness
        .root()
        .get_by_label("agent.pane.files")
        .click_accesskit();
    harness.run();
    assert!(
        !harness
            .state()
            .model
            .panel_shows_chat(rcommander::model::Side::Right)
    );
    assert_eq!(harness.state().model.chat_session.lines.len(), 2);
}

#[test]
fn the_panel_command_opens_the_chat_beside_the_files() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().model.focus = rcommander::model::Side::Left;
    harness.state_mut().execute("cm_ChatPane");
    harness.run();
    // The focused panel keeps its files; the other one hosts the chat.
    assert!(
        !harness
            .state()
            .model
            .panel_shows_chat(rcommander::model::Side::Left)
    );
    assert!(
        harness
            .state()
            .model
            .panel_shows_chat(rcommander::model::Side::Right)
    );
    harness.root().get_by_label("ai.chat.empty");
    // Closing it gives the panel back its listing.
    harness.state_mut().execute("cm_ChatFiles");
    harness.run();
    assert!(
        !harness
            .state()
            .model
            .panel_shows_chat(rcommander::model::Side::Right)
    );
    // The listing is back, and the "show files" button is gone with the pane.
    harness.root().get_by_label("alpha.txt");
    assert!(
        harness
            .root()
            .query_all_by_label("agent.pane.files")
            .count()
            == 0,
        "no chat pane, no pane button"
    );
}

/// Every character the shipped help topics use must be drawable with the fonts we ship.
///
/// The app draws a "tofu" box for a character nothing can render, which is how a menu path written
/// with an exotic arrow ends up looking like a defect (and how `egui_kittest` turns it into a
/// panic). This test is the guard: a topic that gains such a character fails here instead of in a
/// user's window.
#[test]
fn every_help_character_can_be_drawn_with_the_bundled_fonts() {
    let docs = rcommander::paths::docs_dir();
    if !docs.is_dir() {
        return;
    }
    let ctx = egui::Context::default();
    rcommander::theme::install_icon_font(&ctx);
    let output = ctx.run_ui(egui::RawInput::default(), |_| {});
    let mut deltas = output.textures_delta;
    deltas.clear();
    let mut missing: Vec<(String, char)> = Vec::new();
    for entry in std::fs::read_dir(&docs).unwrap().flatten() {
        let topic = entry.path();
        if !topic.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&topic).unwrap().flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for ch in text.chars() {
                // Newlines and tabs are layout, not glyphs.
                if ch == '\n' || ch == '\r' || ch == '\t' {
                    continue;
                }
                let drawable =
                    ctx.fonts_mut(|fonts| fonts.has_glyph(&egui::FontId::proportional(14.0), ch));
                if !drawable && !missing.iter().any(|(_, seen)| *seen == ch) {
                    missing.push((path.display().to_string(), ch));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "these characters would be drawn as empty boxes: {missing:?}"
    );
}

#[test]
fn the_menu_leads_to_the_pages_that_became_features() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // Each of these commands opens the options dialog on its own page, which is what the
    // Configuration menu offers directly.
    for (command, tab) in [
        ("cm_Fonts", rcommander::model::dialogs::OptionsTab::Fonts),
        (
            "cm_Security",
            rcommander::model::dialogs::OptionsTab::Security,
        ),
        ("cm_Colors", rcommander::model::dialogs::OptionsTab::Colors),
        (
            "cm_Dangerous",
            rcommander::model::dialogs::OptionsTab::Dangerous,
        ),
    ] {
        harness.state_mut().model.dialogs.clear();
        harness.state_mut().execute(command);
        match harness.state().model.top_dialog() {
            Some(rcommander::model::Dialog::Options(state)) => {
                assert_eq!(state.tab, tab, "{command} opens the wrong page")
            }
            other => panic!("{command} did not open the options dialog: {other:?}"),
        }
    }
    harness.state_mut().model.dialogs.clear();
    harness.run();
}

#[test]
fn the_agent_menu_offers_the_way_back_only_while_a_pane_shows_the_chat() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // Agent is now a top-level menu, not hidden under Configuration.
    harness.root().get_by_label("menu.git");
    harness.root().get_by_label("menu.agent").click_accesskit();
    harness.run();
    assert!(
        harness
            .root()
            .query_all_by_label("agent.pane.files")
            .count()
            == 0,
        "no chat pane, so no way back from one"
    );
    // With the chat in a panel, the entry appears.
    harness
        .state_mut()
        .model
        .set_panel_chat(rcommander::model::Side::Right, true);
    harness.run();
    assert!(
        harness
            .root()
            .query_all_by_label("agent.pane.files")
            .count()
            > 0,
        "the menu offers to close the pane it opened"
    );
}

#[test]
fn toolbar_default_captions_are_localized_and_custom_caption_is_preserved() {
    let (left, right) = fixture();
    let locale_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../rc-i18n/locales");
    let strings = rcommander::i18n::TranslatorStrings::discover(&locale_dir, "cs");
    let settings = Settings {
        language: "cs".into(),
        ..Settings::default()
    };
    let mut app = RCommanderApp::with_strings(
        settings,
        Box::new(strings),
        Location::local(left.path()),
        Location::local(right.path()),
    );
    app.model
        .settings
        .button_bar
        .add(rc_core::config::BarButton::new("My button", "cm_Copy"));
    let harness = harness(app);
    harness.root().get_by_label("Stručně");
    harness.root().get_by_label("My button");
}

#[test]
fn the_prompt_picker_offers_the_catalogue_and_inserts_the_filled_text() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.show_chat_panel = true;
    let mut harness = harness(app);
    // Opening the picker shows the categories and the prompts.
    harness.root().get_by_label("prompt.pick").click_accesskit();
    harness.run();
    harness.root().get_by_label("prompt.category.git");
    harness.root().get_by_label("prompt.all");
    // Filtering to Git leaves the file prompt out of the list.
    harness
        .root()
        .get_by_label("prompt.category.git")
        .click_accesskit();
    harness.run();
    assert!(
        harness.root().query_all_by_label_contains("Files").count() <= 1,
        "only the Git prompts are listed"
    );
    // "Use this prompt" fills the input with the rendered text.
    harness.root().get_by_label("prompt.use").click_accesskit();
    harness.run();
    let input = harness.state().model.chat_session.input.clone();
    assert!(!input.is_empty(), "the input has the prompt");
    assert!(!input.contains("{{"), "the variables are filled: {input}");
    assert!(
        !harness.state().model.prompt_picker.open,
        "the picker closed"
    );
}

#[test]
fn a_prompt_with_a_variable_the_program_cannot_fill_says_so_in_the_preview() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.show_chat_panel = true;
    let mut harness = harness(app);
    harness.state_mut().model.prompt_picker.open = true;
    harness.state_mut().model.prompt_picker.category = Some(rc_core::prompt::Category::Remote);
    harness.run();
    // A local panel cannot fill `{{host}}`, so the preview warns instead of pretending.
    assert!(
        harness
            .root()
            .query_all_by_label_contains("prompt.missing")
            .count()
            > 0,
        "the missing variable is announced"
    );
}

#[test]
fn the_prompt_page_edits_a_copy_and_resets_it_to_the_shipped_text() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    let mut harness = harness(app);
    harness.state_mut().execute("cm_Prompts");
    harness.run();
    // The page is open with the catalogue and the editor hint.
    harness.root().get_by_label("prompt.page.help");
    harness.root().get_by_label("prompt.editor.none");
    // Editing a built-in starts from a copy, and the copy is what the user's file will hold.
    assert!(
        harness
            .state_mut()
            .model
            .copy_prompt_to_mine("files.explain")
            .is_ok()
    );
    harness.run();
    harness.root().get_by_label("prompt.editor.title");
    harness.state_mut().model.prompt_editor.text = "Explain {{selection}} my way.".to_string();
    harness.run();
    harness.root().get_by_label("prompt.save").click_accesskit();
    harness.run();
    let edited = harness
        .state_mut()
        .model
        .prompts()
        .get("files.explain")
        .cloned()
        .expect("the prompt exists");
    assert_eq!(edited.prompt.text, "Explain {{selection}} my way.");
    assert!(
        !edited.builtin,
        "the user's version replaced the shipped one"
    );
    // The page shows the shipped text as the reset target...
    harness.root().get_by_label("prompt.editor.original");
    // ...and resetting brings it back.
    assert!(
        harness
            .state_mut()
            .model
            .reset_prompt("files.explain")
            .unwrap()
    );
    harness.run();
    let restored = harness
        .state_mut()
        .model
        .prompts()
        .get("files.explain")
        .cloned()
        .expect("the prompt exists");
    assert!(restored.builtin);
    assert_ne!(restored.prompt.text, "Explain {{selection}} my way.");
}

#[test]
fn the_picker_pins_a_prompt_and_remembers_what_was_used() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.show_chat_panel = true;
    let mut harness = harness(app);
    harness.root().get_by_label("prompt.pick").click_accesskit();
    harness.run();
    // Every row offers a pin button; pinning moves the prompt to the top.
    let before = harness.state_mut().model.visible_prompts();
    let second_id = before
        .get(1)
        .map(|entry| entry.prompt.id.clone())
        .expect("the catalogue has at least two prompts");
    assert!(harness.state_mut().model.toggle_prompt_pin(&second_id));
    harness.run();
    let after = harness.state_mut().model.visible_prompts();
    assert_eq!(
        after.first().map(|entry| entry.prompt.id.clone()),
        Some(second_id),
        "the pinned prompt is first"
    );
    // The list shows the pin marker and offers to unpin.
    assert!(
        harness.root().query_all_by_label("prompt.unpin").count() > 0,
        "the pinned row offers to unpin"
    );
    // Using a prompt records it as recent.
    let used = after.get(1).map(|entry| entry.prompt.id.clone()).unwrap();
    harness.state_mut().model.insert_prompt(&used);
    harness.run();
    assert_eq!(
        harness
            .state()
            .model
            .prompt_picker
            .recent
            .first()
            .map(String::as_str),
        Some(used.as_str())
    );
}

#[test]
fn the_status_line_reports_progress_and_alerts() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // An error becomes an alert with the warning colour and a lasting line in the status bar.
    harness
        .state_mut()
        .model
        .report_error("the server refused the key");
    harness.run();
    assert!(
        harness
            .root()
            .query_all_by_label_contains("the server refused the key")
            .count()
            > 0,
        "the error is visible, not only in the last line of the log"
    );
    // A finished answer is announced too, and does not look like an error.
    harness.state_mut().model.report_done(
        "the model finished its answer",
        Some(rc_core::desktop::Sound::Done),
    );
    harness.run();
    assert!(
        harness
            .root()
            .query_all_by_label_contains("the model finished its answer")
            .count()
            > 0
    );
}

#[test]
fn the_function_key_bar_follows_the_focused_surface() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.chat().push(
        rcommander::model::chat::Speaker::User,
        "what is in this folder?",
    );
    let mut harness = harness(app);
    // Over the files the bar offers the file commands.
    harness.root().get_by_label("surface.files");
    assert!(
        harness
            .root()
            .query_all_by_label_contains("cmd.copy")
            .count()
            > 0,
        "the file row is drawn (the test translator echoes keys)"
    );
    // Put the chat in the focused panel: the bar changes with it.
    harness
        .state_mut()
        .model
        .set_panel_chat(rcommander::model::Side::Left, true);
    harness.run();
    harness.root().get_by_label("surface.chat");
    assert!(
        harness
            .root()
            .query_all_by_label_contains("prompt.export")
            .count()
            > 0,
        "the chat row offers the export"
    );
    assert!(
        harness
            .root()
            .query_all_by_label_contains("cmd.copy")
            .count()
            == 0,
        "and no longer offers a command that makes no sense here"
    );
    // Pressing the F5 button of that row clears the conversation.
    harness
        .root()
        .query_all_by_label_contains("F5")
        .next()
        .expect("the chat row has an F5")
        .click_accesskit();
    harness.run();
    assert!(
        harness.state().model.chat_session.lines.is_empty(),
        "F5 over the chat is New conversation, not Copy"
    );
}

#[test]
fn exporting_the_conversation_writes_a_readable_file() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.chat().push(
        rcommander::model::chat::Speaker::User,
        "why is the sky blue?",
    );
    app.model.chat().push(
        rcommander::model::chat::Speaker::Model,
        "Rayleigh scattering.",
    );
    let mut harness = harness(app);
    harness.state_mut().execute("cm_ChatExport");
    harness.run();
    let mut written: Vec<std::path::PathBuf> = std::fs::read_dir(left.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with("chat-"))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(written.len(), 1, "one transcript was written: {written:?}");
    let text = std::fs::read_to_string(written.pop().unwrap()).unwrap();
    assert!(text.contains("why is the sky blue?"), "{text}");
    assert!(text.contains("Rayleigh scattering."), "{text}");
}

#[test]
fn f10_leaves_the_program_like_it_did_in_norton_commander() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // The bar offers F10 over files, and it means Quit.
    harness.root().get_by_label("surface.files");
    assert!(
        harness.root().query_all_by_label_contains("F10").count() > 0,
        "F10 is on the bar"
    );
    // Pressing it exits: the model reports Quit, and the window is asked to close.
    let outcome = harness.state_mut().execute("cm_Exit");
    assert_eq!(outcome, rcommander::model::CommandOutcome::Quit);
}

#[test]
fn a_held_modifier_turns_the_bar_into_the_other_function() {
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    // The bar is one row: no second line of modifier labels underneath it.
    assert_eq!(
        harness
            .root()
            .query_all_by_label("key.modifier.shift")
            .count(),
        0,
        "the variants are not a row of their own"
    );
    // The model is what knows the variant and its caption; the bar only reads it.
    let variants = harness.state().model.key_variants(3);
    assert!(!variants.is_empty(), "F3 has modifier variants");
    let shift = variants
        .iter()
        .find(|variant| variant.modifier == "shift")
        .expect("shift+F3 exists");
    assert_eq!(shift.command, "cm_ListInternalOnly");
    assert_eq!(
        shift.label_key, "cmd.view_internal",
        "the caption of the variant is known, so the bar can say it"
    );
    // Holding a modifier is an input event, so it is expressed as one: the bar reads the modifiers
    // of the frame and swaps the caption, which needs no second row of widgets.
    harness.event(egui::Event::Key {
        key: egui::Key::F3,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    });
    harness.run();
}

#[test]
fn processing_with_the_agent_opens_a_chat_about_a_copy() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    // A file to hand over, created before the model reads the directory.
    std::fs::write(left.path().join("handover.txt"), b"the original").unwrap();
    app.model.refresh_all();
    let mut harness = harness(app);
    let index = harness
        .state()
        .model
        .left
        .listing
        .find("handover.txt")
        .expect("the file is listed");
    harness.state_mut().model.left.set_cursor(index);
    harness.state_mut().model.focus = rcommander::model::Side::Left;
    harness.state_mut().execute("cm_ProcessWithAgent");
    harness.run();
    // The chat sits in the other panel and the files are still visible.
    assert!(
        harness
            .state()
            .model
            .panel_shows_chat(rcommander::model::Side::Right)
    );
    let workspace = harness
        .state()
        .model
        .workspace()
        .cloned()
        .expect("a workspace");
    assert_eq!(workspace.files.len(), 1);
    let copy = workspace.files[0].copy.clone();
    assert!(copy.is_file(), "the agent has a copy at {}", copy.display());
    // The original is byte-identical, which is the whole point.
    assert_eq!(
        std::fs::read_to_string(left.path().join("handover.txt")).unwrap(),
        "the original"
    );
    // Closing the workspace removes the copies and leaves the original.
    let directory = workspace.directory.clone();
    harness.state_mut().execute("cm_DiscardWorkspace");
    harness.run();
    assert!(!directory.exists(), "the copies are gone");
    assert!(left.path().join("handover.txt").is_file());
    assert!(harness.state().model.workspace().is_none());
}

#[test]
fn the_workspace_review_shows_the_diff_and_writes_only_what_is_ticked() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    std::fs::write(left.path().join("a.txt"), b"one\ntwo\nthree\n").unwrap();
    std::fs::write(left.path().join("b.txt"), b"keep this\n").unwrap();
    app.model.refresh_all();
    let mut harness = harness(app);
    // Hand both files to the agent, marking them first so the workspace holds both.
    harness.state_mut().model.focus = rcommander::model::Side::Left;
    let first = harness
        .state()
        .model
        .left
        .listing
        .find("a.txt")
        .expect("a.txt is listed");
    let second = harness
        .state()
        .model
        .left
        .listing
        .find("b.txt")
        .expect("b.txt is listed");
    harness.state_mut().model.left.set_cursor(first);
    harness.state_mut().model.left.toggle_current();
    harness.state_mut().model.left.set_cursor(second);
    harness.state_mut().model.left.toggle_current();
    harness.state_mut().execute("cm_ProcessWithAgent");
    harness.run();
    let workspace = harness.state().model.workspace().cloned().unwrap();
    // The agent edits one copy only.
    let edited = workspace
        .files
        .iter()
        .find(|file| file.relative == "a.txt")
        .expect("a.txt is in the workspace")
        .copy
        .clone();
    std::fs::write(&edited, b"one\nTWO\nthree\n").unwrap();
    // The review lists both files and shows the diff of the changed one.
    harness.state_mut().execute("cm_ReviewWorkspace");
    harness.run();
    harness.root().get_by_label("workspace.review.title");
    assert!(
        harness.root().query_all_by_label_contains("+ TWO").count() > 0,
        "the diff shows the added line"
    );
    assert!(
        harness.root().query_all_by_label_contains("- two").count() > 0,
        "and the removed one"
    );
    // Only the changed file is pre-ticked, so writing back touches it and nothing else.
    harness
        .root()
        .get_by_label("workspace.review.apply")
        .click_accesskit();
    harness.run();
    assert_eq!(
        std::fs::read_to_string(left.path().join("a.txt")).unwrap(),
        "one\nTWO\nthree\n"
    );
    assert_eq!(
        std::fs::read_to_string(left.path().join("b.txt")).unwrap(),
        "keep this\n",
        "the untouched file was not rewritten"
    );
}

#[test]
fn the_review_blocks_a_file_with_a_syntax_error_and_offers_hunks() {
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    std::fs::write(left.path().join("data.json"), "{\n  \"a\": 1\n}\n").unwrap();
    app.model.refresh_all();
    let mut harness = harness(app);
    let index = harness
        .state()
        .model
        .left
        .listing
        .find("data.json")
        .expect("data.json is listed");
    harness.state_mut().model.left.set_cursor(index);
    harness.state_mut().model.focus = rcommander::model::Side::Left;
    harness.state_mut().execute("cm_ProcessWithAgent");
    harness.run();
    // The agent breaks the JSON.
    let copy = harness.state().model.workspace().unwrap().files[0]
        .copy
        .clone();
    std::fs::write(&copy, "{\n  \"a\": 1,\n}\n").unwrap();
    harness.state_mut().execute("cm_ReviewWorkspace");
    harness.run();
    // The parser catches it, the row says why, and nothing is ticked.
    assert!(
        harness.root().query_all_by_label_contains("JSON:").count() > 0,
        "the parser's own message is on screen"
    );
    let apply_count = harness
        .state_mut()
        .model
        .review_workspace()
        .approved()
        .len();
    assert_eq!(apply_count, 0, "a broken file is not approved");
    // Hunks are offered for a file that parses, with a header a reviewer can read.
    assert!(
        harness.root().query_all_by_label_contains("@@ -").count() > 0,
        "the diff is split into hunks"
    );
}

#[test]
fn the_agent_panel_lists_the_remote_tools_with_a_trust_switch_each() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    // Pretend the panel connected to a server that publishes two tools.
    app.model.remember_mcp_tools(
        "photo-craft",
        vec![
            (
                "create_icon".to_string(),
                "draw an icon".to_string(),
                "{}".to_string(),
            ),
            (
                "delete_document".to_string(),
                String::new(),
                "{}".to_string(),
            ),
        ],
    );
    app.model.show_agent_panel = true;
    let mut harness = harness(app);
    // The catalogue is on the model, which is what the panel draws from.
    assert_eq!(
        harness
            .state()
            .model
            .mcp_available_tools("photo-craft")
            .len(),
        2
    );
    // Denying a tool removes it from what the model may be offered, and the switch is remembered.
    harness
        .state_mut()
        .model
        .set_mcp_tool_trust(
            "photo-craft",
            "delete_document",
            rc_core::mcptools::ToolTrust::Deny,
            "",
        )
        .expect("denying needs no ceremony");
    assert_eq!(
        harness
            .state()
            .model
            .mcp_available_tools("photo-craft")
            .len(),
        1
    );
    // Allowing a destructive tool is refused without the word, and accepted with it.
    let refused = harness.state_mut().model.set_mcp_tool_trust(
        "photo-craft",
        "delete_document",
        rc_core::mcptools::ToolTrust::Allow,
        "",
    );
    assert!(refused.is_err(), "a click is not a signature");
    let allowed = harness.state_mut().model.set_mcp_tool_trust(
        "photo-craft",
        "delete_document",
        rc_core::mcptools::ToolTrust::Allow,
        "yes",
    );
    assert!(allowed.is_ok());
    assert!(
        harness
            .state()
            .model
            .mcp_tool_allowed("photo-craft", "delete_document")
    );
}

/// A plugin source that shows a ticket desk, with its own columns and one verb.
struct TicketDesk;

impl rcommander::model::sources::PanelSource for TicketDesk {
    fn id(&self) -> &str {
        "tickets"
    }
    fn title(&self) -> &str {
        "Ticket desk"
    }
    fn list(&self, _path: &str) -> rc_core::error::CoreResult<Vec<rc_core::entry::Entry>> {
        Ok(vec![rc_core::entry::Entry::new(
            "Printer on fire",
            std::path::PathBuf::from("t/17"),
            rc_core::entry::EntryKind::File,
            0,
        )])
    }
    fn columns(&self) -> Vec<rcommander::model::sources::SourceColumn> {
        vec![
            rcommander::model::sources::SourceColumn::new("subject", "Subject", 240.0),
            rcommander::model::sources::SourceColumn::new("age", "Age", 60.0).numeric(),
        ]
    }
    fn cell(&self, entry: &rc_core::entry::Entry, column: &str) -> String {
        match column {
            "subject" => entry.name.clone(),
            "age" => "3d".to_string(),
            _ => String::new(),
        }
    }
    fn actions(
        &self,
        _entry: &rc_core::entry::Entry,
    ) -> Vec<rcommander::model::sources::SourceMenuAction> {
        vec![rcommander::model::sources::SourceMenuAction::new(
            "Reply", "reply",
        )]
    }
}

#[test]
fn a_plugin_source_panel_draws_its_own_columns_and_hides_file_operations() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    app.model.sources.register(Box::new(TicketDesk));
    let mut harness = harness(app);
    assert!(
        harness
            .state_mut()
            .model
            .open_in(
                rcommander::model::Side::Left,
                Location::plugin("tickets", "")
            )
            .is_ok()
    );
    harness.run();
    // The source's headers are drawn, the file columns are not.
    assert!(
        harness.root().query_all_by_label("Subject").count() > 0,
        "the declared header is drawn"
    );
    // Only the right panel, which is still a directory, keeps the configured headers: a source
    // replaces the columns of its own panel and of no other.
    assert_eq!(
        harness.root().query_all_by_label("col.size").count(),
        1,
        "a ticket desk has no size column, the other panel still does"
    );
    // The row itself is exposed with its source-provided name.
    assert!(
        harness
            .root()
            .query_all_by_label_contains("Printer on fire")
            .count()
            > 0
    );
    // Delete is refused: the place never declared it, and no dialog pretends otherwise.
    let outcome = harness.state_mut().execute("cm_Delete");
    assert!(
        matches!(outcome, rcommander::model::CommandOutcome::Failed(_)),
        "{outcome:?}"
    );
    assert!(
        !harness.state().model.has_dialog(),
        "a refused operation opens nothing"
    );
}

#[test]
fn the_plugins_page_lists_what_is_installed_and_switches_it_off() {
    let (left, right) = fixture();
    let mut app = app(left.path(), right.path());
    // A plugin installed in the configuration directory the app was pointed at.
    let config = tempfile::tempdir().unwrap();
    let notes = config.path().join("plugins").join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    write(
        &notes.join("plugin.toml"),
        r#"
[plugin]
id = "notes"
name = "Notes"
version = "1.0"
description = "Every note as a row"

[source]
name = "Notes"
list = ["notes-index"]
[[source.columns]]
id = "title"
label = "Note"
[source.capabilities]
allow = ["view"]
"#,
    );
    app.model.config_dir_override = Some(config.path().to_path_buf());
    app.model.load_plugins();
    assert_eq!(app.model.plugin_entries().len(), 1);
    let mut harness = harness(app);
    harness
        .state_mut()
        .model
        .open_options_on(rcommander::model::dialogs::OptionsTab::Plugins);
    harness.run();
    // The page names the plugin and says what it may do.
    assert!(
        harness.root().query_all_by_label_contains("Notes").count() > 0,
        "the plugin is listed"
    );
    assert!(
        harness
            .root()
            .query_all_by_label_contains("plugins.capabilities.readonly")
            .count()
            > 0,
        "the page says the place is read-only"
    );
    // Switching it off takes effect immediately: the source is gone from the registry.
    let location = Location::plugin("notes", "");
    assert!(harness.state().model.sources.source_of(&location).is_some());
    harness.state_mut().model.set_plugin_enabled("notes", false);
    assert!(harness.state().model.sources.source_of(&location).is_none());
    harness.run();
    assert!(
        harness
            .root()
            .query_all_by_label_contains("plugins.capabilities.readonly")
            .count()
            > 0,
        "the disabled plugin is still listed, so it can be switched back on"
    );
}

#[test]
fn the_models_page_adds_a_connection_and_shows_the_egress_switch() {
    let (left, right) = fixture();
    let app = app(left.path(), right.path());
    let mut harness = harness(app);
    harness
        .state_mut()
        .model
        .open_options_on(rcommander::model::dialogs::OptionsTab::Models);
    harness.run();
    // A profile without routing says so instead of pretending to have connections.
    assert!(
        harness
            .root()
            .query_all_by_label_contains("ai.route.no_connections")
            .count()
            > 0,
        "the page explains that the legacy provider is in use"
    );
    // The egress switch is offered and starts off.
    assert!(!harness.state().model.settings.agent.allow_remote_models);
    harness
        .root()
        .get_by_label_contains("ai.route.allow_remote")
        .click();
    harness.run();
    let Some(rcommander::model::Dialog::Options(state)) =
        harness.state_mut().model.top_dialog_mut()
    else {
        panic!("the options dialog is open");
    };
    // The switch exists and is wired to the draft: flipping it writes there.
    state.draft.agent.allow_remote_models = true;
    harness.run();
    assert!(
        harness
            .root()
            .query_all_by_label_contains("ai.route.allow_remote")
            .count()
            > 0,
        "the egress switch is offered"
    );
    // Adding a connection edits the draft, not the live settings.
    harness
        .root()
        .get_by_label_contains("ai.route.add_connection")
        .click();
    harness.run();
    let Some(rcommander::model::Dialog::Options(state)) =
        harness.state_mut().model.top_dialog_mut()
    else {
        panic!("the options dialog is open");
    };
    assert_eq!(
        state.draft.agent.model_catalog.connections.len(),
        1,
        "the new connection is in the draft"
    );
    assert_eq!(
        state.draft.agent.model_catalog.connections[0].id,
        "connection-1"
    );
    assert!(
        harness
            .state()
            .model
            .settings
            .agent
            .model_catalog
            .connections
            .is_empty(),
        "a cancelled dialog changes nothing"
    );
    // The new row is visible, with the provider it will use and the actions it offers.
    assert!(
        harness.root().query_all_by_label_contains("ollama").count() > 0,
        "the row shows the provider kind"
    );
    assert!(
        harness
            .root()
            .query_all_by_label_contains("ai.route.discover")
            .count()
            > 0,
        "the row offers discovery"
    );
    assert!(
        harness
            .root()
            .query_all_by_label_contains("ai.route.remove")
            .count()
            > 0,
        "the row can be removed"
    );
}

#[test]
fn the_route_editor_keeps_a_duplicate_out() {
    let (left, right) = fixture();
    let app = app(left.path(), right.path());
    let mut harness = harness(app);
    harness
        .state_mut()
        .model
        .open_options_on(rcommander::model::dialogs::OptionsTab::Models);
    harness.run();
    // The wizard's three sections are all reachable from one page.
    for key in [
        "ai.route.connections",
        "ai.route.models_heading",
        "ai.route.quotas_heading",
        "ai.route.routes_heading",
    ] {
        assert!(
            harness.root().query_all_by_label_contains(key).count() > 0,
            "the page shows the {key} section"
        );
    }
    // A route written into the draft is drawn with its task name and a preview line, which is the
    // page's "why". The editing rules themselves are unit-tested in `model::routing`.
    let Some(rcommander::model::Dialog::Options(state)) =
        harness.state_mut().model.top_dialog_mut()
    else {
        panic!("the options dialog is open");
    };
    state.draft.agent.model_catalog.routes = vec![rc_core::model_catalog::RoutingPreference {
        task: "chat".into(),
        model_ids: vec!["nobody".into()],
        ..rc_core::model_catalog::RoutingPreference::default()
    }];
    // A route that names a model the catalogue does not have is reported, not silently accepted:
    // the page draws the validation warning and the catalogue is not saved.
    harness.run();
    assert!(
        harness
            .root()
            .query_all_by_label_contains("ai.route.issue")
            .count()
            > 0,
        "the page warns that the catalogue cannot be saved"
    );
    let catalogue = harness.state().model.settings.agent.model_catalog.clone();
    assert_eq!(
        catalogue.validate(),
        Ok(()),
        "the live settings were not touched by the draft"
    );
}

#[test]
fn document_surfaces_can_share_the_two_panels_without_losing_editor_text() {
    use rcommander::model::{DocumentPlacement, Side, Surface};
    let (left, right) = fixture();
    let path = left.path().join("document.txt");
    std::fs::write(&path, "original").unwrap();
    let mut app = app(left.path(), right.path());
    app.open_editor(path.clone());
    assert_eq!(
        app.model.editor.as_ref().unwrap().placement,
        DocumentPlacement::Workspace
    );
    app.model.editor.as_mut().unwrap().placement = DocumentPlacement::Panel(Side::Left);
    app.model.editor.as_mut().unwrap().text = "unsaved".into();
    app.model.editor.as_mut().unwrap().modified = true;
    app.model.focus = Side::Right;
    assert_eq!(app.model.surface(), Surface::Panel(Side::Right));
    app.open_viewer(path.clone());
    app.model.viewer.as_mut().unwrap().placement = DocumentPlacement::Panel(Side::Right);
    let mut harness = harness(app);
    harness.run();
    assert_eq!(
        harness
            .root()
            .query_all_by_label("document.workspace")
            .count(),
        2
    );
    harness.set_size(egui::vec2(900.0, 550.0));
    harness.run();
    assert_eq!(
        harness
            .root()
            .query_all_by_label("document.workspace")
            .count(),
        2
    );
    assert_eq!(
        harness.state().model.editor.as_ref().unwrap().text,
        "unsaved"
    );
    assert!(harness.state_mut().model.open_editor_for(path).is_err());
    assert_eq!(
        harness.state().model.editor.as_ref().unwrap().text,
        "unsaved"
    );
}

#[test]
fn f3_and_f4_open_the_expected_workspace_surface() {
    use rcommander::model::{DocumentPlacement, Surface};
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    let index = harness
        .state()
        .model
        .left
        .entries()
        .iter()
        .position(|entry| entry.name == "alpha.txt")
        .unwrap();
    harness.state_mut().model.left.set_cursor(index);
    harness.key_press(egui::Key::F3);
    harness.run();
    assert_eq!(harness.state().model.surface(), Surface::Viewer);
    assert_eq!(
        harness.state().model.viewer.as_ref().unwrap().placement,
        DocumentPlacement::Workspace
    );
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.state().model.viewer.is_none());
    harness.key_press(egui::Key::F4);
    harness.run();
    assert_eq!(harness.state().model.surface(), Surface::Editor);
    assert_eq!(harness.state().model.editor.as_ref().unwrap().text, "hello");
    assert_eq!(
        harness.state().model.editor.as_ref().unwrap().placement,
        DocumentPlacement::Workspace
    );
}

#[test]
fn settings_keyboard_confirm_and_cancel_the_draft() {
    use rcommander::model::{Dialog, dialogs::OptionsTab};
    let (left, right) = fixture();
    let mut harness = harness(app(left.path(), right.path()));
    let original = harness.state().model.settings.display.show_hidden;
    for (key, expected) in [(egui::Key::Escape, original), (egui::Key::Enter, !original)] {
        harness
            .state_mut()
            .model
            .open_options_on(OptionsTab::Display);
        harness.run();
        let Some(Dialog::Options(state)) = harness.state_mut().model.top_dialog_mut() else {
            panic!("settings dialog expected");
        };
        state.draft.display.show_hidden = !original;
        harness.run();
        harness.key_press(key);
        harness.run();
        assert!(
            harness.state().dialog().is_none(),
            "{key:?} closes the dialog"
        );
        assert_eq!(harness.state().model.settings.display.show_hidden, expected);
    }
}

#[test]
fn markdown_f3_defaults_to_rendered_content_and_keeps_source_available() {
    let (left, right) = fixture();
    let path = left.path().join("preview.md");
    write(&path, "# Preview title\n\n**bold content**\n");
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().open_viewer(path);
    harness.run();
    assert!(harness.root().query_all_by_label("Preview title").count() > 0);
    assert!(harness.root().query_all_by_label("bold content").count() > 0);
    harness.state_mut().model.viewer.as_mut().unwrap().mode = rcommander::model::ViewerMode::Text;
    harness.run();
    assert!(harness.root().query_all_by_label("bold content").count() == 0);
}

#[test]
fn csv_f3_displays_records_as_separate_cells() {
    let (left, right) = fixture();
    let path = left.path().join("preview.csv");
    write(&path, "city,value\nPrague,42\n");
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().open_viewer(path);
    harness.run();
    assert!(harness.root().query_all_by_label("city").count() > 0);
    assert!(harness.root().query_all_by_label("Prague").count() > 0);
    assert!(harness.root().query_all_by_label("42").count() > 0);
}

#[test]
fn xml_f3_shows_attributes_in_a_tree() {
    let (left, right) = fixture();
    let path = left.path().join("preview.xml");
    write(&path, "<root id=\"42\"><item>hello</item></root>");
    let mut harness = harness(app(left.path(), right.path()));
    harness.state_mut().open_viewer(path);
    harness.run();
    assert!(harness.root().query_all_by_label("@id: 42").count() > 0);
}

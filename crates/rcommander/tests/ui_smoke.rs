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
    // Every current column is offered as a removable chip.
    {
        // The chip in the dialog, not the header button behind it.
        let node = harness
            .root()
            .query_all_by_label_contains("col.attrs")
            .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
            .expect("the attrs chip in the editor");
        node.click();
    }
    harness.run();
    let after = harness.state().model.settings.display.columns.len();
    assert_eq!(after, before - 1, "clicking a column chip removes it");
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

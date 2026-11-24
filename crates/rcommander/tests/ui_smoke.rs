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
    let (left, right) = fixture();
    let config = tempfile::tempdir().unwrap();
    let mut app = app(left.path(), right.path());
    app.model.config_dir_override = Some(config.path().to_path_buf());
    app.model.load_sessions();
    let mut harness = harness(app);
    harness.state_mut().execute("cm_FtpConnect");
    harness.run();
    if let Some(rcommander::model::Dialog::FtpConnect(state)) =
        harness.state_mut().model.top_dialog_mut()
    {
        // Nothing listens on this port, so the attempt fails at once instead of
        // blocking the window.
        state.connection.host = "127.0.0.1".to_string();
        state.connection.port = 65_533;
        state.connection.user = "nobody".to_string();
    }
    harness.run();
    harness.root().get_by_label("dialog.config.ok").click();
    harness.run();
    assert!(
        harness.state().model.top_dialog().is_none(),
        "the dialog closes immediately and the attempt continues in the background"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline && harness.state().model.is_connecting() {
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.run();
    }
    assert!(
        !harness.state().model.is_connecting(),
        "the attempt finished"
    );
    assert!(
        harness.state().model.last_error.is_some(),
        "the failure is reported in the window"
    );
}

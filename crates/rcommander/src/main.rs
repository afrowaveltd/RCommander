//! RCommander entry point.
//!
//! The release Windows build is a GUI application: without this attribute the
//! linker produces a console binary and every start would open a black window
//! next to the file manager. Debug builds keep the console so panics and logs
//! stay visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rc_core::config::Settings;
use rc_core::location::Location;
use rcommander::model::Model;
use rcommander::ui::app::RCommanderApp;

const HELP: &str = "\
RCommander — a cross-platform dual-pane file manager

USAGE:
    rcommander [LEFT_DIR] [RIGHT_DIR]
    rcommander mcp [--http PORT --token TOKEN]
    rcommander --help | --version

OPTIONS:
    -h, --help        show this help
    -V, --version     print the version
    mcp               run as a Model Context Protocol server on stdio

STATUS:
    settings     the configuration file in use
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        println!(
            "    settings     {}",
            rcommander::paths::settings_path().display()
        );
        println!(
            "    locales      {}",
            rcommander::paths::locales_dir().display()
        );
        println!(
            "    help topics  {}",
            rcommander::paths::docs_dir().display()
        );
        return;
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("rcommander {}", rcommander::VERSION);
        return;
    }
    if args.first().map(|a| a.as_str()) == Some("mcp") {
        run_mcp(&args[1..]);
        return;
    }

    let locations: Vec<Location> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(|a| Location::parse(a))
        .filter_map(Result::ok)
        .collect();
    let left = locations.first().cloned();
    let right = locations.get(1).cloned();
    if let Err(e) = run_gui(left, right) {
        eprintln!("rcommander: {e}");
        std::process::exit(1);
    }
}

fn run_gui(left: Option<Location>, right: Option<Location>) -> Result<(), String> {
    let app = RCommanderApp::boot(left, right);
    let settings: &Settings = &app.model.settings;
    let title = app.title();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([settings.layout.window_width, settings.layout.window_height])
            .with_min_inner_size([640.0, 400.0])
            .with_title(title)
            .with_maximized(settings.layout.window_maximized),
        ..Default::default()
    };
    eframe::run_native(
        "RCommander",
        options,
        Box::new(move |_cc| Ok(Box::new(app))),
    )
    .map_err(|e| e.to_string())
}

fn run_mcp(args: &[String]) {
    // The MCP server host is implemented in `rcommander::agent_host`; the
    // subcommand wires it to stdin/stdout.
    let model = build_headless_model();
    match rcommander::agent_host::serve_stdio(model) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("rcommander mcp: {e}");
            std::process::exit(1);
        }
    }
    let _ = args;
}

fn build_headless_model() -> Model {
    let settings = Settings::load(&rcommander::paths::settings_path()).unwrap_or_default();
    let strings = Box::new(rcommander::i18n::TranslatorStrings::discover(
        &rcommander::paths::locales_dir(),
        &settings.language,
    ));
    let home = rc_core::drive::home_dir()
        .map(Location::local)
        .unwrap_or_else(|| Location::local("/"));
    Model::boot(settings, strings, home.clone(), home)
}

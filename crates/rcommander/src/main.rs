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
    rcommander mcp [--ask | --yes | --no-ask]
    rcommander --help | --version

OPTIONS:
    -h, --help        show this help
    -V, --version     print the version
    mcp               run as a Model Context Protocol server on stdio
      --ask         ask on this terminal before a destructive tool (default)
      --yes, -y     allow destructive tools without asking; the audit log still records them
      --no-ask      never allow destructive tools (read-only agent)

STATUS:
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

/// Which approval source the `mcp` subcommand uses for destructive tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApprovalMode {
    /// Ask on the terminal; without a terminal, refuse (the historical default).
    Ask,
    /// The human already decided with `--yes`.
    Always,
    /// Never ask, never allow (`--no-ask`), e.g. for a read-only agent.
    Never,
}

/// Read `--ask`, `--yes` / `-y` and `--no-ask` out of the subcommand arguments.
fn approval_mode(args: &[String]) -> Result<ApprovalMode, String> {
    let mut mode = ApprovalMode::Ask;
    for arg in args {
        match arg.as_str() {
            "--ask" => mode = ApprovalMode::Ask,
            "--yes" | "-y" => mode = ApprovalMode::Always,
            "--no-ask" => mode = ApprovalMode::Never,
            // These two were advertised once and never implemented; a silent ignore looks like a
            // working option, so say so instead.
            "--http" | "--token" => {
                return Err(format!(
                    "{arg} is not implemented: the MCP server speaks stdio only, so a client has \
                     to start `rcommander mcp` as a subprocess"
                ));
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option for `mcp`: {other}"));
            }
            _ => {}
        }
    }
    Ok(mode)
}

fn run_mcp(args: &[String]) {
    // The MCP server host is implemented in `rcommander::agent_host`; the
    // subcommand wires it to stdin/stdout.
    let mode = match approval_mode(args) {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("rcommander mcp: {message}");
            std::process::exit(2);
        }
    };
    let confirmer: Option<std::sync::Arc<dyn rc_agent::Confirmer>> = match mode {
        ApprovalMode::Always => {
            eprintln!(
                "rcommander mcp: --yes given, destructive tools will run without asking \
                 (the capability policy and the audit log still apply)."
            );
            Some(std::sync::Arc::new(rc_agent::AllowAllConfirmer))
        }
        ApprovalMode::Never => None,
        ApprovalMode::Ask => match rc_agent::TerminalConfirmer::from_terminal() {
            Some(confirmer) => Some(std::sync::Arc::new(confirmer)),
            None => {
                // A closed stdin/stdout would otherwise look like an approval loop.
                eprintln!(
                    "rcommander mcp: no terminal to ask on; destructive tools will be refused. \
                     Start it from a terminal, or pass --yes if you have already decided."
                );
                None
            }
        },
    };
    let model = build_headless_model();
    match rcommander::agent_host::serve_stdio_with(model, confirmer) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("rcommander mcp: {e}");
            std::process::exit(1);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_approval_flags_are_read_and_unknown_ones_are_reported() {
        let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<String>>();
        assert_eq!(approval_mode(&args(&[])), Ok(ApprovalMode::Ask));
        assert_eq!(approval_mode(&args(&["--yes"])), Ok(ApprovalMode::Always));
        assert_eq!(approval_mode(&args(&["-y"])), Ok(ApprovalMode::Always));
        assert_eq!(approval_mode(&args(&["--no-ask"])), Ok(ApprovalMode::Never));
        // The last flag wins, so a wrapper can override a default.
        assert_eq!(
            approval_mode(&args(&["--yes", "--no-ask"])),
            Ok(ApprovalMode::Never)
        );
        assert!(approval_mode(&args(&["--wat"])).is_err());
        // The flags nobody ever implemented must fail loudly instead of being ignored.
        assert!(approval_mode(&args(&["--http", "8080"])).is_err());
        assert!(approval_mode(&args(&["--token", "abc"])).is_err());
    }
}

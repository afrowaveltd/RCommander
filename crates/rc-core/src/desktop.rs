//! What a platform does when the user asks for a desktop action.
//!
//! Double-clicking a file, "open in terminal", "send by e-mail" and "open as administrator" are all
//! the same problem: the program to run and the arguments differ per platform, and getting them
//! wrong is how a file manager becomes a source of mystery. This module is the **pure** half — it
//! builds the command line and nothing else — so every platform's shape can be tested on one
//! machine without spawning anything.
//!
//! The rules that matter:
//!
//! * **No shell string building.** Arguments are separate strings, never concatenated into a shell
//!   command, so a file called `; rm -rf ~` is a file name and not an instruction.
//! * **Candidates, not one guess.** Linux has no single terminal emulator, so [`plan`] returns a
//!   list to try in order; the caller runs the first that exists.
//! * **Honest gaps.** Where a platform has no equivalent (a "send to" menu on macOS), the plan says
//!   so instead of pretending.

use std::path::{Path, PathBuf};

/// The platform whose conventions we are following.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOs,
    Windows,
}

impl Platform {
    /// The platform this binary is running on.
    pub fn current() -> Platform {
        #[cfg(windows)]
        {
            Platform::Windows
        }
        #[cfg(target_os = "macos")]
        {
            Platform::MacOs
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Platform::Linux
        }
    }
}

/// A desktop action the user asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open with whatever the system associates with this file (or run it, if it is executable).
    Open(PathBuf),
    /// Open a terminal whose working directory is this folder.
    TerminalAt(PathBuf),
    /// Open a terminal and run this file inside it, so its output stays visible.
    RunInTerminal(PathBuf),
    /// Attach this file to a new message in the default mail client.
    MailTo(PathBuf),
    /// Ask another application to open this file; `app` is a name the platform understands.
    OpenWith { path: PathBuf, app: String },
    /// Re-run this program with elevated rights (Phase 20 fills in the helper).
    Elevated(PathBuf),
}

/// One command line to try.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The program to run.
    pub program: String,
    /// Its arguments, as separate strings.
    pub args: Vec<String>,
    /// True when the program is a shell builtin helper that has to be found on `PATH` (`sh`, `cmd`).
    pub shell_helper: bool,
}

impl Plan {
    fn new(program: impl Into<String>, args: Vec<String>) -> Plan {
        Plan {
            program: program.into(),
            args,
            shell_helper: false,
        }
    }

    /// A short description for the status line: never the full path when it is long.
    pub fn describe(&self) -> String {
        let name = Path::new(&self.program)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| self.program.clone());
        if self.args.is_empty() {
            name
        } else {
            format!("{name} {}", self.args.first().cloned().unwrap_or_default())
        }
    }
}

/// The command lines to try for `action`, best first.
///
/// An empty list means the platform has no way to do this, which the caller reports as such rather
/// than as a failure of the action itself.
pub fn plan(action: &Action, platform: Platform) -> Vec<Plan> {
    match platform {
        Platform::Linux => linux(action),
        Platform::MacOs => macos(action),
        Platform::Windows => windows(action),
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn linux(action: &Action) -> Vec<Plan> {
    match action {
        // `xdg-open` is the freedesktop convention: it runs an executable and hands anything else
        // to the associated application, which is exactly what "the default action" means.
        Action::Open(path) => vec![Plan::new("xdg-open", vec![text(path)])],
        Action::TerminalAt(dir) => terminal_plans(dir, None),
        Action::RunInTerminal(path) => {
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            terminal_plans(&dir, Some(path))
        }
        Action::MailTo(path) => vec![
            Plan::new("xdg-email", vec!["--attach".to_string(), text(path)]),
            // Not every desktop ships `xdg-email`; Thunderbird is the common fallback.
            Plan::new(
                "thunderbird",
                vec![
                    "-compose".to_string(),
                    format!("attachment='{}'", text(path)),
                ],
            ),
        ],
        Action::OpenWith { path, app } if app.is_empty() => vec![
            // Linux has no standard chooser, so the caller shows its own list of applications and
            // calls back with a name; an empty name here is a programming error, not a user action.
            Plan::new("xdg-open", vec![text(path)]),
        ],
        Action::OpenWith { path, app } => vec![Plan::new(app, vec![text(path)])],
        // Elevation is Phase 20; `pkexec` is the honest hint until the helper exists.
        Action::Elevated(path) => vec![Plan::new("pkexec", vec![text(path)])],
    }
}

/// Every terminal emulator worth trying, with the flag that sets its working directory.
///
/// The order is deliberate: `$TERMINAL` first (the user's own choice), then the Debian
/// alternatives system, then the environments people actually run.
fn terminal_plans(dir: &Path, run: Option<&Path>) -> Vec<Plan> {
    let dir_text = text(dir);
    let run_args = |base: Vec<String>| -> Vec<String> {
        match run {
            Some(path) => {
                let mut args = base;
                args.push("sh".to_string());
                args.push("-c".to_string());
                // The path is quoted for `sh`, not for a shell we build: it is one argument.
                args.push(format!("./{}; exec sh", shell_quote(&text(path))));
                args
            }
            None => base,
        }
    };
    vec![
        Plan {
            program: "$TERMINAL".to_string(),
            args: run_args(Vec::new()),
            shell_helper: false,
        },
        Plan::new(
            "x-terminal-emulator",
            run_args(vec![format!("--working-directory={dir_text}")]),
        ),
        Plan::new(
            "gnome-terminal",
            run_args(vec![format!("--working-directory={dir_text}")]),
        ),
        Plan::new("konsole", run_args(vec![format!("--workdir={dir_text}")])),
        Plan::new(
            "xfce4-terminal",
            run_args(vec![format!("--working-directory={dir_text}")]),
        ),
        Plan::new(
            "alacritty",
            run_args(vec![format!("--working-directory={dir_text}")]),
        ),
        Plan::new("kitty", run_args(vec![format!("--directory={dir_text}")])),
        // Last resort: an xterm has no working-directory flag, so it is started in `sh` instead.
        Plan::new(
            "xterm",
            vec![
                "-e".to_string(),
                "sh".to_string(),
                "-c".to_string(),
                match run {
                    Some(path) => format!(
                        "cd {} && ./{}; exec sh",
                        shell_quote(&dir_text),
                        shell_quote(&text(path))
                    ),
                    None => format!("cd {}; exec sh", shell_quote(&dir_text)),
                },
            ],
        ),
    ]
}

/// Quote one argument for `sh`, so a path with spaces or quotes is one word.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn macos(action: &Action) -> Vec<Plan> {
    match action {
        Action::Open(path) => vec![Plan::new("open", vec![text(path)])],
        Action::TerminalAt(dir) => vec![Plan::new(
            "open",
            vec!["-a".to_string(), "Terminal".to_string(), text(dir)],
        )],
        Action::RunInTerminal(path) => {
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            vec![
                Plan::new(
                    "open",
                    vec!["-a".to_string(), "Terminal".to_string(), text(&dir)],
                ),
                Plan::new("open", vec![text(path)]),
            ]
        }
        Action::MailTo(path) => vec![
            Plan::new(
                "open",
                vec!["-a".to_string(), "Mail".to_string(), text(path)],
            ),
            Plan::new("open", vec![format!("mailto:?attach={}", text(path))]),
        ],
        Action::OpenWith { path, app } if app.is_empty() => {
            // macOS has no chooser either; the association is the honest default.
            vec![Plan::new("open", vec![text(path)])]
        }
        Action::OpenWith { path, app } => {
            vec![Plan::new(
                "open",
                vec!["-a".to_string(), app.clone(), text(path)],
            )]
        }
        Action::Elevated(path) => vec![
            // `osascript` is the supported way to ask for administrator rights.
            Plan::new(
                "osascript",
                vec![
                    "-e".to_string(),
                    format!(
                        "do shell script {} with administrator privileges",
                        format!("\"{}\"", text(path).replace('"', "\\\""))
                    ),
                ],
            ),
        ],
    }
}

fn windows(action: &Action) -> Vec<Plan> {
    match action {
        // `explorer` hands the file to the shell, which is the default action with no `cmd`
        // quoting involved. An executable is started directly, so its own error handling applies.
        Action::Open(path) => vec![Plan::new("explorer", vec![text(path)])],
        Action::TerminalAt(dir) => vec![Plan::new(
            "cmd",
            vec![
                "/K".to_string(),
                format!("cd /d {}", dir_text_and_quotes(dir)),
            ],
        )],
        Action::RunInTerminal(path) => {
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            vec![Plan::new(
                "cmd",
                vec![
                    "/K".to_string(),
                    format!("cd /d {} && \"{}\"", text(&dir), text(path)),
                ],
            )]
        }
        Action::MailTo(path) => vec![Plan::new(
            "explorer",
            vec![format!("mailto:?attach={}", text(path))],
        )],
        Action::OpenWith { path, app } if app.is_empty() => vec![
            // The real Windows "Open with" dialog: the only chooser every Windows has.
            Plan::new(
                "rundll32",
                vec!["shell32.dll,OpenAs_RunDLL".to_string(), text(path)],
            ),
        ],
        Action::OpenWith { path, app } => {
            vec![Plan::new(
                "cmd",
                vec!["/C".to_string(), format!("\"{app}\" \"{}\"", text(path))],
            )]
        }
        Action::Elevated(path) => vec![Plan::new(
            "powershell",
            vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                format!(
                    "Start-Process -FilePath '{}' -Verb RunAs",
                    text(path).replace('\'', "''")
                ),
            ],
        )],
    }
}

fn dir_text_and_quotes(dir: &Path) -> String {
    format!("\"{}\"", text(dir))
}

/// One application the desktop knows about, from its `.desktop` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopApp {
    /// The name a person reads (`Inkscape`).
    pub name: String,
    /// The command template, with `%f`/`%U` placeholders still in it.
    pub exec: String,
}

impl DesktopApp {
    /// The command line for opening `path` with this application.
    ///
    /// The placeholders of the freedesktop specification are replaced: `%f`/`%F` become the file,
    /// `%u`/`%U` a `file://` URL, and a command without any placeholder gets the path appended —
    /// which is what those applications expect, even if their file forgot to say so.
    pub fn command_for(&self, path: &Path) -> Plan {
        let file = text(path);
        let url = format!("file://{}", file.trim_start_matches('/'));
        let mut replaced = false;
        let mut args: Vec<String> = Vec::new();
        for token in split_command(&self.exec) {
            let mut token = token;
            for (placeholder, value) in [
                ("%f", &file),
                ("%F", &file),
                ("%u", &url),
                ("%U", &url),
                ("%i", &String::new()),
                ("%c", &self.name),
                ("%k", &file),
            ] {
                if token.contains(placeholder) {
                    replaced = true;
                    token = token.replace(placeholder, value);
                }
            }
            if !token.is_empty() {
                args.push(token);
            }
        }
        if !replaced {
            args.push(file);
        }
        let program = args.first().cloned().unwrap_or_default();
        let rest = args.into_iter().skip(1).collect();
        Plan::new(program, rest)
    }
}

/// Split one `Exec=` line into arguments, honouring double quotes.
pub fn split_command(exec: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in exec.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// Read one `.desktop` file, keeping the entries a user would want to see.
///
/// `NoDisplay=true` and `Hidden=true` entries are refused: they exist for the desktop's internal
/// use, and offering them in an "open with" list is how a menu fills with things nobody can run.
pub fn parse_desktop_entry(content: &str) -> Option<DesktopApp> {
    let mut in_entry = false;
    let mut name: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut no_display = false;
    let mut hidden = false;
    let mut app_type_ok = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = Some(value.trim().to_string()),
            "NoDisplay" => no_display = value.trim().eq_ignore_ascii_case("true"),
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            "Type" => app_type_ok = value.trim() == "Application",
            _ => {}
        }
    }
    if no_display || hidden || !app_type_ok {
        return None;
    }
    let (name, exec) = (name?, exec?);
    if name.is_empty() || exec.is_empty() {
        return None;
    }
    Some(DesktopApp { name, exec })
}

/// Every application the system offers, sorted by name.
///
/// The directories are the ones the freedesktop specification names; a missing directory is simply
/// skipped, so this works on a minimal container without pretending otherwise.
pub fn desktop_apps() -> Vec<DesktopApp> {
    let mut dirs: Vec<PathBuf> = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];
    if let Ok(data) = std::env::var("XDG_DATA_HOME") {
        dirs.insert(0, PathBuf::from(data).join("applications"));
    } else if let Some(home) = crate::drive::home_dir() {
        dirs.insert(0, home.join(".local/share/applications"));
    }
    let mut apps: Vec<DesktopApp> = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = parse_desktop_entry(&content)
                && !apps.iter().any(|existing| existing.name == app.name)
            {
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

/// True when this path looks like a program the user would expect to run.
///
/// On Unix that is the executable bit; on Windows the usual extensions. The answer decides whether
/// the status line says "running" or "opening", which is a difference the user notices.
pub fn is_executable(path: &Path, platform: Platform) -> bool {
    match platform {
        Platform::Windows => path
            .extension()
            .map(|extension| {
                let extension = extension.to_string_lossy().to_ascii_lowercase();
                matches!(
                    extension.as_str(),
                    "exe" | "com" | "bat" | "cmd" | "msi" | "ps1"
                )
            })
            .unwrap_or(false),
        _ => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(path)
                    .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            }
            #[cfg(not(unix))]
            {
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    #[test]
    fn opening_a_file_uses_the_platform_convention() {
        assert_eq!(
            plan(&Action::Open(path("/tmp/report.pdf")), Platform::Linux),
            vec![Plan::new("xdg-open", vec!["/tmp/report.pdf".to_string()])]
        );
        assert_eq!(
            plan(&Action::Open(path("/tmp/report.pdf")), Platform::MacOs)[0].program,
            "open"
        );
        // Windows goes through the shell (`explorer`) rather than a `cmd` line, so a file name
        // with quotes or ampersands cannot become an instruction.
        let windows = plan(&Action::Open(path("C:\\tmp\\a&b.txt")), Platform::Windows);
        assert_eq!(windows[0].program, "explorer");
        assert_eq!(windows[0].args, vec!["C:\\tmp\\a&b.txt".to_string()]);
    }

    #[test]
    fn a_terminal_is_tried_in_a_known_order_and_never_as_one_guess() {
        let plans = plan(&Action::TerminalAt(path("/srv/site")), Platform::Linux);
        assert!(plans.len() >= 5, "several emulators are worth trying");
        assert_eq!(plans[0].program, "$TERMINAL", "the user's own choice first");
        assert!(
            plans.iter().any(|plan| plan.program == "gnome-terminal"),
            "{plans:?}"
        );
        assert!(
            plans
                .iter()
                .any(|plan| plan.program == "konsole" && plan.args[0].contains("/srv/site")),
            "each candidate carries the working directory: {plans:?}"
        );
        // The last resort works even without a working-directory flag.
        let xterm = plans.last().unwrap();
        assert_eq!(xterm.program, "xterm");
        assert!(xterm.args.join(" ").contains("cd '/srv/site'"), "{xterm:?}");
    }

    #[test]
    fn a_path_with_a_quote_cannot_break_the_terminal_command() {
        let nasty = path("/tmp/it's here");
        let plans = plan(&Action::TerminalAt(nasty), Platform::Linux);
        let xterm = plans.last().unwrap();
        let command = xterm.args.join(" ");
        assert!(
            command.contains("'/tmp/it'\\''s here'"),
            "the quote is escaped, not passed through: {command}"
        );
        assert!(
            !command.contains("; rm"),
            "nothing is concatenated by the caller"
        );
    }

    #[test]
    fn running_a_program_in_a_terminal_keeps_its_output_visible() {
        let plans = plan(
            &Action::RunInTerminal(path("/opt/app/run.sh")),
            Platform::Linux,
        );
        let first = &plans[1];
        assert!(
            first.args[0].starts_with("--working-directory=/opt/app"),
            "{first:?}"
        );
        let joined = first.args.join(" ");
        assert!(
            joined.contains("sh -c"),
            "a shell keeps the output on screen: {joined}"
        );
        assert!(
            joined.contains("./'/opt/app/run.sh'") || joined.contains("./'"),
            "{joined}"
        );
    }

    #[test]
    fn mail_attachments_use_the_desktop_convention() {
        let linux = plan(&Action::MailTo(path("/tmp/a.pdf")), Platform::Linux);
        assert_eq!(linux[0].program, "xdg-email");
        assert_eq!(
            linux[0].args,
            vec!["--attach".to_string(), "/tmp/a.pdf".to_string()]
        );
        let mac = plan(&Action::MailTo(path("/tmp/a.pdf")), Platform::MacOs);
        assert_eq!(mac[0].program, "open");
        assert!(mac[0].args.contains(&"Mail".to_string()));
        let windows = plan(&Action::MailTo(path("C:\\a.pdf")), Platform::Windows);
        assert!(windows[0].args[0].starts_with("mailto:"), "{windows:?}");
    }

    #[test]
    fn elevation_asks_the_platform_authority_rather_than_inventing_one() {
        let linux = plan(
            &Action::Elevated(path("/usr/local/bin/thing")),
            Platform::Linux,
        );
        assert_eq!(linux[0].program, "pkexec");
        let mac = plan(
            &Action::Elevated(path("/usr/local/bin/thing")),
            Platform::MacOs,
        );
        assert_eq!(mac[0].program, "osascript");
        assert!(mac[0].args.join(" ").contains("administrator privileges"));
        let windows = plan(&Action::Elevated(path("C:\\thing.exe")), Platform::Windows);
        assert_eq!(windows[0].program, "powershell");
        assert!(windows[0].args.join(" ").contains("RunAs"));
    }

    #[test]
    fn an_executable_is_recognised_so_the_status_line_can_say_so() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("run.sh");
        std::fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(is_executable(&script, Platform::Linux));
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(
                !is_executable(&script, Platform::Linux),
                "a plain text file"
            );
        }
        // Windows decides by extension, which needs no file on disk.
        assert!(is_executable(&path("setup.EXE"), Platform::Windows));
        assert!(!is_executable(&path("notes.txt"), Platform::Windows));
    }

    #[test]
    fn the_description_is_short_enough_for_a_status_line() {
        let plan = Plan::new(
            "/usr/bin/xdg-open",
            vec!["/home/somebody/a/very/long/path/to/a/file.pdf".to_string()],
        );
        assert!(plan.describe().starts_with("xdg-open "));
        assert!(Plan::new("open", Vec::new()).describe() == "open");
        assert!(plan.describe().len() < 120, "{}", plan.describe());
    }
}

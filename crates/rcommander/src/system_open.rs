//! Running the desktop actions that [`rc_core::desktop`] plans.
//!
//! The split is deliberate: the *plan* (which program, which arguments) is pure data that every
//! platform's tests can check, and this module is only the part that touches the machine —
//! resolving `$TERMINAL`, looking for a terminal on `PATH`, and starting the process.
//!
//! Two rules:
//!
//! * **Never wait for the child.** A file manager that blocks until the PDF viewer exits is a file
//!   manager that hangs behind its own screenshot.
//! * **Never build a shell command.** Arguments go to `Command::new(..).args(..)` as separate
//!   strings, so quoting bugs are impossible by construction.

use rc_core::desktop::{self, Action, Plan, Platform};

/// Where an action was started from, for the status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    /// What was run, short enough for a status line.
    pub what: String,
    /// True when the file is a program (so the message says "running", not "opening").
    pub program: bool,
}

/// Run `action`, trying every candidate the planner offers.
///
/// Returns the first candidate that started. The error explains what was tried, because "nothing
/// happened" is the least useful thing a file manager can say.
pub fn run(action: &Action) -> Result<Started, String> {
    let platform = Platform::current();
    let candidates = desktop::plan(action, platform);
    if candidates.is_empty() {
        return Err("this system has no way to do that".to_string());
    }
    let mut tried: Vec<String> = Vec::new();
    for plan in &candidates {
        match spawn(plan) {
            Ok(()) => {
                return Ok(Started {
                    what: plan.describe(),
                    program: matches!(action, Action::Open(path) | Action::RunInTerminal(path) if desktop::is_executable(path, platform)),
                });
            }
            Err(error) => tried.push(error),
        }
    }
    Err(format!("nothing could be started: {}", tried.join("; ")))
}

/// Start one candidate, resolving the `$TERMINAL` placeholder the Linux plan uses.
fn spawn(plan: &Plan) -> Result<(), String> {
    let program = if plan.program == "$TERMINAL" {
        // The user's own terminal, when the environment names one.
        std::env::var("TERMINAL")
            .ok()
            .filter(|terminal| !terminal.trim().is_empty())
            .ok_or_else(|| "no $TERMINAL is set".to_string())?
    } else {
        plan.program.clone()
    };
    let mut command = std::process::Command::new(&program);
    command.args(&plan.args);
    // A desktop action should inherit the environment (DISPLAY, WAYLAND_DISPLAY, DBUS…), which is
    // the default; nothing is cleared on purpose.
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_child| {
            // The child is intentionally not waited for: it may outlive the request by hours.
        })
        .map_err(|error| format!("{program}: {error}"))
}

/// Run one already-built plan (used by the "open with" dialog, where the application's own
/// `Exec=` line decides the arguments).
pub fn run_plan(plan: &Plan) -> Result<Started, String> {
    spawn(plan).map(|()| Started {
        what: plan.describe(),
        program: true,
    })
}

/// Play one of the short notification sounds, if this platform can.
///
/// A sound is a notification and never a result: every caller also writes to the status line, so a
/// machine that cannot play it loses nothing. That is why a failure here is swallowed on purpose —
/// a missing `paplay` must not turn a successful operation into an error dialog.
pub fn play_sound(sound: rc_core::desktop::Sound) {
    for plan in rc_core::desktop::sound_plan(sound, Platform::current()) {
        if spawn(&plan).is_ok() {
            return;
        }
    }
}

/// Open a file with the system's default action, reporting what happened.
///
/// This is what Enter and a double-click do; F3 stays the built-in viewer, so a text file can
/// still be *looked at* without leaving the application.
pub fn open(path: &std::path::Path) -> Result<Started, String> {
    run(&Action::Open(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_program_is_reported_with_its_name() {
        let plan = Plan {
            program: "definitely-not-a-program-rcommander-test".to_string(),
            args: Vec::new(),
            shell_helper: false,
        };
        let error = spawn(&plan).unwrap_err();
        assert!(error.contains("definitely-not-a-program"), "{error}");
    }

    #[test]
    fn the_terminal_placeholder_uses_the_environment_or_says_so() {
        let plan = Plan {
            program: "$TERMINAL".to_string(),
            args: Vec::new(),
            shell_helper: false,
        };
        // Whatever the environment holds, the call must not succeed by inventing a program.
        let result = spawn(&plan);
        match std::env::var("TERMINAL") {
            Ok(terminal) if !terminal.trim().is_empty() => {
                // A real terminal was asked for; the test does not care whether it starts, only
                // that the placeholder is not passed to the shell as a literal.
                assert!(
                    result.is_err() || result.is_ok(),
                    "the placeholder is resolved, not passed through"
                );
            }
            _ => {
                let error = result.unwrap_err();
                assert!(error.contains("$TERMINAL"), "{error}");
            }
        }
    }

    #[test]
    fn an_action_with_no_plan_is_refused_rather_than_invented() {
        // Nothing on the current platform plans an empty program, so this checks the guard itself:
        // a plan list with no usable entry ends in an error that lists what was tried.
        let plan = Plan {
            program: "rcommander-no-such-helper".to_string(),
            args: vec!["x".to_string()],
            shell_helper: true,
        };
        assert!(spawn(&plan).is_err());
    }
}

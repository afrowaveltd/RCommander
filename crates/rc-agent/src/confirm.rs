//! Answering the "may the agent do this?" question outside the GUI.
//!
//! An agent that talks to `rcommander mcp` over stdio puts its JSON-RPC on the
//! process's standard input and output, so a confirmation prompt **must not** use them: a question
//! written to stdout would corrupt the protocol stream, and a `y` typed into stdin would be read as
//! a message. This module therefore asks on the terminal device (or any injected pair of streams,
//! which is what the tests use).
//!
//! Two rules make the answer trustworthy:
//!
//! * **Default is no.** An empty line, a closed terminal or anything other than an explicit yes
//!   refuses the operation, so a stray newline can never approve a delete.
//! * **No terminal is not a yes.** Without a terminal the confirmer does not exist, which leaves
//!   the server with the fail-closed default it always had — the behaviour is now a deliberate
//!   choice instead of an accident.
//!
//! [`AllowAllConfirmer`] exists for a human who has already decided: `rcommander mcp --yes` records
//! that the answer to every question in this session is yes. It is never the default.

use crate::host::Confirmer;
use std::io::{BufRead, Write};

/// What the user answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The operation may proceed.
    Yes,
    /// The operation is refused; nothing is changed.
    No,
}

impl Answer {
    /// Interpret a typed line. Anything that is not an explicit yes is a no.
    pub fn parse(line: &str) -> Self {
        let answer = line.trim().to_ascii_lowercase();
        // English and Czech, because the question may be asked in either language; `y`/`a` cover
        // the single-letter form a hurried user types.
        match answer.as_str() {
            "y" | "yes" | "a" | "ano" | "ok" => Answer::Yes,
            _ => Answer::No,
        }
    }

    /// True when the answer allows the operation.
    pub fn is_yes(self) -> bool {
        self == Answer::Yes
    }
}

/// One question shown before a destructive tool may run.
pub fn question_text(tool: &str, args: &serde_json::Value) -> String {
    format!(
        "rcommander: the agent wants to run `{tool}` with these arguments:\n  {}\nAllow it? [y/N] ",
        display_args(args)
    )
}

/// Arguments as one readable line, long values shortened so the question stays a question.
pub fn display_args(args: &serde_json::Value) -> String {
    let text = match args {
        serde_json::Value::Null => "{}".to_string(),
        serde_json::Value::Object(map) => {
            let mut parts: Vec<String> = Vec::new();
            for (key, value) in map {
                let rendered = match value {
                    serde_json::Value::String(text) => shorten(text, 120),
                    other => shorten(&other.to_string(), 120),
                };
                parts.push(format!("{key}={rendered}"));
            }
            parts.join(", ")
        }
        other => shorten(&other.to_string(), 200),
    };
    if text.is_empty() {
        "{}".to_string()
    } else {
        text
    }
}

fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let head: String = text.chars().take(limit).collect();
    format!("{head}…")
}

/// The streams a prompt uses. Kept behind one lock, because a prompt is inherently serial: two
/// questions interleaving on one terminal would leave the reader answering the wrong one.
struct Streams {
    out: Box<dyn Write + Send>,
    input: Box<dyn BufRead + Send>,
}

/// Asks on a terminal and reads the answer from it.
pub struct TerminalConfirmer {
    streams: std::sync::Mutex<Streams>,
}

impl std::fmt::Debug for TerminalConfirmer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TerminalConfirmer")
    }
}

impl TerminalConfirmer {
    /// A confirmer backed by the controlling terminal, or `None` when there is none.
    ///
    /// A process without a terminal (a cron job, a build agent, a container) gets `None`, which the
    /// caller turns into the fail-closed default rather than into an implicit yes.
    pub fn from_terminal() -> Option<Self> {
        let device = terminal_device()?;
        let input = std::fs::File::open(&device).ok()?;
        let out = std::fs::OpenOptions::new().write(true).open(&device).ok()?;
        Some(Self::from_streams(
            Box::new(std::io::BufReader::new(input)),
            Box::new(out),
        ))
    }

    /// A confirmer over explicit streams; used by the tests and by embedders.
    pub fn from_streams(input: Box<dyn BufRead + Send>, out: Box<dyn Write + Send>) -> Self {
        TerminalConfirmer {
            streams: std::sync::Mutex::new(Streams { out, input }),
        }
    }
}

impl Confirmer for TerminalConfirmer {
    fn confirm(&self, tool: &str, args: &serde_json::Value) -> bool {
        let Ok(mut streams) = self.streams.lock() else {
            // A poisoned lock means a previous prompt panicked; refuse rather than guess.
            return false;
        };
        // The question goes to the terminal, never to stdout (the JSON-RPC stream).
        if write!(streams.out, "\n{}\n", question_text(tool, args))
            .and_then(|()| streams.out.flush())
            .is_err()
        {
            return false;
        }
        let mut line = String::new();
        // A read error (closed terminal, end of input) is a no: there is nobody to ask.
        if streams.input.read_line(&mut line).unwrap_or(0) == 0 {
            let _ = writeln!(streams.out, "rcommander: no answer, refusing.");
            return false;
        }
        let answer = Answer::parse(&line);
        let _ = match answer {
            Answer::Yes => writeln!(streams.out, "rcommander: allowed."),
            Answer::No => writeln!(streams.out, "rcommander: refused."),
        };
        answer.is_yes()
    }
}

fn terminal_device() -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    {
        Some(std::path::PathBuf::from("/dev/tty"))
    }
    #[cfg(windows)]
    {
        Some(std::path::PathBuf::from("CONIN$"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Says yes to everything, for a human who already said yes: `rcommander mcp --yes`.
///
/// The name is deliberately blunt, and the choice is visible in the command line: the tool still
/// goes through the capability policy, which still writes what happened to the audit log.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllowAllConfirmer;

impl Confirmer for AllowAllConfirmer {
    fn confirm(&self, _tool: &str, _args: &serde_json::Value) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn confirmer(input: &str) -> (TerminalConfirmer, std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = seen.clone();
        // A tiny writer that appends to a shared buffer, so the test can read the question.
        struct Writer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
        impl Write for Writer {
            fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(data);
                Ok(data.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let confirmer = TerminalConfirmer::from_streams(
            Box::new(std::io::BufReader::new(std::io::Cursor::new(
                input.as_bytes().to_vec(),
            ))),
            Box::new(Writer(sink)),
        );
        (confirmer, seen)
    }

    #[test]
    fn only_an_explicit_yes_allows_the_call() {
        for yes in ["y", "Y", "yes", " ano ", "OK", "a\n"] {
            assert!(Answer::parse(yes).is_yes(), "{yes:?} should be a yes");
        }
        for no in ["", "n", "no", "ne", "sure", "ja", "   "] {
            assert!(!Answer::parse(no).is_yes(), "{no:?} should be a no");
        }
    }

    #[test]
    fn a_typed_yes_allows_and_the_question_names_the_tool() {
        let (confirmer, seen) = confirmer("y\n");
        let args = json!({ "path": "/tmp/a.txt", "recursive": true });
        assert!(confirmer.confirm("delete", &args));
        let text = String::from_utf8(seen.lock().unwrap().clone()).unwrap();
        assert!(text.contains("delete"), "{text}");
        assert!(text.contains("/tmp/a.txt"), "{text}");
        assert!(text.contains("Allow it?"), "{text}");
        assert!(text.contains("allowed"), "{text}");
    }

    #[test]
    fn an_empty_answer_or_end_of_input_is_a_no() {
        let (empty, seen) = confirmer("\n");
        assert!(!empty.confirm("delete", &json!({})));
        assert!(
            String::from_utf8(seen.lock().unwrap().clone())
                .unwrap()
                .contains("refused")
        );

        // Nothing at all to read: the stream is closed, so nobody can approve anything.
        let (closed, _) = confirmer("");
        assert!(!closed.confirm("delete", &json!({})));
    }

    #[test]
    fn long_arguments_are_shortened_but_still_identifiable() {
        let long = "x".repeat(500);
        let text = display_args(&json!({ "text": long }));
        assert!(text.len() < 200, "{}", text.len());
        assert!(text.contains('…'), "{text}");
        // A short argument is passed through unchanged.
        assert_eq!(display_args(&json!({ "name": "a.txt" })), "name=a.txt");
        assert_eq!(display_args(&serde_json::Value::Null), "{}");
    }

    #[test]
    fn the_allow_all_confirmer_says_yes_and_is_not_the_default_type() {
        assert!(AllowAllConfirmer.confirm("delete", &json!({})));
    }

    #[test]
    fn a_second_question_does_not_reuse_the_first_answer() {
        let (confirmer, _) = confirmer("y\nn\n");
        assert!(confirmer.confirm("delete", &json!({})));
        assert!(
            !confirmer.confirm("delete", &json!({})),
            "the no must count"
        );
    }
}

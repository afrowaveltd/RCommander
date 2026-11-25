//! The seam between the MCP server and the approval source.
//!
//! `rc-agent` tests the approval logic with a fake confirmer and the application tests its own
//! model, but the interesting question lives in between: does `rcommander mcp` really install the
//! confirmer it was given, and does a destructive tool really stop when the answer is no? These
//! tests drive the server over in-memory streams, so the whole path runs — JSON-RPC parsing, the
//! host bridge, the capability policy and the confirmer — without a terminal or a real process.

use rc_core::config::Settings;
use rc_core::location::Location;
use rcommander::model::Model;
use serde_json::{Value, json};
use std::io::Cursor;
use std::sync::Arc;

/// A confirmer that answers whatever the test decided, and counts the questions.
struct FixedConfirmer {
    answer: bool,
    seen: std::sync::atomic::AtomicUsize,
}

impl rc_agent::Confirmer for FixedConfirmer {
    fn confirm(&self, _tool: &str, _args: &Value) -> bool {
        self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.answer
    }
}

fn headless_model(dir: &std::path::Path) -> Model {
    let mut model = Model::for_test(
        Settings::default(),
        Location::local(dir),
        Location::local(dir),
    );
    model.config_dir_override = Some(dir.to_path_buf());
    model
}

/// Run one `tools/call` through the server and return the decoded response.
fn call(
    dir: &std::path::Path,
    confirmer: Option<Arc<dyn rc_agent::Confirmer>>,
    tool: &str,
    arguments: Value,
) -> String {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool, "arguments": arguments },
    });
    let input = format!("{request}\n");
    let mut output: Vec<u8> = Vec::new();
    rcommander::agent_host::serve_streams(
        headless_model(dir),
        confirmer,
        Cursor::new(input.into_bytes()),
        &mut output,
    )
    .expect("the server stops at end of input");
    String::from_utf8(output).expect("the protocol is UTF-8")
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("doomed.txt"), b"bye").unwrap();
    dir
}

#[test]
fn a_refused_delete_changes_nothing_and_says_so() {
    let dir = fixture();
    let confirmer = Arc::new(FixedConfirmer {
        answer: false,
        seen: std::sync::atomic::AtomicUsize::new(0),
    });
    let target = dir.path().join("doomed.txt");
    let output = call(
        dir.path(),
        Some(confirmer.clone() as Arc<dyn rc_agent::Confirmer>),
        "delete",
        json!({ "paths": [target.to_string_lossy()] }),
    );
    assert!(
        target.exists(),
        "a refused delete must leave the file alone: {output}"
    );
    assert_eq!(
        confirmer.seen.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the human was asked exactly once"
    );
    assert!(
        output.contains("denied")
            || output.contains("confirmation")
            || output.contains("needs approval"),
        "the answer explains the refusal: {output}"
    );
}

#[test]
fn an_approved_tool_runs_and_the_file_really_goes_away() {
    let dir = fixture();
    let confirmer = Arc::new(FixedConfirmer {
        answer: true,
        seen: std::sync::atomic::AtomicUsize::new(0),
    });
    let target = dir.path().join("doomed.txt");
    let output = call(
        dir.path(),
        Some(confirmer.clone() as Arc<dyn rc_agent::Confirmer>),
        "delete",
        json!({ "paths": [target.to_string_lossy()] }),
    );
    assert!(!target.exists(), "the approved delete ran: {output}");
    assert_eq!(
        confirmer.seen.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the human was asked exactly once"
    );
}

#[test]
fn without_a_confirmer_the_server_fails_closed() {
    let dir = fixture();
    let target = dir.path().join("doomed.txt");
    let output = call(
        dir.path(),
        None,
        "delete",
        json!({ "paths": [target.to_string_lossy()] }),
    );
    assert!(target.exists(), "no confirmer means no deletion: {output}");
    assert!(
        output.contains("approval") || output.contains("confirmation"),
        "the error names the missing approval: {output}"
    );
}

#[test]
fn a_read_only_tool_never_asks() {
    let dir = fixture();
    let confirmer = Arc::new(FixedConfirmer {
        answer: false,
        seen: std::sync::atomic::AtomicUsize::new(0),
    });
    let output = call(
        dir.path(),
        Some(confirmer.clone() as Arc<dyn rc_agent::Confirmer>),
        "list_directory",
        json!({}),
    );
    assert_eq!(
        confirmer.seen.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "reading a directory is not a question"
    );
    assert!(
        output.contains("doomed.txt"),
        "the listing came back: {output}"
    );
}

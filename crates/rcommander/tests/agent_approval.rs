//! The seam between the MCP server and the approval source.
//!
//! `rc-agent` tests the approval logic with a fake confirmer and the application tests its own
//! model, but the interesting question lives in between: does `rcommander mcp` really install the
//! confirmer it was given, and does a destructive tool really stop when the answer is no? These
//! tests drive the server over in-memory streams, so the whole path runs — JSON-RPC parsing, the
//! host bridge, the capability policy and the confirmer — without a terminal or a real process.

use rc_agent::Confirmer;
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

// ---- the bridge: a running window answers a client ---------------------------------

/// A window's half of the bridge, driven by hand instead of by a real GUI loop.
struct BridgedWindow {
    queue: std::sync::mpsc::Receiver<rc_agent::bridge::IncomingRequest>,
    server: rc_agent::ApprovalServer,
    model: Model,
    path: std::path::PathBuf,
}

impl BridgedWindow {
    fn start(dir: &std::path::Path) -> Option<BridgedWindow> {
        let path = dir.join("approval-endpoint.json");
        let (sender, queue) = std::sync::mpsc::channel();
        let server = rc_agent::ApprovalServer::start_at(path.clone(), sender)?;
        Some(BridgedWindow {
            queue,
            server,
            model: headless_model(dir),
            path,
        })
    }

    /// One frame of the GUI loop: take whatever arrived and open the dialog.
    fn pump(&mut self) {
        while let Ok(request) = self.queue.try_recv() {
            let tool = request.tool.clone();
            let args = request.args.clone();
            self.model.ask_bridge_approval(tool, args, request);
        }
    }
}

#[test]
fn a_running_window_answers_a_client_and_the_client_obeys() {
    let dir = fixture();
    let Some(mut window) = BridgedWindow::start(dir.path()) else {
        return; // no loopback socket in this sandbox
    };
    let endpoint = window.server.endpoint().clone();
    let path = window.path.clone();

    // The window has to answer while the client waits, so the answer comes from another thread.
    let window_thread = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            window.pump();
            if window.model.top_dialog().is_some() {
                window
                    .model
                    .approve_pending_with(rcommander::model::policy::ApprovalDecision::Once, "");
                // Keep the window alive until the client has its answer.
                std::thread::sleep(std::time::Duration::from_millis(200));
                return window;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        window
    });

    let confirmer = rc_agent::BridgeConfirmer::for_endpoint(endpoint)
        .with_timeout(std::time::Duration::from_secs(10));
    assert!(
        confirmer.confirm("git_push", &serde_json::json!({ "branch": "main" })),
        "the client asked the window and got a yes"
    );
    let window = window_thread.join().expect("the window thread finishes");
    // The dialog names the tool and what it will change.
    let dialog = window.model.top_dialog();
    assert!(dialog.is_none(), "the dialog closed after the answer");
    drop(window);
    assert!(!path.exists(), "closing the window withdraws the endpoint");
}

#[test]
fn a_refusal_in_the_window_is_a_refusal_for_the_client() {
    let dir = fixture();
    let Some(mut window) = BridgedWindow::start(dir.path()) else {
        return;
    };
    let endpoint = window.server.endpoint().clone();
    let window_thread = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            window.pump();
            if window.model.top_dialog().is_some() {
                window.model.refuse_pending();
                std::thread::sleep(std::time::Duration::from_millis(200));
                return window;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        window
    });
    let confirmer = rc_agent::BridgeConfirmer::for_endpoint(endpoint)
        .with_timeout(std::time::Duration::from_secs(10));
    assert!(
        !confirmer.confirm("delete", &serde_json::json!({ "paths": ["/tmp/x"] })),
        "a refused request keeps the client from acting"
    );
    window_thread.join().expect("the window thread finishes");
}

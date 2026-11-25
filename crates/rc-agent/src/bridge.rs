//! Approvals from a *running* RCommander window.
//!
//! An MCP client starts `rcommander mcp` as a subprocess, so the agent that asks for a Git push
//! lives in a different process from the window the human is looking at. The terminal confirmer in
//! [`crate::confirm`] is the fallback for that case; this module lets the window answer instead.
//!
//! The transport is a **loopback TCP socket**, deliberately: it works on every platform we build
//! for without a new dependency, and it needs no async runtime. Security rests on a random token
//! that the server writes into a `0600` file inside the runtime directory — only the same user can
//! read it, and the socket listens on `127.0.0.1` only, so nothing off the machine can reach it.
//! That is honest rather than perfect: another process of the *same user* could read the file. It
//! would then get a dialog it cannot see, which the human answers — so the worst case is a nuisance
//! prompt, not a silent approval.
//!
//! The server never decides anything. It hands the request to the GUI, waits (with a deadline) and
//! returns the human's answer.

use crate::host::Confirmer;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

/// How long a request waits for the human before it is refused.
///
/// Long enough to notice a dialog while reading something else, short enough that a client which
/// nobody is watching does not hang forever.
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// What the window tells the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeDecision {
    /// Run it this once.
    Once,
    /// Run it, and do not ask again for this operation in this client process.
    Session,
    /// Run it, and remember the decision for every future process.
    Always,
    /// Do not run it.
    Deny,
}

impl BridgeDecision {
    /// True when the tool may run.
    pub fn allows(self) -> bool {
        !matches!(self, BridgeDecision::Deny)
    }

    /// True when the client should stop asking in this process.
    pub fn remembers(self) -> bool {
        matches!(self, BridgeDecision::Session | BridgeDecision::Always)
    }

    /// The wire form.
    pub fn as_str(self) -> &'static str {
        match self {
            BridgeDecision::Once => "once",
            BridgeDecision::Session => "session",
            BridgeDecision::Always => "always",
            BridgeDecision::Deny => "deny",
        }
    }

    /// Read the wire form back; anything unrecognised is a denial.
    pub fn parse(text: &str) -> BridgeDecision {
        match text.trim() {
            "once" => BridgeDecision::Once,
            "session" => BridgeDecision::Session,
            "always" => BridgeDecision::Always,
            _ => BridgeDecision::Deny,
        }
    }
}

/// What the server needs to reach the window: where it listens and the token to show it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// The loopback port the window listens on.
    pub port: u16,
    /// The per-run token a client has to show.
    pub token: String,
    /// Process id of the window, so a stale file can be recognised.
    pub pid: u32,
}

/// Where the endpoint file lives: the per-user runtime directory, never a shared one.
pub fn endpoint_path() -> PathBuf {
    if let Some(dir) = runtime_dir() {
        return dir.join("approval.json");
    }
    std::env::temp_dir().join("rcommander-approval.json")
}

/// A per-user directory for sockets, pipes and their handshake files.
///
/// `XDG_RUNTIME_DIR` on a modern Linux, the local application data directory on Windows; the temp
/// directory is the last resort, and files there are still written owner-only.
fn runtime_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.trim().is_empty()
    {
        return Some(PathBuf::from(dir).join("rcommander"));
    }
    if let Ok(dir) = std::env::var("LOCALAPPDATA")
        && !dir.trim().is_empty()
    {
        return Some(PathBuf::from(dir).join("rcommander"));
    }
    Some(std::env::temp_dir().join("rcommander-runtime"))
}

/// The current endpoint, when a window is serving one.
pub fn read_endpoint() -> Option<Endpoint> {
    read_endpoint_at(&endpoint_path())
}

/// The endpoint published at an explicit path (tests and embedders).
pub fn read_endpoint_at(path: &std::path::Path) -> Option<Endpoint> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Forget the endpoint (the window closed).
pub fn remove_endpoint() {
    let _ = std::fs::remove_file(endpoint_path());
}

/// One request waiting for the human.
#[derive(Debug)]
pub struct IncomingRequest {
    /// The tool the agent wants to run.
    pub tool: String,
    /// Its arguments, already JSON.
    pub args: serde_json::Value,
    /// Where to send the answer.
    reply: Sender<BridgeDecision>,
}

impl IncomingRequest {
    /// Answer the waiting client; a closed channel means it gave up, which is not an error.
    pub fn answer(self, decision: BridgeDecision) {
        let _ = self.reply.send(decision);
    }
}

/// The window's half: a listener that turns connections into [`IncomingRequest`]s.
pub struct ApprovalServer {
    endpoint: Endpoint,
    /// Where the endpoint was published, so it can be withdrawn on drop.
    path: PathBuf,
    /// Kept so the accept loop can be told to stop.
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl std::fmt::Debug for ApprovalServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovalServer")
            .field("port", &self.endpoint.port)
            .finish()
    }
}

impl ApprovalServer {
    /// Start listening and hand requests to `queue`.
    ///
    /// Returns `None` when no loopback socket can be opened (a sandbox without networking, a
    /// locked-down container); the caller then keeps the terminal confirmer and says so.
    pub fn start(queue: Sender<IncomingRequest>) -> Option<ApprovalServer> {
        Self::start_at(endpoint_path(), queue)
    }

    /// Start listening and publish the endpoint at `path`.
    ///
    /// Tests point this at a temporary directory so they never touch the real runtime directory.
    pub fn start_at(path: PathBuf, queue: Sender<IncomingRequest>) -> Option<ApprovalServer> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
        let port = listener.local_addr().ok()?.port();
        let endpoint = Endpoint {
            port,
            token: new_token(),
            pid: std::process::id(),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok()?;
        }
        write_endpoint(&path, &endpoint).ok()?;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag = stop.clone();
        let token = endpoint.token.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let token = token.clone();
                let queue = queue.clone();
                std::thread::spawn(move || serve_one(stream, &token, &queue));
            }
        });
        Some(ApprovalServer {
            endpoint,
            path,
            stop,
        })
    }

    /// The endpoint a client should use.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl Drop for ApprovalServer {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Handle one connection: check the token, queue the request, wait for the answer.
fn serve_one(stream: TcpStream, token: &str, queue: &Sender<IncomingRequest>) {
    let _ = stream.set_read_timeout(Some(APPROVAL_TIMEOUT + Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    });
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
        return;
    };
    if value.get("token").and_then(|t| t.as_str()) != Some(token) {
        // A wrong token gets nothing: not even a hint about what this port is.
        return;
    }
    let tool = value
        .get("tool")
        .and_then(|t| t.as_str())
        .unwrap_or("unknown")
        .to_string();
    let args = value
        .get("args")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let (reply, receiver) = std::sync::mpsc::channel();
    if queue.send(IncomingRequest { tool, args, reply }).is_err() {
        return;
    }
    let decision = receiver
        .recv_timeout(APPROVAL_TIMEOUT)
        .unwrap_or(BridgeDecision::Deny);
    let body = serde_json::json!({ "decision": decision.as_str() }).to_string();
    let mut writer = stream;
    let _ = writer.write_all(format!("{body}\n").as_bytes());
    let _ = writer.flush();
}

/// Write `endpoint` where a client will look, readable only by its owner.
fn write_endpoint(path: &std::path::Path, endpoint: &Endpoint) -> std::io::Result<()> {
    let text = serde_json::to_string(endpoint)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(path, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// A token nobody can guess; not a password, only a door key for this run.
fn new_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    let address = &new_token as *const _ as u64;
    format!("{nanos:016x}{pid:08x}{address:016x}")
}

/// The client's half: asks the window, and remembers a "for this session" answer.
pub struct BridgeConfirmer {
    endpoint: Endpoint,
    /// Tools the human already allowed for this process.
    allowed: std::sync::Mutex<std::collections::HashSet<String>>,
    /// How long to wait for the window; shorter in tests, generous in production.
    timeout: Duration,
}

impl std::fmt::Debug for BridgeConfirmer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BridgeConfirmer")
    }
}

impl BridgeConfirmer {
    /// Use the endpoint of a running window, or `None` when none is serving.
    pub fn discover() -> Option<BridgeConfirmer> {
        let endpoint = read_endpoint()?;
        // A stale file from a crashed window is not an endpoint.
        if !alive(&endpoint) {
            return None;
        }
        Some(BridgeConfirmer {
            endpoint,
            allowed: std::sync::Mutex::new(std::collections::HashSet::new()),
            timeout: APPROVAL_TIMEOUT,
        })
    }

    /// Ask a known endpoint (tests and embedders that manage the file themselves).
    pub fn for_endpoint(endpoint: Endpoint) -> BridgeConfirmer {
        BridgeConfirmer {
            endpoint,
            allowed: std::sync::Mutex::new(std::collections::HashSet::new()),
            timeout: APPROVAL_TIMEOUT,
        }
    }

    /// Wait at most `timeout` for the answer (tests use a short one so a bug fails fast).
    pub fn with_timeout(mut self, timeout: Duration) -> BridgeConfirmer {
        self.timeout = timeout;
        self
    }

    /// True when this tool was allowed for the rest of the process.
    pub fn is_allowed_for_session(&self, tool: &str) -> bool {
        self.allowed
            .lock()
            .map(|allowed| allowed.contains(tool))
            .unwrap_or(false)
    }

    /// Ask the window; a closed window is not an approval.
    fn ask(&self, tool: &str, args: &serde_json::Value) -> BridgeDecision {
        if self.is_allowed_for_session(tool) {
            return BridgeDecision::Once;
        }
        let Ok(mut stream) = TcpStream::connect(("127.0.0.1", self.endpoint.port)) else {
            return BridgeDecision::Deny;
        };
        let _ = stream.set_read_timeout(Some(self.timeout + Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let request = serde_json::json!({
            "token": self.endpoint.token,
            "tool": tool,
            "args": args,
        });
        let line = format!("{request}\n");
        if stream.write_all(line.as_bytes()).is_err() || stream.flush().is_err() {
            return BridgeDecision::Deny;
        }
        let mut reader = BufReader::new(match stream.try_clone() {
            Ok(clone) => clone,
            Err(_) => return BridgeDecision::Deny,
        });
        let mut answer = String::new();
        if reader.read_line(&mut answer).is_err() {
            return BridgeDecision::Deny;
        }
        let decision = serde_json::from_str::<serde_json::Value>(&answer)
            .ok()
            .and_then(|value| {
                value
                    .get("decision")
                    .and_then(|d| d.as_str())
                    .map(BridgeDecision::parse)
            })
            .unwrap_or(BridgeDecision::Deny);
        if decision.remembers()
            && let Ok(mut allowed) = self.allowed.lock()
        {
            allowed.insert(tool.to_string());
        }
        decision
    }
}

impl Confirmer for BridgeConfirmer {
    fn confirm(&self, tool: &str, args: &serde_json::Value) -> bool {
        self.ask(tool, args).allows()
    }
}

/// True when the window that wrote `endpoint` is still able to answer.
fn alive(endpoint: &Endpoint) -> bool {
    let deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", endpoint.port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// The operation a tool name stands for, for the dialog's risk statement.
///
/// Only used to describe the request in words; the decision itself is the human's. An unknown tool
/// is described as running a program, because that is the least reassuring reading of "we do not
/// know what this is".
pub fn tool_operation(tool: &str) -> rc_core::policy::Operation {
    use rc_core::policy::Operation;
    match tool.trim().to_ascii_lowercase().as_str() {
        "delete" => Operation::Delete,
        "move" | "rename" => Operation::Rename,
        "copy" | "write_text_file" | "create_directory" => Operation::Write,
        "git_commit" | "git_checkout" | "git_stage" => Operation::Write,
        "git_push" | "git_pull" => Operation::Publish,
        "search" | "list_directory" | "read_text_file" => Operation::Read,
        _ => Operation::Execute,
    }
}

/// The most interesting argument: the one that names what will change.
pub fn notable_argument(args: &serde_json::Value) -> String {
    const KEYS: &[&str] = &[
        "path",
        "paths",
        "target",
        "destination",
        "to",
        "from",
        "remote",
        "url",
        "repo",
        "branch",
        "message",
    ];
    let Some(map) = args.as_object() else {
        return crate::confirm::display_args(args);
    };
    for key in KEYS {
        if let Some(value) = map.get(*key) {
            let text = match value {
                serde_json::Value::Array(items) => items
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| item.to_string())
                    })
                    .collect::<Vec<String>>()
                    .join(", "),
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            if !text.is_empty() {
                return shorten(&text, 200);
            }
        }
    }
    crate::confirm::display_args(args)
}

fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    format!("{}…", text.chars().take(limit).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_round_trip_and_only_a_yes_allows() {
        for decision in [
            BridgeDecision::Once,
            BridgeDecision::Session,
            BridgeDecision::Always,
            BridgeDecision::Deny,
        ] {
            assert_eq!(BridgeDecision::parse(decision.as_str()), decision);
        }
        assert!(BridgeDecision::Once.allows());
        assert!(!BridgeDecision::Deny.allows());
        assert!(!BridgeDecision::Once.remembers());
        assert!(BridgeDecision::Session.remembers());
        // Anything unexpected is a denial, not an approval.
        for text in ["", "yes", "true", "allow", "  "] {
            assert_eq!(
                BridgeDecision::parse(text),
                BridgeDecision::Deny,
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_tool_name_becomes_the_risk_statement_of_its_operation() {
        use rc_core::policy::Operation;
        assert_eq!(tool_operation("delete"), Operation::Delete);
        assert_eq!(tool_operation("git_push"), Operation::Publish);
        assert_eq!(tool_operation("git_commit"), Operation::Write);
        assert_eq!(tool_operation("move"), Operation::Rename);
        assert_eq!(tool_operation("list_directory"), Operation::Read);
        // An unknown tool is described as running a program: the least comforting reading.
        assert_eq!(tool_operation("what_is_this"), Operation::Execute);
    }

    #[test]
    fn the_notable_argument_is_what_will_change() {
        let args = serde_json::json!({
            "paths": ["/tmp/a.txt", "/tmp/b.txt"],
            "recursive": true
        });
        assert_eq!(notable_argument(&args), "/tmp/a.txt, /tmp/b.txt");
        let args = serde_json::json!({ "path": "/srv/index.php", "content": "x".repeat(500) });
        assert_eq!(notable_argument(&args), "/srv/index.php");
        let long = serde_json::json!({ "path": "a".repeat(400) });
        assert!(notable_argument(&long).chars().count() <= 201);
        // Nothing recognisable still shows the arguments, so the dialog is never empty.
        let odd = serde_json::json!({ "colour": "blue" });
        assert!(notable_argument(&odd).contains("colour"));
    }

    fn temp_endpoint(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("rc-bridge-{}-{name}", std::process::id()))
            .join("approval.json")
    }

    #[test]
    fn a_window_answers_the_question() {
        let path = temp_endpoint("answer");
        let (queue, requests) = std::sync::mpsc::channel();
        let Some(server) = ApprovalServer::start_at(path.clone(), queue) else {
            // No loopback socket in this environment; the terminal confirmer would be used.
            return;
        };
        let endpoint = server.endpoint().clone();
        let worker = std::thread::spawn(move || {
            let request = requests
                .recv_timeout(Duration::from_secs(5))
                .expect("the client's request arrives");
            assert_eq!(request.tool, "git_push");
            assert_eq!(request.args["branch"], "main");
            request.answer(BridgeDecision::Session);
        });
        let mut stream = TcpStream::connect(("127.0.0.1", endpoint.port)).unwrap();
        let body = serde_json::json!({
            "token": endpoint.token,
            "tool": "git_push",
            "args": { "branch": "main" },
        })
        .to_string();
        stream.write_all(format!("{body}\n").as_bytes()).unwrap();
        stream.flush().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("session"), "{line}");
        worker.join().unwrap();
        drop(server);
        assert!(!path.exists(), "closing the window withdraws the endpoint");
    }

    #[test]
    fn a_wrong_token_gets_no_answer() {
        let path = temp_endpoint("token");
        let (queue, _requests) = std::sync::mpsc::channel();
        let Some(server) = ApprovalServer::start_at(path, queue) else {
            return;
        };
        let endpoint = server.endpoint().clone();
        let mut stream = TcpStream::connect(("127.0.0.1", endpoint.port)).unwrap();
        let body = serde_json::json!({ "token": "not-the-token", "tool": "delete" }).to_string();
        stream.write_all(format!("{body}\n").as_bytes()).unwrap();
        let _ = stream.flush();
        // Closing the connection without a token check has to end in silence, not an approval.
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        assert!(line.is_empty(), "no decision without the token: {line}");
    }

    #[test]
    fn the_client_honours_a_session_answer_without_asking_again() {
        let path = temp_endpoint("session");
        let (queue, requests) = std::sync::mpsc::channel();
        let Some(server) = ApprovalServer::start_at(path, queue) else {
            return;
        };
        let endpoint = server.endpoint().clone();
        std::thread::spawn(move || {
            if let Ok(request) = requests.recv_timeout(Duration::from_secs(5)) {
                request.answer(BridgeDecision::Session);
            }
        });
        let confirmer = BridgeConfirmer::for_endpoint(endpoint);
        assert!(confirmer.confirm("git_push", &serde_json::json!({})));
        assert!(
            confirmer.is_allowed_for_session("git_push"),
            "the session answer is remembered"
        );
        assert!(!confirmer.is_allowed_for_session("delete"));
        drop(server);
    }
}

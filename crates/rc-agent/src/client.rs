//! The MCP **client**: consumes external MCP servers from the agent panel.
//!
//! A client is synchronous and blocking. It speaks JSON-RPC 2.0 over one of two
//! transports:
//!
//! * [`Transport::Stdio`] spawns a child process, talks newline-delimited
//!   JSON-RPC on its stdin/stdout and drains stderr into error messages.
//! * [`Transport::Http`] posts one JSON-RPC message per HTTP request and reads
//!   one JSON-RPC response back.
//!
//! # Documented limitations
//!
//! * **HTTP**: plain `http://` only — there is no TLS stack in this crate, so
//!   `https://` URLs are rejected. Streaming is not implemented: when the
//!   response is a `text/event-stream`, the first `data:` payload is used and
//!   the rest of the stream is discarded. Only one response per request is
//!   understood, so server-initiated sampling/elicitation cannot work.
//! * **Timeouts**: `timeout_ms` is enforced per request. The stdio transport
//!   reads on a helper thread and uses `recv_timeout`, so a silent server
//!   produces [`McpError::Timeout`] instead of hanging; the helper thread is
//!   detached and ends when the child closes its stdout.
//! * **Cancellation**: `call_tool` cannot be cancelled once a request has been
//!   written; the client sends no `notifications/cancelled`.
//! * Server-to-client traffic (notifications and requests such as
//!   `sampling/createMessage`) is ignored: while waiting for a response the
//!   client skips anything that carries a `method`, and answers nothing.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PROTOCOL_VERSION;
use crate::jsonrpc::{self, RpcError, RpcResponse};
use crate::server::ToolDescriptor;

/// Default per-request timeout of an [`McpClient`], in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// How the client reaches an MCP server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Transport {
    /// Spawn a child process and speak JSON-RPC over stdin/stdout.
    Stdio {
        /// Executable to run.
        command: String,
        /// Command line arguments.
        #[serde(default)]
        args: Vec<String>,
        /// Extra environment variables for the child.
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    /// POST JSON-RPC messages to an HTTP endpoint.
    Http {
        /// Endpoint URL; only `http://` is supported, see the module docs.
        url: String,
        /// Optional bearer token sent as `Authorization: Bearer …`.
        #[serde(default)]
        token: Option<String>,
    },
}

/// Configuration of an [`McpClient`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpClientConfig {
    /// Name reported to the server as `clientInfo.name`.
    pub name: String,
    /// How to reach the server.
    pub transport: Transport,
    /// Per-request timeout in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

impl Default for McpClientConfig {
    fn default() -> Self {
        McpClientConfig {
            name: "rcommander".to_string(),
            transport: Transport::Stdio {
                command: String::new(),
                args: Vec::new(),
                env: BTreeMap::new(),
            },
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

impl McpClientConfig {
    /// Configuration for a server started as a child process.
    pub fn stdio(name: impl Into<String>, command: impl Into<String>) -> Self {
        McpClientConfig {
            name: name.into(),
            transport: Transport::Stdio {
                command: command.into(),
                args: Vec::new(),
                env: BTreeMap::new(),
            },
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    /// Configuration for an HTTP endpoint.
    pub fn http(name: impl Into<String>, url: impl Into<String>) -> Self {
        McpClientConfig {
            name: name.into(),
            transport: Transport::Http {
                url: url.into(),
                token: None,
            },
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    /// Add a command line argument to a stdio transport.
    #[must_use]
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        if let Transport::Stdio { args, .. } = &mut self.transport {
            args.push(arg.into());
        }
        self
    }

    /// Override the per-request timeout.
    #[must_use]
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}

/// What an MCP server reported about itself during `initialize`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    /// `serverInfo.name`.
    pub name: String,
    /// `serverInfo.version`.
    pub version: String,
    /// The protocol revision the server answered with.
    pub protocol_version: String,
}

/// The decoded result of a `tools/call` request.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// All `text` content blocks, joined with `\n`.
    pub text: String,
    /// Whether the server flagged the call as failed.
    pub is_error: bool,
    /// The untouched `result` object.
    pub raw: Value,
}

/// Everything that can go wrong in the client.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// The transport itself failed: spawn, socket, framing, encoding.
    #[error("transport failure: {0}")]
    Transport(String),
    /// No response arrived within the configured timeout.
    #[error("the MCP server did not answer within {0} ms")]
    Timeout(u64),
    /// The peer violated the JSON-RPC contract.
    #[error("protocol violation: {0}")]
    Protocol(String),
    /// The server answered with a JSON-RPC error.
    #[error("the MCP server reported error {code}: {message}")]
    Rpc {
        /// Numeric JSON-RPC error code.
        code: i64,
        /// Human readable message from the server.
        message: String,
    },
    /// The connection ended before a response arrived.
    #[error("the MCP server closed the connection")]
    Closed,
    /// An underlying I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl McpError {
    /// Add the operation that failed to the message.
    ///
    /// [`McpError::Timeout`], [`McpError::Closed`] and [`McpError::Io`] carry no
    /// free-form text and keep their variant, so callers can match on them
    /// whatever layer produced the failure.
    #[must_use]
    pub fn with_context(self, context: &str) -> Self {
        match self {
            McpError::Timeout(ms) => McpError::Timeout(ms),
            McpError::Io(error) => McpError::Io(error),
            McpError::Closed => McpError::Closed,
            McpError::Rpc { code, message } => McpError::Rpc {
                code,
                message: format!("{message} ({context})"),
            },
            McpError::Transport(message) => McpError::Transport(format!("{message} ({context})")),
            McpError::Protocol(message) => McpError::Protocol(format!("{message} ({context})")),
        }
    }

    fn from_rpc(error: RpcError) -> Self {
        McpError::Rpc {
            code: error.code,
            message: error.message,
        }
    }
}

/// A connected MCP client.
///
/// Construct it with [`McpClient::connect`] (real transport) or
/// [`McpClient::connect_with_io`] (any reader/writer pair, which is how the test
/// suite drives a scripted server).
pub struct McpClient {
    config: McpClientConfig,
    channel: Channel,
    next_id: u64,
    server_info: Option<ServerInfo>,
    tools: Vec<ToolDescriptor>,
    closed: bool,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("name", &self.config.name)
            .field("transport", &transport_kind(&self.config.transport))
            .field("server_info", &self.server_info)
            .field("tools", &self.tools.len())
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}

/// A short label for a transport, used in `Debug` output.
fn transport_kind(transport: &Transport) -> &'static str {
    match transport {
        Transport::Stdio { .. } => "stdio",
        Transport::Http { .. } => "http",
    }
}

impl McpClient {
    /// Connect, run the `initialize` handshake and fetch the tool list.
    pub fn connect(config: &McpClientConfig) -> Result<Self, McpError> {
        let channel = match &config.transport {
            Transport::Stdio { command, args, env } => {
                Channel::Io(IoChannel::spawn(command, args, env)?)
            }
            Transport::Http { url, token } => {
                let timeout = Duration::from_millis(config.timeout_ms.max(1));
                Channel::Http(HttpChannel::new(url, token.clone(), timeout)?)
            }
        };
        Self::finish_connect(config.clone(), channel)
    }

    /// Connect over an arbitrary reader/writer pair that carries
    /// newline-delimited JSON-RPC.
    ///
    /// This is the transport the stdio mode uses, exposed so that tests (and
    /// unusual embeddings such as a WebSocket bridge) can drive the client with
    /// a scripted peer. The handshake runs exactly as in [`McpClient::connect`]:
    /// `initialize` (request), `notifications/initialized`, `tools/list`.
    pub fn connect_with_io<R, W>(
        config: &McpClientConfig,
        reader: R,
        writer: W,
    ) -> Result<Self, McpError>
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        let channel = Channel::Io(IoChannel::from_parts(
            reader,
            writer,
            None,
            Arc::new(Mutex::new(String::new())),
        )?);
        Self::finish_connect(config.clone(), channel)
    }

    fn finish_connect(config: McpClientConfig, channel: Channel) -> Result<Self, McpError> {
        let mut client = McpClient {
            config,
            channel,
            next_id: 1,
            server_info: None,
            tools: Vec::new(),
            closed: false,
        };
        client.handshake()?;
        Ok(client)
    }

    fn handshake(&mut self) -> Result<(), McpError> {
        let params = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": self.config.name,
                "version": env!("CARGO_PKG_VERSION"),
            },
        });
        let result = self.rpc("initialize", Some(params))?;
        self.server_info = parse_server_info(&result);
        self.channel
            .notify(&jsonrpc::notification("notifications/initialized", None))
            .map_err(|error| error.with_context("while completing the MCP handshake"))?;
        // A server may legitimately publish no tools; a failing tools/list must
        // not invalidate the handshake that just succeeded.
        if let Ok(list) = self.rpc("tools/list", None) {
            self.tools = parse_tool_list(&list);
        }
        Ok(())
    }

    /// What the server said about itself during `initialize`.
    pub fn server_info(&self) -> Option<ServerInfo> {
        self.server_info.clone()
    }

    /// The tools reported by the server's `tools/list`.
    pub fn tools(&self) -> Vec<ToolDescriptor> {
        self.tools.clone()
    }

    /// Whether [`McpClient::close`] was already called.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Call a tool and decode its content blocks.
    ///
    /// A tool-level failure does **not** produce `Err`: the server answers with
    /// `isError: true` and the client reports that in [`ToolResult::is_error`].
    /// Only transport, timeout and JSON-RPC errors are `Err`.
    pub fn call_tool(&mut self, name: &str, args: Value) -> Result<ToolResult, McpError> {
        let params = json!({ "name": name, "arguments": args });
        let result = self.rpc("tools/call", Some(params))?;
        Ok(decode_tool_result(result))
    }

    /// List the resources the server exposes.
    pub fn list_resources(&mut self) -> Result<Vec<Value>, McpError> {
        let result = self.rpc("resources/list", None)?;
        match result.get("resources") {
            Some(Value::Array(items)) => Ok(items.clone()),
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(_) => Err(McpError::Protocol(
                "`resources/list` returned a `resources` member that is not an array".to_string(),
            )),
        }
    }

    /// Read one resource and return its text, e.g. a help topic loaded from
    /// `rcommander://help/<topic>/<lang>`.
    pub fn read_resource(&mut self, uri: &str) -> Result<String, McpError> {
        let result = self.rpc("resources/read", Some(json!({ "uri": uri })))?;
        let contents = result
            .get("contents")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                McpError::Protocol("`resources/read` returned no `contents` array".to_string())
            })?;
        let mut text = String::new();
        for item in contents {
            if let Some(part) = item.get("text").and_then(Value::as_str) {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(part);
            }
        }
        Ok(text)
    }

    /// Shut the connection down: send `shutdown`, then `exit`, then release the
    /// child process (if any).
    ///
    /// Best effort — a server that ignores `shutdown` is still killed off, so
    /// the only errors come from the transport teardown itself, which is why
    /// this rarely fails.
    pub fn close(mut self) -> Result<(), McpError> {
        self.shutdown_best_effort();
        self.closed = true;
        Ok(())
    }

    fn shutdown_best_effort(&mut self) {
        if self.closed {
            return;
        }
        let id = self.next_id;
        let _ = self.channel.notify(&jsonrpc::request(id, "shutdown", None));
        let _ = self.channel.notify(&jsonrpc::notification("exit", None));
    }

    fn rpc(&mut self, method: &str, params: Option<Value>) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = jsonrpc::request(id, method, params);
        let timeout = Duration::from_millis(self.config.timeout_ms.max(1));
        self.channel
            .transact(&request, timeout)
            .map_err(|error| error.with_context(&format!("while calling `{method}`")))
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.shutdown_best_effort();
        self.closed = true;
        self.channel.close();
    }
}

/// Extract [`ServerInfo`] from an `initialize` result.
fn parse_server_info(result: &Value) -> Option<ServerInfo> {
    let protocol_version = result
        .get("protocolVersion")
        .and_then(Value::as_str)?
        .to_string();
    let info = result.get("serverInfo");
    Some(ServerInfo {
        name: info
            .and_then(|info| info.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        version: info
            .and_then(|info| info.get("version"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        protocol_version,
    })
}

/// Extract the tool descriptors of a `tools/list` result.
fn parse_tool_list(result: &Value) -> Vec<ToolDescriptor> {
    result
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| tools.iter().filter_map(ToolDescriptor::from_json).collect())
        .unwrap_or_default()
}

/// Decode a `tools/call` result into [`ToolResult`].
fn decode_tool_result(result: Value) -> ToolResult {
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut text = String::new();
    if let Some(blocks) = result.get("content").and_then(Value::as_array) {
        for block in blocks {
            if let Some(part) = block.get("text").and_then(Value::as_str) {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(part);
            }
        }
    } else if let Some(part) = result.as_str() {
        // Tolerate servers that answer with a bare string.
        text.push_str(part);
    }
    ToolResult {
        text,
        is_error,
        raw: result,
    }
}

/// Interpret one incoming line as a JSON-RPC response; server notifications and
/// requests are skipped (`None`).
fn decode_response_line(line: &str) -> Result<Option<RpcResponse>, McpError> {
    let value: Value = serde_json::from_str(line).map_err(|error| {
        McpError::Protocol(format!(
            "the MCP server sent invalid JSON `{}`: {error}",
            shorten(line)
        ))
    })?;
    decode_response_value(&value)
}

fn decode_response_value(value: &Value) -> Result<Option<RpcResponse>, McpError> {
    if !value.is_object() {
        return Err(McpError::Protocol(
            "the MCP server sent a JSON value that is not a JSON-RPC object".to_string(),
        ));
    }
    if value.get("method").is_some() {
        // A server-initiated notification or request: this client supports
        // neither, and skipping it keeps the wait loop alive.
        return Ok(None);
    }
    RpcResponse::from_value(value)
        .map(Some)
        .map_err(|error| McpError::Protocol(format!("malformed JSON-RPC response: {error}")))
}

/// First `n` characters of `text`, with an ellipsis when it was cut.
fn shorten(text: &str) -> String {
    const LIMIT: usize = 200;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let mut out: String = text.chars().take(LIMIT).collect();
    out.push('…');
    out
}

// ---------------------------------------------------------------- transports

/// One live transport.
enum Channel {
    Io(IoChannel),
    Http(HttpChannel),
}

impl Channel {
    fn transact(&mut self, request: &Value, timeout: Duration) -> Result<Value, McpError> {
        match self {
            Channel::Io(io) => io.transact(request, timeout),
            Channel::Http(http) => http.transact(request, timeout),
        }
    }

    fn notify(&mut self, message: &Value) -> Result<(), McpError> {
        match self {
            Channel::Io(io) => io.write_message(message),
            Channel::Http(http) => http.notify(message),
        }
    }

    fn close(&mut self) {
        match self {
            Channel::Io(io) => io.close(),
            Channel::Http(_) => {}
        }
    }
}

/// A message read by the stdio reader thread.
enum ReadEvent {
    Line(String),
    Failed(String),
    Eof,
}

/// Newline-delimited JSON-RPC over a reader/writer pair.
struct IoChannel {
    writer: Box<dyn Write + Send>,
    events: Receiver<ReadEvent>,
    child: Option<Child>,
    stderr_tail: Arc<Mutex<String>>,
}

impl IoChannel {
    /// Spawn `command` with piped stdio.
    fn spawn(
        command: &str,
        args: &[String],
        env: &BTreeMap<String, String>,
    ) -> Result<Self, McpError> {
        if command.trim().is_empty() {
            return Err(McpError::Transport(
                "the stdio transport needs a non-empty `command`".to_string(),
            ));
        }
        let mut process = Command::new(command);
        process
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in env {
            process.env(key, value);
        }
        let mut child = process.spawn().map_err(|error| {
            McpError::Transport(format!(
                "could not start the MCP server `{command}`: {error}"
            ))
        })?;
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            return Err(McpError::Transport(
                "the MCP server has no stdin pipe".to_string(),
            ));
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            return Err(McpError::Transport(
                "the MCP server has no stdout pipe".to_string(),
            ));
        };
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = child.stderr.take() {
            drain_stderr(stderr, Arc::clone(&stderr_tail));
        }
        Self::from_parts(BufReader::new(stdout), stdin, Some(child), stderr_tail)
    }

    /// Wrap an existing reader/writer pair and start the reader thread.
    fn from_parts<R, W>(
        mut reader: R,
        writer: W,
        child: Option<Child>,
        stderr_tail: Arc<Mutex<String>>,
    ) -> Result<Self, McpError>
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        let (sender, events) = mpsc::channel();
        thread::Builder::new()
            .name("rc-agent-mcp-reader".to_string())
            .spawn(move || {
                let mut buffer: Vec<u8> = Vec::new();
                loop {
                    buffer.clear();
                    match reader.read_until(b'\n', &mut buffer) {
                        Ok(0) => {
                            let _ = sender.send(ReadEvent::Eof);
                            return;
                        }
                        Ok(_) => {
                            let line = String::from_utf8_lossy(&buffer);
                            let line = line.trim();
                            if line.is_empty() {
                                continue;
                            }
                            if sender.send(ReadEvent::Line(line.to_string())).is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            let _ = sender.send(ReadEvent::Failed(error.to_string()));
                            return;
                        }
                    }
                }
            })
            .map_err(|error| {
                McpError::Transport(format!("could not start the MCP reader thread: {error}"))
            })?;
        Ok(IoChannel {
            writer: Box::new(writer),
            events,
            child,
            stderr_tail,
        })
    }

    fn write_message(&mut self, message: &Value) -> Result<(), McpError> {
        let text = serde_json::to_string(message).map_err(|error| {
            McpError::Transport(format!("could not encode a JSON-RPC message: {error}"))
        })?;
        let outcome = self
            .writer
            .write_all(text.as_bytes())
            .and_then(|()| self.writer.write_all(b"\n"))
            .and_then(|()| self.writer.flush());
        outcome.map_err(|error| self.transport_error("writing to the MCP server failed", error))
    }

    fn transport_error(&self, context: &str, error: std::io::Error) -> McpError {
        let tail = self
            .stderr_tail
            .lock()
            .map(|tail| tail.trim().to_string())
            .unwrap_or_default();
        if tail.is_empty() {
            McpError::Transport(format!("{context}: {error}"))
        } else {
            McpError::Transport(format!("{context}: {error} (server stderr: {tail})"))
        }
    }

    fn transact(&mut self, request: &Value, timeout: Duration) -> Result<Value, McpError> {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        self.write_message(request)?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .unwrap_or_else(Instant::now);
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(McpError::Timeout(timeout.as_millis() as u64));
            };
            if remaining.is_zero() {
                return Err(McpError::Timeout(timeout.as_millis() as u64));
            }
            match self.events.recv_timeout(remaining) {
                Ok(ReadEvent::Line(line)) => {
                    let Some(response) = decode_response_line(&line)? else {
                        continue;
                    };
                    if response.id.as_ref() != Some(&id) {
                        // Late answer to an earlier request, or a response to a
                        // notification: not ours, keep waiting.
                        continue;
                    }
                    return response.into_result().map_err(McpError::from_rpc);
                }
                Ok(ReadEvent::Failed(message)) => {
                    return Err(McpError::Transport(format!(
                        "reading from the MCP server failed: {message}"
                    )));
                }
                Ok(ReadEvent::Eof) => return Err(McpError::Closed),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(McpError::Timeout(timeout.as_millis() as u64));
                }
                Err(RecvTimeoutError::Disconnected) => return Err(McpError::Closed),
            }
        }
    }

    fn close(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Append the child's stderr to a bounded buffer for later error messages.
fn drain_stderr(mut stderr: impl Read + Send + 'static, tail: Arc<Mutex<String>>) {
    let _ = thread::Builder::new()
        .name("rc-agent-mcp-stderr".to_string())
        .spawn(move || {
            let mut buffer = [0u8; 1024];
            loop {
                match stderr.read(&mut buffer) {
                    Ok(0) | Err(_) => return,
                    Ok(read) => {
                        if let Ok(mut tail) = tail.lock() {
                            tail.push_str(&String::from_utf8_lossy(&buffer[..read]));
                            let overflow = tail.len().saturating_sub(STDERR_TAIL_LIMIT);
                            if overflow > 0 {
                                let mut cut = overflow;
                                while cut < tail.len() && !tail.is_char_boundary(cut) {
                                    cut += 1;
                                }
                                tail.drain(..cut);
                            }
                        }
                    }
                }
            }
        });
}

/// How many bytes of the child's stderr are kept for error messages.
const STDERR_TAIL_LIMIT: usize = 4096;

/// One JSON-RPC message per HTTP POST.
struct HttpChannel {
    host: String,
    port: u16,
    path: String,
    token: Option<String>,
    timeout: Duration,
}

impl HttpChannel {
    fn new(url: &str, token: Option<String>, timeout: Duration) -> Result<Self, McpError> {
        let (host, port, path) = parse_http_url(url)?;
        if let Some(token) = &token
            && (token.contains('\r') || token.contains('\n'))
        {
            return Err(McpError::Transport(
                "the bearer token must not contain CR or LF".to_string(),
            ));
        }
        Ok(HttpChannel {
            host,
            port,
            path,
            token,
            timeout,
        })
    }

    fn transact(&mut self, request: &Value, _timeout: Duration) -> Result<Value, McpError> {
        let body = serde_json::to_string(request).map_err(|error| {
            McpError::Transport(format!("could not encode a JSON-RPC message: {error}"))
        })?;
        let response_body = self.post(&body)?;
        let value: Value = serde_json::from_str(&response_body).map_err(|error| {
            McpError::Protocol(format!(
                "the MCP server answered with invalid JSON `{}`: {error}",
                shorten(&response_body)
            ))
        })?;
        let Some(response) = decode_response_value(&value)? else {
            return Err(McpError::Protocol(
                "the MCP server answered with a notification instead of a response".to_string(),
            ));
        };
        if response.id.as_ref() != request.get("id") {
            return Err(McpError::Protocol(format!(
                "the MCP server answered with id {} instead of {}",
                response
                    .id
                    .as_ref()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                request
                    .get("id")
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )));
        }
        response.into_result().map_err(McpError::from_rpc)
    }

    fn notify(&mut self, message: &Value) -> Result<(), McpError> {
        let body = serde_json::to_string(message).map_err(|error| {
            McpError::Transport(format!("could not encode a JSON-RPC message: {error}"))
        })?;
        // A notification has no response; do not wait for one.
        self.send(&body).map(|_stream| ())
    }

    fn post(&self, body: &str) -> Result<String, McpError> {
        let stream = self.send(body)?;
        let mut reader = BufReader::new(stream);
        read_http_response(&mut reader, self.timeout)
    }

    fn send(&self, body: &str) -> Result<TcpStream, McpError> {
        let stream = connect_tcp(&self.host, self.port, self.timeout)?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|()| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|error| {
                McpError::Transport(format!("could not configure the HTTP socket: {error}"))
            })?;
        let mut writer = stream.try_clone().map_err(|error| {
            McpError::Transport(format!("could not duplicate the HTTP socket: {error}"))
        })?;

        let mut request = String::with_capacity(body.len() + 256);
        request.push_str(&format!("POST {} HTTP/1.1\r\n", self.path));
        request.push_str(&format!("Host: {}:{}\r\n", self.host, self.port));
        request.push_str("Content-Type: application/json\r\n");
        request.push_str("Accept: application/json, text/event-stream\r\n");
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        if let Some(token) = &self.token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        request.push_str("Connection: close\r\n\r\n");
        request.push_str(body);

        writer.write_all(request.as_bytes()).map_err(|error| {
            McpError::Transport(format!("could not send the HTTP request: {error}"))
        })?;
        writer.flush().map_err(|error| {
            McpError::Transport(format!("could not flush the HTTP request: {error}"))
        })?;
        drop(writer);
        Ok(stream)
    }
}

/// Turn a socket error into a timeout or a transport failure.
fn http_io_error(error: std::io::Error, timeout: Duration, context: &str) -> McpError {
    match error.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
            McpError::Timeout(timeout.as_millis() as u64)
        }
        _ => McpError::Transport(format!("{context}: {error}")),
    }
}

/// Read one HTTP response and return its body as text.
fn read_http_response(reader: &mut impl BufRead, timeout: Duration) -> Result<String, McpError> {
    let mut status_line = String::new();
    if reader
        .read_line(&mut status_line)
        .map_err(|error| http_io_error(error, timeout, "could not read the HTTP status line"))?
        == 0
    {
        return Err(McpError::Closed);
    }
    let status_text = status_line.trim_end().to_string();
    let mut parts = status_text.split_whitespace();
    let version = parts.next().unwrap_or_default();
    let status = parts
        .next()
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| {
            McpError::Transport(format!("malformed HTTP status line `{status_text}`"))
        })?;
    if !version.starts_with("HTTP/") {
        return Err(McpError::Transport(format!(
            "malformed HTTP status line `{status_text}`"
        )));
    }

    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    loop {
        let mut line = String::new();
        if reader
            .read_line(&mut line)
            .map_err(|error| http_io_error(error, timeout, "could not read HTTP headers"))?
            == 0
        {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let chunked = headers
        .get("transfer-encoding")
        .map(|value| value.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);
    let body = if chunked {
        read_chunked(reader)?
    } else if let Some(length) = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
    {
        let mut buffer = vec![0u8; length];
        reader
            .read_exact(&mut buffer)
            .map_err(|error| http_io_error(error, timeout, "truncated HTTP body"))?;
        buffer
    } else {
        let mut buffer = Vec::new();
        reader
            .read_to_end(&mut buffer)
            .map_err(|error| http_io_error(error, timeout, "could not read the HTTP body"))?;
        buffer
    };
    let text = String::from_utf8_lossy(&body).to_string();
    if !(200..300).contains(&status) {
        return Err(McpError::Transport(format!(
            "the MCP server answered with HTTP {status}: {}",
            shorten(&text)
        )));
    }
    let event_stream = headers
        .get("content-type")
        .map(|value| value.to_ascii_lowercase().contains("event-stream"))
        .unwrap_or(false);
    Ok(if event_stream {
        first_event_payload(&text).unwrap_or(text)
    } else {
        text
    })
}

/// Decode a `Transfer-Encoding: chunked` body.
fn read_chunked(reader: &mut impl BufRead) -> Result<Vec<u8>, McpError> {
    let mut body = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).map_err(|error| {
            McpError::Transport(format!("could not read a chunk header: {error}"))
        })? == 0
        {
            break;
        }
        let size_text = line
            .trim()
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        let size = usize::from_str_radix(&size_text, 16)
            .map_err(|_| McpError::Protocol(format!("malformed HTTP chunk size `{size_text}`")))?;
        if size == 0 {
            // Skip the (usually empty) trailer section.
            loop {
                let mut trailer = String::new();
                if reader.read_line(&mut trailer).unwrap_or(0) == 0 || trailer.trim().is_empty() {
                    break;
                }
            }
            break;
        }
        let mut chunk = vec![0u8; size];
        reader
            .read_exact(&mut chunk)
            .map_err(|error| McpError::Transport(format!("truncated HTTP chunk: {error}")))?;
        body.extend_from_slice(&chunk);
        let mut crlf = String::new();
        let _ = reader.read_line(&mut crlf);
    }
    Ok(body)
}

/// First `data:` payload of a server-sent-event response.
fn first_event_payload(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("data:"))
        .map(|payload| payload.trim().to_string())
}

/// Split `http://host:port/path` into its parts.
fn parse_http_url(url: &str) -> Result<(String, u16, String), McpError> {
    let trimmed = url.trim();
    let Some(rest) = trimmed.strip_prefix("http://") else {
        let hint = if trimmed.starts_with("https://") {
            " (TLS is not implemented by rc-agent; terminate HTTPS in a local proxy and point the client at http://127.0.0.1)"
        } else {
            ""
        };
        return Err(McpError::Transport(format!(
            "unsupported MCP endpoint `{trimmed}`: expected `http://host[:port]/path`{hint}"
        )));
    };
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err(McpError::Transport(format!(
            "the MCP endpoint `{trimmed}` has no host"
        )));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => {
            let port = port
                .parse::<u16>()
                .ok()
                .ok_or_else(|| McpError::Transport(format!("invalid port in `{trimmed}`")))?;
            (host.to_string(), port)
        }
        None => (authority.to_string(), 80u16),
    };
    if host.is_empty() || host.starts_with('[') {
        return Err(McpError::Transport(format!(
            "unsupported host in `{trimmed}`: IPv6 literals are not supported"
        )));
    }
    Ok((host, port, path.to_string()))
}

/// Connect a TCP socket with a connect timeout, trying every resolved address.
fn connect_tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, McpError> {
    let addresses = (host, port).to_socket_addrs().map_err(|error| {
        McpError::Transport(format!("could not resolve `{host}:{port}`: {error}"))
    })?;
    let mut last: Option<std::io::Error> = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return Err(McpError::Timeout(timeout.as_millis() as u64));
            }
            Err(error) => last = Some(error),
        }
    }
    Err(McpError::Transport(format!(
        "could not connect to `{host}:{port}`: {}",
        last.map(|error| error.to_string())
            .unwrap_or_else(|| "no address resolved".to_string())
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_split_into_host_port_and_path() {
        assert_eq!(
            parse_http_url("http://127.0.0.1:8931/mcp").expect("parses"),
            ("127.0.0.1".to_string(), 8931, "/mcp".to_string())
        );
        assert_eq!(
            parse_http_url("http://localhost").expect("parses"),
            ("localhost".to_string(), 80, "/".to_string())
        );
        assert_eq!(
            parse_http_url("  http://example.invalid:8080/a/b  ").expect("parses"),
            ("example.invalid".to_string(), 8080, "/a/b".to_string())
        );
    }

    #[test]
    fn unsupported_urls_are_rejected_with_a_hint() {
        let error = parse_http_url("https://example.invalid/mcp").expect_err("no TLS");
        let message = error.to_string();
        assert!(message.contains("TLS"), "{message}");
        assert!(parse_http_url("/tmp/socket").is_err());
        assert!(parse_http_url("http://").is_err());
        assert!(parse_http_url("http://host:notaport/").is_err());
        assert!(parse_http_url("http://[::1]:80/").is_err());
    }

    #[test]
    fn tool_results_join_text_blocks_and_keep_raw() {
        let raw = json!({
            "content": [{ "type": "text", "text": "one" }, { "type": "image" }, { "type": "text", "text": "two" }],
            "isError": true,
        });
        let result = decode_tool_result(raw.clone());
        assert_eq!(result.text, "one\ntwo");
        assert!(result.is_error);
        assert_eq!(result.raw, raw);

        let empty = decode_tool_result(json!({ "content": [] }));
        assert_eq!(empty.text, "");
        assert!(!empty.is_error);

        let bare = decode_tool_result(json!("just text"));
        assert_eq!(bare.text, "just text");
    }

    #[test]
    fn server_info_requires_a_protocol_version() {
        let info = parse_server_info(&json!({
            "protocolVersion": PROTOCOL_VERSION,
            "serverInfo": { "name": "demo", "version": "1.2.3" },
        }))
        .expect("complete");
        assert_eq!(info.name, "demo");
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.protocol_version, PROTOCOL_VERSION);
        assert!(parse_server_info(&json!({ "serverInfo": { "name": "x" } })).is_none());
    }

    #[test]
    fn tool_lists_ignore_entries_without_a_name() {
        let tools = parse_tool_list(&json!({
            "tools": [
                { "name": "a", "inputSchema": { "type": "object" } },
                { "title": "nameless" },
                "nonsense",
            ],
        }));
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "a");
        assert_eq!(tools[0].title, "a");
        assert_eq!(tools[0].input_schema, json!({ "type": "object" }));
        assert!(parse_tool_list(&json!({})).is_empty());
    }

    #[test]
    fn notifications_and_requests_from_the_server_are_skipped() {
        assert!(
            decode_response_line(r#"{"jsonrpc":"2.0","method":"notifications/message"}"#)
                .expect("skipped")
                .is_none()
        );
        assert!(
            decode_response_line(r#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#)
                .expect("skipped")
                .is_none()
        );
        let response = decode_response_line(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#)
            .expect("parsed")
            .expect("a response");
        assert_eq!(response.id, Some(json!(1)));
    }

    #[test]
    fn protocol_violations_are_reported_not_panicked() {
        let error = decode_response_line("not json").expect_err("must fail");
        assert!(matches!(error, McpError::Protocol(_)));
        assert!(decode_response_line("[1,2]").is_err());
        assert!(decode_response_line(r#"{"jsonrpc":"2.0","id":1}"#).is_err());
    }

    #[test]
    fn rpc_errors_carry_code_and_message() {
        let error = McpError::from_rpc(RpcError::new(-32601, "unknown method `x`"));
        assert!(matches!(error, McpError::Rpc { code: -32601, .. }));
        let context = error.with_context("while calling `tools/list`");
        assert!(
            context.to_string().contains("while calling `tools/list`"),
            "{context}"
        );
        assert!(matches!(
            McpError::Timeout(5).with_context("x"),
            McpError::Timeout(5)
        ));
    }

    #[test]
    fn event_stream_payloads_are_extracted() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        assert_eq!(
            first_event_payload(body).expect("payload"),
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}"
        );
        assert!(first_event_payload("no payload here").is_none());
    }

    #[test]
    fn chunked_bodies_are_decoded() {
        let raw = "4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        let mut reader = BufReader::new(raw.as_bytes());
        let body = read_chunked(&mut reader).expect("decodes");
        assert_eq!(String::from_utf8_lossy(&body), "{\"a\":1}");
    }

    #[test]
    fn http_responses_are_parsed_with_and_without_length() {
        let timeout = Duration::from_secs(5);
        let raw = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        let mut reader = BufReader::new(raw.as_bytes());
        assert_eq!(
            read_http_response(&mut reader, timeout).expect("body"),
            "{}"
        );

        let raw = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: {}\n\n";
        let mut reader = BufReader::new(raw.as_bytes());
        assert_eq!(
            read_http_response(&mut reader, timeout).expect("body"),
            "{}"
        );

        let raw = "HTTP/1.1 500 Boom\r\nContent-Length: 4\r\n\r\nnope";
        let mut reader = BufReader::new(raw.as_bytes());
        let error = read_http_response(&mut reader, timeout).expect_err("status");
        assert!(error.to_string().contains("HTTP 500"), "{error}");

        let mut reader = BufReader::new("garbage\r\n\r\n".as_bytes());
        assert!(read_http_response(&mut reader, timeout).is_err());

        // A socket timeout becomes McpError::Timeout, not a generic failure.
        let timed_out = http_io_error(
            std::io::Error::new(std::io::ErrorKind::WouldBlock, "again"),
            Duration::from_millis(1234),
            "reading",
        );
        assert!(
            matches!(timed_out, McpError::Timeout(1234)),
            "{timed_out:?}"
        );
    }

    #[test]
    fn config_defaults_and_builders() {
        let config = McpClientConfig::stdio("panel", "npx").with_arg("server.js");
        assert_eq!(config.name, "panel");
        assert_eq!(config.timeout_ms, DEFAULT_TIMEOUT_MS);
        match &config.transport {
            Transport::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(args, &vec!["server.js".to_string()]);
                assert!(env.is_empty());
            }
            other => panic!("unexpected transport {other:?}"),
        }
        let config = McpClientConfig::http("web", "http://127.0.0.1:1/mcp").with_timeout_ms(5);
        assert_eq!(config.timeout_ms, 5);
        assert_eq!(transport_kind(&config.transport), "http");
        assert_eq!(McpClientConfig::default().name, "rcommander");
    }

    #[test]
    fn long_text_is_shortened_for_messages() {
        let long = "x".repeat(500);
        let short = shorten(&long);
        assert!(short.ends_with('…'));
        assert_eq!(short.chars().count(), 201);
        assert_eq!(shorten("ok"), "ok");
    }
}

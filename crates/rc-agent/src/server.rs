//! The MCP **server**: exposes the file manager to external AI agents.
//!
//! [`McpServer`] is transport agnostic. [`McpServer::handle_json`] turns one
//! JSON-RPC message into one response string and is what a GUI thread or an HTTP
//! bridge should call; [`McpServer::serve_stdio`] is the ready-made
//! newline-delimited stdio loop that MCP hosts such as Claude Desktop expect.
//!
//! Protocol revision: [`PROTOCOL_VERSION`] (`2025-06-18`). Implemented methods:
//! `initialize`, `notifications/initialized`, `ping`, `tools/list`,
//! `tools/call`, `resources/list`, `resources/read`, `logging/setLevel` and
//! `shutdown`.

use std::io::{BufRead, Write};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde_json::{Map, Value, json};

use crate::host::{
    Confirmer, FileManagerHost, HostPanel, LanguageHost, PanelSide, SearchRequest, SelectMode,
};
use crate::jsonrpc::{self, RpcError, RpcMessage};
use crate::{DEFAULT_MAX_READ_BYTES, DEFAULT_SERVER_NAME, HELP_URI_PREFIX, PROTOCOL_VERSION};

/// Run a fallible host call inside a tool body and turn a failure into an
/// `isError` tool result (a tool-level failure, not a protocol error).
macro_rules! host_call {
    ($tool:expr, $call:expr) => {
        match $call {
            Ok(value) => value,
            Err(message) => {
                return Ok(error_result(format!("tool `{}` failed: {message}", $tool)));
            }
        }
    };
}

/// A syslog/RFC-5424 severity accepted by `logging/setLevel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Verbose diagnostics.
    Debug,
    /// Informational messages.
    Info,
    /// Normal but significant events.
    Notice,
    /// Unexpected conditions that are not errors.
    Warning,
    /// Tool calls that failed.
    Error,
    /// Critical conditions.
    Critical,
    /// Action must be taken immediately.
    Alert,
    /// System is unusable.
    Emergency,
}

impl LogLevel {
    /// Every level, least severe first.
    pub const ALL: [LogLevel; 8] = [
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Notice,
        LogLevel::Warning,
        LogLevel::Error,
        LogLevel::Critical,
        LogLevel::Alert,
        LogLevel::Emergency,
    ];

    /// Parse the protocol spelling, case-insensitively.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "notice" => Some(LogLevel::Notice),
            "warning" | "warn" => Some(LogLevel::Warning),
            "error" => Some(LogLevel::Error),
            "critical" | "crit" => Some(LogLevel::Critical),
            "alert" => Some(LogLevel::Alert),
            "emergency" | "emerg" => Some(LogLevel::Emergency),
            _ => None,
        }
    }

    /// The protocol spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Notice => "notice",
            LogLevel::Warning => "warning",
            LogLevel::Error => "error",
            LogLevel::Critical => "critical",
            LogLevel::Alert => "alert",
            LogLevel::Emergency => "emergency",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Configuration of an [`McpServer`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    /// Name reported as `serverInfo.name`.
    pub name: String,
    /// Version reported as `serverInfo.version`.
    pub version: String,
    /// Tool names that need approval from a [`Confirmer`] before they run.
    ///
    /// The destructive tools (`delete`, `move`, `rename`, `write_text_file`) are
    /// always in this list because [`ToolDescriptor::requires_confirmation`] is
    /// derived from [`ToolDescriptor::destructive`] as well; adding further
    /// names here gates additional tools.
    pub require_confirmation_for: Vec<String>,
    /// Hard upper bound on the bytes `read_text_file` may return.
    pub max_read_bytes: usize,
    /// Whether the help topics are published as MCP resources.
    pub enable_resources: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            name: DEFAULT_SERVER_NAME.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            require_confirmation_for: ["delete", "move", "write_text_file", "rename"]
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
            enable_resources: true,
        }
    }
}

/// A tool published by [`McpServer::tools`].
///
/// The same type is used by [`crate::McpClient`] when it parses a remote
/// `tools/list` result; in that case only `name`, `title`, `description` and
/// `input_schema` are filled in and the two flags stay `false`.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolDescriptor {
    /// Tool name the agent uses in `tools/call`.
    pub name: String,
    /// Human readable title shown in tool pickers.
    pub title: String,
    /// Description written for the model, not for the developer.
    pub description: String,
    /// JSON Schema of the `arguments` object.
    pub input_schema: Value,
    /// Whether the tool can destroy or overwrite user data.
    pub destructive: bool,
    /// Whether the GUI must approve this call before it runs.
    pub requires_confirmation: bool,
}

impl ToolDescriptor {
    /// The wire form used in a `tools/list` result.
    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": self.input_schema,
        })
    }

    /// Parse one entry of a remote `tools/list` result.
    pub fn from_json(value: &Value) -> Option<Self> {
        let name = value.get("name")?.as_str()?.to_string();
        let title = value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(&name)
            .to_string();
        let description = value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let input_schema = value
            .get("inputSchema")
            .cloned()
            .unwrap_or_else(|| json!({}));
        Some(ToolDescriptor {
            name,
            title,
            description,
            input_schema,
            destructive: false,
            requires_confirmation: false,
        })
    }
}

/// Mutable server state kept behind a lock so that every `handle_json` call can
/// work with `&self` (the GUI calls in from a plain event handler).
struct ServerState {
    log_level: LogLevel,
    shutdown: bool,
    confirmer: Option<Arc<dyn Confirmer>>,
    language_host: Option<Arc<dyn LanguageHost>>,
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerState")
            .field("log_level", &self.log_level)
            .field("shutdown", &self.shutdown)
            .field("has_confirmer", &self.confirmer.is_some())
            .field("has_language_host", &self.language_host.is_some())
            .finish()
    }
}

/// The MCP server over a [`FileManagerHost`].
///
/// Cheap to share: every method takes `&self` and the little state it keeps
/// (log level, shutdown flag, approval callback) is behind a lock.
pub struct McpServer<H: FileManagerHost> {
    host: Arc<H>,
    config: ServerConfig,
    state: RwLock<ServerState>,
}

impl<H: FileManagerHost> std::fmt::Debug for McpServer<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServer")
            .field("config", &self.config)
            .field("tools", &self.tools().len())
            .finish_non_exhaustive()
    }
}

impl<H: FileManagerHost> McpServer<H> {
    /// Build a server over `host`.
    pub fn new(host: Arc<H>, config: ServerConfig) -> Self {
        McpServer {
            host,
            config,
            state: RwLock::new(ServerState {
                log_level: LogLevel::Info,
                shutdown: false,
                confirmer: None,
                language_host: None,
            }),
        }
    }

    /// Register the approval callback (builder style).
    #[must_use]
    pub fn with_confirmer(self, confirmer: Arc<dyn Confirmer>) -> Self {
        self.state_write().confirmer = Some(confirmer);
        self
    }

    /// Register (or with `None`, remove) the approval callback at runtime.
    pub fn set_confirmer(&self, confirmer: Option<Arc<dyn Confirmer>>) {
        self.state_write().confirmer = confirmer;
    }

    /// Register the language-switching capability (builder style); this is what
    /// makes the server publish the `set_language` tool.
    #[must_use]
    pub fn with_language_host(self, language_host: Arc<dyn LanguageHost>) -> Self {
        self.state_write().language_host = Some(language_host);
        self
    }

    /// Register (or with `None`, remove) the language-switching capability.
    pub fn set_language_host(&self, language_host: Option<Arc<dyn LanguageHost>>) {
        self.state_write().language_host = language_host;
    }

    /// The configuration this server was built with.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// The host this server drives.
    pub fn host(&self) -> &Arc<H> {
        &self.host
    }

    /// Whether `set_language` is published (and therefore callable).
    pub fn supports_set_language(&self) -> bool {
        self.state_read().language_host.is_some()
    }

    /// The log level last requested with `logging/setLevel`.
    pub fn log_level(&self) -> LogLevel {
        self.state_read().log_level
    }

    /// Whether `shutdown` has been received; [`McpServer::serve_stdio`] stops
    /// once it is `true`.
    pub fn is_shutdown(&self) -> bool {
        self.state_read().shutdown
    }

    /// Tools this server publishes, in the order they appear in `tools/list`.
    pub fn tools(&self) -> Vec<ToolDescriptor> {
        let mut tools = self.catalogue();
        for tool in &mut tools {
            tool.requires_confirmation = tool.destructive
                || self
                    .config
                    .require_confirmation_for
                    .iter()
                    .any(|name| name == &tool.name);
        }
        tools
    }

    /// Handle one JSON-RPC message (request or notification) and return the
    /// response message, or `None` for notifications.
    ///
    /// Malformed input never panics: it produces a JSON-RPC error object with
    /// code `-32700` or `-32600`.
    pub fn handle_json(&self, request: &str) -> Option<String> {
        let message = match RpcMessage::parse(request) {
            Ok(message) => message,
            Err(error) => return Some(jsonrpc::failure(Value::Null, &error)),
        };
        let id = message.id.clone();
        match self.dispatch(&message) {
            Ok(result) => id.map(|id| jsonrpc::success(id, result)),
            Err(error) => id.map(|id| jsonrpc::failure(id, &error)),
        }
    }

    /// Stdio loop: read newline-delimited JSON-RPC from `input`, write the
    /// responses to `output`.
    ///
    /// Blank lines are ignored, a line that is not valid JSON produces a
    /// `-32700` response, and the loop ends at end of input or after a
    /// `shutdown` request.
    pub fn serve_stdio(
        &self,
        mut input: impl BufRead,
        mut output: impl Write,
    ) -> std::io::Result<()> {
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            let read = input.read_until(b'\n', &mut buffer)?;
            if read == 0 {
                return Ok(());
            }
            // Lossy on purpose: a non-UTF-8 line must become a parse error, not
            // an I/O failure that kills the loop.
            let line = String::from_utf8_lossy(&buffer);
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(response) = self.handle_json(line) {
                output.write_all(response.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
            if self.is_shutdown() {
                return Ok(());
            }
        }
    }

    // ---------------------------------------------------------------- internals

    fn state_read(&self) -> RwLockReadGuard<'_, ServerState> {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn state_write(&self) -> RwLockWriteGuard<'_, ServerState> {
        self.state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn dispatch(&self, message: &RpcMessage) -> Result<Value, RpcError> {
        match message.method.as_str() {
            "initialize" => Ok(self.initialize_result()),
            "notifications/initialized" => Ok(json!({})),
            "notifications/cancelled" => Ok(json!({})),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.tools_list_result()),
            "tools/call" => self.call_tool(&message.params),
            "resources/list" => self.resources_list_result(),
            "resources/read" => self.resources_read_result(&message.params),
            "logging/setLevel" => self.set_level(&message.params),
            "shutdown" => {
                self.state_write().shutdown = true;
                Ok(json!({}))
            }
            other => Err(RpcError::method_not_found(other)),
        }
    }

    fn initialize_result(&self) -> Value {
        let mut capabilities = Map::new();
        capabilities.insert("tools".to_string(), json!({ "listChanged": false }));
        if self.config.enable_resources {
            capabilities.insert(
                "resources".to_string(),
                json!({ "subscribe": false, "listChanged": false }),
            );
        }
        capabilities.insert("logging".to_string(), json!({}));
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": Value::Object(capabilities),
            "serverInfo": {
                "name": self.config.name,
                "version": self.config.version,
            },
            "instructions": concat!(
                "RCommander is a dual-pane file manager. Two panels (left and right) each show a ",
                "directory; the active panel is the focused one. Use list_panels/get_active_panel to ",
                "orient yourself, list_directory to read a listing, and navigate to change a panel's ",
                "directory. Destructive tools (delete, move, rename, write_text_file) require the ",
                "user's approval and may be refused; prefer move over delete and check paths first."
            ),
        })
    }

    fn tools_list_result(&self) -> Value {
        let tools: Vec<Value> = self.tools().iter().map(ToolDescriptor::to_json).collect();
        json!({ "tools": tools })
    }

    fn call_tool(&self, params: &Value) -> Result<Value, RpcError> {
        let Some(object) = params.as_object() else {
            return Err(RpcError::invalid_params(
                "`tools/call` requires an object with a `name` member and an optional `arguments` object",
            ));
        };
        let Some(name) = object.get("name").and_then(Value::as_str) else {
            return Err(RpcError::invalid_params(
                "`tools/call` requires a string `name` member",
            ));
        };
        let arguments = match object.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(value @ Value::Object(_)) => value.clone(),
            Some(_) => {
                return Err(RpcError::invalid_params(
                    "`arguments` must be an object when present",
                ));
            }
        };

        let Some(descriptor) = self.tools().into_iter().find(|tool| tool.name == name) else {
            let available: Vec<String> = self.tools().into_iter().map(|tool| tool.name).collect();
            return Ok(error_result(format!(
                "unknown tool `{name}`: this server publishes {}",
                available.join(", ")
            )));
        };

        if descriptor.requires_confirmation {
            self.check_confirmation(name, &arguments)?;
        }

        // Tool bodies call into the GUI, which is not allowed to take the whole
        // MCP server down: a panic becomes a `-32603` error with the message.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.run_tool(name, &arguments)
        })) {
            Ok(outcome) => outcome,
            Err(payload) => Err(RpcError::internal(format!(
                "tool `{name}` failed internally: {}",
                panic_message(payload.as_ref())
            ))),
        }
    }

    /// Ask the registered [`Confirmer`] for permission; never runs a destructive
    /// tool without one.
    fn check_confirmation(&self, tool: &str, arguments: &Value) -> Result<(), RpcError> {
        let confirmer = self.state_read().confirmer.clone();
        let Some(confirmer) = confirmer else {
            return Err(RpcError::confirmation_required(format!(
                "confirmation required: the destructive tool `{tool}` needs approval, but no approval \
                 handler is registered with this MCP server, so nothing was changed"
            )));
        };
        // A panicking approval handler must not take the server down.
        let approved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            confirmer.confirm(tool, arguments)
        }));
        match approved {
            Ok(true) => Ok(()),
            Ok(false) => Err(RpcError::confirmation_required(format!(
                "confirmation required: the user denied `{tool}`, so nothing was changed"
            ))),
            Err(payload) => Err(RpcError::internal(format!(
                "the approval handler for `{tool}` panicked: {}",
                panic_message(payload.as_ref())
            ))),
        }
    }

    fn set_level(&self, params: &Value) -> Result<Value, RpcError> {
        let Some(level) = params.get("level").and_then(Value::as_str) else {
            return Err(RpcError::invalid_params(
                "`logging/setLevel` requires a string `level` member (`debug`, `info`, `notice`, \
                 `warning`, `error`, `critical`, `alert` or `emergency`)",
            ));
        };
        let Some(level) = LogLevel::parse(level) else {
            return Err(RpcError::invalid_params(format!(
                "unknown log level `{level}`: expected one of debug, info, notice, warning, error, \
                 critical, alert, emergency"
            )));
        };
        self.state_write().log_level = level;
        Ok(json!({}))
    }

    fn resources_list_result(&self) -> Result<Value, RpcError> {
        if !self.config.enable_resources {
            return Err(RpcError::method_not_found(
                "resources/list is disabled in this server configuration",
            ));
        }
        let languages = self.host.available_languages();
        let mut resources = Vec::new();
        for (topic, title) in self.host.help_topics() {
            for language in &languages {
                resources.push(json!({
                    "uri": help_uri(&topic, language),
                    "name": format!("{topic}-{language}"),
                    "title": format!("{title} ({language})"),
                    "description": format!("RCommander help topic `{topic}` in language `{language}`."),
                    "mimeType": "text/markdown",
                }));
            }
        }
        Ok(json!({ "resources": resources }))
    }

    fn resources_read_result(&self, params: &Value) -> Result<Value, RpcError> {
        if !self.config.enable_resources {
            return Err(RpcError::method_not_found(
                "resources/read is disabled in this server configuration",
            ));
        }
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return Err(RpcError::invalid_params(
                "`resources/read` requires a string `uri` member",
            ));
        };
        let Some((topic, language)) = parse_help_uri(uri) else {
            return Err(RpcError::invalid_params(format!(
                "unsupported resource URI `{uri}`: expected `{HELP_URI_PREFIX}<topic>/<language>`"
            )));
        };
        let language = language.unwrap_or_else(|| self.host.current_language());
        match self.host.help_body(&topic, &language) {
            Some(text) => Ok(json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/markdown",
                    "text": text,
                }]
            })),
            None => Err(RpcError::invalid_params(format!(
                "no help topic `{topic}` in language `{language}`"
            ))),
        }
    }

    // ------------------------------------------------------------ tool helpers

    fn active_side(&self) -> PanelSide {
        self.host.active_panel().side
    }

    fn resolved_side(&self, panel: Option<PanelSide>) -> PanelSide {
        panel.unwrap_or_else(|| self.active_side())
    }

    fn panel_snapshot(&self, side: PanelSide) -> Option<HostPanel> {
        let active = self.host.active_panel();
        if active.side == side {
            return Some(active);
        }
        self.host
            .panels()
            .into_iter()
            .find(|panel| panel.side == side)
    }

    fn panel_path(&self, side: PanelSide) -> Option<String> {
        self.panel_snapshot(side).map(|panel| panel.path)
    }

    // -------------------------------------------------------------- tool bodies

    fn run_tool(&self, name: &str, arguments: &Value) -> Result<Value, RpcError> {
        let args = Args::new(arguments);
        match name {
            "list_directory" => {
                let panel = args.opt_panel()?;
                let path = args.opt_str("path")?;
                let include_hidden = args.opt_bool("include_hidden", false)?;
                let limit = args.opt_u64("limit")?.unwrap_or(DEFAULT_LIST_LIMIT as u64) as usize;
                let side = self.resolved_side(panel);
                let shown_path = match (path.as_deref(), self.panel_path(side)) {
                    (Some(path), _) => path.to_string(),
                    (None, Some(panel_path)) => panel_path,
                    (None, None) => "(unknown)".to_string(),
                };
                let entries = host_call!(name, self.host.list_directory(panel, path.as_deref()));
                let visible: Vec<&rc_core::entry::Entry> = entries
                    .iter()
                    .filter(|entry| include_hidden || !entry.is_hidden())
                    .collect();
                let total = visible.len();
                let hidden = entries.len() - total;
                let mut text = String::new();
                text.push_str(&format!(
                    "Directory `{shown_path}` (panel {side}): {total} visible entries"
                ));
                if hidden > 0 {
                    text.push_str(&format!(", {hidden} hidden"));
                }
                text.push_str(".\n");
                if total == 0 {
                    text.push_str("The directory is empty.\n");
                } else {
                    text.push_str("attrs  size        modified             name\n");
                    for entry in visible.iter().take(limit) {
                        text.push_str(&format!(
                            "{:<6} {:>10}  {}  {}\n",
                            entry.attr_string(),
                            rc_core::format::format_size_bytes(entry.display_size()),
                            rc_core::format::format_datetime(entry.mtime),
                            entry.name,
                        ));
                    }
                    if total > limit {
                        text.push_str(&format!(
                            "... {} more entries omitted; raise `limit` to see them\n",
                            total - limit
                        ));
                    }
                }
                Ok(text_result(text))
            }
            "list_panels" => {
                let panels = self.host.panels();
                if panels.is_empty() {
                    return Ok(text_result("No panel is open."));
                }
                let mut text = String::new();
                for panel in panels {
                    text.push_str(&format!(
                        "{} panel: `{}`{}  view={}  {} entries, {} selected\n",
                        panel.side,
                        panel.path,
                        if panel.active { " (active)" } else { "" },
                        panel.view_mode,
                        panel.entries,
                        panel.selected,
                    ));
                }
                Ok(text_result(text))
            }
            "get_active_panel" => {
                let panel = self.host.active_panel();
                Ok(text_result(format!(
                    "The active panel is the {} one, in `{}`, showing {} entries ({} selected), view mode `{}`.",
                    panel.side, panel.path, panel.entries, panel.selected, panel.view_mode
                )))
            }
            "navigate" => {
                let panel = args.opt_panel()?;
                let path = args.req_str("path")?;
                let side = self.resolved_side(panel);
                host_call!(name, self.host.open_directory(side, &path));
                Ok(text_result(format!("The {side} panel now shows `{path}`.")))
            }
            "get_selection" => {
                let panel = args.opt_panel()?;
                let side = self.resolved_side(panel);
                let selection = host_call!(name, self.host.selection(panel));
                if selection.is_empty() {
                    Ok(text_result(format!(
                        "Nothing is selected in the {side} panel."
                    )))
                } else {
                    let mut text = format!(
                        "{} selected entries in the {side} panel:\n",
                        selection.len()
                    );
                    for path in &selection {
                        text.push_str(path);
                        text.push('\n');
                    }
                    Ok(text_result(text))
                }
            }
            "select" => {
                let panel = args.opt_panel()?;
                let mask = args.req_str("mask")?;
                let mode = args.opt_select_mode("mode")?.unwrap_or(SelectMode::Set);
                let side = self.resolved_side(panel);
                let count = host_call!(name, self.host.select(panel, &mask, mode));
                Ok(text_result(format!(
                    "Applied selection mode `{mode}` with mask `{mask}` to the {side} panel; {count} entries are affected."
                )))
            }
            "copy" | "move" => {
                let move_it = name == "move";
                let sources = args.req_str_list("sources")?;
                let dest = args.req_str("destination")?;
                let overwrite = args.opt_bool("overwrite", false)?;
                let result = host_call!(name, self.host.copy(&sources, &dest, move_it, overwrite));
                Ok(text_result(format!(
                    "{} {} to `{dest}`.",
                    if move_it { "Moved" } else { "Copied" },
                    result.summary()
                )))
            }
            "delete" => {
                let paths = args.req_str_list("paths")?;
                let use_trash = args.opt_bool("use_trash", true)?;
                let result = host_call!(name, self.host.delete(&paths, use_trash));
                Ok(text_result(format!(
                    "Deleted {} ({}).",
                    result.summary(),
                    if use_trash {
                        "recycled when the platform supports a trash"
                    } else {
                        "permanently"
                    }
                )))
            }
            "create_directory" => {
                let path = args.req_str("path")?;
                host_call!(name, self.host.create_directory(&path));
                Ok(text_result(format!("Created the directory `{path}`.")))
            }
            "rename" => {
                let from = args.req_str("from")?;
                let to = args.req_str("to")?;
                host_call!(name, self.host.rename(&from, &to));
                Ok(text_result(format!("Renamed `{from}` to `{to}`.")))
            }
            "file_info" => self.tool_file_info(&args),
            "read_text_file" => {
                let path = args.req_str("path")?;
                let requested = args.opt_u64("max_bytes")?;
                let limit = self.read_limit(requested);
                let content = host_call!(name, self.host.read_text(&path, limit));
                let mut text = content;
                let mut truncated = false;
                if text.len() > limit {
                    let mut cut = limit;
                    while !text.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    text.truncate(cut);
                    truncated = true;
                } else if text.len() >= limit {
                    // The host stopped exactly at the limit, so the file is at
                    // least this long: the remainder may be missing.
                    truncated = true;
                }
                let mut content = vec![json!({ "type": "text", "text": text })];
                if truncated {
                    content.push(json!({
                        "type": "text",
                        "text": format!(
                            "Note: `{path}` was cut off at the {limit}-byte read limit (server maximum {} bytes). Use a narrower file or read it in another program.",
                            self.config.max_read_bytes
                        ),
                    }));
                }
                Ok(json!({ "content": content, "isError": false }))
            }
            "write_text_file" => {
                let path = args.req_str("path")?;
                let content = args.req_str_allow_empty("content")?;
                let written = host_call!(name, self.host.write_text(&path, &content));
                Ok(text_result(format!(
                    "Wrote {} to `{path}`.",
                    rc_core::format::format_size_bytes(written)
                )))
            }
            "search_files" => {
                let root = match args.opt_str("root")? {
                    Some(root) => root,
                    None => self
                        .panel_path(self.active_side())
                        .unwrap_or_else(|| ".".to_string()),
                };
                let request = SearchRequest {
                    root,
                    name_mask: args
                        .opt_str("name_mask")?
                        .unwrap_or_else(|| "*".to_string()),
                    content: args.opt_str("content")?,
                    recursive: args.opt_bool("recursive", true)?,
                    case_sensitive: args.opt_bool("case_sensitive", false)?,
                    max_results: args
                        .opt_u64("max_results")?
                        .unwrap_or(DEFAULT_SEARCH_RESULTS as u64)
                        .clamp(1, MAX_SEARCH_RESULTS as u64)
                        as usize,
                };
                let found = host_call!(name, self.host.search(&request));
                let mut text = format!(
                    "{} match{} for mask `{}`{} below `{}`",
                    found.len(),
                    if found.len() == 1 { "" } else { "es" },
                    request.name_mask,
                    match &request.content {
                        Some(needle) => format!(" containing `{needle}`"),
                        None => String::new(),
                    },
                    request.root,
                );
                if found.len() >= request.max_results {
                    text.push_str(&format!(
                        " (result limit {} reached, there may be more)",
                        request.max_results
                    ));
                }
                text.push_str(":\n");
                for path in &found {
                    text.push_str(path);
                    text.push('\n');
                }
                Ok(text_result(text))
            }
            "disk_space" => {
                let path = match args.opt_str("path")? {
                    Some(path) => path,
                    None => self
                        .panel_path(self.active_side())
                        .unwrap_or_else(|| ".".to_string()),
                };
                let space = host_call!(name, self.host.disk_space(&path));
                Ok(match space {
                    Some(space) => text_result(format!(
                        "`{path}`: total {}, free {}, used {} ({:.1}% full).",
                        rc_core::format::format_size_bytes(space.total),
                        rc_core::format::format_size_bytes(space.available),
                        rc_core::format::format_size_bytes(space.used()),
                        space.used_percent(),
                    )),
                    None => text_result(format!(
                        "Free space of `{path}` is not available on this platform."
                    )),
                })
            }
            "help_search" => {
                let query = args.opt_str("query")?.unwrap_or_default();
                let needle = query.to_lowercase();
                let mut text = String::new();
                let mut hits = 0usize;
                for (topic, title) in self.host.help_topics() {
                    if needle.is_empty()
                        || topic.to_lowercase().contains(&needle)
                        || title.to_lowercase().contains(&needle)
                    {
                        text.push_str(&format!("{topic} — {title}\n"));
                        hits += 1;
                    }
                }
                if hits == 0 {
                    Ok(text_result(format!(
                        "No help topic matches `{query}`. Use help_search without a query to list every topic."
                    )))
                } else {
                    Ok(text_result(format!("{hits} help topics:\n{text}")))
                }
            }
            "help_read" => {
                let topic = args.req_str("topic")?;
                let language = match args.opt_str("language")? {
                    Some(language) => language,
                    None => self.host.current_language(),
                };
                match self.host.help_body(&topic, &language) {
                    Some(body) => Ok(text_result(body)),
                    None => Ok(error_result(format!(
                        "no help topic `{topic}` in language `{language}`; use help_search to find a topic id"
                    ))),
                }
            }
            "get_language" => {
                let available = self.host.available_languages();
                Ok(text_result(format!(
                    "Current language: `{}`. Available: {}.",
                    self.host.current_language(),
                    available.join(", ")
                )))
            }
            "set_language" => {
                let language = args.req_str("language")?;
                let language_host = self.state_read().language_host.clone();
                let Some(language_host) = language_host else {
                    return Ok(error_result(
                        "this RCommander instance cannot change its language at runtime",
                    ));
                };
                match language_host.set_language(&language) {
                    Ok(()) => Ok(text_result(format!(
                        "The interface language is now `{language}`."
                    ))),
                    Err(message) => Ok(error_result(format!(
                        "could not switch to language `{language}`: {message}"
                    ))),
                }
            }
            "get_view_mode" => {
                let panel = args.opt_panel()?;
                let side = self.resolved_side(panel);
                let Some(snapshot) = self.panel_snapshot(side) else {
                    return Ok(error_result(format!(
                        "the {side} panel is not open in this window"
                    )));
                };
                Ok(text_result(format!(
                    "The {side} panel uses view mode `{}` ({} entries, {} selected) in `{}`.",
                    snapshot.view_mode, snapshot.entries, snapshot.selected, snapshot.path
                )))
            }
            other => Ok(error_result(format!("unknown tool `{other}`"))),
        }
    }

    fn tool_file_info(&self, args: &Args<'_>) -> Result<Value, RpcError> {
        let panel = args.opt_panel()?;
        let path = args.req_str("path")?;
        let as_path = std::path::Path::new(&path);
        let parent = as_path
            .parent()
            .map(|parent| parent.to_string_lossy().to_string())
            .filter(|parent| !parent.is_empty())
            .unwrap_or_else(|| ".".to_string());
        let Some(file_name) = as_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            return Ok(error_result(format!(
                "`{path}` does not name a file inside a directory"
            )));
        };
        let entries = host_call!("file_info", self.host.list_directory(panel, Some(&parent)));
        let Some(entry) = entries.iter().find(|entry| entry.name == file_name) else {
            return Ok(error_result(format!(
                "`{path}` is not present in `{parent}`"
            )));
        };

        let mut text = String::new();
        text.push_str(&format!("Path: {}\n", entry.path.display()));
        text.push_str(&format!("Name: {}\n", entry.name));
        text.push_str(&format!("Kind: {}\n", kind_name(entry)));
        if entry.is_file() {
            text.push_str(&format!(
                "Size: {} ({} bytes)\n",
                rc_core::format::format_size_bytes(entry.size),
                entry.size
            ));
        }
        text.push_str(&format!(
            "Modified: {}\n",
            rc_core::format::format_datetime(entry.mtime)
        ));
        if let Some(ctime) = entry.ctime {
            text.push_str(&format!(
                "Created: {}\n",
                rc_core::format::format_datetime(Some(ctime))
            ));
        }
        let attributes = entry.attr_string();
        text.push_str(&format!(
            "Attributes: {}\n",
            if attributes.is_empty() {
                "(none)"
            } else {
                &attributes
            }
        ));
        if let Some(mode) = entry.attrs.unix_mode {
            text.push_str(&format!(
                "Unix mode: {:04o} ({})\n",
                mode,
                rc_core::entry::unix_mode_string(mode)
            ));
        }
        if let Some(target) = &entry.link_target {
            text.push_str(&format!("Link target: {}\n", target.display()));
        }
        Ok(text_result(text))
    }

    fn read_limit(&self, requested: Option<u64>) -> usize {
        let configured = self.config.max_read_bytes.max(1);
        match requested {
            Some(value) => (value.max(1) as usize).min(configured),
            None => configured,
        }
    }

    // ---------------------------------------------------------------- catalogue

    fn catalogue(&self) -> Vec<ToolDescriptor> {
        let panel = || {
            json!({
                "type": "string",
                "enum": ["left", "right"],
                "description": "Which panel to use. Omit to use the active (focused) panel.",
            })
        };
        let mut tools = vec![
            ToolDescriptor {
                name: "list_directory".to_string(),
                title: "List a directory".to_string(),
                description: "List the entries of a directory as a text table with attributes, size, \
                              modification time and name. Without arguments it lists the active panel's \
                              current directory; pass `panel` and/or `path` to read any other directory. \
                              Hidden entries are omitted unless `include_hidden` is true."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "panel": panel(),
                        "path": {
                            "type": "string",
                            "description": "Absolute directory path to list. Omit to list the panel's current directory.",
                        },
                        "include_hidden": {
                            "type": "boolean",
                            "description": "Also list hidden entries (dotfiles and hidden attribute). Defaults to false.",
                            "default": false,
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 100000,
                            "description": "Maximum number of entries to render. Defaults to 2000.",
                        },
                    },
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "list_panels".to_string(),
                title: "List the two file panels".to_string(),
                description: "Report both panels: side, current directory, view mode, entry count, \
                              selection count and which one is active. Start here to find out where the \
                              user currently is."
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": {} }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "get_active_panel".to_string(),
                title: "Get the active panel".to_string(),
                description: "Report the focused panel: its side, current directory, view mode and \
                              selection size."
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": {} }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "navigate".to_string(),
                title: "Change the current directory".to_string(),
                description: "Point a panel at another directory, exactly like the user pressing \
                              Enter on a folder. Use an absolute path. Fails when the path does not \
                              exist or is not a directory."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path of the directory to show.",
                        },
                        "panel": panel(),
                    },
                    "required": ["path"],
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "get_selection".to_string(),
                title: "Read the selection".to_string(),
                description: "Return the full paths of the entries currently selected (marked) in a \
                              panel. An empty selection is normal and not an error."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": { "panel": panel() },
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "select".to_string(),
                title: "Select entries by mask".to_string(),
                description: "Select entries of a panel with a wildcard mask, the way the user \
                              presses `+`, `-` or `*`. Modes: `set` (replace the selection), `add`, \
                              `remove`, `invert_all`."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "mask": {
                            "type": "string",
                            "description": "Wildcard mask such as `*.txt`, `*.rs;*.toml` or `*` for everything.",
                        },
                        "panel": panel(),
                        "mode": {
                            "type": "string",
                            "enum": ["set", "add", "remove", "invert_all"],
                            "description": "How to combine the matching entries with the current selection. Defaults to `set`.",
                            "default": "set",
                        },
                    },
                    "required": ["mask"],
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "copy".to_string(),
                title: "Copy files or directories".to_string(),
                description: "Copy files and/or whole directories into a destination directory. \
                              Existing files are left alone unless `overwrite` is true. The copy runs \
                              without a progress dialog, so large trees block the call."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sources": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths of the files and directories to copy.",
                        },
                        "destination": {
                            "type": "string",
                            "description": "Absolute path of the target directory.",
                        },
                        "overwrite": {
                            "type": "boolean",
                            "description": "Replace existing files in the destination. Defaults to false.",
                            "default": false,
                        },
                    },
                    "required": ["sources", "destination"],
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "move".to_string(),
                title: "Move or rename files".to_string(),
                description: "Move files and directories into a destination directory (or rename them \
                              when the destination is a path inside the same directory). Existing \
                              files are skipped unless `overwrite` is true. Destructive: the sources \
                              disappear from their old location."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sources": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths of the files and directories to move.",
                        },
                        "destination": {
                            "type": "string",
                            "description": "Absolute path of the target directory.",
                        },
                        "overwrite": {
                            "type": "boolean",
                            "description": "Replace existing files in the destination. Defaults to false.",
                            "default": false,
                        },
                    },
                    "required": ["sources", "destination"],
                }),
                destructive: true,
                requires_confirmation: true,
            },
            ToolDescriptor {
                name: "delete".to_string(),
                title: "Delete files or directories".to_string(),
                description: "Delete files and directories. By default the platform trash is used so \
                              the user can restore them; set `use_trash` to false to delete \
                              permanently. Destructive and irreversible when no trash is available: \
                              confirm the paths first."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths to delete. Directories are deleted recursively.",
                        },
                        "use_trash": {
                            "type": "boolean",
                            "description": "Move to the platform trash instead of deleting permanently. Defaults to true.",
                            "default": true,
                        },
                    },
                    "required": ["paths"],
                }),
                destructive: true,
                requires_confirmation: true,
            },
            ToolDescriptor {
                name: "create_directory".to_string(),
                title: "Create a directory".to_string(),
                description: "Create a directory including any missing parents. Fails when the path \
                              already exists as a file."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path of the directory to create.",
                        },
                    },
                    "required": ["path"],
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "rename".to_string(),
                title: "Rename one entry".to_string(),
                description: "Rename a file or directory inside its own parent directory. Use `move` \
                              to relocate something into a different directory. Destructive: the old \
                              name stops existing."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "from": {
                            "type": "string",
                            "description": "Current absolute path.",
                        },
                        "to": {
                            "type": "string",
                            "description": "New absolute path; the parent directory must be the same.",
                        },
                    },
                    "required": ["from", "to"],
                }),
                destructive: true,
                requires_confirmation: true,
            },
            ToolDescriptor {
                name: "file_info".to_string(),
                title: "Describe one entry".to_string(),
                description: "Report type, size, timestamps, attributes and symlink target of a single \
                              file or directory, using the panel's own listing so remote and virtual \
                              file systems work too."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path of the entry to describe.",
                        },
                        "panel": panel(),
                    },
                    "required": ["path"],
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "read_text_file".to_string(),
                title: "Read a text file".to_string(),
                description: "Return the first bytes of a text file as UTF-8 (lossy). The read is \
                              capped at the server's maximum; use `max_bytes` to read less. Binary files \
                              are not detected — check `file_info` first."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path of the file to read.",
                        },
                        "max_bytes": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum number of bytes to return. Capped by the server limit.",
                        },
                    },
                    "required": ["path"],
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "write_text_file".to_string(),
                title: "Write a text file".to_string(),
                description: "Create or overwrite a file with the given UTF-8 text. The whole previous \
                              content is replaced. Destructive: confirm the target path with the user \
                              before overwriting an existing file."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path of the file to write.",
                        },
                        "content": {
                            "type": "string",
                            "description": "Complete new content of the file.",
                        },
                    },
                    "required": ["path", "content"],
                }),
                destructive: true,
                requires_confirmation: true,
            },
            ToolDescriptor {
                name: "search_files".to_string(),
                title: "Search files recursively".to_string(),
                description: "Search below a directory for names matching a wildcard mask and, \
                              optionally, for files whose text contains a string. Returns matching full \
                              paths up to `max_results`."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "root": {
                            "type": "string",
                            "description": "Directory to start from. Defaults to the active panel's directory.",
                        },
                        "name_mask": {
                            "type": "string",
                            "description": "Wildcard mask such as `*.txt` or `report*;*.csv`. Defaults to `*`.",
                            "default": "*",
                        },
                        "content": {
                            "type": "string",
                            "description": "Only return files whose text contains this substring. Omit to search by name only.",
                        },
                        "recursive": {
                            "type": "boolean",
                            "description": "Descend into sub-directories. Defaults to true.",
                            "default": true,
                        },
                        "case_sensitive": {
                            "type": "boolean",
                            "description": "Match the mask and the content needle case-sensitively. Defaults to false.",
                            "default": false,
                        },
                        "max_results": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 10000,
                            "description": "Upper bound on returned paths. Defaults to 200.",
                        },
                    },
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "disk_space".to_string(),
                title: "Report free disk space".to_string(),
                description: "Report total, free and used space of the volume holding a path. \
                              Defaults to the active panel's directory."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path on the volume to inspect. Defaults to the active panel's directory.",
                        },
                    },
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "help_search".to_string(),
                title: "Search the help topics".to_string(),
                description: "Search RCommander's own help topics by id or title and return `id — \
                              title` lines. Call it without a query to list every topic, then read one \
                              with `help_read`."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Case-insensitive text to look for in topic ids and titles. Omit to list everything.",
                        },
                    },
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "help_read".to_string(),
                title: "Read a help topic".to_string(),
                description: "Return the markdown body of one help topic in one language, describing \
                              how a RCommander feature is used. Use `help_search` first to find the id."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "topic": {
                            "type": "string",
                            "description": "Help topic id, as returned by help_search.",
                        },
                        "language": {
                            "type": "string",
                            "description": "Language pack id such as `en` or `cs`. Defaults to the current interface language.",
                        },
                    },
                    "required": ["topic"],
                }),
                destructive: false,
                requires_confirmation: false,
            },
            ToolDescriptor {
                name: "get_language".to_string(),
                title: "Get the interface language".to_string(),
                description: "Report the active language pack of the running RCommander instance and \
                              which other language packs are installed."
                    .to_string(),
                input_schema: json!({ "type": "object", "properties": {} }),
                destructive: false,
                requires_confirmation: false,
            },
        ];

        if self.supports_set_language() {
            tools.push(ToolDescriptor {
                name: "set_language".to_string(),
                title: "Switch the interface language".to_string(),
                description: "Switch the user interface to another installed language pack. Only call \
                              this when the user explicitly asks for it; check `get_language` for the \
                              available ids."
                    .to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "language": {
                            "type": "string",
                            "description": "Language pack id to activate, for example `en`, `cs` or `de`.",
                        },
                    },
                    "required": ["language"],
                }),
                destructive: false,
                requires_confirmation: false,
            });
        }

        tools.push(ToolDescriptor {
            name: "get_view_mode".to_string(),
            title: "Get the panel view mode".to_string(),
            description: "Report how a panel currently renders its entries (`brief`, `full`, \
                          `thumbnails`, …) plus its entry and selection counts."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "panel": panel() },
            }),
            destructive: false,
            requires_confirmation: false,
        });

        tools
    }
}

/// Default number of entries `list_directory` renders.
const DEFAULT_LIST_LIMIT: usize = 2000;
/// Default number of hits `search_files` returns.
const DEFAULT_SEARCH_RESULTS: usize = 200;
/// Hard upper bound on `search_files` results.
const MAX_SEARCH_RESULTS: usize = 10_000;

/// Build the resource URI of a help topic.
pub fn help_uri(topic: &str, language: &str) -> String {
    format!("{HELP_URI_PREFIX}{topic}/{language}")
}

/// Split `rcommander://help/<topic>/<lang>` into its parts; the language is
/// `None` when the URI only names a topic.
pub fn parse_help_uri(uri: &str) -> Option<(String, Option<String>)> {
    let rest = uri.strip_prefix(HELP_URI_PREFIX)?;
    let (topic, language) = match rest.split_once('/') {
        Some((topic, language)) => (topic, Some(language)),
        None => (rest, None),
    };
    if topic.is_empty() || topic.contains('/') {
        return None;
    }
    match language {
        Some(language) if language.is_empty() || language.contains('/') => None,
        Some(language) => Some((topic.to_string(), Some(language.to_string()))),
        None => Some((topic.to_string(), None)),
    }
}

/// A successful `tools/call` result carrying one text block.
fn text_result(text: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": false,
    })
}

/// A failed `tools/call` result carrying one text block.
fn error_result(text: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": true,
    })
}

/// Best-effort text of a caught panic payload.
///
/// Callers must hand over the *boxed value* (`payload.as_ref()`), not the box.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// Friendly name of an entry kind, for `file_info`.
fn kind_name(entry: &rc_core::entry::Entry) -> &'static str {
    use rc_core::entry::EntryKind;
    match entry.kind {
        EntryKind::File => "file",
        EntryKind::Dir => "directory",
        EntryKind::SymlinkToFile => "symlink to a file",
        EntryKind::SymlinkToDir => "symlink to a directory",
        EntryKind::SymlinkBroken => "broken symlink",
        EntryKind::Special => "special file",
    }
}

/// Typed access to the `arguments` object of a `tools/call` request.
///
/// Every accessor returns a `-32602` [`RpcError`] with a message that names the
/// offending member, so an agent can correct its own call.
#[derive(Clone, Copy, Debug)]
struct Args<'a> {
    value: &'a Value,
}

impl<'a> Args<'a> {
    fn new(value: &'a Value) -> Self {
        Args { value }
    }

    fn object(&self) -> Result<&'a Map<String, Value>, RpcError> {
        match self.value {
            Value::Object(object) => Ok(object),
            Value::Null => Err(RpcError::invalid_params(
                "this tool takes an `arguments` object; send one, possibly empty",
            )),
            other => Err(RpcError::invalid_params(format!(
                "`arguments` must be an object, got {other}"
            ))),
        }
    }

    fn get(&self, key: &str) -> Option<&'a Value> {
        match self.value {
            Value::Object(object) => object.get(key),
            _ => None,
        }
    }

    fn req_str(&self, key: &str) -> Result<String, RpcError> {
        self.object()?;
        match self.get(key) {
            Some(Value::String(text)) if !text.trim().is_empty() => Ok(text.clone()),
            Some(Value::String(_)) => Err(RpcError::invalid_params(format!(
                "`{key}` must not be empty"
            ))),
            Some(other) => Err(RpcError::invalid_params(format!(
                "`{key}` must be a string, got {other}"
            ))),
            None => Err(RpcError::invalid_params(format!(
                "missing required argument `{key}`"
            ))),
        }
    }

    /// Like [`Args::req_str`] but an empty string is a valid value (file
    /// content of zero length).
    fn req_str_allow_empty(&self, key: &str) -> Result<String, RpcError> {
        self.object()?;
        match self.get(key) {
            Some(Value::String(text)) => Ok(text.clone()),
            Some(other) => Err(RpcError::invalid_params(format!(
                "`{key}` must be a string, got {other}"
            ))),
            None => Err(RpcError::invalid_params(format!(
                "missing required argument `{key}`"
            ))),
        }
    }

    fn opt_str(&self, key: &str) -> Result<Option<String>, RpcError> {
        self.object()?;
        match self.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
            Some(Value::String(text)) => Ok(Some(text.clone())),
            Some(other) => Err(RpcError::invalid_params(format!(
                "`{key}` must be a string, got {other}"
            ))),
        }
    }

    fn opt_bool(&self, key: &str, default: bool) -> Result<bool, RpcError> {
        self.object()?;
        match self.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(Value::Bool(value)) => Ok(*value),
            Some(other) => Err(RpcError::invalid_params(format!(
                "`{key}` must be a boolean, got {other}"
            ))),
        }
    }

    fn opt_u64(&self, key: &str) -> Result<Option<u64>, RpcError> {
        self.object()?;
        match self.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(number)) => match number.as_u64() {
                Some(value) => Ok(Some(value)),
                None => Err(RpcError::invalid_params(format!(
                    "`{key}` must be a non-negative integer, got {number}"
                ))),
            },
            Some(other) => Err(RpcError::invalid_params(format!(
                "`{key}` must be a non-negative integer, got {other}"
            ))),
        }
    }

    fn opt_panel(&self) -> Result<Option<PanelSide>, RpcError> {
        self.object()?;
        match self.get("panel") {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) => match PanelSide::parse(text) {
                Some(side) => Ok(Some(side)),
                None => Err(RpcError::invalid_params(format!(
                    "`panel` must be \"left\" or \"right\", got {text:?}"
                ))),
            },
            Some(other) => Err(RpcError::invalid_params(format!(
                "`panel` must be \"left\" or \"right\", got {other}"
            ))),
        }
    }

    fn opt_select_mode(&self, key: &str) -> Result<Option<SelectMode>, RpcError> {
        self.object()?;
        match self.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) => match SelectMode::parse(text) {
                Some(mode) => Ok(Some(mode)),
                None => Err(RpcError::invalid_params(format!(
                    "`{key}` must be one of \"set\", \"add\", \"remove\", \"invert_all\"; got {text:?}"
                ))),
            },
            Some(other) => Err(RpcError::invalid_params(format!(
                "`{key}` must be a string, got {other}"
            ))),
        }
    }

    /// A required list of strings; a single string is accepted as a
    /// one-element list because models routinely send `"path": "x"` where an
    /// array is expected.
    fn req_str_list(&self, key: &str) -> Result<Vec<String>, RpcError> {
        self.object()?;
        match self.get(key) {
            Some(Value::Array(items)) => {
                if items.is_empty() {
                    return Err(RpcError::invalid_params(format!(
                        "`{key}` must not be empty"
                    )));
                }
                let mut out = Vec::with_capacity(items.len());
                for (index, item) in items.iter().enumerate() {
                    match item {
                        Value::String(text) if !text.trim().is_empty() => out.push(text.clone()),
                        _ => {
                            return Err(RpcError::invalid_params(format!(
                                "`{key}[{index}]` must be a non-empty string, got {item}"
                            )));
                        }
                    }
                }
                Ok(out)
            }
            Some(Value::String(text)) if !text.trim().is_empty() => Ok(vec![text.clone()]),
            Some(Value::String(_)) => Err(RpcError::invalid_params(format!(
                "`{key}` must not be empty"
            ))),
            Some(other) => Err(RpcError::invalid_params(format!(
                "`{key}` must be an array of strings, got {other}"
            ))),
            None => Err(RpcError::invalid_params(format!(
                "missing required argument `{key}`"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_uris_round_trip() {
        let uri = help_uri("copy-dialog", "cs");
        assert_eq!(uri, "rcommander://help/copy-dialog/cs");
        assert_eq!(
            parse_help_uri(&uri),
            Some(("copy-dialog".to_string(), Some("cs".to_string())))
        );
    }

    #[test]
    fn help_uri_without_language_is_accepted() {
        assert_eq!(
            parse_help_uri("rcommander://help/quicksearch"),
            Some(("quicksearch".to_string(), None))
        );
    }

    #[test]
    fn malformed_help_uris_are_rejected() {
        assert_eq!(parse_help_uri("rcommander://help/"), None);
        assert_eq!(parse_help_uri("rcommander://help/a/b/c"), None);
        assert_eq!(parse_help_uri("rcommander://help/a/"), None);
        assert_eq!(parse_help_uri("file:///tmp/x"), None);
        assert_eq!(parse_help_uri("rcommander://panels/left"), None);
    }

    #[test]
    fn log_levels_parse_and_render() {
        assert_eq!(LogLevel::parse("WARN"), Some(LogLevel::Warning));
        assert_eq!(LogLevel::parse("emerg"), Some(LogLevel::Emergency));
        assert_eq!(LogLevel::parse("chatty"), None);
        assert_eq!(LogLevel::Notice.as_str(), "notice");
        assert_eq!(LogLevel::Debug.to_string(), "debug");
        assert_eq!(LogLevel::ALL.len(), 8);
    }

    #[test]
    fn args_report_actionable_param_errors() {
        let value = json!({ "path": "", "panel": "middle", "limit": -1, "ok": true });
        let args = Args::new(&value);
        assert_eq!(
            args.req_str("path").expect_err("empty").code,
            jsonrpc::INVALID_PARAMS
        );
        assert_eq!(
            args.req_str("missing").expect_err("missing").code,
            jsonrpc::INVALID_PARAMS
        );
        assert_eq!(
            args.opt_panel().expect_err("bad panel").code,
            jsonrpc::INVALID_PARAMS
        );
        assert_eq!(
            args.opt_u64("limit").expect_err("negative").code,
            jsonrpc::INVALID_PARAMS
        );
        assert!(args.opt_bool("ok", false).expect("bool"));
        assert!(args.opt_bool("nope", true).expect("default"));
        assert!(
            args.opt_bool("panel", false)
                .expect_err("not a bool")
                .message
                .contains("must be a boolean")
        );
    }

    #[test]
    fn string_lists_accept_a_single_string() {
        let value = json!({ "sources": "/tmp/a", "paths": ["/tmp/b", "/tmp/c"] });
        let args = Args::new(&value);
        assert_eq!(
            args.req_str_list("sources").expect("one"),
            vec!["/tmp/a".to_string()]
        );
        assert_eq!(args.req_str_list("paths").expect("two").len(), 2);
        assert_eq!(
            args.req_str_list("missing").expect_err("required").code,
            jsonrpc::INVALID_PARAMS
        );

        let empty = json!({ "paths": [] });
        let args = Args::new(&empty);
        assert_eq!(
            args.req_str_list("paths").expect_err("empty").code,
            jsonrpc::INVALID_PARAMS
        );

        let numbers = json!({ "paths": [1] });
        let args = Args::new(&numbers);
        assert!(
            args.req_str_list("paths")
                .expect_err("bad item")
                .message
                .contains("paths[0]")
        );
    }

    #[test]
    fn empty_content_is_allowed_for_written_files() {
        let value = json!({ "content": "" });
        let args = Args::new(&value);
        assert_eq!(args.req_str_allow_empty("content").expect("empty ok"), "");
        assert!(args.req_str("content").is_err());
    }

    #[test]
    fn non_object_arguments_are_invalid_params() {
        let bad = json!("oops");
        let args = Args::new(&bad);
        assert_eq!(
            args.opt_bool("x", false).expect_err("bad").code,
            jsonrpc::INVALID_PARAMS
        );

        let null = Value::Null;
        let args = Args::new(&null);
        assert_eq!(
            args.opt_str("x").expect_err("null args").code,
            jsonrpc::INVALID_PARAMS
        );
    }
}

//! `rc-agent` — the Model Context Protocol (MCP) integration of RCommander.
//!
//! The crate works in both directions and stays completely free of GUI code, so
//! it can be unit-tested without a windowing system:
//!
//! * **Server** ([`McpServer`]) publishes the file manager's capabilities as MCP
//!   tools and help topics, so an external AI agent (Claude Desktop, an IDE
//!   assistant, …) can list directories, copy, search and read files of a
//!   running RCommander instance. The GUI only has to implement
//!   [`FileManagerHost`] and, optionally, [`Confirmer`] and [`LanguageHost`].
//! * **Client** ([`McpClient`]) consumes external MCP servers for the built-in
//!   agent panel, over a child process (stdio) or a plain JSON-RPC HTTP POST.
//!
//! Protocol version: **2025-06-18** ([`PROTOCOL_VERSION`]).
//!
//! # Serving an external agent
//!
//! ```text
//! let host: Arc<Gui> = ...;                       // implements FileManagerHost
//! let server = McpServer::new(host.clone(), ServerConfig::default())
//!     .with_confirmer(host.clone())               // implements Confirmer
//!     .with_language_host(host);                  // implements LanguageHost (optional)
//! server.serve_stdio(std::io::stdin().lock(), std::io::stdout().lock())?;
//! ```
//!
//! The GUI can also drive the protocol itself by calling
//! [`McpServer::handle_json`] on each incoming line, which is what an HTTP or
//! WebSocket bridge would do.
//!
//! # Consuming another server
//!
//! ```no_run
//! use rc_agent::{McpClient, McpClientConfig};
//!
//! let config = McpClientConfig::http("agent-panel", "http://127.0.0.1:8931/mcp");
//! let mut client = McpClient::connect(&config).expect("the MCP server is running");
//! for tool in client.tools() {
//!     println!("{}: {}", tool.name, tool.title);
//! }
//! let result = client
//!     .call_tool("list_directory", serde_json::json!({ "panel": "left" }))
//!     .expect("the call reached the server");
//! println!("{} (error: {})", result.text, result.is_error);
//! ```
//!
//! # Documented limitations
//!
//! * The client's HTTP transport speaks plain `http://` only — no TLS — and
//!   expects one JSON-RPC response per POST. Server-sent-event streaming is
//!   reduced to "use the first payload of the stream".
//! * Transports are synchronous and blocking; there is no async runtime.
//! * Cancellation (`notifications/cancelled`) is accepted but ignored: tool
//!   calls are not interruptible once started, because they run synchronously on
//!   the thread that called [`McpServer::handle_json`].
//! * The server is stateless with respect to the MCP session: it does not
//!   enforce that `initialize` ran before `tools/call`, and it does not track
//!   client capabilities. That is legal MCP behaviour and keeps a GUI-embedded
//!   server simple.
//! * The server publishes one fixed tool set; `notifications/tools/list_changed`
//!   is never sent (the capabilities advertise `listChanged: false`).

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod client;
pub mod host;
pub mod jsonrpc;
pub mod server;

pub use client::{McpClient, McpClientConfig, McpError, ServerInfo, ToolResult, Transport};
pub use host::{
    Confirmer, FileManagerHost, HostOpResult, HostPanel, LanguageHost, PanelSide, SearchRequest,
    SelectMode,
};
pub use jsonrpc::{RpcError, RpcMessage, RpcResponse};
pub use server::{McpServer, ServerConfig, ToolDescriptor};

/// The MCP protocol revision implemented by this crate.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Default server name reported in `initialize` when [`ServerConfig::name`] is
/// left at its default.
pub const DEFAULT_SERVER_NAME: &str = "rcommander";

/// Default upper bound on the number of bytes `read_text_file` returns.
pub const DEFAULT_MAX_READ_BYTES: usize = 1024 * 1024;

/// URI scheme prefix of the help-topic resources.
pub const HELP_URI_PREFIX: &str = "rcommander://help/";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_the_expected_revision() {
        assert_eq!(PROTOCOL_VERSION, "2025-06-18");
        assert_eq!(DEFAULT_SERVER_NAME, "rcommander");
        assert_eq!(DEFAULT_MAX_READ_BYTES, 1 << 20);
        assert!(HELP_URI_PREFIX.ends_with('/'));
    }
}

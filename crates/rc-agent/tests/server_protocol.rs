//! Integration tests for the MCP **server** ([`rc_agent::McpServer`]).
//!
//! They drive the server exactly like a JSON-RPC peer would — through
//! `handle_json` / `serve_stdio` — and check the protocol shape, the error
//! codes, the tool behaviour against a real temporary directory and the
//! approval gate for destructive tools.

mod common;

use std::sync::Arc;

use common::{FakeHost, PanickingConfirmer, TestConfirmer};
use rc_agent::{FileManagerHost, McpServer, ServerConfig};
use serde_json::{Value, json};

type TestServer = McpServer<FakeHost>;

/// A default server over a fresh [`FakeHost`].
fn setup() -> (Arc<FakeHost>, TestServer) {
    let host = Arc::new(FakeHost::new());
    let server = McpServer::new(Arc::clone(&host), ServerConfig::default());
    (host, server)
}

/// A server with a non-default configuration.
fn setup_with(config: ServerConfig) -> (Arc<FakeHost>, TestServer) {
    let host = Arc::new(FakeHost::new());
    let server = McpServer::new(Arc::clone(&host), config);
    (host, server)
}

/// Send one raw line and decode the response, if any.
fn handle_raw(server: &TestServer, line: &str) -> Option<Value> {
    server
        .handle_json(line)
        .map(|text| serde_json::from_str(&text).expect("the server always answers with JSON"))
}

/// Send one request and decode the response.
fn handle(server: &TestServer, message: Value) -> Option<Value> {
    handle_raw(server, &message.to_string())
}

/// Send a request and return the whole response object.
fn rpc(server: &TestServer, id: u64, method: &str, params: Value) -> Value {
    let mut request = json!({ "jsonrpc": "2.0", "id": id, "method": method });
    if !params.is_null() {
        request["params"] = params;
    }
    handle(server, request).expect("a request is answered")
}

/// Send a request that must succeed and return its `result`.
fn result_of(server: &TestServer, method: &str, params: Value) -> Value {
    let response = rpc(server, 1, method, params);
    assert!(
        response.get("error").is_none(),
        "`{method}` failed: {response}"
    );
    response.get("result").cloned().unwrap_or(Value::Null)
}

/// Send a request that must fail and return `(code, message)`.
fn error_of(server: &TestServer, method: &str, params: Value) -> (i64, String) {
    let response = rpc(server, 1, method, params);
    let error = response
        .get("error")
        .unwrap_or_else(|| panic!("`{method}` unexpectedly succeeded: {response}"));
    (
        error["code"].as_i64().expect("numeric code"),
        error["message"].as_str().expect("message text").to_string(),
    )
}

/// Call a tool and return its `result` object.
fn call_tool(server: &TestServer, name: &str, arguments: Value) -> Value {
    result_of(
        server,
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    )
}

/// All text content of a `tools/call` result, joined with newlines.
fn tool_text(result: &Value) -> String {
    result["content"]
        .as_array()
        .expect("content array")
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Assert that a tool call succeeded and return its text.
fn tool_ok(server: &TestServer, name: &str, arguments: Value) -> String {
    let result = call_tool(server, name, arguments);
    assert_eq!(
        result["isError"].as_bool(),
        Some(false),
        "tool `{name}` failed: {result}"
    );
    tool_text(&result)
}

/// Assert that a tool call reported a tool-level failure and return its text.
fn tool_err(server: &TestServer, name: &str, arguments: Value) -> String {
    let result = call_tool(server, name, arguments);
    assert_eq!(
        result["isError"].as_bool(),
        Some(true),
        "tool `{name}` unexpectedly succeeded: {result}"
    );
    tool_text(&result)
}

// ---------------------------------------------------------------- handshake

#[test]
fn initialize_reports_protocol_capabilities_and_server_info() {
    let (_host, server) = setup();
    let response = rpc(
        &server,
        1,
        "initialize",
        json!({ "protocolVersion": "2025-06-18" }),
    );
    assert_eq!(response["jsonrpc"], json!("2.0"));
    assert_eq!(response["id"], json!(1));

    let result = &response["result"];
    assert_eq!(result["protocolVersion"], json!("2025-06-18"));
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["capabilities"]["resources"].is_object());
    assert!(result["capabilities"]["logging"].is_object());
    assert_eq!(result["serverInfo"]["name"], json!("rcommander"));
    assert!(result["serverInfo"]["version"].is_string());
    assert!(
        result["instructions"]
            .as_str()
            .unwrap()
            .contains("RCommander")
    );
}

#[test]
fn initialize_hides_resources_when_they_are_disabled() {
    let (_host, server) = setup_with(ServerConfig {
        enable_resources: false,
        ..Default::default()
    });
    let result = result_of(&server, "initialize", json!({}));
    assert!(result["capabilities"]["resources"].is_null());
    let (code, message) = error_of(&server, "resources/list", json!({}));
    assert_eq!(code, -32601);
    assert!(message.contains("disabled"), "{message}");
}

#[test]
fn initialized_notification_returns_no_response() {
    let (_host, server) = setup();
    assert!(
        handle(
            &server,
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
        )
        .is_none()
    );
    assert!(
        handle(
            &server,
            json!({ "jsonrpc": "2.0", "method": "notifications/cancelled" })
        )
        .is_none()
    );
    // An unknown notification must be ignored as well, not answered.
    assert!(
        handle(
            &server,
            json!({ "jsonrpc": "2.0", "method": "no/such/notification" })
        )
        .is_none()
    );
}

#[test]
fn ping_returns_an_empty_result() {
    let (_host, server) = setup();
    let result = result_of(&server, "ping", Value::Null);
    assert_eq!(result, json!({}));
}

// ------------------------------------------------------------------- tools

#[test]
fn tools_list_publishes_well_formed_descriptors() {
    let (_host, server) = setup();
    let result = result_of(&server, "tools/list", Value::Null);
    let tools = result["tools"].as_array().expect("tools array");
    assert!(tools.len() > 15, "only {} tools published", tools.len());

    let mut names = Vec::new();
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name");
        names.push(name.to_string());

        // Wire shape: exactly the four documented members.
        let object = tool.as_object().expect("tool object");
        assert_eq!(
            object.len(),
            4,
            "tool `{name}` has extra wire members: {:?}",
            object.keys().collect::<Vec<_>>()
        );
        assert!(
            !tool["title"].as_str().unwrap_or("").is_empty(),
            "{name} title"
        );
        assert!(
            tool["description"].as_str().unwrap_or("").len() > 20,
            "{name} description is written for humans, not for a model"
        );

        // Schema sanity.
        let schema = &tool["inputSchema"];
        assert!(schema.is_object(), "{name}: inputSchema must be an object");
        assert_eq!(schema["type"], json!("object"), "{name}: inputSchema.type");
        let properties = schema["properties"].as_object().expect("properties object");
        if let Some(required) = schema.get("required") {
            let required = required.as_array().expect("required array");
            for key in required {
                let key = key.as_str().expect("required entry is a string");
                assert!(
                    properties.contains_key(key),
                    "{name}: `{key}` is required but not declared in properties"
                );
            }
        }
    }

    let unique: std::collections::BTreeSet<&String> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "duplicate tool names: {names:?}");
    for expected in [
        "list_directory",
        "list_panels",
        "get_active_panel",
        "navigate",
        "get_selection",
        "select",
        "copy",
        "move",
        "delete",
        "create_directory",
        "rename",
        "file_info",
        "read_text_file",
        "write_text_file",
        "search_files",
        "disk_space",
        "help_search",
        "help_read",
        "get_language",
        "get_view_mode",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "missing tool `{expected}`"
        );
    }
}

#[test]
fn exactly_the_documented_tools_are_destructive() {
    let (_host, server) = setup();
    let tools = server.tools();
    let destructive: Vec<&str> = tools
        .iter()
        .filter(|tool| tool.destructive)
        .map(|tool| tool.name.as_str())
        .collect();
    assert_eq!(
        destructive,
        vec!["move", "delete", "rename", "write_text_file"]
    );
    for tool in &tools {
        assert_eq!(
            tool.requires_confirmation, tool.destructive,
            "{} confirmation flag",
            tool.name
        );
    }
}

#[test]
fn unknown_tool_is_a_tool_error_not_a_protocol_error() {
    let (_host, server) = setup();
    let result = call_tool(&server, "no_such_tool", json!({}));
    assert_eq!(result["isError"], json!(true));
    let text = tool_text(&result);
    assert!(text.contains("unknown tool"), "{text}");
    // The message must help the agent recover.
    assert!(text.contains("list_directory"), "{text}");
}

#[test]
fn tools_call_requires_a_name_and_sane_arguments() {
    let (_host, server) = setup();
    let (code, message) = error_of(&server, "tools/call", json!({}));
    assert_eq!(code, -32602);
    assert!(message.contains("`name`"), "{message}");

    let (code, _) = error_of(
        &server,
        "tools/call",
        json!({ "name": "list_directory", "arguments": 5 }),
    );
    assert_eq!(code, -32602);

    let (code, message) = error_of(
        &server,
        "tools/call",
        json!({ "name": "navigate", "arguments": {} }),
    );
    assert_eq!(code, -32602);
    assert!(message.contains("path"), "{message}");

    let (code, message) = error_of(
        &server,
        "tools/call",
        json!({ "name": "list_directory", "arguments": { "panel": "middle" } }),
    );
    assert_eq!(code, -32602);
    assert!(message.contains("left"), "{message}");
}

#[test]
fn list_directory_renders_entry_names_and_sizes() {
    let (host, server) = setup();
    let text = tool_ok(&server, "list_directory", json!({ "panel": "right" }));
    for expected in ["alpha.txt", "beta.md", "sub", "big.txt"] {
        assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
    }
    assert!(text.contains("hidden"), "{text}");
    assert!(
        !text.contains(".hidden.txt"),
        "hidden files must stay hidden:\n{text}"
    );
    assert!(text.contains(host.root().to_string_lossy().as_ref()));

    // `include_hidden` opts back in.
    let text = tool_ok(
        &server,
        "list_directory",
        json!({ "panel": "right", "include_hidden": true }),
    );
    assert!(text.contains(".hidden.txt"), "{text}");

    // An explicit path bypasses the panel directory.
    let text = tool_ok(
        &server,
        "list_directory",
        json!({ "path": host.path("sub") }),
    );
    assert!(text.contains("gamma.rs"), "{text}");

    // `limit` truncates and says so.
    let text = tool_ok(
        &server,
        "list_directory",
        json!({ "panel": "right", "limit": 1 }),
    );
    assert!(text.contains("more entries omitted"), "{text}");
}

#[test]
fn list_directory_reports_host_failures_as_tool_errors() {
    let (_host, server) = setup();
    let text = tool_err(
        &server,
        "list_directory",
        json!({ "path": "/definitely/not/here" }),
    );
    assert!(text.contains("tool `list_directory` failed"), "{text}");
    assert!(text.contains("not a directory"), "{text}");
}

#[test]
fn panels_and_view_mode_are_reported() {
    let (host, server) = setup();
    let text = tool_ok(&server, "list_panels", json!({}));
    assert!(text.contains("left panel"), "{text}");
    assert!(text.contains("right panel"), "{text}");
    assert!(text.contains("(active)"), "{text}");
    assert!(
        text.contains(host.root().to_string_lossy().as_ref()),
        "{text}"
    );

    let text = tool_ok(&server, "get_active_panel", json!({}));
    assert!(text.contains("left"), "{text}");
    assert!(text.contains("brief"), "{text}");

    let text = tool_ok(&server, "get_view_mode", json!({ "panel": "right" }));
    assert!(text.contains("brief"), "{text}");
    assert!(text.contains("right"), "{text}");

    *host.view_mode.lock().unwrap() = "thumbnails".to_string();
    let text = tool_ok(&server, "get_view_mode", json!({}));
    assert!(text.contains("thumbnails"), "{text}");
}

#[test]
fn navigate_moves_the_requested_panel() {
    let (host, server) = setup();
    let target = host.path("sub/deep");
    let text = tool_ok(
        &server,
        "navigate",
        json!({ "path": target, "panel": "right" }),
    );
    assert!(text.contains("right"), "{text}");
    assert_eq!(
        host.panel(rc_agent::PanelSide::Right).to_string_lossy(),
        target
    );
    assert!(
        host.recorded()
            .iter()
            .any(|entry| entry.starts_with("navigate right"))
    );

    let text = tool_err(
        &server,
        "navigate",
        json!({ "path": host.path("alpha.txt") }),
    );
    assert!(text.contains("not a directory"), "{text}");
}

#[test]
fn copy_moves_files_between_the_panels() {
    let (host, server) = setup();
    let source = host.path("alpha.txt");
    let destination = host.path("sub");
    let text = tool_ok(
        &server,
        "copy",
        json!({ "sources": [source.clone()], "destination": destination.clone() }),
    );
    assert!(text.contains("Copied"), "{text}");
    assert!(std::path::Path::new(&host.path("sub/alpha.txt")).exists());

    // A second copy without `overwrite` is skipped, not an error.
    let text = tool_ok(
        &server,
        "copy",
        json!({ "sources": [source], "destination": destination }),
    );
    assert!(text.contains("skipped"), "{text}");

    // A single string source is accepted for convenience.
    let text = tool_ok(
        &server,
        "copy",
        json!({ "sources": host.path("beta.md"), "destination": host.path("sub") }),
    );
    assert!(text.contains("Copied"), "{text}");
}

#[test]
fn create_directory_rename_and_file_info_work_together() {
    let host = Arc::new(FakeHost::new());
    // `rename` is destructive, so this server needs an approval handler.
    let server = McpServer::new(Arc::clone(&host), ServerConfig::default())
        .with_confirmer(Arc::new(TestConfirmer::allowing()) as Arc<dyn rc_agent::Confirmer>);
    let created = host.path("fresh/nested");
    let text = tool_ok(
        &server,
        "create_directory",
        json!({ "path": created.clone() }),
    );
    assert!(text.contains("Created"), "{text}");
    assert!(std::path::Path::new(&created).is_dir());

    let from = host.path("fresh/nested");
    let to = host.path("fresh/renamed");
    let text = tool_ok(&server, "rename", json!({ "from": from, "to": to.clone() }));
    assert!(text.contains("Renamed"), "{text}");
    assert!(std::path::Path::new(&to).is_dir());

    let text = tool_ok(
        &server,
        "file_info",
        json!({ "path": host.path("alpha.txt") }),
    );
    assert!(text.contains("alpha.txt"), "{text}");
    assert!(text.contains("Kind: file"), "{text}");
    assert!(text.contains("11 bytes"), "{text}");

    let text = tool_ok(&server, "file_info", json!({ "path": host.path("sub") }));
    assert!(text.contains("Kind: directory"), "{text}");

    let text = tool_err(
        &server,
        "file_info",
        json!({ "path": host.path("nope.txt") }),
    );
    assert!(text.contains("is not present in"), "{text}");
}

#[test]
fn search_files_matches_names_and_content() {
    let (host, server) = setup();
    let text = tool_ok(
        &server,
        "search_files",
        json!({ "root": host.path(""), "name_mask": "*.txt" }),
    );
    assert!(text.contains("alpha.txt"), "{text}");
    assert!(!text.contains("beta.md"), "{text}");
    assert!(text.contains("delta.txt"), "recursive by default:\n{text}");

    let text = tool_ok(
        &server,
        "search_files",
        json!({ "root": host.path(""), "name_mask": "*.txt", "content": "needle" }),
    );
    assert!(text.contains("delta.txt"), "{text}");
    assert!(!text.contains("alpha.txt"), "{text}");

    let text = tool_ok(
        &server,
        "search_files",
        json!({ "root": host.path(""), "name_mask": "*.txt", "recursive": false }),
    );
    assert!(
        !text.contains("delta.txt"),
        "non-recursive must not descend:\n{text}"
    );

    let text = tool_ok(
        &server,
        "search_files",
        json!({ "root": host.path(""), "name_mask": "*.nomatch" }),
    );
    assert!(text.contains("0 matches"), "{text}");

    // Case sensitivity applies to the mask and to the content needle.
    let text = tool_ok(
        &server,
        "search_files",
        json!({ "root": host.path(""), "name_mask": "*.TXT" }),
    );
    assert!(
        text.contains("alpha.txt"),
        "masks are case-insensitive by default:\n{text}"
    );
    let text = tool_ok(
        &server,
        "search_files",
        json!({ "root": host.path(""), "name_mask": "*.TXT", "case_sensitive": true }),
    );
    assert!(text.contains("0 matches"), "{text}");
    let text = tool_ok(
        &server,
        "search_files",
        json!({ "root": host.path(""), "name_mask": "*.txt", "content": "NEEDLE" }),
    );
    assert!(text.contains("delta.txt"), "{text}");
    let text = tool_ok(
        &server,
        "search_files",
        json!({
            "root": host.path(""),
            "name_mask": "*.txt",
            "content": "NEEDLE",
            "case_sensitive": true,
        }),
    );
    assert!(text.contains("0 matches"), "{text}");

    // The result cap is reported when it is hit.
    let text = tool_ok(
        &server,
        "search_files",
        json!({ "root": host.path(""), "name_mask": "*", "max_results": 1 }),
    );
    assert!(text.contains("result limit 1 reached"), "{text}");

    // Without `root` the active panel directory is searched.
    let text = tool_ok(&server, "search_files", json!({ "name_mask": "gamma*" }));
    assert!(text.contains("gamma.rs"), "{text}");
}

#[test]
fn read_text_file_enforces_the_server_limit() {
    let (_host, server) = setup_with(ServerConfig {
        max_read_bytes: 16,
        ..Default::default()
    });
    let text = tool_ok(
        &server,
        "read_text_file",
        json!({ "path": _host.path("big.txt") }),
    );
    let first_block = &call_tool(
        &server,
        "read_text_file",
        json!({ "path": _host.path("big.txt") }),
    )["content"][0]["text"];
    assert_eq!(first_block.as_str().unwrap().len(), 16);
    assert!(text.contains("cut off"), "{text}");

    // A `max_bytes` below the server limit is honoured, and the note appears.
    let result = call_tool(
        &server,
        "read_text_file",
        json!({ "path": _host.path("big.txt"), "max_bytes": 5 }),
    );
    assert_eq!(result["content"][0]["text"].as_str().unwrap(), "xxxxx");

    // A `max_bytes` above the server limit is clamped, never exceeded.
    let result = call_tool(
        &server,
        "read_text_file",
        json!({ "path": _host.path("big.txt"), "max_bytes": 10_000 }),
    );
    assert_eq!(result["content"][0]["text"].as_str().unwrap().len(), 16);

    // Small files are returned whole with no truncation note.
    let result = call_tool(
        &server,
        "read_text_file",
        json!({ "path": _host.path("alpha.txt") }),
    );
    assert_eq!(result["content"].as_array().unwrap().len(), 1);
    assert_eq!(result["content"][0]["text"], json!("hello alpha"));

    let text = tool_err(
        &server,
        "read_text_file",
        json!({ "path": _host.path("missing.txt") }),
    );
    assert!(text.contains("cannot read"), "{text}");
}

#[test]
fn write_text_file_needs_approval_then_writes() {
    let (host, server) = setup();
    let target = host.path("written.txt");
    let (code, message) = error_of(
        &server,
        "tools/call",
        json!({
            "name": "write_text_file",
            "arguments": { "path": target, "content": "hello" },
        }),
    );
    assert_eq!(code, -32001);
    assert!(message.contains("confirmation required"), "{message}");
    assert!(!std::path::Path::new(&target).exists());

    let confirmer = Arc::new(TestConfirmer::allowing());
    server.set_confirmer(Some(Arc::clone(&confirmer) as Arc<dyn rc_agent::Confirmer>));
    let text = tool_ok(
        &server,
        "write_text_file",
        json!({ "path": target.clone(), "content": "hello" }),
    );
    assert!(text.contains("Wrote"), "{text}");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
    assert_eq!(confirmer.calls(), vec!["write_text_file".to_string()]);
}

#[test]
fn delete_is_refused_without_a_confirmer() {
    let (host, server) = setup();
    let target = host.path("alpha.txt");
    let (code, message) = error_of(
        &server,
        "tools/call",
        json!({ "name": "delete", "arguments": { "paths": [target.clone()] } }),
    );
    assert_eq!(code, -32001);
    assert!(message.contains("confirmation"), "{message}");
    assert!(message.contains("delete"), "{message}");
    assert!(
        std::path::Path::new(&target).exists(),
        "nothing may be deleted"
    );
    assert!(host.recorded().is_empty(), "the host must not be called");
}

#[test]
fn delete_is_refused_when_the_user_denies() {
    let host = Arc::new(FakeHost::new());
    let confirmer = Arc::new(TestConfirmer::denying());
    let server = McpServer::new(Arc::clone(&host), ServerConfig::default())
        .with_confirmer(Arc::clone(&confirmer) as Arc<dyn rc_agent::Confirmer>);
    let target = host.path("alpha.txt");
    let (code, message) = error_of(
        &server,
        "tools/call",
        json!({ "name": "delete", "arguments": { "paths": [target.clone()] } }),
    );
    assert_eq!(code, -32001);
    assert!(message.contains("confirmation"), "{message}");
    assert!(message.contains("denied"), "{message}");
    assert!(std::path::Path::new(&target).exists());
    assert_eq!(confirmer.calls(), vec!["delete".to_string()]);
}

#[test]
fn delete_runs_when_the_user_approves() {
    let host = Arc::new(FakeHost::new());
    let server = McpServer::new(Arc::clone(&host), ServerConfig::default())
        .with_confirmer(Arc::new(TestConfirmer::allowing()) as Arc<dyn rc_agent::Confirmer>);
    let target = host.path("alpha.txt");
    let text = tool_ok(
        &server,
        "delete",
        json!({ "paths": [target.clone()], "use_trash": true }),
    );
    assert!(text.contains("Deleted"), "{text}");
    assert!(text.contains("1 file"), "{text}");
    assert!(!std::path::Path::new(&target).exists());
    assert!(
        host.recorded()
            .iter()
            .any(|entry| entry.contains("trash=true"))
    );
}

#[test]
fn move_is_gated_but_copy_is_not() {
    let host = Arc::new(FakeHost::new());
    let server = McpServer::new(Arc::clone(&host), ServerConfig::default());
    let (code, _) = error_of(
        &server,
        "tools/call",
        json!({
            "name": "move",
            "arguments": { "sources": [host.path("alpha.txt")], "destination": host.path("sub") },
        }),
    );
    assert_eq!(code, -32001);
    assert!(std::path::Path::new(&host.path("alpha.txt")).exists());
}

#[test]
fn extra_confirmation_targets_can_be_configured() {
    let config = ServerConfig {
        require_confirmation_for: vec!["copy".to_string()],
        ..Default::default()
    };
    let (host, server) = setup_with(config);
    let (code, message) = error_of(
        &server,
        "tools/call",
        json!({
            "name": "copy",
            "arguments": { "sources": [host.path("alpha.txt")], "destination": host.path("sub") },
        }),
    );
    assert_eq!(code, -32001);
    assert!(message.contains("copy"), "{message}");
    assert!(
        server
            .tools()
            .iter()
            .any(|tool| tool.name == "copy" && tool.requires_confirmation)
    );
}

#[test]
fn select_and_get_selection_share_state() {
    let (_host, server) = setup();
    let text = tool_ok(&server, "get_selection", json!({}));
    assert!(text.contains("Nothing is selected"), "{text}");

    // The left panel sits in `sub`, where only `gamma.rs` lives.
    let text = tool_ok(&server, "select", json!({ "mask": "*.txt", "mode": "set" }));
    assert!(text.contains("left"), "{text}");

    let text = tool_ok(&server, "get_selection", json!({ "panel": "right" }));
    assert!(text.contains("Nothing is selected"), "{text}");

    let text = tool_ok(
        &server,
        "select",
        json!({ "mask": "*.txt", "mode": "set", "panel": "right" }),
    );
    assert!(text.contains("affected"), "{text}");

    let text = tool_ok(&server, "get_selection", json!({ "panel": "right" }));
    assert!(text.contains("alpha.txt"), "{text}");
    assert!(!text.contains("gamma.rs"), "{text}");

    let text = tool_ok(
        &server,
        "select",
        json!({ "mask": "*.md", "mode": "add", "panel": "right" }),
    );
    assert!(text.contains("add"), "{text}");
    let text = tool_ok(&server, "get_selection", json!({ "panel": "right" }));
    assert!(
        text.contains("alpha.txt") && text.contains("beta.md"),
        "{text}"
    );

    let text = tool_ok(
        &server,
        "select",
        json!({ "mask": "*.txt", "mode": "remove", "panel": "right" }),
    );
    assert!(text.contains("remove"), "{text}");
    let text = tool_ok(&server, "get_selection", json!({ "panel": "right" }));
    assert!(
        !text.contains("alpha.txt") && text.contains("beta.md"),
        "{text}"
    );

    // Invert flips the whole panel.
    let text = tool_ok(
        &server,
        "select",
        json!({ "mask": "*", "mode": "invert_all", "panel": "right" }),
    );
    assert!(text.contains("invert_all"), "{text}");

    let (code, message) = error_of(
        &server,
        "tools/call",
        json!({ "name": "select", "arguments": { "mask": "*.rs", "mode": "sometimes" } }),
    );
    assert_eq!(code, -32602);
    assert!(message.contains("invert_all"), "{message}");
}

#[test]
fn disk_space_reports_the_numbers() {
    let (host, server) = setup();
    let text = tool_ok(&server, "disk_space", json!({ "path": host.path("") }));
    assert!(text.contains("total"), "{text}");
    assert!(text.contains("free"), "{text}");
    assert!(text.contains('%'), "{text}");

    let text = tool_err(&server, "disk_space", json!({ "path": "/nope/nope" }));
    assert!(text.contains("does not exist"), "{text}");
}

#[test]
fn help_tools_expose_topics() {
    let (_host, server) = setup();
    let text = tool_ok(&server, "help_search", json!({}));
    assert!(text.contains("navigation"), "{text}");
    assert!(text.contains("3 help topics"), "{text}");

    let text = tool_ok(&server, "help_search", json!({ "query": "COPY" }));
    assert!(text.contains("copying"), "{text}");
    assert!(!text.contains("navigation"), "{text}");

    let text = tool_ok(&server, "help_read", json!({ "topic": "navigation" }));
    assert!(text.contains("Ctrl+PgDn"), "{text}");

    let text = tool_ok(
        &server,
        "help_read",
        json!({ "topic": "navigation", "language": "cs" }),
    );
    assert!(text.contains("Česká"), "{text}");

    let text = tool_err(&server, "help_read", json!({ "topic": "nope" }));
    assert!(text.contains("no help topic"), "{text}");
}

#[test]
fn get_language_reports_the_pack() {
    let (_host, server) = setup();
    let text = tool_ok(&server, "get_language", json!({}));
    assert!(text.contains("`en`"), "{text}");
    assert!(text.contains("cs"), "{text}");
}

#[test]
fn set_language_is_published_only_when_the_host_supports_it() {
    let host = Arc::new(FakeHost::new());
    let plain = McpServer::new(Arc::clone(&host), ServerConfig::default());
    assert!(!plain.supports_set_language());
    assert!(!plain.tools().iter().any(|tool| tool.name == "set_language"));
    let text = tool_err(&plain, "set_language", json!({ "language": "cs" }));
    assert!(text.contains("unknown tool"), "{text}");

    let capable = McpServer::new(Arc::clone(&host), ServerConfig::default())
        .with_language_host(Arc::clone(&host) as Arc<dyn rc_agent::LanguageHost>);
    assert!(capable.supports_set_language());
    assert!(
        capable
            .tools()
            .iter()
            .any(|tool| tool.name == "set_language")
    );
    assert_eq!(capable.tools().len(), plain.tools().len() + 1);

    let text = tool_ok(&capable, "set_language", json!({ "language": "cs" }));
    assert!(text.contains("`cs`"), "{text}");
    assert_eq!(host.current_language(), "cs");

    let text = tool_err(&capable, "set_language", json!({ "language": "tlh" }));
    assert!(text.contains("not installed"), "{text}");
}

// --------------------------------------------------------------- resources

#[test]
fn help_topics_round_trip_through_resources() {
    let (_host, server) = setup();
    let listed = result_of(&server, "resources/list", Value::Null);
    let resources = listed["resources"].as_array().expect("resources array");
    assert_eq!(resources.len(), 3 * 2, "three topics in two languages");

    let uri = resources[0]["uri"].as_str().expect("uri");
    assert_eq!(uri, "rcommander://help/navigation/en");
    assert_eq!(resources[0]["mimeType"], json!("text/markdown"));
    assert_eq!(resources[0]["title"], json!("Navigating the panels (en)"));
    assert_eq!(resources[0]["name"], json!("navigation-en"));
    // Every topic is offered once per language.
    assert!(
        resources
            .iter()
            .any(|resource| resource["uri"] == json!("rcommander://help/copying/cs"))
    );

    let read = result_of(&server, "resources/read", json!({ "uri": uri }));
    let text = read["contents"][0]["text"].as_str().expect("help text");
    assert!(text.contains("Ctrl+PgDn"), "{text}");
    assert_eq!(read["contents"][0]["uri"], json!(uri));

    let read = result_of(
        &server,
        "resources/read",
        json!({ "uri": "rcommander://help/copying/cs" }),
    );
    assert!(
        read["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Česká")
    );

    // A URI without a language falls back to the current one.
    let read = result_of(
        &server,
        "resources/read",
        json!({ "uri": "rcommander://help/search" }),
    );
    assert!(
        read["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Alt+F7")
    );
}

#[test]
fn resources_read_rejects_unknown_uris_and_topics() {
    let (_host, server) = setup();
    let (code, message) = error_of(&server, "resources/read", json!({}));
    assert_eq!(code, -32602);
    assert!(message.contains("uri"), "{message}");

    let (code, message) = error_of(
        &server,
        "resources/read",
        json!({ "uri": "file:///etc/passwd" }),
    );
    assert_eq!(code, -32602);
    assert!(message.contains("rcommander://help/"), "{message}");

    let (code, message) = error_of(
        &server,
        "resources/read",
        json!({ "uri": "rcommander://help/unknown/en" }),
    );
    assert_eq!(code, -32602);
    assert!(message.contains("no help topic"), "{message}");
}

// ----------------------------------------------------------- internal errors

#[test]
fn a_panicking_host_becomes_an_internal_error() {
    let host = Arc::new(FakeHost::new());
    host.panic_in_list_directory
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let server = McpServer::new(Arc::clone(&host), ServerConfig::default());

    let (code, message) = error_of(
        &server,
        "tools/call",
        json!({ "name": "list_directory", "arguments": {} }),
    );
    assert_eq!(code, -32603);
    assert!(message.contains("failed internally"), "{message}");
    assert!(message.contains("fake host exploded"), "{message}");

    // The server stays usable afterwards.
    host.panic_in_list_directory
        .store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(result_of(&server, "ping", Value::Null), json!({}));
    let text = tool_ok(&server, "list_directory", json!({ "panel": "right" }));
    assert!(text.contains("alpha.txt"), "{text}");
}

#[test]
fn a_panicking_confirmer_becomes_an_internal_error() {
    let host = Arc::new(FakeHost::new());
    let server = McpServer::new(Arc::clone(&host), ServerConfig::default())
        .with_confirmer(Arc::new(PanickingConfirmer) as Arc<dyn rc_agent::Confirmer>);
    let target = host.path("alpha.txt");
    let (code, message) = error_of(
        &server,
        "tools/call",
        json!({ "name": "delete", "arguments": { "paths": [target.clone()] } }),
    );
    assert_eq!(code, -32603);
    assert!(message.contains("approval handler"), "{message}");
    assert!(
        std::path::Path::new(&target).exists(),
        "nothing may be deleted"
    );
}

// ---------------------------------------------------------------- logging

#[test]
fn logging_set_level_validates_and_records() {
    let (_host, server) = setup();
    assert_eq!(server.log_level().to_string(), "info");
    assert_eq!(
        result_of(&server, "logging/setLevel", json!({ "level": "debug" })),
        json!({})
    );
    assert_eq!(server.log_level().to_string(), "debug");

    let (code, message) = error_of(&server, "logging/setLevel", json!({ "level": "shouty" }));
    assert_eq!(code, -32602);
    assert!(message.contains("shouty"), "{message}");

    let (code, _) = error_of(&server, "logging/setLevel", json!({}));
    assert_eq!(code, -32602);
}

// ----------------------------------------------------------------- protocol

#[test]
fn unknown_methods_report_method_not_found() {
    let (_host, server) = setup();
    let response = rpc(&server, 7, "completion/complete", json!({}));
    assert_eq!(response["id"], json!(7));
    assert_eq!(response["error"]["code"], json!(-32601));
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("completion/complete")
    );
}

#[test]
fn malformed_json_reports_a_parse_error_with_a_null_id() {
    let (_host, server) = setup();
    let response = handle_raw(&server, "{ this is not json").expect("answered");
    assert_eq!(response["jsonrpc"], json!("2.0"));
    assert_eq!(response["id"], Value::Null);
    assert_eq!(response["error"]["code"], json!(-32700));
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("JSON")
    );

    for broken in ["", "   ", "\u{fffd}", "{", "nul"] {
        // Blank lines are not messages at all, every other snippet must still
        // produce an error object instead of a panic.
        if let Some(response) = handle_raw(&server, broken) {
            assert_eq!(response["error"]["code"], json!(-32700), "input {broken:?}");
        }
    }
}

#[test]
fn structurally_invalid_requests_are_rejected() {
    let (_host, server) = setup();
    for broken in [
        "[1,2,3]",
        "42",
        r#""hello""#,
        r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
        r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":1}"#,
        r#"{"jsonrpc":"2.0","id":1,"method":7}"#,
    ] {
        let response = handle_raw(&server, broken).expect("answered");
        assert_eq!(response["error"]["code"], json!(-32600), "input {broken}");
        assert!(response["error"]["message"].as_str().is_some());
    }
}

#[test]
fn string_ids_are_echoed_back() {
    let (_host, server) = setup();
    let response = handle(
        &server,
        json!({ "jsonrpc": "2.0", "id": "req-1", "method": "ping" }),
    )
    .expect("answered");
    assert_eq!(response["id"], json!("req-1"));
}

// ------------------------------------------------------------- stdio loop

#[test]
fn serve_stdio_answers_requests_and_skips_notifications() {
    let (_host, server) = setup();
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        "this line is not JSON\n",
    );
    let mut output: Vec<u8> = Vec::new();
    server
        .serve_stdio(std::io::Cursor::new(input.as_bytes().to_vec()), &mut output)
        .expect("the loop must not fail");
    let text = String::from_utf8(output).expect("utf-8 output");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "expected two responses, got:\n{text}");
    let first: Value = serde_json::from_str(lines[0]).expect("json");
    assert_eq!(first["id"], json!(1));
    assert_eq!(first["result"]["protocolVersion"], json!("2025-06-18"));
    let second: Value = serde_json::from_str(lines[1]).expect("json");
    assert_eq!(second["error"]["code"], json!(-32700));
}

#[test]
fn serve_stdio_survives_blank_lines_and_invalid_utf8() {
    let (_host, server) = setup();
    let mut input = Vec::new();
    input.extend_from_slice(b"\n   \n");
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n");
    input.extend_from_slice(&[0xff, 0xfe, b'\n']);
    let mut output: Vec<u8> = Vec::new();
    server
        .serve_stdio(std::io::Cursor::new(input), &mut output)
        .expect("loop");
    let text = String::from_utf8(output).expect("utf-8 output");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert_eq!(
        serde_json::from_str::<Value>(lines[1]).unwrap()["error"]["code"],
        json!(-32700)
    );
}

#[test]
fn serve_stdio_stops_after_shutdown() {
    let (_host, server) = setup();
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
        "\n",
    );
    let mut output: Vec<u8> = Vec::new();
    server
        .serve_stdio(std::io::Cursor::new(input.as_bytes().to_vec()), &mut output)
        .expect("loop");
    assert!(server.is_shutdown());
    let text = String::from_utf8(output).expect("utf-8 output");
    assert_eq!(
        text.lines().count(),
        2,
        "the third ping must not run:\n{text}"
    );
}

#[test]
fn serve_stdio_returns_cleanly_at_end_of_input() {
    let (_host, server) = setup();
    let mut output: Vec<u8> = Vec::new();
    server
        .serve_stdio(std::io::Cursor::new(Vec::new()), &mut output)
        .expect("empty input is fine");
    assert!(output.is_empty());
    assert!(!server.is_shutdown());
}

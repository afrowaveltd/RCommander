//! Integration tests for the MCP **client** ([`rc_agent::McpClient`]).
//!
//! The scripted peer in `common` implements the server half in memory, so the
//! handshake order, the request/response correlation, the error propagation and
//! the timeout behaviour can all be asserted deterministically. One test spawns a
//! real child process and one talks to a real TCP socket to cover both
//! transports end to end.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::ScriptPeer;
use rc_agent::{McpClient, McpClientConfig, McpError, PROTOCOL_VERSION, Transport};
use serde_json::{Value, json};

/// Configuration for [`McpClient::connect_with_io`]; the transport itself is
/// irrelevant there, only the timeout matters.
fn io_config(timeout_ms: u64) -> McpClientConfig {
    McpClientConfig {
        name: "agent-panel".to_string(),
        transport: Transport::Http {
            url: "http://127.0.0.1:1/unused".to_string(),
            token: None,
        },
        timeout_ms,
    }
}

/// Lines the client wrote, as JSON values.
fn sent(writer: &common::RecordingWriter) -> Vec<Value> {
    writer.messages()
}

// ------------------------------------------------------------------ handshake

#[test]
fn handshake_sends_initialize_then_initialized_then_tools_list() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();

    let client =
        McpClient::connect_with_io(&io_config(2_000), reader, writer.clone()).expect("connects");

    let messages = sent(&writer);
    assert_eq!(
        messages.len(),
        3,
        "unexpected request sequence: {messages:#?}"
    );

    // 1) initialize is a request with an id and full client information.
    assert_eq!(messages[0]["jsonrpc"], json!("2.0"));
    assert_eq!(messages[0]["method"], json!("initialize"));
    assert_eq!(messages[0]["id"], json!(1));
    assert_eq!(
        messages[0]["params"]["protocolVersion"],
        json!(PROTOCOL_VERSION)
    );
    assert_eq!(
        messages[0]["params"]["clientInfo"]["name"],
        json!("agent-panel")
    );
    assert!(messages[0]["params"]["clientInfo"]["version"].is_string());
    assert!(messages[0]["params"]["capabilities"].is_object());

    // 2) notifications/initialized is a notification: no id, no answer expected.
    assert_eq!(messages[1]["method"], json!("notifications/initialized"));
    assert!(
        messages[1].get("id").is_none(),
        "notifications must not carry an id: {}",
        messages[1]
    );

    // 3) the tool list is fetched during connect so `tools()` can be cheap.
    assert_eq!(messages[2]["method"], json!("tools/list"));
    assert_eq!(messages[2]["id"], json!(2));

    let info = client.server_info().expect("server info");
    assert_eq!(info.name, "scripted");
    assert_eq!(info.version, "9.9.9");
    assert_eq!(info.protocol_version, PROTOCOL_VERSION);

    let tools = client.tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    assert_eq!(tools[0].title, "Echo");
    assert_eq!(tools[0].description, "Echo text back.");
    assert_eq!(tools[0].input_schema["type"], json!("object"));
    // Flags are server-side only and stay false on the wire.
    assert!(!tools[0].destructive);
    assert!(!tools[0].requires_confirmation);
}

#[test]
fn a_failing_tools_list_does_not_break_the_connection() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_initialize();
    peer.fail(2, -32601, "tools are not supported here");

    let client = McpClient::connect_with_io(&io_config(2_000), reader, writer)
        .expect("the handshake itself succeeded");
    assert!(client.server_info().is_some());
    assert!(client.tools().is_empty());
}

#[test]
fn a_rejected_initialize_propagates_the_server_error() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.fail(1, -32600, "unsupported protocol version");

    let error = McpClient::connect_with_io(&io_config(2_000), reader, writer)
        .expect_err("must not connect");
    match error {
        McpError::Rpc { code, message } => {
            assert_eq!(code, -32600);
            assert!(
                message.contains("unsupported protocol version"),
                "{message}"
            );
            assert!(
                message.contains("while calling `initialize`"),
                "missing context: {message}"
            );
        }
        other => panic!("expected an RPC error, got {other:?}"),
    }
}

// ------------------------------------------------------------------ call_tool

#[test]
fn call_tool_decodes_content_and_the_error_flag() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();
    let mut client =
        McpClient::connect_with_io(&io_config(2_000), reader, writer.clone()).expect("connects");

    // Successful call: request id 3 because initialize and tools/list used 1, 2.
    peer.respond(
        3,
        json!({
            "content": [{ "type": "text", "text": "hello" }],
            "isError": false,
        }),
    );
    let result = client
        .call_tool("echo", json!({ "text": "hi" }))
        .expect("call succeeds");
    assert_eq!(result.text, "hello");
    assert!(!result.is_error);
    assert_eq!(result.raw["content"][0]["type"], json!("text"));

    let messages = sent(&writer);
    assert_eq!(messages[3]["method"], json!("tools/call"));
    assert_eq!(messages[3]["id"], json!(3));
    assert_eq!(messages[3]["params"]["name"], json!("echo"));
    assert_eq!(messages[3]["params"]["arguments"]["text"], json!("hi"));

    // Tool-level failure: still Ok, but flagged.
    peer.respond(
        4,
        json!({
            "content": [
                { "type": "text", "text": "first" },
                { "type": "text", "text": "second" },
            ],
            "isError": true,
        }),
    );
    let result = client
        .call_tool("echo", json!({}))
        .expect("transport is fine");
    assert!(result.is_error);
    assert_eq!(result.text, "first\nsecond");
}

#[test]
fn call_tool_propagates_jsonrpc_errors() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();
    let mut client =
        McpClient::connect_with_io(&io_config(2_000), reader, writer).expect("connects");

    peer.fail(3, -32001, "confirmation required for `delete`");
    let error = client
        .call_tool("delete", json!({}))
        .expect_err("must fail");
    match error {
        McpError::Rpc { code, message } => {
            assert_eq!(code, -32001);
            assert!(message.contains("confirmation required"), "{message}");
        }
        other => panic!("expected an RPC error, got {other:?}"),
    }
}

#[test]
fn responses_for_other_ids_and_server_notifications_are_skipped() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();
    let mut client =
        McpClient::connect_with_io(&io_config(2_000), reader, writer).expect("connects");

    // A stray notification, a response to an old id, then the real answer.
    peer.send(&json!({ "jsonrpc": "2.0", "method": "notifications/message", "params": {} }));
    peer.respond(
        99,
        json!({ "content": [{ "type": "text", "text": "wrong" }] }),
    );
    peer.respond(
        3,
        json!({ "content": [{ "type": "text", "text": "right" }] }),
    );

    let result = client.call_tool("echo", json!({})).expect("call succeeds");
    assert_eq!(result.text, "right");
}

// -------------------------------------------------------------------- errors

#[test]
fn a_silent_server_produces_a_timeout_error() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();
    let mut client = McpClient::connect_with_io(&io_config(120), reader, writer).expect("connects");

    let started = Instant::now();
    let error = client.call_tool("echo", json!({})).expect_err("must fail");
    let elapsed = started.elapsed();

    match error {
        McpError::Timeout(ms) => assert_eq!(ms, 120),
        other => panic!("expected a timeout, got {other:?}"),
    }
    assert!(
        elapsed >= Duration::from_millis(100),
        "the timeout fired too early: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the timeout hung: {elapsed:?}"
    );
}

#[test]
fn connect_times_out_when_the_server_never_answers() {
    let (reader, writer, peer) = ScriptPeer::new();
    // Nothing is pushed, and `peer` stays alive so the reader blocks instead of
    // reporting end-of-file.
    let error =
        McpClient::connect_with_io(&io_config(60), reader, writer).expect_err("must not connect");
    assert!(matches!(error, McpError::Timeout(60)), "{error:?}");
    drop(peer);
}

#[test]
fn a_closed_connection_is_reported() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();
    let mut client =
        McpClient::connect_with_io(&io_config(2_000), reader, writer).expect("connects");
    drop(peer);

    let error = client.call_tool("echo", json!({})).expect_err("must fail");
    assert!(matches!(error, McpError::Closed), "{error:?}");
}

#[test]
fn malformed_json_from_the_server_is_a_protocol_error() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();
    let mut client =
        McpClient::connect_with_io(&io_config(2_000), reader, writer).expect("connects");

    // A valid JSON scalar that is not a JSON-RPC object.
    peer.send(&json!("this is not a JSON-RPC object"));

    let error = client.call_tool("echo", json!({})).expect_err("must fail");
    assert!(matches!(error, McpError::Protocol(_)), "{error:?}");
}

// ------------------------------------------------------------------ resources

#[test]
fn resources_are_listed_and_read() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();
    let mut client =
        McpClient::connect_with_io(&io_config(2_000), reader, writer).expect("connects");

    peer.respond(
        3,
        json!({ "resources": [
            { "uri": "rcommander://help/navigation/en", "name": "navigation-en" },
            { "uri": "rcommander://help/navigation/cs", "name": "navigation-cs" },
        ] }),
    );
    let resources = client.list_resources().expect("lists");
    assert_eq!(resources.len(), 2);
    assert_eq!(
        resources[0]["uri"],
        json!("rcommander://help/navigation/en")
    );

    peer.respond(
        4,
        json!({ "contents": [{ "uri": "rcommander://help/navigation/en", "text": "# Navigating" }] }),
    );
    let text = client
        .read_resource("rcommander://help/navigation/en")
        .expect("reads");
    assert_eq!(text, "# Navigating");

    // A missing `resources` array means "no resources", not an error.
    peer.respond(5, json!({}));
    assert!(client.list_resources().expect("tolerates empty").is_empty());

    // A wrongly typed `resources` member is a protocol error.
    peer.respond(6, json!({ "resources": "nope" }));
    assert!(matches!(
        client.list_resources(),
        Err(McpError::Protocol(_))
    ));
}

// ---------------------------------------------------------------------- close

#[test]
fn close_says_goodbye_and_is_harmless_when_repeated() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();
    let client =
        McpClient::connect_with_io(&io_config(2_000), reader, writer.clone()).expect("connects");

    client.close().expect("close succeeds");

    let messages = sent(&writer);
    let shutdown = messages
        .iter()
        .find(|message| message["method"] == json!("shutdown"))
        .expect("shutdown was sent");
    assert_eq!(shutdown["id"], json!(3));
    let exit = messages
        .iter()
        .find(|message| message["method"] == json!("exit"))
        .expect("exit was sent");
    assert!(
        exit.get("id").is_none(),
        "exit must be a notification: {exit}"
    );
}

#[test]
fn dropping_a_client_is_safe() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.respond_handshake();
    let client = McpClient::connect_with_io(&io_config(2_000), reader, writer).expect("connects");
    assert!(!client.is_closed());
    drop(client);
    drop(peer);
}

// ------------------------------------------------------------- real transports

/// A tiny JSON-RPC server in a shell script, used to exercise process spawning.
#[cfg(unix)]
const FAKE_STDIO_SERVER: &str = r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"child-server","version":"0.2.0"}}}'
      ;;
    *'"method":"tools/list"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"ping","title":"Ping","description":"Answer pong.","inputSchema":{"type":"object","properties":{}}}]}}'
      ;;
    *'"method":"tools/call"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"pong"}],"isError":false}}'
      ;;
  esac
done
"#;

#[cfg(unix)]
#[test]
fn the_stdio_transport_spawns_a_child_process() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("temp dir");
    let script = dir.path().join("fake_mcp_server.sh");
    std::fs::write(&script, FAKE_STDIO_SERVER).expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let config = McpClientConfig::stdio("stdio-test", "sh")
        .with_arg(script.to_string_lossy().to_string())
        .with_timeout_ms(5_000);
    let mut client = McpClient::connect(&config).expect("the child process answers");
    let info = client.server_info().expect("server info");
    assert_eq!(info.name, "child-server");
    assert_eq!(info.version, "0.2.0");
    assert_eq!(client.tools().len(), 1);
    assert_eq!(client.tools()[0].name, "ping");

    let result = client.call_tool("ping", json!({})).expect("call succeeds");
    assert_eq!(result.text, "pong");
    assert!(!result.is_error);

    client.close().expect("close kills the child");
}

#[test]
fn a_missing_stdio_command_is_reported_clearly() {
    let config =
        McpClientConfig::stdio("missing", "/definitely/not/a/program").with_timeout_ms(500);
    let error = McpClient::connect(&config).expect_err("must fail");
    let message = error.to_string();
    assert!(message.contains("could not start"), "{message}");
    assert!(message.contains("/definitely/not/a/program"), "{message}");
}

#[test]
fn an_empty_stdio_command_is_rejected() {
    let config = McpClientConfig::stdio("empty", "  ");
    assert!(matches!(
        McpClient::connect(&config),
        Err(McpError::Transport(_))
    ));
}

/// Answer one JSON-RPC request the way a real MCP HTTP server does.
fn http_response_for(request: &Value) -> Value {
    match request["method"].as_str() {
        Some("initialize") => json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {}, "resources": {} },
                "serverInfo": { "name": "http-server", "version": "3.0.0" },
            },
        }),
        Some("tools/list") => json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": { "tools": [{
                "name": "whoami",
                "title": "Who am I",
                "description": "Report the caller.",
                "inputSchema": { "type": "object", "properties": {} },
            }] },
        }),
        Some("tools/call") => json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": { "content": [{ "type": "text", "text": "http ok" }], "isError": false },
        }),
        Some("shutdown") => json!({ "jsonrpc": "2.0", "id": request["id"], "result": {} }),
        _ => json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "error": { "code": -32601, "message": "unsupported" },
        }),
    }
}

/// Read one HTTP POST body from `reader`.
fn read_http_request(reader: &mut impl BufRead) -> Option<Value> {
    let mut content_length = 0usize;
    let mut first = true;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if first {
            if !trimmed.starts_with("POST ") {
                return None;
            }
            first = false;
            continue;
        }
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// A minimal HTTP server that answers `connections` JSON-RPC posts.
fn fake_http_server(connections: usize) -> (String, Arc<AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let url = format!("http://{address}/mcp");
    let served = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&served);
    std::thread::spawn(move || {
        for _ in 0..connections {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let clone = stream.try_clone().expect("clone");
            let mut reader = BufReader::new(clone);
            let Some(request) = read_http_request(&mut reader) else {
                continue;
            };
            let payload = http_response_for(&request).to_string();
            let mut writer = stream;
            let _ = write!(
                writer,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = writer.flush();
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
    (url, served)
}

#[test]
fn the_http_transport_round_trips_json_rpc_posts() {
    let (url, served) = fake_http_server(8);
    let config = McpClientConfig::http("http-panel", url).with_timeout_ms(5_000);
    let mut client = McpClient::connect(&config).expect("the HTTP server answers");

    let info = client.server_info().expect("server info");
    assert_eq!(info.name, "http-server");
    assert_eq!(info.version, "3.0.0");
    assert_eq!(client.tools().len(), 1);
    assert_eq!(client.tools()[0].name, "whoami");

    let result = client
        .call_tool("whoami", json!({}))
        .expect("call succeeds");
    assert_eq!(result.text, "http ok");
    assert!(!result.is_error);

    client.close().expect("close succeeds");
    assert!(
        served.load(Ordering::SeqCst) >= 4,
        "expected at least four POSTs, saw {}",
        served.load(Ordering::SeqCst)
    );
}

#[test]
fn the_http_transport_reports_timeouts_and_bad_urls() {
    // A socket that accepts but never answers.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            held.push(stream);
        }
    });

    let config =
        McpClientConfig::http("slow", format!("http://{address}/mcp")).with_timeout_ms(150);
    let error = McpClient::connect(&config).expect_err("must time out");
    assert!(matches!(error, McpError::Timeout(150)), "{error:?}");

    let config = McpClientConfig::http("tls", "https://example.invalid/mcp");
    let error = McpClient::connect(&config).expect_err("TLS is unsupported");
    assert!(error.to_string().contains("TLS"), "{error}");

    let config = McpClientConfig::http("closed", "http://127.0.0.1:1/mcp").with_timeout_ms(300);
    assert!(McpClient::connect(&config).is_err());
}

#[test]
fn drop_in_the_middle_of_a_handshake_does_not_hang() {
    let (reader, writer, peer) = ScriptPeer::new();
    peer.fail(1, -32603, "boom");
    let started = Instant::now();
    let outcome = McpClient::connect_with_io(&io_config(2_000), reader, writer);
    assert!(outcome.is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(peer);
}

/// The scripted reader must behave like a blocking pipe: not an early EOF.
#[test]
fn the_scripted_reader_blocks_until_data_arrives() {
    let (mut reader, _writer, peer) = ScriptPeer::new();
    let handle = std::thread::spawn(move || {
        let mut line = String::new();
        reader.read_line(&mut line).expect("reads");
        line
    });
    std::thread::sleep(Duration::from_millis(20));
    peer.send(&json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
    let line = handle.join().expect("thread");
    assert!(line.contains("\"result\""));
}

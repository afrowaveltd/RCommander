//! Provider tests against an in-process HTTP server: no network, no Ollama needed.

use super::*;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// A one-shot HTTP server that answers each request with a canned response.
pub struct FakeServer {
    pub base_url: String,
    pub requests: Arc<Mutex<Vec<String>>>,
}

impl FakeServer {
    /// Serve `routes`: `(path, status, body)`; unknown paths answer 404.
    pub fn start(routes: Vec<(String, u16, String)>) -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                    continue;
                }
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(value) = lower.strip_prefix("content-length:") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; content_length];
                if content_length > 0 {
                    use std::io::Read;
                    let _ = reader.read_exact(&mut body);
                }
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                log.lock()
                    .unwrap()
                    .push(format!("{path} {}", String::from_utf8_lossy(&body)));
                let (status, response) = routes
                    .iter()
                    .find(|(route, _, _)| *route == path)
                    .map(|(_, status, body)| (*status, body.clone()))
                    .unwrap_or((404, "{\"error\":\"not found\"}".to_string()));
                let reply = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                );
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
            }
        });
        FakeServer {
            base_url: format!("http://{address}"),
            requests,
        }
    }
}

const TAGS: &str = r#"{"models":[
 {"name":"qwen2.5-coder:7b","size":4700000000,"details":{"parameter_size":"7.6B"},"capabilities":["completion","tools"]},
 {"name":"llama3.2:3b","size":2000000000,"details":{"parameter_size":"3.2B"},"capabilities":["completion"]},
 {"name":"","size":0}
]}"#;

const CHAT: &str = r#"{"model":"qwen2.5-coder:7b","message":{"role":"assistant","content":"Hello from the model"},"done_reason":"stop","prompt_eval_count":12,"eval_count":5}"#;

#[test]
fn models_are_listed_with_their_capabilities() {
    let server = FakeServer::start(vec![("/api/tags".to_string(), 200, TAGS.to_string())]);
    let provider = OllamaProvider::new(&server.base_url);
    let models = provider.list_models().unwrap();
    assert_eq!(models.len(), 2, "the nameless entry is dropped");
    assert_eq!(models[0].id, "qwen2.5-coder:7b");
    assert!(models[0].supports_tools());
    assert_eq!(models[0].parameters.as_deref(), Some("7.6B"));
    assert!(!models[1].supports_tools());
    assert!(models[0].describe().contains("7.6B"));
    assert_eq!(provider.kind(), "ollama");
    assert!(provider.is_available());
}

#[test]
fn a_question_is_answered_and_the_request_shape_is_right() {
    let server = FakeServer::start(vec![("/api/chat".to_string(), 200, CHAT.to_string())]);
    let provider = OllamaProvider::new(&server.base_url);
    let request = ChatRequest::new(
        "qwen2.5-coder:7b",
        vec![
            Message::system("You are terse."),
            Message::user("Say hello"),
        ],
    )
    .temperature(0.2);
    let reply = provider.chat(&request).unwrap();
    assert_eq!(reply.content, "Hello from the model");
    assert_eq!(reply.done_reason.as_deref(), Some("stop"));
    assert_eq!(reply.answer_tokens, Some(5));

    let sent = server.requests.lock().unwrap().clone();
    let body = &sent[0];
    assert!(body.starts_with("/api/chat "), "{body}");
    assert!(body.contains("\"model\":\"qwen2.5-coder:7b\""), "{body}");
    assert!(body.contains("\"role\":\"system\""), "{body}");
    assert!(body.contains("\"stream\":false"), "{body}");
    assert!(body.contains("\"temperature\":0.2"), "{body}");
}

#[test]
fn a_server_that_is_not_there_explains_itself() {
    // Port 1 on the loopback interface refuses connections.
    let provider = OllamaProvider::new("http://127.0.0.1:1").with_timeout(Duration::from_secs(2));
    let error = provider.list_models().unwrap_err();
    assert!(error.to_string().contains("cannot reach"), "{error}");
    assert!(!provider.is_available());
}

#[test]
fn an_http_error_and_a_provider_error_are_reported() {
    let server = FakeServer::start(vec![("/api/tags".to_string(), 500, "{}".to_string())]);
    let provider = OllamaProvider::new(&server.base_url);
    assert!(
        provider
            .list_models()
            .unwrap_err()
            .to_string()
            .contains("500")
    );

    let failing = FakeServer::start(vec![(
        "/api/chat".to_string(),
        200,
        r#"{"error":"model 'nope' not found"}"#.to_string(),
    )]);
    let provider = OllamaProvider::new(&failing.base_url);
    let error = provider
        .chat(&ChatRequest::new("nope", vec![Message::user("hi")]))
        .unwrap_err();
    assert!(error.to_string().contains("not found"), "{error}");
}

#[test]
fn malformed_json_is_an_error_not_a_panic() {
    let server = FakeServer::start(vec![("/api/tags".to_string(), 200, "not json".to_string())]);
    let provider = OllamaProvider::new(&server.base_url);
    assert!(provider.list_models().is_err());

    assert!(parse_models("").is_err());
    assert!(
        parse_chat("{\"model\": ").is_err(),
        "truncated JSON is refused"
    );
    assert!(parse_chat("").is_err());
    // A well-formed answer without a message body is empty, not a failure.
    let reply = parse_chat(r#"{"model":"m"}"#).unwrap();
    assert_eq!(reply.content, "");
}

#[test]
fn a_sizes_and_roles_are_rendered_for_the_picker() {
    assert!(format_size(1024).contains("1.0 KB"));
    assert!(format_size(5_000_000_000).contains("GB"));
    assert_eq!(Role::System.as_str(), "system");
    assert_eq!(Role::User.as_str(), "user");
    assert_eq!(Role::Assistant.as_str(), "assistant");
    let model = Model {
        id: "m".into(),
        size: None,
        parameters: None,
        capabilities: Vec::new(),
    };
    assert_eq!(model.describe(), "m");
}

#[test]
fn the_local_ollama_is_probed_when_one_is_running() {
    // Not a unit test: it only asserts something when a real Ollama answers. It never fails
    // when there is none, so the suite stays offline.
    let provider = OllamaProvider::default().with_timeout(Duration::from_millis(800));
    if let Ok(models) = provider.list_models() {
        println!("local Ollama offers {} model(s)", models.len());
        for model in models.iter().take(8) {
            println!("  {}", model.describe());
        }
    }
}

//! Language-model providers (Phase 16.2).
//!
//! The provider is *not* an MCP server: MCP supplies tools, a provider supplies conversation.
//! This crate therefore only knows how to ask a model something and how to list what is
//! available — nothing here can touch the file system.
//!
//! Ollama comes first because it needs no account, no key and no network beyond the loopback
//! interface, which also makes it the provider the tests use. OpenAI-compatible endpoints share
//! the request shape, so they slot in behind the same trait.

pub mod http;

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Timeout for one request; a local model can be slow to start, a hung one must not hang us.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// A failure of either the transport or the provider's protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlmError {
    pub message: String,
}

impl LlmError {
    pub fn new(message: impl Into<String>) -> Self {
        LlmError {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LlmError {}

/// One model a provider offers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    /// Identifier used in requests (`qwen2.5-coder:7b`).
    pub id: String,
    /// Size on disk in bytes, when the provider reports it.
    pub size: Option<u64>,
    /// Parameter count as text (`7B`), when known.
    pub parameters: Option<String>,
    /// What the model can do (`completion`, `tools`, `vision`, `thinking`).
    pub capabilities: Vec<String>,
}

impl Model {
    /// True when the model can call tools, which the agent panel needs.
    pub fn supports_tools(&self) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability == "tools")
    }

    /// `name (7B, tools)` — one line for a picker.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(parameters) = &self.parameters {
            parts.push(parameters.clone());
        }
        if let Some(size) = self.size
            && size > 0
        {
            parts.push(format_size(size));
        }
        if !self.capabilities.is_empty() {
            parts.push(self.capabilities.join("/"));
        }
        if parts.is_empty() {
            self.id.clone()
        } else {
            format!("{}  ({})", self.id, parts.join(", "))
        }
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Who said what.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// One message of a conversation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Message {
        Message {
            role: Role::System,
            content: text.into(),
        }
    }

    pub fn user(text: impl Into<String>) -> Message {
        Message {
            role: Role::User,
            content: text.into(),
        }
    }

    pub fn assistant(text: impl Into<String>) -> Message {
        Message {
            role: Role::Assistant,
            content: text.into(),
        }
    }
}

/// What to ask the model.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    /// Ask for a lower temperature for code work, higher for prose.
    pub temperature: Option<f32>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> ChatRequest {
        ChatRequest {
            model: model.into(),
            messages,
            temperature: None,
        }
    }

    pub fn temperature(mut self, value: f32) -> Self {
        self.temperature = Some(value);
        self
    }
}

/// What the model answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatReply {
    pub content: String,
    pub model: String,
    /// Why generation stopped (`stop`, `length`, …), when the provider says.
    pub done_reason: Option<String>,
    /// Tokens the provider counted, when it reports them.
    pub prompt_tokens: Option<u64>,
    pub answer_tokens: Option<u64>,
}

/// A connection to one model server.
pub trait Provider: std::fmt::Debug + Send + Sync {
    /// Provider name used in the settings (`ollama`).
    fn kind(&self) -> &'static str;
    /// Base URL this provider talks to.
    fn base_url(&self) -> &str;
    /// Models the server currently offers.
    fn list_models(&self) -> Result<Vec<Model>, LlmError>;
    /// Ask one question.
    fn chat(&self, request: &ChatRequest) -> Result<ChatReply, LlmError>;

    /// Ask one question and hand every piece of the answer to `on_chunk` as it arrives.
    ///
    /// The default implementation falls back to a non-streaming call, so a provider that cannot
    /// stream still works — the caller just sees one big chunk.
    fn chat_stream(
        &self,
        request: &ChatRequest,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<ChatReply, LlmError> {
        let reply = self.chat(request)?;
        on_chunk(&reply.content);
        Ok(reply)
    }
    /// True when the server answers at all (used to enable the UI).
    fn is_available(&self) -> bool {
        self.list_models().is_ok()
    }
}

/// The Ollama provider (plain HTTP on the loopback interface by default).
#[derive(Clone, Debug)]
pub struct OllamaProvider {
    base_url: String,
    timeout: Duration,
}

impl Default for OllamaProvider {
    fn default() -> Self {
        OllamaProvider::new("http://127.0.0.1:11434")
    }
}

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>) -> OllamaProvider {
        OllamaProvider {
            base_url: base_url.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn target(&self) -> Result<http::Target, LlmError> {
        http::Target::parse(&self.base_url).map_err(|error| LlmError::new(error.to_string()))
    }

    fn get(&self, path: &str) -> Result<http::Response, LlmError> {
        let target = self.target()?;
        http::get(&target, path, self.timeout).map_err(|error| LlmError::new(error.to_string()))
    }

    fn post(&self, path: &str, body: &str) -> Result<http::Response, LlmError> {
        let target = self.target()?;
        http::post_json(&target, path, body, self.timeout)
            .map_err(|error| LlmError::new(error.to_string()))
    }
}

/// The `/api/tags` answer, reduced to what we use.
#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagEntry>,
}

#[derive(Debug, Deserialize)]
struct TagEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    details: TagDetails,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct TagDetails {
    #[serde(default)]
    parameter_size: String,
}

/// The `/api/chat` answer (non-streaming).
#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    message: Option<ChatMessageBody>,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    eval_count: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessageBody {
    #[serde(default)]
    content: String,
}

/// Parse the bodies separately, so the parsing is testable without a server.
pub fn parse_models(body: &str) -> Result<Vec<Model>, LlmError> {
    let tags: TagsResponse = serde_json::from_str(body)
        .map_err(|error| LlmError::new(format!("the model list is not valid JSON: {error}")))?;
    Ok(tags
        .models
        .into_iter()
        .filter(|entry| !entry.name.is_empty())
        .map(|entry| Model {
            id: entry.name,
            size: Some(entry.size).filter(|size| *size > 0),
            parameters: Some(entry.details.parameter_size).filter(|text| !text.is_empty()),
            capabilities: entry.capabilities,
        })
        .collect())
}

/// Parse a `/api/chat` body.
pub fn parse_chat(body: &str) -> Result<ChatReply, LlmError> {
    let response: ChatResponse = serde_json::from_str(body)
        .map_err(|error| LlmError::new(format!("the answer is not valid JSON: {error}")))?;
    if let Some(error) = response.error {
        return Err(LlmError::new(error));
    }
    let content = response
        .message
        .map(|message| message.content)
        .unwrap_or_default();
    Ok(ChatReply {
        content,
        model: response.model,
        done_reason: response.done_reason,
        prompt_tokens: response.prompt_eval_count,
        answer_tokens: response.eval_count,
    })
}

/// Build the `/api/chat` request body.
pub fn chat_body(request: &ChatRequest) -> String {
    let messages: Vec<serde_json::Value> = request
        .messages
        .iter()
        .map(|message| {
            serde_json::json!({
                "role": message.role.as_str(),
                "content": message.content,
            })
        })
        .collect();
    let mut body = serde_json::json!({
        "model": request.model,
        "messages": messages,
        "stream": false,
    });
    if let Some(temperature) = request.temperature {
        body["options"] = serde_json::json!({ "temperature": temperature });
    }
    body.to_string()
}

impl Provider for OllamaProvider {
    fn kind(&self) -> &'static str {
        "ollama"
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn list_models(&self) -> Result<Vec<Model>, LlmError> {
        let response = self.get("/api/tags")?;
        if !response.is_success() {
            return Err(LlmError::new(format!(
                "the server answered {} for /api/tags",
                response.status
            )));
        }
        parse_models(&response.body)
    }

    fn chat(&self, request: &ChatRequest) -> Result<ChatReply, LlmError> {
        let body = chat_body(request);
        let response = self.post("/api/chat", &body)?;
        if !response.is_success() {
            return Err(LlmError::new(format!(
                "the server answered {} for /api/chat",
                response.status
            )));
        }
        parse_chat(&response.body)
    }

    fn chat_stream(
        &self,
        request: &ChatRequest,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<ChatReply, LlmError> {
        let body = chat_body_streaming(request);
        let target = self.target()?;
        let mut reply = ChatReply {
            content: String::new(),
            model: request.model.clone(),
            done_reason: None,
            prompt_tokens: None,
            answer_tokens: None,
        };
        let mut failure: Option<LlmError> = None;
        let mut lines: Vec<String> = Vec::new();
        http::post_json_stream(&target, "/api/chat", &body, self.timeout, &mut |line| {
            lines.push(line.to_string());
        })
        .map_err(|error| LlmError::new(error.to_string()))?;
        for line in lines {
            match parse_stream_line(&line) {
                Ok(Some(chunk)) => {
                    reply.content.push_str(&chunk.text);
                    if let Some(model) = chunk.model {
                        reply.model = model;
                    }
                    if let Some(reason) = chunk.done_reason {
                        reply.done_reason = Some(reason);
                    }
                    if chunk.prompt_tokens.is_some() {
                        reply.prompt_tokens = chunk.prompt_tokens;
                    }
                    if chunk.answer_tokens.is_some() {
                        reply.answer_tokens = chunk.answer_tokens;
                    }
                    if !chunk.text.is_empty() {
                        on_chunk(&chunk.text);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(reply),
        }
    }
}

/// One piece of a streamed answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamChunk {
    pub text: String,
    pub model: Option<String>,
    pub done_reason: Option<String>,
    pub prompt_tokens: Option<u64>,
    pub answer_tokens: Option<u64>,
}

/// Parse one NDJSON line of a `/api/chat` stream.
///
/// Returns `Ok(None)` for a line that carries no text (a keep-alive or a bare `done`), so the
/// caller can keep streaming without special cases.
pub fn parse_stream_line(line: &str) -> Result<Option<StreamChunk>, LlmError> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| LlmError::new(format!("a streamed line is not JSON: {error}")))?;
    if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
        return Err(LlmError::new(error.to_string()));
    }
    let text = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .unwrap_or_default()
        .to_string();
    let chunk = StreamChunk {
        text,
        model: value
            .get("model")
            .and_then(|model| model.as_str())
            .map(|model| model.to_string()),
        done_reason: value
            .get("done_reason")
            .and_then(|reason| reason.as_str())
            .map(|reason| reason.to_string()),
        prompt_tokens: value
            .get("prompt_eval_count")
            .and_then(|count| count.as_u64()),
        answer_tokens: value.get("eval_count").and_then(|count| count.as_u64()),
    };
    if chunk.text.is_empty() && chunk.done_reason.is_none() {
        return Ok(None);
    }
    Ok(Some(chunk))
}

/// The `/api/chat` body asking for a stream.
pub fn chat_body_streaming(request: &ChatRequest) -> String {
    let mut body: serde_json::Value =
        serde_json::from_str(&chat_body(request)).unwrap_or_else(|_| serde_json::json!({}));
    body["stream"] = serde_json::Value::Bool(true);
    body.to_string()
}

#[cfg(test)]
mod tests;

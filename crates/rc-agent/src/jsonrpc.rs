//! Minimal JSON-RPC 2.0 framing shared by the MCP server and the MCP client.
//!
//! Only the subset that MCP actually uses is implemented: single request
//! objects, notifications, and single response objects. Batches are rejected
//! with `-32600` because MCP forbids them on the wire.

use serde_json::{Map, Value, json};

/// The JSON-RPC version string every message carries.
pub const JSONRPC_VERSION: &str = "2.0";

/// The payload could not be parsed as JSON at all.
pub const PARSE_ERROR: i64 = -32700;
/// The payload is JSON but not a valid JSON-RPC request object.
pub const INVALID_REQUEST: i64 = -32600;
/// The method name is unknown to this server.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// The method exists but the parameters are missing or malformed.
pub const INVALID_PARAMS: i64 = -32602;
/// The server failed internally while handling a valid request.
pub const INTERNAL_ERROR: i64 = -32603;
/// First of the implementation-defined server error codes.
pub const SERVER_ERROR: i64 = -32000;
/// A destructive tool was called without an approval (see [`crate::Confirmer`]).
pub const CONFIRMATION_REQUIRED: i64 = -32001;

/// A JSON-RPC error object: a numeric code plus a human readable message and
/// optional structured data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcError {
    /// Numeric error code, one of the constants in this module for the standard
    /// failures.
    pub code: i64,
    /// Human readable explanation; the server always fills it in.
    pub message: String,
    /// Optional structured detail.
    pub data: Option<Value>,
}

impl RpcError {
    /// Build an error with no `data`.
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Attach structured data.
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// `-32601` for an unsupported method name.
    pub fn method_not_found(method: &str) -> Self {
        RpcError::new(
            METHOD_NOT_FOUND,
            format!(
                "unknown method `{method}`: this server implements the MCP 2025-06-18 core methods only"
            ),
        )
    }

    /// `-32602` for missing or malformed parameters.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        RpcError::new(INVALID_PARAMS, message)
    }

    /// `-32600` for a payload that is not a request object.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        RpcError::new(INVALID_REQUEST, message)
    }

    /// `-32700` for unparsable JSON.
    pub fn parse_error(message: impl Into<String>) -> Self {
        RpcError::new(PARSE_ERROR, message)
    }

    /// `-32603` for an unexpected internal failure.
    pub fn internal(message: impl Into<String>) -> Self {
        RpcError::new(INTERNAL_ERROR, message)
    }

    /// `-32001` for a refused destructive tool call.
    pub fn confirmation_required(message: impl Into<String>) -> Self {
        RpcError::new(CONFIRMATION_REQUIRED, message)
    }

    /// Render as the `error` member of a JSON-RPC response.
    pub fn to_value(&self) -> Value {
        let mut obj = Map::new();
        obj.insert("code".to_string(), json!(self.code));
        obj.insert("message".to_string(), json!(self.message));
        if let Some(data) = &self.data {
            obj.insert("data".to_string(), data.clone());
        }
        Value::Object(obj)
    }

    /// Read an `error` member back; `None` when it has no numeric code.
    pub fn from_value(value: &Value) -> Option<Self> {
        let obj = value.as_object()?;
        let code = obj.get("code")?.as_i64()?;
        let message = obj
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        Some(RpcError {
            code,
            message,
            data: obj.get("data").cloned(),
        })
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// An incoming JSON-RPC request or notification.
#[derive(Clone, Debug, PartialEq)]
pub struct RpcMessage {
    /// Request identifier; `None` means the message is a notification and must
    /// not be answered.
    pub id: Option<Value>,
    /// Method name.
    pub method: String,
    /// Struct-style parameters (`Value::Null` when absent).
    pub params: Value,
}

impl RpcMessage {
    /// Parse a raw JSON text into a request or notification.
    ///
    /// Every structural problem is reported as an [`RpcError`] so the caller can
    /// answer with a proper error object instead of panicking.
    pub fn parse(raw: &str) -> Result<Self, RpcError> {
        let value: Value = serde_json::from_str(raw)
            .map_err(|e| RpcError::parse_error(format!("invalid JSON: {e}")))?;
        Self::from_value(&value)
    }

    /// Parse an already decoded JSON value.
    pub fn from_value(value: &Value) -> Result<Self, RpcError> {
        let obj = match value {
            Value::Object(obj) => obj,
            Value::Array(_) => {
                return Err(RpcError::invalid_request(
                    "batch requests are not supported by MCP; send one JSON-RPC object per message",
                ));
            }
            other => {
                return Err(RpcError::invalid_request(format!(
                    "a JSON-RPC message must be an object, got {}",
                    json_type_name(other)
                )));
            }
        };

        if let Some(version) = obj.get("jsonrpc")
            && version.as_str() != Some(JSONRPC_VERSION)
        {
            return Err(RpcError::invalid_request(format!(
                "unsupported `jsonrpc` version {}: expected \"2.0\"",
                version
            )));
        }

        if obj.contains_key("result") || obj.contains_key("error") {
            return Err(RpcError::invalid_request(
                "this message is a response, not a request; the server cannot answer it",
            ));
        }

        let Some(method) = obj.get("method") else {
            return Err(RpcError::invalid_request("missing `method` member"));
        };
        let Some(method) = method.as_str() else {
            return Err(RpcError::invalid_request("`method` must be a string"));
        };

        let params = obj.get("params").cloned().unwrap_or(Value::Null);
        if !params.is_null() && !params.is_object() && !params.is_array() {
            return Err(RpcError::invalid_params(
                "`params` must be an object or an array when present",
            ));
        }

        Ok(RpcMessage {
            id: obj.get("id").cloned(),
            method: method.to_string(),
            params,
        })
    }

    /// `true` when the message carries no identifier and therefore expects no
    /// response at all.
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// Read a string member of the struct-style parameters.
    pub fn param_str(&self, key: &str) -> Option<&str> {
        self.params.get(key).and_then(Value::as_str)
    }
}

/// An incoming JSON-RPC response.
#[derive(Clone, Debug, PartialEq)]
pub struct RpcResponse {
    /// Identifier of the request this answers; `None` for notifications.
    pub id: Option<Value>,
    /// The `result` member, when the call succeeded.
    pub result: Option<Value>,
    /// The `error` member, when the call failed.
    pub error: Option<RpcError>,
}

impl RpcResponse {
    /// Parse a raw JSON text into a response.
    pub fn parse(raw: &str) -> Result<Self, RpcError> {
        let value: Value = serde_json::from_str(raw)
            .map_err(|e| RpcError::parse_error(format!("invalid JSON: {e}")))?;
        Self::from_value(&value)
    }

    /// Parse an already decoded JSON value.
    pub fn from_value(value: &Value) -> Result<Self, RpcError> {
        let Some(obj) = value.as_object() else {
            return Err(RpcError::invalid_request(format!(
                "a JSON-RPC response must be an object, got {}",
                json_type_name(value)
            )));
        };
        let id = obj.get("id").cloned();
        let result = obj.get("result").cloned();
        let error = obj.get("error").and_then(RpcError::from_value);
        if result.is_none() && error.is_none() {
            return Err(RpcError::invalid_request(
                "a JSON-RPC response must contain either `result` or `error`",
            ));
        }
        Ok(RpcResponse { id, result, error })
    }

    /// Collapse the response into `Result`: `Err` carries the server's error.
    pub fn into_result(self) -> Result<Value, RpcError> {
        match (self.result, self.error) {
            (_, Some(error)) => Err(error),
            (Some(result), None) => Ok(result),
            (None, None) => Err(RpcError::invalid_request(
                "a JSON-RPC response must contain either `result` or `error`",
            )),
        }
    }
}

/// Build a request object.
pub fn request(id: u64, method: &str, params: Option<Value>) -> Value {
    let mut obj = Map::new();
    obj.insert("jsonrpc".to_string(), json!(JSONRPC_VERSION));
    obj.insert("id".to_string(), json!(id));
    obj.insert("method".to_string(), json!(method));
    if let Some(params) = params {
        obj.insert("params".to_string(), params);
    }
    Value::Object(obj)
}

/// Build a notification object (no `id`, so no response is expected).
pub fn notification(method: &str, params: Option<Value>) -> Value {
    let mut obj = Map::new();
    obj.insert("jsonrpc".to_string(), json!(JSONRPC_VERSION));
    obj.insert("method".to_string(), json!(method));
    if let Some(params) = params {
        obj.insert("params".to_string(), params);
    }
    Value::Object(obj)
}

/// Serialize a successful response.
pub fn success(id: Value, result: Value) -> String {
    let mut obj = Map::new();
    obj.insert("jsonrpc".to_string(), json!(JSONRPC_VERSION));
    obj.insert("id".to_string(), id);
    obj.insert("result".to_string(), result);
    Value::Object(obj).to_string()
}

/// Serialize an error response.
pub fn failure(id: Value, error: &RpcError) -> String {
    let mut obj = Map::new();
    obj.insert("jsonrpc".to_string(), json!(JSONRPC_VERSION));
    obj.insert("id".to_string(), id);
    obj.insert("error".to_string(), error.to_value());
    Value::Object(obj).to_string()
}

/// Name of a JSON value's type, for error messages.
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_a_request_with_object_params() {
        let m = RpcMessage::parse(r#"{"jsonrpc":"2.0","id":7,"method":"ping","params":{"a":1}}"#)
            .expect("parses");
        assert_eq!(m.id, Some(json!(7)));
        assert_eq!(m.method, "ping");
        assert!(!m.is_notification());
        assert_eq!(m.params.get("a"), Some(&json!(1)));
    }

    #[test]
    fn parse_treats_a_missing_id_as_a_notification() {
        let m = RpcMessage::parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .expect("parses");
        assert!(m.is_notification());
        assert_eq!(m.params, Value::Null);
    }

    #[test]
    fn string_ids_are_preserved() {
        let m =
            RpcMessage::parse(r#"{"jsonrpc":"2.0","id":"abc","method":"ping"}"#).expect("parses");
        assert_eq!(m.id, Some(json!("abc")));
    }

    #[test]
    fn broken_json_is_a_parse_error() {
        let e = RpcMessage::parse("{not json").expect_err("must fail");
        assert_eq!(e.code, PARSE_ERROR);
        assert!(e.message.contains("invalid JSON"), "{}", e.message);
    }

    #[test]
    fn batches_and_scalars_are_invalid_requests() {
        assert_eq!(
            RpcMessage::parse(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#)
                .expect_err("fail")
                .code,
            INVALID_REQUEST
        );
        assert_eq!(
            RpcMessage::parse("42").expect_err("fail").code,
            INVALID_REQUEST
        );
        assert_eq!(
            RpcMessage::parse("null").expect_err("fail").code,
            INVALID_REQUEST
        );
    }

    #[test]
    fn responses_and_bad_versions_are_invalid_requests() {
        assert_eq!(
            RpcMessage::parse(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#)
                .expect_err("fail")
                .code,
            INVALID_REQUEST
        );
        assert_eq!(
            RpcMessage::parse(r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#)
                .expect_err("fail")
                .code,
            INVALID_REQUEST
        );
        assert_eq!(
            RpcMessage::parse(r#"{"jsonrpc":"2.0","id":1,"method":5}"#)
                .expect_err("fail")
                .code,
            INVALID_REQUEST
        );
        assert_eq!(
            RpcMessage::parse(r#"{"jsonrpc":"2.0","params":1,"method":"ping"}"#)
                .expect_err("fail")
                .code,
            INVALID_PARAMS
        );
    }

    #[test]
    fn responses_round_trip_through_json() {
        let ok = RpcResponse::parse(&success(json!(1), json!({"tools":[]}))).expect("parses");
        assert_eq!(ok.id, Some(json!(1)));
        assert_eq!(ok.into_result().expect("ok"), json!({"tools":[]}));

        let err = failure(json!(2), &RpcError::new(METHOD_NOT_FOUND, "nope"));
        let parsed = RpcResponse::parse(&err).expect("parses");
        let e = parsed.into_result().expect_err("must fail");
        assert_eq!(e.code, METHOD_NOT_FOUND);
        assert_eq!(e.message, "nope");
    }

    #[test]
    fn error_data_survives_serialization() {
        let e = RpcError::new(SERVER_ERROR, "boom").with_data(json!({"tool":"delete"}));
        let back = RpcError::from_value(&e.to_value()).expect("round trip");
        assert_eq!(back.code, SERVER_ERROR);
        assert_eq!(back.data, Some(json!({"tool":"delete"})));
        assert_eq!(back.to_string(), "[-32000] boom");
    }

    #[test]
    fn response_without_result_or_error_is_rejected() {
        assert_eq!(
            RpcResponse::parse(r#"{"jsonrpc":"2.0","id":1}"#)
                .expect_err("fail")
                .code,
            INVALID_REQUEST
        );
        assert_eq!(
            RpcResponse::parse("[]").expect_err("fail").code,
            INVALID_REQUEST
        );
    }

    #[test]
    fn builders_omit_params_when_absent() {
        let req = request(3, "tools/list", None);
        assert_eq!(req.get("id"), Some(&json!(3)));
        assert!(req.get("params").is_none());
        let note = notification("notifications/initialized", None);
        assert!(note.get("id").is_none());
        assert_eq!(note.get("jsonrpc"), Some(&json!("2.0")));
    }
}

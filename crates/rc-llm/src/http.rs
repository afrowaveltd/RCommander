//! Just enough HTTP/1.1 to talk to a local model server.
//!
//! Local providers (Ollama, LM Studio, vLLM) speak plain HTTP on the loopback interface, so a
//! socket client is all that is needed — no TLS stack, no async runtime, no extra dependency in
//! a project that has to build for Android as well. Remote HTTPS endpoints get their own client
//! when the remote providers are added.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// One HTTP response, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// A failure of the HTTP layer (not of the provider protocol).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpError {
    pub message: String,
}

impl HttpError {
    fn new(message: impl Into<String>) -> Self {
        HttpError {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HttpError {}

/// A `host:port` target parsed from a base URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    /// Path prefix the base URL carried (usually empty).
    pub prefix: String,
}

impl Target {
    /// Parse `http://127.0.0.1:11434` (the scheme is optional; `https` is not supported yet).
    pub fn parse(base_url: &str) -> Result<Target, HttpError> {
        let trimmed = base_url.trim().trim_end_matches('/');
        let without_scheme = match trimmed.split_once("://") {
            Some((scheme, rest)) => {
                if scheme.eq_ignore_ascii_case("https") {
                    return Err(HttpError::new(
                        "https endpoints need the remote provider client, which is not built yet",
                    ));
                }
                if !scheme.eq_ignore_ascii_case("http") {
                    return Err(HttpError::new(format!("unsupported scheme “{scheme}”")));
                }
                rest
            }
            None => trimmed,
        };
        let (authority, prefix) = match without_scheme.split_once('/') {
            Some((authority, rest)) => (authority, format!("/{rest}")),
            None => (without_scheme, String::new()),
        };
        if authority.is_empty() {
            return Err(HttpError::new("the base URL has no host"));
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (
                host.to_string(),
                port.parse::<u16>()
                    .map_err(|_| HttpError::new(format!("bad port “{port}”")))?,
            ),
            None => (authority.to_string(), 80),
        };
        Ok(Target { host, port, prefix })
    }
}

/// POST a JSON body and read the whole response.
pub fn post_json(
    target: &Target,
    path: &str,
    body: &str,
    timeout: Duration,
) -> Result<Response, HttpError> {
    request(target, "POST", path, Some(body), timeout)
}

/// GET a path and read the whole response.
pub fn get(target: &Target, path: &str, timeout: Duration) -> Result<Response, HttpError> {
    request(target, "GET", path, None, timeout)
}

fn request(
    target: &Target,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<Response, HttpError> {
    let address = format!("{}:{}", target.host, target.port);
    let stream = TcpStream::connect(&address).map_err(|error| {
        HttpError::new(format!(
            "cannot reach {address}: {error} (is the server running?)"
        ))
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| HttpError::new(error.to_string()))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| HttpError::new(error.to_string()))?;
    let mut stream = stream;
    let full_path = format!("{}{}", target.prefix, path);
    let mut head = format!(
        "{method} {full_path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAccept: application/json\r\n"
    );
    if let Some(body) = body {
        head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .map_err(|error| HttpError::new(error.to_string()))?;
    if let Some(body) = body {
        stream
            .write_all(body.as_bytes())
            .map_err(|error| HttpError::new(error.to_string()))?;
    }
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|error| HttpError::new(error.to_string()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| HttpError::new(format!("bad status line “{}”", status_line.trim())))?;
    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| HttpError::new(error.to_string()))?;
        if read == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().ok();
        } else if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        }
    }
    let body = if chunked {
        read_chunked(&mut reader)?
    } else if let Some(length) = content_length {
        let mut buffer = vec![0u8; length];
        reader
            .read_exact(&mut buffer)
            .map_err(|error| HttpError::new(error.to_string()))?;
        String::from_utf8_lossy(&buffer).to_string()
    } else {
        let mut text = String::new();
        reader
            .read_to_string(&mut text)
            .map_err(|error| HttpError::new(error.to_string()))?;
        text
    };
    Ok(Response { status, body })
}

/// Decode a `Transfer-Encoding: chunked` body.
fn read_chunked(reader: &mut impl BufRead) -> Result<String, HttpError> {
    let mut out = String::new();
    loop {
        let mut size_line = String::new();
        if reader
            .read_line(&mut size_line)
            .map_err(|error| HttpError::new(error.to_string()))?
            == 0
        {
            break;
        }
        let size_text = size_line.trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| HttpError::new(format!("bad chunk size “{size_text}”")))?;
        if size == 0 {
            // Trailing headers, then the final empty line.
            let mut trailer = String::new();
            while reader.read_line(&mut trailer).unwrap_or(0) > 0 && trailer.trim() != "" {
                trailer.clear();
            }
            break;
        }
        let mut buffer = vec![0u8; size];
        reader
            .read_exact(&mut buffer)
            .map_err(|error| HttpError::new(error.to_string()))?;
        out.push_str(&String::from_utf8_lossy(&buffer));
        let mut crlf = String::new();
        let _ = reader.read_line(&mut crlf);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_url_is_parsed_into_a_target() {
        let target = Target::parse("http://127.0.0.1:11434").unwrap();
        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.port, 11434);
        assert!(target.prefix.is_empty());
        let with_path = Target::parse("127.0.0.1:8080/v1/").unwrap();
        assert_eq!(with_path.port, 8080);
        assert_eq!(with_path.prefix, "/v1");
        assert_eq!(Target::parse("example.com").unwrap().port, 80);
        assert!(Target::parse("https://api.example.com").is_err());
        assert!(Target::parse("").is_err());
        assert!(Target::parse("http://host:notaport").is_err());
    }
}

//! Just enough HTTP/1.1 to talk to a local model server.
//!
//! Local providers (Ollama, LM Studio, vLLM) speak plain HTTP on the loopback interface, so a
//! socket client is all that is needed — no TLS stack, no async runtime, no extra dependency in
//! a project that has to build for Android as well. Remote HTTPS endpoints get their own client
//! when the remote providers are added.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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

/// Owns only the active request socket; cancelling never kills the Ollama server.
#[derive(Clone, Debug, Default)]
pub struct StreamControl {
    cancelled: Arc<AtomicBool>,
    socket: Arc<Mutex<Option<TcpStream>>>,
}

impl StreamControl {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut slot) = self.socket.lock()
            && let Some(socket) = slot.take()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    fn attach(&self, stream: &TcpStream) -> Result<SocketGuard<'_>, HttpError> {
        let mut slot = self
            .socket
            .lock()
            .map_err(|_| HttpError::new("request lock poisoned"))?;
        if self.is_cancelled() {
            return Err(HttpError::new("request cancelled"));
        }
        *slot = Some(
            stream
                .try_clone()
                .map_err(|e| HttpError::new(e.to_string()))?,
        );
        Ok(SocketGuard(self))
    }
}

struct SocketGuard<'a>(&'a StreamControl);

impl Drop for SocketGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.0.socket.lock() {
            *slot = None;
        }
    }
}

/// POST a JSON body and hand every line of the response to `on_line` as it arrives.
///
/// This is what streaming needs: the caller sees each NDJSON object while the model is still
/// generating. Both a chunked response (what Ollama sends) and a plain one are handled.
pub fn post_json_stream(
    target: &Target,
    path: &str,
    body: &str,
    timeout: Duration,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), HttpError> {
    stream_impl(target, path, body, timeout, None, on_line)
}

/// No read deadline: the owner may cancel the socket while waiting for a cold model.
pub fn post_json_stream_controlled(
    target: &Target,
    path: &str,
    body: &str,
    timeout: Duration,
    control: &StreamControl,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), HttpError> {
    stream_impl(target, path, body, timeout, Some(control), on_line)
}

fn stream_impl(
    target: &Target,
    path: &str,
    body: &str,
    timeout: Duration,
    control: Option<&StreamControl>,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), HttpError> {
    let address = format!("{}:{}", target.host, target.port);
    let mut connected = None;
    let mut last_error = None;
    for addr in address
        .to_socket_addrs()
        .map_err(|e| HttpError::new(e.to_string()))?
    {
        if control.is_some_and(StreamControl::is_cancelled) {
            return Err(HttpError::new("request cancelled"));
        }
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                connected = Some(stream);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let stream = connected.ok_or_else(|| {
        HttpError::new(format!(
            "cannot reach {address}: {} (is the server running?)",
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "no address".into())
        ))
    })?;
    let _guard = control.map(|c| c.attach(&stream)).transpose()?;
    stream
        .set_read_timeout(if control.is_some() {
            None
        } else {
            Some(timeout)
        })
        .map_err(|error| HttpError::new(error.to_string()))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| HttpError::new(error.to_string()))?;
    let mut stream = stream;
    let full_path = format!("{}{}", target.prefix, path);
    let head = format!(
        "POST {full_path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body.as_bytes()))
        .map_err(|error| HttpError::new(error.to_string()))?;
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
    if !(200..300).contains(&status) {
        return Err(HttpError::new(format!(
            "the server answered {status} for {full_path}"
        )));
    }
    let mut chunked = false;
    loop {
        let mut line = String::new();
        if reader
            .read_line(&mut line)
            .map_err(|error| HttpError::new(error.to_string()))?
            == 0
        {
            break;
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some(encoding) = line.to_ascii_lowercase().strip_prefix("transfer-encoding:")
            && encoding.contains("chunked")
        {
            chunked = true;
        }
    }
    if !chunked {
        // A plain body: forward each line as it arrives.
        let mut line = String::new();
        while reader
            .read_line(&mut line)
            .map_err(|error| HttpError::new(error.to_string()))?
            > 0
        {
            let trimmed = line.trim_end();
            if !trimmed.is_empty() {
                on_line(trimmed);
            }
            line.clear();
        }
        return Ok(());
    }
    stream_chunked(&mut reader, on_line)
}

/// Read a chunked body incrementally, forwarding each completed line.
fn stream_chunked(
    reader: &mut impl BufRead,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), HttpError> {
    let mut pending = String::new();
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
        if size_text.is_empty() {
            continue;
        }
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| HttpError::new(format!("bad chunk size “{size_text}”")))?;
        if size == 0 {
            break;
        }
        let mut buffer = vec![0u8; size];
        reader
            .read_exact(&mut buffer)
            .map_err(|error| HttpError::new(error.to_string()))?;
        // A trailing CRLF after every chunk.
        let mut crlf = String::new();
        let _ = reader.read_line(&mut crlf);
        pending.push_str(&String::from_utf8_lossy(&buffer));
        while let Some(index) = pending.find('\n') {
            let line = pending[..index].trim_end().to_string();
            pending.drain(..=index);
            if !line.is_empty() {
                on_line(&line);
            }
        }
    }
    let rest = pending.trim_end();
    if !rest.is_empty() {
        on_line(rest);
    }
    Ok(())
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
    fn silent_stream_can_be_cancelled() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let target = Target::parse(&format!("http://{addr}")).unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buffer = [0; 1024];
            let _ = socket.read(&mut buffer).unwrap();
            ready_tx.send(()).unwrap();
            while matches!(socket.read(&mut buffer), Ok(n) if n > 0) {}
        });
        let control = StreamControl::default();
        let worker_control = control.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = post_json_stream_controlled(
                &target,
                "/api/chat",
                "{}",
                Duration::from_secs(1),
                &worker_control,
                &mut |_| {},
            );
            done_tx.send(result).unwrap();
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(50)).is_err());
        control.cancel();
        assert!(
            done_rx
                .recv_timeout(Duration::from_secs(3))
                .unwrap()
                .is_err()
        );
        worker.join().unwrap();
        server.join().unwrap();
    }

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

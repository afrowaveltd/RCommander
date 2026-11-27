//! Shared HTTP/1.1 transport for model adapters.
//!
//! Desktop HTTPS uses platform certificate and hostname verification. No redirects, implicit
//! downgrade, authentication or proxy discovery. The application registry deliberately continues
//! to gate HTTPS until its credential and egress contracts are ready. Android stays HTTP-only.

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
    pub retry_after_seconds: Option<u64>,
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
    pub(crate) notice: Option<crate::ProviderNotice>,
}

impl HttpError {
    fn new(message: impl Into<String>) -> Self {
        HttpError {
            message: message.into(),
            notice: None,
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
    tls: bool,
}

impl Target {
    /// Parse a base URL without credentials, query or fragment.
    pub fn parse(base_url: &str) -> Result<Target, HttpError> {
        let trimmed = base_url.trim().trim_end_matches('/');
        if trimmed.bytes().any(|b| !b.is_ascii_graphic())
            || trimmed.contains(['\\', '?', '#'])
        {
            return Err(HttpError::new("invalid characters in provider base URL"));
        }
        let (tls, without_scheme) = match trimmed.split_once("://") {
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => (true, rest),
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => (false, rest),
            Some(_) => return Err(HttpError::new("unsupported provider URL scheme")),
            None => (false, trimmed),
        };
        if tls && !cfg!(any(target_os = "linux", target_os = "windows", target_os = "macos")) {
            return Err(HttpError::new("HTTPS is not supported on this platform"));
        }
        let (authority, prefix) = match without_scheme.split_once('/') {
            Some((authority, rest)) => (authority, format!("/{rest}")),
            None => (without_scheme, String::new()),
        };
        if authority.is_empty() || authority.contains('@') {
            return Err(HttpError::new("missing host or credentials in provider URL"));
        }
        let default_port = if tls { 443 } else { 80 };
        let parse_port = |port: &str| {
            port.parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| HttpError::new("invalid provider port"))
        };
        let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
            let (host, tail) = rest.split_once(']')
                .ok_or_else(|| HttpError::new("invalid IPv6 authority"))?;
            host.parse::<std::net::Ipv6Addr>()
                .map_err(|_| HttpError::new("invalid IPv6 host"))?;
            let port = if tail.is_empty() {
                default_port
            } else {
                parse_port(tail.strip_prefix(':')
                    .ok_or_else(|| HttpError::new("invalid IPv6 authority"))?)?
            };
            (host.to_owned(), port)
        } else {
            let (host, port) = match authority.split_once(':') {
                Some((host, port)) => (host, parse_port(port)?),
                None => (authority, default_port),
            };
            if host.is_empty() || !host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') {
                return Err(HttpError::new("invalid provider host"));
            }
            (host.to_owned(), port)
        };
        Ok(Target { host, port, prefix, tls })
    }

    pub fn is_tls(&self) -> bool {
        self.tls
    }

    fn authority(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn request_path(&self, path: &str) -> Result<String, HttpError> {
        if !path.starts_with('/') || path.starts_with("//")
            || path.bytes().any(|b| !b.is_ascii_graphic())
            || path.contains(['\\', '#'])
        {
            return Err(HttpError::new("invalid provider request path"));
        }
        Ok(format!("{}{}", self.prefix, path))
    }
}

/// A common byte stream; TLS never changes HTTP parsing or the provider protocol.
enum Wire {
    Plain(TcpStream),
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    Tls(native_tls::TlsStream<TcpStream>),
}

impl Wire {
    fn socket(&self) -> &TcpStream {
        match self {
            Self::Plain(stream) => stream,
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Tls(stream) => stream.get_ref(),
        }
    }
}

impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buf),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buf),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Tls(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Self::Tls(stream) => stream.flush(),
        }
    }
}

fn connect_socket(
    target: &Target,
    timeout: Duration,
    control: Option<&StreamControl>,
) -> Result<TcpStream, HttpError> {
    let mut last_error = None;
    for addr in (target.host.as_str(), target.port)
        .to_socket_addrs()
        .map_err(|e| HttpError::new(e.to_string()))?
    {
        if control.is_some_and(StreamControl::is_cancelled) {
            return Err(HttpError::new("request cancelled"));
        }
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(HttpError::new(format!(
        "cannot reach {}: {} (is the server running?)",
        target.authority(),
        last_error.map(|e| e.to_string()).unwrap_or_else(|| "no address".into())
    )))
}

fn open_wire(target: &Target, stream: TcpStream, timeout: Duration) -> Result<Wire, HttpError> {
    stream.set_read_timeout(Some(timeout)).map_err(|e| HttpError::new(e.to_string()))?;
    stream.set_write_timeout(Some(timeout)).map_err(|e| HttpError::new(e.to_string()))?;
    if !target.tls {
        return Ok(Wire::Plain(stream));
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    {
        let connector = native_tls::TlsConnector::builder()
            .min_protocol_version(Some(native_tls::Protocol::Tlsv12))
            .build()
            .map_err(|e| HttpError::new(format!("TLS initialization failed: {e}")))?;
        connector.connect(&target.host, stream)
            .map(Wire::Tls)
            .map_err(|e| HttpError::new(format!("TLS connection failed: {e}")))
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(HttpError::new("HTTPS is not supported on this platform"))
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
    let full_path = target.request_path(path)?;
    let address = target.authority();
    let mut stream = open_wire(target, connect_socket(target, timeout, None)?, timeout)?;
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
    let mut retry_after_seconds = None;
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
        } else if let Some(value) = lower.strip_prefix("retry-after:") {
            retry_after_seconds = value.trim().parse::<u64>().ok();
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
    Ok(Response {
        status,
        body,
        retry_after_seconds,
    })
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
        if slot.is_some() {
            return Err(HttpError::new("stream control already owns an active request"));
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
    let full_path = target.request_path(path)?;
    let address = target.authority();
    let socket = connect_socket(target, timeout, control)?;
    // Attach the underlying socket before TLS so cancellation can interrupt a handshake too.
    let _guard = control.map(|c| c.attach(&socket)).transpose()?;
    let mut stream = open_wire(target, socket, timeout)?;
    stream.socket().set_read_timeout(if control.is_some() { None } else { Some(timeout) })
        .map_err(|error| HttpError::new(error.to_string()))?;
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
    let mut content_length: Option<usize> = None;
    let mut retry_after_seconds = None;
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
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().ok();
        } else if let Some(value) = lower.strip_prefix("retry-after:") {
            retry_after_seconds = value.trim().parse::<u64>().ok();
        } else if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        }
    }
    if !(200..300).contains(&status) {
        // Error evidence is never forwarded as generated assistant text.
        // Bound the body sample and deadline; chunked error bodies are not
        // decoded by this slice. Keep status even if reading evidence fails.
        let mut sample = Vec::new();
        if !chunked
            && reader
                .get_ref()
                .socket()
                .set_read_timeout(Some(timeout.min(Duration::from_secs(5))))
                .is_ok()
        {
            let limit = content_length.unwrap_or(8192).min(8192);
            let _ = reader.by_ref().take(limit as u64).read_to_end(&mut sample);
        }
        let mut error = HttpError::new(format!("the server answered {status} for {full_path}"));
        error.notice = Some(crate::ProviderNotice::new(
            status,
            retry_after_seconds,
            &String::from_utf8_lossy(&sample),
        ));
        return Err(error);
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
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        {
            let tls = Target::parse("https://api.example.com/v1").unwrap();
            assert!(tls.is_tls());
            assert_eq!(tls.port, 443);
            assert_eq!(tls.prefix, "/v1");
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        assert!(Target::parse("https://api.example.com").is_err());
        assert!(Target::parse("").is_err());
        assert!(Target::parse("http://host:notaport").is_err());
    }

    #[test]
    fn malformed_authorities_and_request_lines_are_refused() {
        for url in ["http://user:secret@host", "http://host:0", "http://host/a?b",
            "http://host/a#b", "http://host/a\r\nX:1", "http://[::1]bad", "http://::1"] {
            assert!(Target::parse(url).is_err(), "{url}");
        }
        let target = Target::parse("http://[::1]:11434/v1").unwrap();
        assert_eq!(target.host, "::1");
        assert_eq!(target.authority(), "[::1]:11434");
        assert_eq!(target.request_path("/models").unwrap(), "/v1/models");
        for path in ["https://other", "//other", "/a\r\nX:1", "/a b"] {
            assert!(target.request_path(path).is_err(), "{path}");
        }
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    #[test]
    fn tls_failure_does_not_send_a_plaintext_http_request() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut first = [0; 1];
            socket.read_exact(&mut first).unwrap();
            // A TLS handshake record, not the first byte of GET or POST.
            assert_eq!(first[0], 0x16);
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").unwrap();
        });
        let target = Target::parse(&format!("https://{addr}")).unwrap();
        assert!(get(&target, "/api/tags", Duration::from_secs(5)).is_err());
        worker.join().unwrap();
    }
}

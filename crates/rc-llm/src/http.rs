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
        if trimmed.bytes().any(|b| !b.is_ascii_graphic()) || trimmed.contains(['\\', '?', '#']) {
            return Err(HttpError::new("invalid characters in provider base URL"));
        }
        let (tls, without_scheme) = match trimmed.split_once("://") {
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => (true, rest),
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => (false, rest),
            Some(_) => return Err(HttpError::new("unsupported provider URL scheme")),
            None => (false, trimmed),
        };
        if tls
            && !cfg!(any(
                target_os = "linux",
                target_os = "windows",
                target_os = "macos"
            ))
        {
            return Err(HttpError::new("HTTPS is not supported on this platform"));
        }
        let (authority, prefix) = match without_scheme.split_once('/') {
            Some((authority, rest)) => (authority, format!("/{rest}")),
            None => (without_scheme, String::new()),
        };
        if authority.is_empty() || authority.contains('@') {
            return Err(HttpError::new(
                "missing host or credentials in provider URL",
            ));
        }
        let default_port = if tls { 443 } else { 80 };
        let parse_port = |port: &str| {
            port.parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| HttpError::new("invalid provider port"))
        };
        let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
            let (host, tail) = rest
                .split_once(']')
                .ok_or_else(|| HttpError::new("invalid IPv6 authority"))?;
            host.parse::<std::net::Ipv6Addr>()
                .map_err(|_| HttpError::new("invalid IPv6 host"))?;
            let port = if tail.is_empty() {
                default_port
            } else {
                parse_port(
                    tail.strip_prefix(':')
                        .ok_or_else(|| HttpError::new("invalid IPv6 authority"))?,
                )?
            };
            (host.to_owned(), port)
        } else {
            let (host, port) = match authority.split_once(':') {
                Some((host, port)) => (host, parse_port(port)?),
                None => (authority, default_port),
            };
            if host.is_empty()
                || !host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
            {
                return Err(HttpError::new("invalid provider host"));
            }
            (host.to_owned(), port)
        };
        Ok(Target {
            host,
            port,
            prefix,
            tls,
        })
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
        if !path.starts_with('/')
            || path.starts_with("//")
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
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "no address".into())
    )))
}

fn open_wire(target: &Target, stream: TcpStream, timeout: Duration) -> Result<Wire, HttpError> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| HttpError::new(e.to_string()))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| HttpError::new(e.to_string()))?;
    if !target.tls {
        return Ok(Wire::Plain(stream));
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    {
        let connector = native_tls::TlsConnector::builder()
            .min_protocol_version(Some(native_tls::Protocol::Tlsv12))
            .build()
            .map_err(|e| HttpError::new(format!("TLS initialization failed: {e}")))?;
        connector
            .connect(&target.host, stream)
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
    let head = read_head(&mut reader)?;
    let mut bytes = Vec::new();
    if head.is_success() {
        read_body(&mut reader, head.framing, MAX_BODY, false, &mut |part| {
            bytes.extend_from_slice(part);
            Ok(())
        })?;
    } else {
        // Retain the status and bounded evidence even if the error body is truncated.
        reader
            .get_ref()
            .socket()
            .set_read_timeout(Some(timeout.min(Duration::from_secs(5))))
            .map_err(|e| HttpError::new(e.to_string()))?;
        let _ = read_body(&mut reader, head.framing, ERROR_SAMPLE, true, &mut |part| {
            bytes.extend_from_slice(part);
            Ok(())
        });
    }
    Ok(Response {
        status: head.status,
        body: if head.is_success() {
            String::from_utf8(bytes).map_err(|_| HttpError::new("invalid UTF-8 response"))?
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        },
        retry_after_seconds: head.retry_after_seconds,
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
            return Err(HttpError::new(
                "stream control already owns an active request",
            ));
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
    stream
        .socket()
        .set_read_timeout(if control.is_some() {
            None
        } else {
            Some(timeout)
        })
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
    let head = read_head(&mut reader)?;
    if !head.is_success() {
        // Error evidence must never become assistant output.
        let mut sample = Vec::new();
        if reader
            .get_ref()
            .socket()
            .set_read_timeout(Some(timeout.min(Duration::from_secs(5))))
            .is_ok()
        {
            let _ = read_body(&mut reader, head.framing, ERROR_SAMPLE, true, &mut |part| {
                sample.extend_from_slice(part);
                Ok(())
            });
        }
        let mut error = HttpError::new(format!(
            "the server answered {} for {full_path}",
            head.status
        ));
        error.notice = Some(crate::ProviderNotice::new(
            head.status,
            head.retry_after_seconds,
            &String::from_utf8_lossy(&sample),
        ));
        return Err(error);
    }
    read_stream(&mut reader, head.framing, on_line)
}

// Limits are transport safety ceilings, not model context or token limits.
const MAX_HEAD: usize = 64 * 1024;
const MAX_HEADER_LINE: usize = 8 * 1024;
const MAX_BODY: usize = 64 * 1024 * 1024;
const MAX_STREAM_LINE: usize = 1024 * 1024;
const ERROR_SAMPLE: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Framing {
    Length(usize),
    Chunked,
    UntilEof,
}

struct Head {
    status: u16,
    retry_after_seconds: Option<u64>,
    framing: Framing,
}

impl Head {
    fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Read a bounded CRLF-delimited protocol line without allocating attacker-selected sizes.
fn protocol_line(reader: &mut impl BufRead) -> Result<Vec<u8>, HttpError> {
    let mut line = Vec::new();
    reader
        .take((MAX_HEADER_LINE + 1) as u64)
        .read_until(b'\n', &mut line)
        .map_err(|e| HttpError::new(e.to_string()))?;
    if line.len() > MAX_HEADER_LINE || !line.ends_with(b"\r\n") {
        return Err(HttpError::new("oversized or incomplete HTTP protocol line"));
    }
    line.truncate(line.len() - 2);
    Ok(line)
}

fn read_head(reader: &mut impl BufRead) -> Result<Head, HttpError> {
    let line = protocol_line(reader)?;
    let mut total = line.len() + 2;
    let status_line =
        std::str::from_utf8(&line).map_err(|_| HttpError::new("invalid HTTP status line"))?;
    let mut fields = status_line.split_whitespace();
    if !matches!(fields.next(), Some("HTTP/1.0" | "HTTP/1.1")) {
        return Err(HttpError::new("unsupported HTTP response version"));
    }
    let status = fields
        .next()
        .and_then(|code| code.parse::<u16>().ok())
        .filter(|code| (200..600).contains(code))
        .ok_or_else(|| HttpError::new("unsupported or invalid HTTP status"))?;
    let mut length = None;
    let mut chunked = false;
    let mut retry_after_seconds = None;
    loop {
        let line = protocol_line(reader)?;
        total += line.len() + 2;
        if total > MAX_HEAD {
            return Err(HttpError::new("HTTP response headers exceed limit"));
        }
        if line.is_empty() {
            break;
        }
        let line = std::str::from_utf8(&line).map_err(|_| HttpError::new("invalid HTTP header"))?;
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| HttpError::new("malformed HTTP header"))?;
        if name.is_empty() || name.bytes().any(|b| !b.is_ascii_graphic()) {
            return Err(HttpError::new("invalid HTTP header name"));
        }
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(HttpError::new("ambiguous or invalid Content-Length"));
            }
            length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| HttpError::new("invalid Content-Length"))?,
            );
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            if chunked || !value.eq_ignore_ascii_case("chunked") {
                return Err(HttpError::new("unsupported Transfer-Encoding"));
            }
            chunked = true;
        } else if name.eq_ignore_ascii_case("retry-after") {
            retry_after_seconds = value.parse::<u64>().ok();
        } else if name.eq_ignore_ascii_case("content-encoding")
            && !value.eq_ignore_ascii_case("identity")
        {
            return Err(HttpError::new("unsupported Content-Encoding"));
        }
    }
    if chunked && length.is_some() {
        return Err(HttpError::new("conflicting HTTP body framing"));
    }
    let framing = if status == 204 || status == 304 {
        Framing::Length(0)
    } else if chunked {
        Framing::Chunked
    } else if let Some(length) = length {
        Framing::Length(length)
    } else {
        Framing::UntilEof
    };
    Ok(Head {
        status,
        retry_after_seconds,
        framing,
    })
}

/// Deliver decoded body bytes in small blocks. For an error sample, stop exactly at the
/// sample limit without draining the rest. For successful responses, require complete framing.
fn read_body(
    reader: &mut impl BufRead,
    framing: Framing,
    limit: usize,
    sample: bool,
    emit: &mut dyn FnMut(&[u8]) -> Result<(), HttpError>,
) -> Result<(), HttpError> {
    let mut remaining = limit;
    let mut buffer = [0_u8; 8192];
    match framing {
        Framing::Length(length) => {
            if !sample && length > limit {
                return Err(HttpError::new("HTTP response body exceeds limit"));
            }
            read_fixed(reader, length.min(limit), &mut buffer, emit)
        }
        Framing::UntilEof => loop {
            if remaining == 0 {
                if sample
                    || reader
                        .fill_buf()
                        .map_err(|e| HttpError::new(e.to_string()))?
                        .is_empty()
                {
                    return Ok(());
                }
                return Err(HttpError::new("HTTP response body exceeds limit"));
            }
            let size = remaining.min(buffer.len());
            let n = reader
                .read(&mut buffer[..size])
                .map_err(|e| HttpError::new(e.to_string()))?;
            if n == 0 {
                return Ok(());
            }
            remaining -= n;
            emit(&buffer[..n])?;
        },
        Framing::Chunked => loop {
            if sample && remaining == 0 {
                return Ok(());
            }
            let line = protocol_line(reader)?;
            let size_text = std::str::from_utf8(&line)
                .map_err(|_| HttpError::new("invalid HTTP chunk size"))?;
            let size_text = size_text.split(';').next().unwrap_or_default();
            if size_text.is_empty() || !size_text.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(HttpError::new("invalid HTTP chunk size"));
            }
            let size = usize::from_str_radix(size_text, 16)
                .map_err(|_| HttpError::new("invalid HTTP chunk size"))?;
            if size == 0 {
                let mut trailers = 0;
                loop {
                    let line = protocol_line(reader)?;
                    trailers += line.len() + 2;
                    if trailers > MAX_HEAD {
                        return Err(HttpError::new("HTTP trailers exceed limit"));
                    }
                    if line.is_empty() {
                        return Ok(());
                    }
                }
            }
            if !sample && size > remaining {
                return Err(HttpError::new("HTTP response body exceeds limit"));
            }
            let count = size.min(remaining);
            read_fixed(reader, count, &mut buffer, emit)?;
            remaining -= count;
            if sample && remaining == 0 {
                return Ok(());
            }
            let mut crlf = [0; 2];
            reader
                .read_exact(&mut crlf)
                .map_err(|e| HttpError::new(e.to_string()))?;
            if crlf != *b"\r\n" {
                return Err(HttpError::new("invalid HTTP chunk terminator"));
            }
        },
    }
}

fn read_fixed(
    reader: &mut impl Read,
    mut count: usize,
    buffer: &mut [u8],
    emit: &mut dyn FnMut(&[u8]) -> Result<(), HttpError>,
) -> Result<(), HttpError> {
    while count > 0 {
        let size = count.min(buffer.len());
        let n = reader
            .read(&mut buffer[..size])
            .map_err(|e| HttpError::new(e.to_string()))?;
        if n == 0 {
            return Err(HttpError::new("truncated HTTP response body"));
        }
        emit(&buffer[..n])?;
        count -= n;
    }
    Ok(())
}

fn read_stream(
    reader: &mut impl BufRead,
    framing: Framing,
    on_line: &mut dyn FnMut(&str),
) -> Result<(), HttpError> {
    let mut pending = Vec::new();
    let mut deliver = |bytes: &[u8]| {
        let line = std::str::from_utf8(bytes)
            .map_err(|_| HttpError::new("invalid UTF-8 in model stream"))?
            .trim_end();
        if !line.is_empty() {
            on_line(line);
        }
        Ok(())
    };
    read_body(reader, framing, MAX_BODY, false, &mut |part| {
        for segment in part.split_inclusive(|byte| *byte == b'\n') {
            if pending.len() + segment.len() > MAX_STREAM_LINE {
                return Err(HttpError::new("model stream line exceeds limit"));
            }
            pending.extend_from_slice(segment);
            if segment.ends_with(b"\n") {
                deliver(&pending)?;
                pending.clear();
            }
        }
        Ok(())
    })?;
    if !pending.is_empty() {
        deliver(&pending)?;
    }
    Ok(())
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
        for url in [
            "http://user:secret@host",
            "http://host:0",
            "http://host/a?b",
            "http://host/a#b",
            "http://host/a\r\nX:1",
            "http://[::1]bad",
            "http://::1",
        ] {
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
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut first = [0; 1];
            socket.read_exact(&mut first).unwrap();
            // A TLS handshake record, not the first byte of GET or POST.
            assert_eq!(first[0], 0x16);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });
        let target = Target::parse(&format!("https://{addr}")).unwrap();
        assert!(get(&target, "/api/tags", Duration::from_secs(5)).is_err());
        worker.join().unwrap();
    }

    #[test]
    fn chunk_boundaries_preserve_utf8_and_extensions() {
        let mut body =
            std::io::Cursor::new(b"2;test=yes\r\nA\xc5\r\n3\r\n\xbe!\n\r\n0\r\nX-End: yes\r\n\r\n");
        let mut lines = Vec::new();
        read_stream(&mut body, Framing::Chunked, &mut |line| {
            lines.push(line.to_owned())
        })
        .unwrap();
        assert_eq!(lines, vec!["Až!"]);
    }

    #[test]
    fn fixed_length_stream_stops_at_the_declared_boundary() {
        let mut body = std::io::Cursor::new(b"ok\nextra");
        let mut lines = Vec::new();
        read_stream(&mut body, Framing::Length(3), &mut |line| {
            lines.push(line.to_owned())
        })
        .unwrap();
        assert_eq!(lines, vec!["ok"]);
        assert_eq!(body.position(), 3);
    }

    #[test]
    fn broken_or_ambiguous_framing_is_refused() {
        for head in [
            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: no\r\n\r\n",
            "HTTP/1.1 200 OK\r\n",
        ] {
            assert!(read_head(&mut std::io::Cursor::new(head.as_bytes())).is_err());
        }
        for body in ["2\r\na", "1\r\naXX0\r\n\r\n", "1\r\na\r\n", "0\r\n"] {
            assert!(
                read_stream(
                    &mut std::io::Cursor::new(body.as_bytes()),
                    Framing::Chunked,
                    &mut |_| {},
                )
                .is_err()
            );
        }
        assert!(
            read_stream(
                &mut std::io::Cursor::new(b"a"),
                Framing::Length(2),
                &mut |_| {},
            )
            .is_err()
        );
    }

    #[test]
    fn declared_sizes_and_unterminated_lines_are_bounded() {
        let mut empty = std::io::Cursor::new(b"");
        assert!(read_body(&mut empty, Framing::Length(100), 10, false, &mut |_| Ok(())).is_err());
        let mut huge_chunk = std::io::Cursor::new(b"fffffff\r\n");
        assert!(
            read_body(
                &mut huge_chunk,
                Framing::Chunked,
                10,
                false,
                &mut |_| Ok(())
            )
            .is_err()
        );
        let mut line = std::io::Cursor::new(vec![b'a'; MAX_STREAM_LINE + 1]);
        assert!(read_stream(&mut line, Framing::UntilEof, &mut |_| {}).is_err());
        let mut head = std::io::Cursor::new(vec![b'a'; MAX_HEADER_LINE + 1]);
        assert!(read_head(&mut head).is_err());
    }

    #[test]
    fn chunked_error_notice_is_decoded_and_never_streamed_as_assistant_text() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            loop {
                line.clear();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = [0; 2];
            reader.read_exact(&mut body).unwrap();
            reader.get_mut().write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 300\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nwait\r\n0\r\n\r\n").unwrap();
        });
        let target = Target::parse(&format!("http://{addr}")).unwrap();
        let mut lines = Vec::new();
        let error = post_json_stream(
            &target,
            "/api/chat",
            "{}",
            Duration::from_secs(5),
            &mut |line| lines.push(line.to_owned()),
        )
        .unwrap_err();
        assert!(lines.is_empty());
        let notice = error.notice.unwrap();
        assert_eq!(notice.retry_after_seconds, Some(300));
        worker.join().unwrap();
    }

    #[test]
    fn error_sample_stops_without_allocating_or_draining_the_advertised_chunk() {
        let mut body = std::io::Cursor::new(b"fffffff\r\nwait");
        let mut sample = Vec::new();
        read_body(&mut body, Framing::Chunked, 4, true, &mut |part| {
            sample.extend_from_slice(part);
            Ok(())
        })
        .unwrap();
        assert_eq!(sample, b"wait");
    }
}

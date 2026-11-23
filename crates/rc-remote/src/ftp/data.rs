//! Data-channel establishment and bulk byte helpers.
//!
//! FTP opens a second TCP connection for every transfer. In **passive** mode the
//! client connects to a port the server announced (`227`/`229`); in **active**
//! mode the client listens and the server connects back (`PORT`/`EPRT`). Both
//! paths are implemented here, plus the byte helpers the transfer loops need.

use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::error::RemoteError;

use super::control::BoxStream;

/// TLS parameters for the data connection (only with the `ftps` feature).
#[derive(Default)]
pub(crate) struct TlsState {
    /// Connector and server name to wrap data connections with, when `PROT P`
    /// was negotiated.
    #[cfg(feature = "ftps")]
    pub(crate) session: Option<(native_tls::TlsConnector, String)>,
}

/// A data connection that has been announced but not used yet.
///
/// Passive mode only needs the address; active mode keeps its listener, because
/// the server only connects once the transfer command has been sent.
pub(crate) enum Channel {
    /// Client connects out to the announced address.
    Passive(SocketAddr),
    /// Client listens and accepts one connection from the server.
    Active(TcpListener),
}

impl Channel {
    /// Open the actual socket, honouring the configured timeout.
    pub(crate) fn open(self, timeout: Duration, tls: &TlsState) -> Result<BoxStream, RemoteError> {
        match self {
            Channel::Passive(address) => {
                let socket = connect_socket(address, timeout)?;
                wrap(socket, tls)
            }
            Channel::Active(listener) => {
                let socket = accept_with_timeout(&listener, timeout)?;
                wrap(socket, tls)
            }
        }
    }
}

/// Optionally layer TLS on a data socket (no-op without the `ftps` feature).
fn wrap(socket: TcpStream, tls: &TlsState) -> Result<BoxStream, RemoteError> {
    #[cfg(feature = "ftps")]
    if let Some((connector, domain)) = &tls.session {
        let stream = connector.connect(domain, socket).map_err(|e| {
            RemoteError::Protocol(format!("tls handshake on the data connection failed: {e}"))
        })?;
        return Ok(Box::new(stream));
    }
    let _ = tls;
    Ok(Box::new(socket))
}

/// Apply the timeout to both directions and split the socket into a stream.
fn connect_socket(address: SocketAddr, timeout: Duration) -> Result<TcpStream, RemoteError> {
    let socket = TcpStream::connect_timeout(&address, timeout).map_err(|e| match e.kind() {
        std::io::ErrorKind::TimedOut => {
            RemoteError::timeout(format!("opening the data connection to {address}"), timeout)
        }
        _ => RemoteError::Protocol(format!("cannot open the data connection to {address}: {e}")),
    })?;
    let _ = socket.set_nodelay(true);
    let _ = socket.set_read_timeout(Some(timeout));
    let _ = socket.set_write_timeout(Some(timeout));
    Ok(socket)
}

/// Connect the control socket to `host:port`, trying every resolved address.
pub(crate) fn connect_tcp(
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<TcpStream, RemoteError> {
    let trimmed = host.trim();
    let bare = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);
    let connect_error = |reason: String| RemoteError::Connect {
        host: host.to_string(),
        port,
        reason,
    };

    let addresses = (bare, port)
        .to_socket_addrs()
        .map_err(|e| connect_error(format!("cannot resolve the host: {e}")))?;
    let mut last_error: Option<std::io::Error> = None;
    let mut tried = 0usize;
    for address in addresses {
        tried += 1;
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(socket) => {
                let _ = socket.set_nodelay(true);
                let _ = socket.set_read_timeout(Some(timeout));
                let _ = socket.set_write_timeout(Some(timeout));
                return Ok(socket);
            }
            Err(error) => last_error = Some(error),
        }
    }
    if tried == 0 {
        return Err(connect_error(
            "the host did not resolve to any address".to_string(),
        ));
    }
    match last_error {
        Some(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) =>
        {
            Err(RemoteError::Timeout(format!(
                "connecting to {host}:{port} (no answer within {timeout:.1?})"
            )))
        }
        Some(error) => Err(connect_error(error.to_string())),
        None => Err(connect_error(
            "the connection could not be established".to_string(),
        )),
    }
}

/// Listen on `ip` and an ephemeral port for an active-mode transfer.
pub(crate) fn listen(ip: IpAddr) -> Result<TcpListener, RemoteError> {
    TcpListener::bind(SocketAddr::new(ip, 0)).map_err(|e| {
        RemoteError::Protocol(format!(
            "cannot listen for the data connection on {ip}: {e}"
        ))
    })
}

/// The argument of an `EPRT` command (RFC 2428), used for IPv6 and as the
/// modern replacement for `PORT`.
pub(crate) fn eprt_argument(address: SocketAddr) -> String {
    let (family, host) = match address.ip() {
        IpAddr::V4(v4) => (1, v4.to_string()),
        IpAddr::V6(v6) => (2, v6.to_string()),
    };
    format!("|{family}|{host}|{}|", address.port())
}

/// The argument of a `PORT` command (RFC 959); IPv4 only.
pub(crate) fn port_argument(address: SocketAddr) -> Option<String> {
    let IpAddr::V4(v4) = address.ip() else {
        return None;
    };
    let octets = v4.octets();
    let port = address.port();
    Some(format!(
        "{},{},{},{},{},{}",
        octets[0],
        octets[1],
        octets[2],
        octets[3],
        port >> 8,
        port & 0xFF
    ))
}

/// Accept the server's connection for an active-mode transfer, bounded by
/// `timeout` because a broken server would otherwise hang the worker forever.
pub(crate) fn accept_with_timeout(
    listener: &TcpListener,
    timeout: Duration,
) -> Result<TcpStream, RemoteError> {
    listener.set_nonblocking(true).map_err(RemoteError::Io)?;
    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((socket, _peer)) => {
                let _ = listener.set_nonblocking(false);
                let _ = socket.set_nodelay(true);
                let _ = socket.set_read_timeout(Some(timeout));
                let _ = socket.set_write_timeout(Some(timeout));
                return Ok(socket);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    let _ = listener.set_nonblocking(false);
                    return Err(RemoteError::timeout(
                        "waiting for the server's data connection",
                        timeout,
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => {
                let _ = listener.set_nonblocking(false);
                return Err(RemoteError::Io(error));
            }
        }
    }
}

/// Parse a `227 Entering Passive Mode (h1,h2,h3,h4,p1,p2)` reply.
///
/// Takes the last six numbers on the line, which also covers servers that echo
/// the code inside the parenthesis or add a comment before it.
pub(crate) fn parse_pasv(text: &str) -> Option<SocketAddr> {
    let mut numbers: Vec<u64> = Vec::new();
    let mut current: Option<u64> = None;
    for c in text.chars() {
        if let Some(digit) = c.to_digit(10) {
            let value = current.unwrap_or(0);
            current = Some((value.saturating_mul(10)).saturating_add(u64::from(digit)));
        } else if let Some(value) = current.take() {
            numbers.push(value);
        }
    }
    if let Some(value) = current {
        numbers.push(value);
    }
    if numbers.len() < 6 {
        return None;
    }
    let tail = &numbers[numbers.len() - 6..];
    if tail.iter().any(|value| *value > 255) {
        return None;
    }
    let ip = Ipv4Addr::new(tail[0] as u8, tail[1] as u8, tail[2] as u8, tail[3] as u8);
    let port = ((tail[4] as u16) << 8) | tail[5] as u16;
    if port == 0 {
        return None;
    }
    Some(SocketAddr::new(IpAddr::V4(ip), port))
}

/// Is `ip` an address that only makes sense inside the server's own network?
///
/// Servers behind NAT routinely announce their private address in `227`; a
/// client that dials it hangs until the timeout.
fn is_local_only(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

/// Choose the address to dial for a passive transfer.
///
/// A server that announces a private or loopback address while the control
/// connection is not local cannot be reached there; the port is right, so the
/// address of the control connection is used instead. Every other announcement
/// is taken literally (a server may legitimately hand out a different host).
pub(crate) fn passive_address(announced: SocketAddr, peer: Option<SocketAddr>) -> SocketAddr {
    match peer {
        Some(peer) if peer.ip() != announced.ip() && is_local_only(announced.ip()) => {
            SocketAddr::new(peer.ip(), announced.port())
        }
        _ => announced,
    }
}

/// Parse a `229 Entering Extended Passive Mode (|||6446|)` reply.
pub(crate) fn parse_epsv(text: &str) -> Option<u16> {
    let open = text.rfind('(')?;
    let inner = &text[open + 1..];
    let close = inner.find(')')?;
    let inner = &inner[..close];
    let mut chars = inner.chars();
    let delimiter = chars.next()?;
    let body: String = chars.collect();
    let body = body.trim_end_matches(delimiter);
    let port: u16 = body
        .split(delimiter)
        .rfind(|part| !part.is_empty())?
        .parse()
        .ok()?;
    if port == 0 {
        return None;
    }
    Some(port)
}

/// Read until EOF, refusing to buffer more than `cap` bytes.
pub(crate) fn read_all(stream: &mut dyn Read, cap: usize) -> Result<Vec<u8>, RemoteError> {
    let mut out = Vec::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|e| super::control::map_io(e, "reading the data connection"))?;
        if read == 0 {
            return Ok(out);
        }
        if out.len() + read > cap {
            return Err(RemoteError::Protocol(format!(
                "the server sent more than {cap} bytes for a directory listing"
            )));
        }
        out.extend_from_slice(&buffer[..read]);
    }
}

/// Split a listing into lines, stripping CRLF and skipping empty lines.
pub(crate) fn split_lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for line in bytes.split(|&b| b == b'\n') {
        let line = if line.last() == Some(&b'\r') {
            &line[..line.len() - 1]
        } else {
            line
        };
        if !line.is_empty() {
            out.push(line.to_vec());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasv_replies_are_parsed_in_every_shape() {
        assert_eq!(
            parse_pasv("Entering Passive Mode (127,0,0,1,200,45)"),
            Some("127.0.0.1:51245".parse().unwrap())
        );
        assert_eq!(
            parse_pasv("227 Entering Passive Mode (10,1,2,3,4,5)."),
            Some("10.1.2.3:1029".parse().unwrap())
        );
        assert_eq!(
            parse_pasv("Entering Passive Mode (192,168,0,1,255,255)"),
            Some("192.168.0.1:65535".parse().unwrap())
        );
        assert_eq!(
            parse_pasv("Entering Passive Mode 127,0,0,1,4,1"),
            Some("127.0.0.1:1025".parse().unwrap())
        );
        assert_eq!(parse_pasv("no numbers here"), None);
        assert_eq!(parse_pasv("(300,0,0,1,4,1)"), None);
        assert_eq!(parse_pasv("(127,0,0,1,0,0)"), None);
        assert_eq!(parse_pasv("(127,0,0,1,4)"), None);
        assert_eq!(parse_pasv(""), None);
    }

    #[test]
    fn epsv_replies_are_parsed() {
        assert_eq!(
            parse_epsv("Entering Extended Passive Mode (|||6446|)"),
            Some(6446)
        );
        assert_eq!(parse_epsv("(!!!30000!)"), Some(30000));
        assert_eq!(parse_epsv("Entering Extended Passive Mode (|||0|)"), None);
        assert_eq!(parse_epsv("garbage"), None);
        assert_eq!(parse_epsv("()"), None);
    }

    #[test]
    fn port_and_eprt_arguments_are_formatted() {
        let v4: SocketAddr = "192.168.1.10:40001".parse().unwrap();
        assert_eq!(port_argument(v4).as_deref(), Some("192,168,1,10,156,65"));
        assert_eq!(eprt_argument(v4), "|1|192.168.1.10|40001|");
        let v6: SocketAddr = "[::1]:2121".parse().unwrap();
        assert_eq!(port_argument(v6), None);
        assert_eq!(eprt_argument(v6), "|2|::1|2121|");
    }

    #[test]
    fn lines_are_split_and_trimmed() {
        let lines = split_lines(b"a\r\nb\n\nc");
        assert_eq!(lines, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
        assert!(split_lines(b"").is_empty());
        assert!(split_lines(b"\r\n\r\n").is_empty());
        assert_eq!(split_lines(b"\n").len(), 0);
    }

    #[test]
    fn read_all_honours_the_cap() {
        let mut source: &[u8] = b"0123456789";
        assert_eq!(read_all(&mut source, 10).unwrap(), b"0123456789");
        let mut source: &[u8] = b"0123456789";
        assert!(read_all(&mut source, 4).is_err());
        let mut empty: &[u8] = b"";
        assert!(read_all(&mut empty, 4).unwrap().is_empty());
    }

    #[test]
    fn passive_addresses_behind_nat_fall_back_to_the_control_peer() {
        let announced: SocketAddr = "10.0.0.5:40000".parse().unwrap();
        let peer: SocketAddr = "192.0.2.10:40000".parse().unwrap();
        assert_eq!(
            passive_address(announced, Some(peer)),
            "192.0.2.10:40000".parse().unwrap()
        );
        // A public announcement is taken literally.
        let public: SocketAddr = "198.51.100.7:40000".parse().unwrap();
        assert_eq!(passive_address(public, Some(peer)), public);
        // So is the server's own loopback when we are talking to it locally.
        let local: SocketAddr = "127.0.0.1:40000".parse().unwrap();
        assert_eq!(passive_address(announced, Some(local)), local);
        assert_eq!(passive_address(announced, None), announced);
    }

    #[test]
    fn connecting_to_a_closed_port_fails_quickly() {
        // Bind and immediately drop, so the port is (almost certainly) free.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let error = connect_tcp(
            &address.ip().to_string(),
            address.port(),
            Duration::from_millis(500),
        )
        .unwrap_err();
        assert!(matches!(error, RemoteError::Connect { .. }), "{error:?}");
    }

    #[test]
    fn a_listener_accepts_a_connection_within_the_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || connect_socket(address, Duration::from_secs(2)));
        let socket = accept_with_timeout(&listener, Duration::from_secs(2)).unwrap();
        assert!(socket.peer_addr().is_ok());
        assert!(handle.join().unwrap().is_ok());
    }

    #[test]
    fn accepting_without_a_client_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let error = accept_with_timeout(&listener, Duration::from_millis(80)).unwrap_err();
        assert!(matches!(error, RemoteError::Timeout(_)), "{error:?}");
    }
}

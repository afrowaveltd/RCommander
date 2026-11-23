//! Byte-stream plumbing for the control channel.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Duration;

use crate::error::RemoteError;

/// Longest control line accepted before the server is considered hostile.
pub(crate) const MAX_LINE: usize = 8 * 1024;

/// A byte stream usable as the FTP control or data connection.
///
/// It exists so that plain TCP and (with the `ftps` feature) TLS connections can
/// live behind the same boxed type: FTP commands and data are byte streams, but
/// their concrete type differs per transport.
pub(crate) trait Stream: Read + Write + Send {
    /// Apply the socket timeouts (implementations ignore unsupported values).
    fn set_timeouts(&self, timeout: Option<Duration>);
    /// Half-close the connection, which is how an upload signals EOF.
    fn shutdown_write(&self);
    /// Local address of the connection, if the transport has one.
    fn local_addr(&self) -> Option<SocketAddr>;
    /// Peer address of the connection, if the transport has one.
    fn peer_addr(&self) -> Option<SocketAddr>;
    /// Recover the underlying TCP socket; `None` once TLS has been layered on.
    #[cfg(feature = "ftps")]
    fn take_tcp(self: Box<Self>) -> Option<TcpStream>;
}

impl Stream for TcpStream {
    fn set_timeouts(&self, timeout: Option<Duration>) {
        let _ = self.set_read_timeout(timeout);
        let _ = self.set_write_timeout(timeout);
    }

    fn shutdown_write(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        TcpStream::local_addr(self).ok()
    }

    fn peer_addr(&self) -> Option<SocketAddr> {
        TcpStream::peer_addr(self).ok()
    }

    #[cfg(feature = "ftps")]
    fn take_tcp(self: Box<Self>) -> Option<TcpStream> {
        Some(*self)
    }
}

#[cfg(feature = "ftps")]
impl Stream for native_tls::TlsStream<TcpStream> {
    fn set_timeouts(&self, timeout: Option<Duration>) {
        let raw = self.get_ref();
        let _ = raw.set_read_timeout(timeout);
        let _ = raw.set_write_timeout(timeout);
    }

    fn shutdown_write(&self) {
        let _ = self.get_ref().shutdown(Shutdown::Write);
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.get_ref().local_addr().ok()
    }

    fn peer_addr(&self) -> Option<SocketAddr> {
        self.get_ref().peer_addr().ok()
    }

    #[cfg(feature = "ftps")]
    fn take_tcp(self: Box<Self>) -> Option<TcpStream> {
        None
    }
}

/// A boxed [`Stream`].
pub(crate) type BoxStream = Box<dyn Stream>;

/// Buffered line access to the control connection.
pub(crate) struct Control {
    stream: BufReader<BoxStream>,
}

impl Control {
    /// Wrap a freshly connected stream.
    pub(crate) fn new(stream: BoxStream) -> Self {
        Control {
            stream: BufReader::new(stream),
        }
    }

    /// The underlying stream, for addresses and timeout tweaks.
    pub(crate) fn stream(&self) -> &dyn Stream {
        &**self.stream.get_ref()
    }

    /// Change the socket timeouts.
    pub(crate) fn set_timeouts(&mut self, timeout: Option<Duration>) {
        self.stream.get_ref().set_timeouts(timeout);
    }

    /// Send raw bytes (a complete command including CRLF).
    pub(crate) fn write_all(&mut self, bytes: &[u8]) -> Result<(), RemoteError> {
        let stream = self.stream.get_mut();
        stream
            .write_all(bytes)
            .map_err(|e| map_io(e, "sending a command"))?;
        stream.flush().map_err(|e| map_io(e, "flushing a command"))
    }

    /// Read one CRLF terminated line, without the terminator.
    ///
    /// The line is returned as bytes because the character set of the control
    /// channel belongs to the caller. A line longer than [`MAX_LINE`] is a
    /// protocol error rather than an unbounded allocation.
    pub(crate) fn read_line(&mut self) -> Result<Vec<u8>, RemoteError> {
        let mut out = Vec::new();
        loop {
            let (found, consumed) = {
                let buffered = self
                    .stream
                    .fill_buf()
                    .map_err(|e| map_io(e, "reading a reply"))?;
                if buffered.is_empty() {
                    return Err(if out.is_empty() {
                        RemoteError::Protocol(
                            "the server closed the control connection".to_string(),
                        )
                    } else {
                        RemoteError::Protocol("the server sent a truncated reply".to_string())
                    });
                }
                match buffered.iter().position(|&b| b == b'\n') {
                    Some(position) => {
                        out.extend_from_slice(&buffered[..position]);
                        (true, position + 1)
                    }
                    None => {
                        out.extend_from_slice(buffered);
                        (false, buffered.len())
                    }
                }
            };
            self.stream.consume(consumed);
            if found {
                break;
            }
            if out.len() > MAX_LINE {
                return Err(RemoteError::Protocol(
                    "the server sent an overlong control line".to_string(),
                ));
            }
        }
        if out.last() == Some(&b'\r') {
            out.pop();
        }
        if out.len() > MAX_LINE {
            return Err(RemoteError::Protocol(
                "the server sent an overlong control line".to_string(),
            ));
        }
        Ok(out)
    }

    /// Give up the stream, e.g. to upgrade it to TLS.
    ///
    /// The buffer must be empty: dropping buffered bytes would desynchronise the
    /// protocol, so this refuses instead of guessing.
    #[cfg(feature = "ftps")]
    pub(crate) fn into_stream(self) -> Result<BoxStream, RemoteError> {
        if !self.stream.buffer().is_empty() {
            return Err(RemoteError::Protocol(
                "unexpected buffered data on the control connection".to_string(),
            ));
        }
        Ok(self.stream.into_inner())
    }
}

/// Translate a socket error into the error the GUI understands.
pub(crate) fn map_io(error: std::io::Error, what: &str) -> RemoteError {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            RemoteError::Timeout(format!("{what}: the server did not answer in time"))
        }
        std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::ConnectionAborted
        | std::io::ErrorKind::BrokenPipe => {
            RemoteError::Protocol(format!("{what}: the connection was closed by the server"))
        }
        _ => RemoteError::Io(error),
    }
}

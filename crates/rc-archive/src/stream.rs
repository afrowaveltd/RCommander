//! Chunked, cancellable copying between a member and a writer.

use std::io::{self, Read, Write};
use std::path::Path;

use rc_core::ops::CancelToken;

use crate::error::{ArchiveError, Result};

/// Size of the copy buffer used for every `copy_to`/extraction path.
pub(crate) const CHUNK: usize = 64 * 1024;

/// Copy `reader` into `writer` in chunks.
///
/// * `cancel` — polled before every chunk, so a cancelled extraction stops
///   within one chunk instead of after the member.
/// * `expect_crc` — when `Some`, the running CRC-32 of the copied bytes is
///   compared against it at the end; a mismatch is reported as
///   [`ArchiveError::Malformed`] (this is how a damaged zip member is detected
///   without trusting the container's metadata).
/// * `archive`/`member` — used only to build readable error messages.
pub(crate) fn pump(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    cancel: Option<&CancelToken>,
    expect_crc: Option<u32>,
    archive: &Path,
    member: &str,
) -> Result<u64> {
    let mut buffer = vec![0u8; CHUNK];
    let mut total = 0u64;
    let mut hasher = expect_crc.map(|_| crc32fast::Hasher::new());

    loop {
        if let Some(token) = cancel
            && token.is_cancelled()
        {
            return Err(ArchiveError::Cancelled);
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(ArchiveError::io(archive, e)),
        };
        if let Some(hasher) = hasher.as_mut() {
            hasher.update(&buffer[..read]);
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|e| ArchiveError::io(archive, e))?;
        total += read as u64;
    }

    if let (Some(hasher), Some(expected)) = (hasher, expect_crc)
        && hasher.finalize() != expected
    {
        return Err(ArchiveError::Malformed(format!(
            "crc32 mismatch for member {member:?} in {}",
            archive.display()
        )));
    }
    Ok(total)
}

/// [`pump`] into a fresh `Vec` — used by [`crate::ArchiveRead::read`].
pub(crate) fn pump_vec(
    reader: &mut dyn Read,
    cancel: Option<&CancelToken>,
    expect_crc: Option<u32>,
    archive: &Path,
    member: &str,
) -> Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    pump(reader, &mut out, cancel, expect_crc, archive, member)?;
    Ok(out)
}

/// A writer that fails once the token is set.
///
/// Extraction wraps the destination file in this adapter, which lets *any*
/// backend (even one that only knows how to write into a `Write`) stop
/// mid-member; the caller then turns the error into
/// [`ArchiveError::Cancelled`].
///
/// The error kind is deliberately **not** [`io::ErrorKind::Interrupted`]:
/// [`std::io::Write::write_all`] (and `io::copy`) retry that kind forever, which
/// would turn a cancellation into an infinite loop.
pub(crate) struct CancelWriter<'a> {
    inner: &'a mut dyn Write,
    cancel: &'a CancelToken,
}

impl<'a> CancelWriter<'a> {
    /// Wrap `inner`, polling `cancel` before each write.
    pub(crate) fn new(inner: &'a mut dyn Write, cancel: &'a CancelToken) -> Self {
        CancelWriter { inner, cancel }
    }
}

impl Write for CancelWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::other("cancelled by user"));
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn pump_copies_bytes_and_counts_them() {
        let mut reader = Cursor::new(vec![7u8; CHUNK + 5]);
        let mut out = Vec::new();
        let n = pump(
            &mut reader,
            &mut out,
            None,
            None,
            Path::new("/tmp/a.zip"),
            "m",
        )
        .unwrap();
        assert_eq!(n as usize, CHUNK + 5);
        assert_eq!(out.len(), n as usize);
    }

    #[test]
    fn pump_verifies_the_crc_when_asked() {
        let data = b"hello archive".to_vec();
        let crc = crc32fast::hash(&data);

        let mut reader = Cursor::new(data.clone());
        let mut out = Vec::new();
        assert_eq!(
            pump(
                &mut reader,
                &mut out,
                None,
                Some(crc),
                Path::new("/tmp/a.zip"),
                "m"
            )
            .unwrap(),
            data.len() as u64
        );

        let mut reader = Cursor::new(data);
        let mut out = Vec::new();
        let err = pump(
            &mut reader,
            &mut out,
            None,
            Some(crc ^ 0xFF),
            Path::new("/tmp/a.zip"),
            "m",
        )
        .unwrap_err();
        match err {
            ArchiveError::Malformed(message) => assert!(message.contains("crc32"), "{message}"),
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn pump_stops_when_the_token_is_set() {
        let token = CancelToken::new();
        token.cancel();
        let mut reader = Cursor::new(vec![1u8; 1024]);
        let mut out = Vec::new();
        let err = pump(
            &mut reader,
            &mut out,
            Some(&token),
            None,
            Path::new("/tmp/a.zip"),
            "m",
        )
        .unwrap_err();
        assert!(err.is_cancelled());
        assert!(out.is_empty());
    }

    #[test]
    fn cancel_writer_refuses_writes_after_cancellation() {
        let token = CancelToken::new();
        let mut sink = Vec::new();
        {
            let mut writer = CancelWriter::new(&mut sink, &token);
            assert!(writer.write_all(b"ok").is_ok());
            token.cancel();
            let err = writer.write_all(b"more").unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::Other);
            assert!(err.to_string().contains("cancelled"));
        }
        assert_eq!(sink, b"ok");
    }

    #[test]
    fn pump_reports_io_errors_with_the_archive_path() {
        struct Failing;
        impl Read for Failing {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::InvalidData, "boom"))
            }
        }
        let mut out = Vec::new();
        let err = pump(
            &mut Failing,
            &mut out,
            None,
            None,
            Path::new("/tmp/broken.zip"),
            "m",
        )
        .unwrap_err();
        assert_eq!(err.path(), Some(Path::new("/tmp/broken.zip")));
        assert!(err.to_string().contains("boom"));
    }
}

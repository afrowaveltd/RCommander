//! One reader implementation per container family.

pub(crate) mod sevenz;
pub(crate) mod single;
pub(crate) mod tar;
#[cfg(test)]
pub(crate) mod testutil;
pub(crate) mod zip;

use std::io::Read;

use crate::error::{ArchiveError, Result};
use crate::kind::ArchiveKind;
use crate::member;

/// Wrap `reader` in the streaming decompressor that matches `kind`.
///
/// `ArchiveKind::Tar` returns the reader unchanged. Every decoder is used as a
/// plain [`Read`] adapter, which is what allows the tar and single-file
/// backends to stream without buffering the whole member.
pub(crate) fn decompress<'a>(
    kind: ArchiveKind,
    reader: Box<dyn Read + 'a>,
) -> Result<Box<dyn Read + 'a>> {
    // `TarGz`, `TarBz2`, ... are tar *containers*: what this function has to
    // strip is the bare compressor underneath.
    let kind = kind.compressor();
    let reader: Box<dyn Read + 'a> = match kind {
        ArchiveKind::Tar => reader,
        ArchiveKind::Gz => Box::new(flate2::read::MultiGzDecoder::new(reader)),
        ArchiveKind::Bz2 => Box::new(bzip2::read::MultiBzDecoder::new(reader)),
        ArchiveKind::Xz => Box::new(lzma_rust2::XzReader::new(reader, true)),
        ArchiveKind::Zst => match ruzstd::decoding::StreamingDecoder::new(reader) {
            Ok(decoder) => Box::new(decoder),
            Err(e) => return Err(ArchiveError::malformed(format!("zstd: {e}"))),
        },
        other => {
            return Err(ArchiveError::unsupported(format!(
                "no decompressor for {}",
                other.name()
            )));
        }
    };
    Ok(reader)
}

/// Map an [`std::io::Error`] coming from a container parser.
///
/// Structural problems become [`ArchiveError::Malformed`]; everything else stays
/// an [`ArchiveError::Io`] so callers can still see the underlying error.
pub(crate) fn parse_error(path: &std::path::Path, error: std::io::Error) -> ArchiveError {
    match error.kind() {
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
            ArchiveError::Malformed(format!("{}: {error}", path.display()))
        }
        _ => ArchiveError::io(path, error),
    }
}

/// Look up the index of `path` in a normalised path table.
pub(crate) fn lookup(
    table: &std::collections::HashMap<String, usize>,
    path: &str,
) -> Option<usize> {
    table.get(&member::normalize(path)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// `unwrap_err` needs the success type to be `Debug`, which `Box<dyn Read>`
    /// is not; this helper keeps the tests readable.
    fn expect_error<T>(result: Result<T>) -> ArchiveError {
        match result {
            Ok(_) => panic!("expected an error"),
            Err(error) => error,
        }
    }

    #[test]
    fn tar_returns_the_reader_unchanged() {
        let data = b"plain bytes";
        let mut reader = decompress(
            ArchiveKind::Tar,
            Box::new(std::io::Cursor::new(data.to_vec())),
        )
        .unwrap();
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn unsupported_kinds_are_reported_not_panicked() {
        let err = expect_error(decompress(
            ArchiveKind::Zip,
            Box::new(std::io::Cursor::new(Vec::new())),
        ));
        assert!(matches!(err, ArchiveError::Unsupported(_)));
        let err = expect_error(decompress(
            ArchiveKind::Rar,
            Box::new(std::io::Cursor::new(Vec::new())),
        ));
        assert!(err.to_string().contains("RAR"));
    }

    #[test]
    fn corrupt_zstd_header_is_an_error() {
        let err = expect_error(decompress(
            ArchiveKind::Zst,
            Box::new(std::io::Cursor::new(vec![0u8; 4])),
        ));
        assert!(matches!(err, ArchiveError::Malformed(_)), "{err:?}");
    }

    #[test]
    fn parse_errors_are_classified() {
        let malformed = parse_error(
            std::path::Path::new("a.tar"),
            std::io::Error::new(std::io::ErrorKind::InvalidData, "bad"),
        );
        assert!(matches!(malformed, ArchiveError::Malformed(_)));
        let io = parse_error(
            std::path::Path::new("a.tar"),
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        );
        assert!(matches!(io, ArchiveError::Io { .. }));
    }

    #[test]
    fn lookup_normalises_the_query() {
        let mut table = std::collections::HashMap::new();
        table.insert("a/b.txt".to_string(), 3usize);
        assert_eq!(lookup(&table, "a/b.txt"), Some(3));
        assert_eq!(lookup(&table, "/a\\b.txt"), Some(3));
        assert_eq!(lookup(&table, "other"), None);
    }
}

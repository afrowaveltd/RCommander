//! Fixture builders shared by the backend tests.
//!
//! Only `ruzstd` is a dependency and it can only *decode*, so the tests build a
//! legal zstd frame out of raw (uncompressed) blocks. That keeps the fixture
//! dependency-free and still exercises the frame parser.

/// Wrap `data` in a legal zstd frame that uses a single raw (uncompressed) block.
///
/// * `data.len()` must be in `256..=65_791`, which is what the two-byte frame
///   content size can express (`size - 256`).
/// * Raw blocks are limited to 128 KiB, so larger payloads are split.
pub(crate) fn zstd_raw_frame(data: &[u8]) -> Vec<u8> {
    assert!(
        (256..=65_791).contains(&data.len()),
        "test payload must fit a 2-byte frame content size"
    );
    let mut out = Vec::with_capacity(data.len() + 16);
    // Magic number.
    out.extend_from_slice(&[0x28, 0xb5, 0x2f, 0xfd]);
    // Frame header descriptor: content-size flag 1 (2 bytes, value-256),
    // single-segment flag set, no checksum, no dictionary id.
    out.push(0b0110_0000);
    out.extend_from_slice(&((data.len() - 256) as u16).to_le_bytes());

    let mut chunks = data.chunks(128 * 1024).peekable();
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        // Block header: last-block bit, block type 0 (raw), size << 3.
        let header = u32::from(last) | ((chunk.len() as u32) << 3);
        out.extend_from_slice(&header.to_le_bytes()[..3]);
        out.extend_from_slice(chunk);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn raw_frames_decode_with_ruzstd() {
        let data = vec![b'q'; 4096];
        let frame = zstd_raw_frame(&data);
        let mut decoder =
            ruzstd::decoding::StreamingDecoder::new(std::io::Cursor::new(frame)).unwrap();
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).unwrap();
        assert_eq!(out, data);
    }
}

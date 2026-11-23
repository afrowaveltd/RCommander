//! Archive container detection.
//!
//! Detection uses the magic bytes first and the file name second, which is what
//! a dual-pane file manager needs: `foo.zip` that actually contains a gzip
//! stream is still opened as gzip, while a nameless blob in `/tmp` is not.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// How many leading bytes [`detect`] looks at: enough for the `ustar` marker of
/// a tar header, which sits at offset 257.
const HEAD_LEN: usize = 512;

/// What kind of container a file is, decided by magic bytes first, extension
/// second.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArchiveKind {
    /// A zip archive (`PK\x03\x04`, `PK\x05\x06`, `PK\x07\x08`).
    Zip,
    /// An uncompressed tar archive (no signature; the `ustar` marker at offset
    /// 257 is used, plus the `.tar` extension).
    Tar,
    /// A gzip-compressed tar archive (`.tar.gz`, `.tgz`).
    TarGz,
    /// A bzip2-compressed tar archive (`.tar.bz2`, `.tbz`, `.tbz2`).
    TarBz2,
    /// An xz-compressed tar archive (`.tar.xz`, `.txz`).
    TarXz,
    /// A zstd-compressed tar archive (`.tar.zst`, `.tzst`).
    TarZst,
    /// A single gzip-compressed file (`.gz`).
    Gz,
    /// A single bzip2-compressed file (`.bz2`).
    Bz2,
    /// A single xz-compressed file (`.xz`).
    Xz,
    /// A single zstd-compressed file (`.zst`).
    Zst,
    /// A 7-zip archive (`.7z`).
    SevenZip,
    /// A RAR archive — recognised, not decodable.
    Rar,
    /// A Microsoft cabinet archive — recognised, not decodable.
    Cab,
    /// Not an archive as far as this crate can tell.
    Unknown,
}

impl ArchiveKind {
    /// Short upper-case label used in descriptions and error messages, e.g.
    /// `ZIP`, `TAR.GZ`, `7ZIP`.
    pub fn name(self) -> &'static str {
        match self {
            ArchiveKind::Zip => "ZIP",
            ArchiveKind::Tar => "TAR",
            ArchiveKind::TarGz => "TAR.GZ",
            ArchiveKind::TarBz2 => "TAR.BZ2",
            ArchiveKind::TarXz => "TAR.XZ",
            ArchiveKind::TarZst => "TAR.ZST",
            ArchiveKind::Gz => "GZIP",
            ArchiveKind::Bz2 => "BZIP2",
            ArchiveKind::Xz => "XZ",
            ArchiveKind::Zst => "ZSTD",
            ArchiveKind::SevenZip => "7ZIP",
            ArchiveKind::Rar => "RAR",
            ArchiveKind::Cab => "CAB",
            ArchiveKind::Unknown => "unknown",
        }
    }

    /// True for every kind except [`ArchiveKind::Unknown`].
    pub fn is_known(self) -> bool {
        self != ArchiveKind::Unknown
    }

    /// True for the tar family, compressed or not.
    pub fn is_tar(self) -> bool {
        matches!(
            self,
            ArchiveKind::Tar
                | ArchiveKind::TarGz
                | ArchiveKind::TarBz2
                | ArchiveKind::TarXz
                | ArchiveKind::TarZst
        )
    }

    /// True for a compressed tar container (`tar.gz`, `tar.bz2`, `tar.xz`,
    /// `tar.zst`), i.e. a tar stream behind a compressor.
    pub fn is_compressed_tar(self) -> bool {
        matches!(
            self,
            ArchiveKind::TarGz | ArchiveKind::TarBz2 | ArchiveKind::TarXz | ArchiveKind::TarZst
        )
    }

    /// True for the compressors that wrap a *single* file rather than a tar
    /// stream (`gz`, `bz2`, `xz`, `zst`).
    pub fn is_single_file_compressed(self) -> bool {
        matches!(
            self,
            ArchiveKind::Gz | ArchiveKind::Bz2 | ArchiveKind::Xz | ArchiveKind::Zst
        )
    }

    /// The bare compressor behind this kind: `TarGz -> Gz`, `Tar -> Tar`.
    pub fn compressor(self) -> ArchiveKind {
        match self {
            ArchiveKind::TarGz => ArchiveKind::Gz,
            ArchiveKind::TarBz2 => ArchiveKind::Bz2,
            ArchiveKind::TarXz => ArchiveKind::Xz,
            ArchiveKind::TarZst => ArchiveKind::Zst,
            other => other,
        }
    }
}

impl std::fmt::Display for ArchiveKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// File-name suffixes this crate understands, without a leading dot.
///
/// Multi-part suffixes (`tar.gz`, `tar.bz2`, `tar.xz`, `tar.zst`) are listed
/// besides the short aliases (`tgz`, `tbz`, ...). Matching is suffix based, see
/// [`is_archive_path`].
const EXTENSIONS: &[&str] = &[
    "7z", "bz2", "cab", "gz", "rar", "tar", "tar.bz2", "tar.gz", "tar.xz", "tar.zst", "tbz",
    "tbz2", "tgz", "txz", "tzst", "xz", "zip", "zst",
];

/// Longest-suffix-first rules that map a file name to an [`ArchiveKind`].
///
/// The order matters: `x.tar.gz` must match `tar.gz` before `gz`, and
/// `x.tbz2` must match `tbz2` before `bz2`.
const NAME_RULES: &[(&str, ArchiveKind)] = &[
    ("tar.gz", ArchiveKind::TarGz),
    ("tar.bz2", ArchiveKind::TarBz2),
    ("tar.zst", ArchiveKind::TarZst),
    ("tar.zstd", ArchiveKind::TarZst),
    ("tar.xz", ArchiveKind::TarXz),
    ("tgz", ArchiveKind::TarGz),
    ("tbz2", ArchiveKind::TarBz2),
    ("tbz", ArchiveKind::TarBz2),
    ("txz", ArchiveKind::TarXz),
    ("tzst", ArchiveKind::TarZst),
    ("tzstd", ArchiveKind::TarZst),
    ("zip", ArchiveKind::Zip),
    ("7z", ArchiveKind::SevenZip),
    ("rar", ArchiveKind::Rar),
    ("cab", ArchiveKind::Cab),
    ("tar", ArchiveKind::Tar),
    ("gz", ArchiveKind::Gz),
    ("bz2", ArchiveKind::Bz2),
    ("xz", ArchiveKind::Xz),
    ("zst", ArchiveKind::Zst),
    ("zstd", ArchiveKind::Zst),
];

/// Classify a file by its name only.
pub(crate) fn kind_from_name(name: &str) -> ArchiveKind {
    let lower = name.to_ascii_lowercase();
    for (rule, kind) in NAME_RULES {
        if lower.len() > rule.len() && lower.ends_with(rule) {
            let boundary = lower.as_bytes()[lower.len() - rule.len() - 1];
            if boundary == b'.' {
                return *kind;
            }
        }
    }
    ArchiveKind::Unknown
}

/// Classify a buffer by its magic bytes only.
fn kind_from_magic(head: &[u8]) -> ArchiveKind {
    fn starts(head: &[u8], magic: &[u8]) -> bool {
        head.len() >= magic.len() && &head[..magic.len()] == magic
    }

    // Zip: local file header, empty archive, spanned marker, zip64 EOCD.
    if starts(head, b"PK\x03\x04")
        || starts(head, b"PK\x05\x06")
        || starts(head, b"PK\x07\x08")
        || starts(head, b"PK\x06\x06")
    {
        return ArchiveKind::Zip;
    }
    if starts(head, &[0x1f, 0x8b]) {
        return ArchiveKind::Gz;
    }
    if starts(head, b"BZh") && head.len() >= 4 && head[3].is_ascii_digit() {
        return ArchiveKind::Bz2;
    }
    if starts(head, &[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
        return ArchiveKind::Xz;
    }
    if starts(head, &[0x28, 0xb5, 0x2f, 0xfd]) {
        return ArchiveKind::Zst;
    }
    if starts(head, b"7z\xbc\xaf\x27\x1c") {
        return ArchiveKind::SevenZip;
    }
    if starts(head, b"Rar!\x1a\x07") {
        return ArchiveKind::Rar;
    }
    if starts(head, b"MSCF") {
        return ArchiveKind::Cab;
    }
    // tar has no signature at offset 0; the POSIX marker lives at offset 257.
    if head.len() >= 262 && &head[257..262] == b"ustar" {
        return ArchiveKind::Tar;
    }
    ArchiveKind::Unknown
}

/// Refine a single-file compressor into its tar variant when the name says so.
fn refine_with_name(name: &str, single: ArchiveKind, tar: ArchiveKind) -> ArchiveKind {
    if kind_from_name(name) == tar {
        tar
    } else {
        single
    }
}

/// What kind of container a file is, decided by magic bytes first, extension
/// second.
///
/// The whole `head` buffer is inspected, so passing at least 512 bytes lets tar
/// be recognised. A buffer shorter than the magic it would need simply falls
/// through to the extension rule.
pub fn detect_bytes(name: &str, head: &[u8]) -> ArchiveKind {
    match kind_from_magic(head) {
        ArchiveKind::Gz => refine_with_name(name, ArchiveKind::Gz, ArchiveKind::TarGz),
        ArchiveKind::Bz2 => refine_with_name(name, ArchiveKind::Bz2, ArchiveKind::TarBz2),
        ArchiveKind::Xz => refine_with_name(name, ArchiveKind::Xz, ArchiveKind::TarXz),
        ArchiveKind::Zst => refine_with_name(name, ArchiveKind::Zst, ArchiveKind::TarZst),
        ArchiveKind::Unknown => kind_from_name(name),
        other => other,
    }
}

/// Read at most [`HEAD_LEN`] bytes from the start of `path`.
fn read_head(path: &Path) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    let mut head = Vec::with_capacity(HEAD_LEN);
    // `take` keeps a huge file from being read; a short read is fine.
    file.take(HEAD_LEN as u64).read_to_end(&mut head).ok()?;
    Some(head)
}

/// What kind of container a file is, decided by magic bytes first, extension
/// second.
///
/// Unreadable paths fall back to the extension rule, so `detect` never fails and
/// never panics; a path with no useful extension that cannot be opened is
/// [`ArchiveKind::Unknown`].
pub fn detect(path: &Path) -> ArchiveKind {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match read_head(path) {
        Some(head) => detect_bytes(&name, &head),
        None => kind_from_name(&name),
    }
}

/// True when `path` looks like an archive this crate (or the packer dialog)
/// should treat as one.
///
/// Conservative: a known extension is enough, and magic bytes are only consulted
/// when the extension says nothing.
pub fn is_archive_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if kind_from_name(&name).is_known() {
        return true;
    }
    detect(path).is_known()
}

/// The default list of archive suffixes, without a leading dot.
///
/// Matching is suffix based, so `archive.tar.gz` matches both `tar.gz` and `gz`.
pub fn extensions() -> &'static [&'static str] {
    EXTENSIONS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tar_head() -> Vec<u8> {
        let mut head = vec![0u8; HEAD_LEN];
        head[..3].copy_from_slice(b"abc");
        head[257..262].copy_from_slice(b"ustar");
        head
    }

    #[test]
    fn magic_bytes_identify_every_container() {
        assert_eq!(detect_bytes("x", b"PK\x03\x04rest"), ArchiveKind::Zip);
        assert_eq!(detect_bytes("x", b"PK\x05\x06"), ArchiveKind::Zip);
        assert_eq!(
            detect_bytes("x", &[0x1f, 0x8b, 0x08, 0x00]),
            ArchiveKind::Gz
        );
        assert_eq!(detect_bytes("x", b"BZh9some"), ArchiveKind::Bz2);
        assert_eq!(
            detect_bytes("x", &[0xfd, b'7', b'z', b'X', b'Z', 0x00, 0x00]),
            ArchiveKind::Xz
        );
        assert_eq!(
            detect_bytes("x", &[0x28, 0xb5, 0x2f, 0xfd, 0x00]),
            ArchiveKind::Zst
        );
        assert_eq!(
            detect_bytes("x", b"7z\xbc\xaf\x27\x1c"),
            ArchiveKind::SevenZip
        );
        assert_eq!(detect_bytes("x", b"Rar!\x1a\x07\x00"), ArchiveKind::Rar);
        assert_eq!(detect_bytes("x", b"Rar!\x1a\x07\x01\x00"), ArchiveKind::Rar);
        assert_eq!(detect_bytes("x", b"MSCF\x00\x00"), ArchiveKind::Cab);
        assert_eq!(detect_bytes("payload", &tar_head()), ArchiveKind::Tar);
    }

    #[test]
    fn extension_rules_cover_short_and_multi_part_suffixes() {
        assert_eq!(detect_bytes("a.zip", b"nope"), ArchiveKind::Zip);
        assert_eq!(detect_bytes("a.tar.gz", b"nope"), ArchiveKind::TarGz);
        assert_eq!(detect_bytes("a.tgz", b""), ArchiveKind::TarGz);
        assert_eq!(detect_bytes("a.tar.bz2", b""), ArchiveKind::TarBz2);
        assert_eq!(detect_bytes("a.tbz", b""), ArchiveKind::TarBz2);
        assert_eq!(detect_bytes("a.tbz2", b""), ArchiveKind::TarBz2);
        assert_eq!(detect_bytes("a.tar.xz", b""), ArchiveKind::TarXz);
        assert_eq!(detect_bytes("a.txz", b""), ArchiveKind::TarXz);
        assert_eq!(detect_bytes("a.tar.zst", b""), ArchiveKind::TarZst);
        assert_eq!(detect_bytes("a.tzst", b""), ArchiveKind::TarZst);
        assert_eq!(detect_bytes("a.tar", b""), ArchiveKind::Tar);
        assert_eq!(detect_bytes("A.TAR.GZ", b""), ArchiveKind::TarGz);
        assert_eq!(detect_bytes("a.gz", b""), ArchiveKind::Gz);
        assert_eq!(detect_bytes("a.bz2", b""), ArchiveKind::Bz2);
        assert_eq!(detect_bytes("a.xz", b""), ArchiveKind::Xz);
        assert_eq!(detect_bytes("a.zst", b""), ArchiveKind::Zst);
        assert_eq!(detect_bytes("a.7z", b""), ArchiveKind::SevenZip);
        assert_eq!(detect_bytes("a.rar", b""), ArchiveKind::Rar);
        assert_eq!(detect_bytes("a.cab", b""), ArchiveKind::Cab);
        assert_eq!(detect_bytes("noext", b""), ArchiveKind::Unknown);
        assert_eq!(detect_bytes("a.zipx", b""), ArchiveKind::Unknown);
    }

    #[test]
    fn magic_wins_over_a_misleading_extension() {
        // A zip that is called .tar.gz is still a zip.
        assert_eq!(
            detect_bytes("misleading.tar.gz", b"PK\x03\x04"),
            ArchiveKind::Zip
        );
        // A gzip stream called .gz is a single member; called .tgz it is a tar.
        assert_eq!(
            detect_bytes("payload.gz", &[0x1f, 0x8b, 0x08, 0x00]),
            ArchiveKind::Gz
        );
        assert_eq!(
            detect_bytes("payload.tgz", &[0x1f, 0x8b, 0x08, 0x00]),
            ArchiveKind::TarGz
        );
    }

    #[test]
    fn short_buffers_do_not_panic() {
        for len in 0..=270usize {
            let head = vec![0xAAu8; len];
            let _ = detect_bytes("x.tar", &head);
            let _ = detect_bytes("", &head);
        }
    }

    #[test]
    fn extension_list_is_stable_and_sorted() {
        let list = extensions();
        let mut sorted = list.to_vec();
        sorted.sort_unstable();
        assert_eq!(list, sorted.as_slice());
        assert!(list.contains(&"zip"));
        assert!(list.contains(&"tar.gz"));
        assert!(!list.iter().any(|e| e.starts_with('.')));
    }

    #[test]
    fn kind_predicates_and_names() {
        assert!(ArchiveKind::TarGz.is_tar() && ArchiveKind::TarGz.is_compressed_tar());
        assert!(!ArchiveKind::Gz.is_tar() && ArchiveKind::Gz.is_single_file_compressed());
        assert_eq!(ArchiveKind::TarGz.compressor(), ArchiveKind::Gz);
        assert_eq!(ArchiveKind::Tar.compressor(), ArchiveKind::Tar);
        assert!(!ArchiveKind::Unknown.is_known());
        assert_eq!(ArchiveKind::SevenZip.to_string(), "7ZIP");
        assert_eq!(ArchiveKind::Unknown.name(), "unknown");
    }

    #[test]
    fn detect_reads_the_head_of_a_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("real.bin");
        let mut f = std::fs::File::create(&zip).unwrap();
        f.write_all(b"PK\x03\x04payload").unwrap();
        drop(f);
        assert_eq!(detect(&zip), ArchiveKind::Zip);

        let tar = dir.path().join("real.tar");
        std::fs::write(&tar, tar_head()).unwrap();
        assert_eq!(detect(&tar), ArchiveKind::Tar);

        // A gzip stream with a .gz name.
        let gz = dir.path().join("real.gz");
        std::fs::write(&gz, [0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0]).unwrap();
        assert_eq!(detect(&gz), ArchiveKind::Gz);

        // A missing file falls back to the extension.
        assert_eq!(detect(&dir.path().join("gone.zip")), ArchiveKind::Zip);
        assert_eq!(detect(&dir.path().join("gone")), ArchiveKind::Unknown);
    }

    #[test]
    fn is_archive_path_is_conservative() {
        let dir = tempfile::tempdir().unwrap();
        assert!(is_archive_path(Path::new("/tmp/a.zip")));
        assert!(is_archive_path(Path::new("/tmp/a.tar.gz")));
        assert!(!is_archive_path(Path::new("/tmp/a.txt")));

        // A known extension wins even when the file does not exist yet.
        assert!(is_archive_path(&dir.path().join("not-yet.tar")));
        // Magic bytes are enough without an extension.
        let blob = dir.path().join("blob");
        std::fs::write(&blob, b"7z\xbc\xaf\x27\x1c").unwrap();
        assert!(is_archive_path(&blob));
    }
}

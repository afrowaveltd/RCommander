//! Opening an archive and advertising what this crate can handle.

use std::path::Path;

use crate::backend::{
    sevenz::SevenZBackend, single::SingleBackend, tar::TarBackend, zip::ZipBackend,
};
use crate::error::{ArchiveError, Result};
use crate::kind::{ArchiveKind, detect};
use crate::read::ArchiveRead;

/// Open an archive for reading.
///
/// The container is decided by [`detect`] (magic bytes first, extension
/// second), then handed to the matching backend. Every kind is implemented
/// except:
///
/// * [`ArchiveKind::Rar`] and [`ArchiveKind::Cab`] — no pure-Rust decoder is
///   available, so they return [`ArchiveError::Unsupported`];
/// * [`ArchiveKind::Unknown`] — not an archive this crate recognises.
///
/// An encrypted archive does not fail here: the member list is still useful, and
/// reading an encrypted member returns [`ArchiveError::PasswordRequired`].
pub fn open(path: &Path) -> Result<Box<dyn ArchiveRead>> {
    let kind = detect(path);
    match kind {
        ArchiveKind::Zip => Ok(Box::new(ZipBackend::open(path)?)),
        ArchiveKind::Tar
        | ArchiveKind::TarGz
        | ArchiveKind::TarBz2
        | ArchiveKind::TarXz
        | ArchiveKind::TarZst => Ok(Box::new(TarBackend::open(path, kind)?)),
        ArchiveKind::Gz | ArchiveKind::Bz2 | ArchiveKind::Xz | ArchiveKind::Zst => {
            Ok(Box::new(SingleBackend::open(path, kind)?))
        }
        ArchiveKind::SevenZip => Ok(Box::new(SevenZBackend::open(path)?)),
        ArchiveKind::Rar => Err(ArchiveError::unsupported(format!(
            "RAR archives are recognised but cannot be read: {}",
            path.display()
        ))),
        ArchiveKind::Cab => Err(ArchiveError::unsupported(format!(
            "CAB archives are recognised but cannot be read: {}",
            path.display()
        ))),
        ArchiveKind::Unknown => Err(ArchiveError::unsupported(format!(
            "{} is not a recognised archive",
            path.display()
        ))),
    }
}

/// Every container kind [`open`] can read.
///
/// RAR, CAB and [`ArchiveKind::Unknown`] are deliberately absent: they return
/// [`ArchiveError::Unsupported`].
pub fn supported_for_reading() -> Vec<ArchiveKind> {
    vec![
        ArchiveKind::Zip,
        ArchiveKind::Tar,
        ArchiveKind::TarGz,
        ArchiveKind::TarBz2,
        ArchiveKind::TarXz,
        ArchiveKind::TarZst,
        ArchiveKind::Gz,
        ArchiveKind::Bz2,
        ArchiveKind::Xz,
        ArchiveKind::Zst,
        ArchiveKind::SevenZip,
    ]
}

/// Every container kind [`crate::create`] can write.
pub fn supported_for_writing() -> Vec<ArchiveKind> {
    vec![ArchiveKind::Zip, ArchiveKind::TarGz]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create::{PackOptions, create};

    #[test]
    fn supported_sets_match_the_documented_behaviour() {
        let reading = supported_for_reading();
        assert!(reading.contains(&ArchiveKind::SevenZip));
        assert!(!reading.contains(&ArchiveKind::Rar));
        assert!(!reading.contains(&ArchiveKind::Cab));
        assert!(!reading.contains(&ArchiveKind::Unknown));
        assert_eq!(
            supported_for_writing(),
            vec![ArchiveKind::Zip, ArchiveKind::TarGz]
        );
    }

    #[test]
    fn unsupported_kinds_produce_clear_errors() {
        let dir = tempfile::tempdir().unwrap();

        let rar = dir.path().join("a.rar");
        std::fs::write(&rar, b"Rar!\x1a\x07\x00rest").unwrap();
        let err = match open(&rar) {
            Ok(_) => panic!("RAR must not open"),
            Err(err) => err,
        };
        assert!(matches!(err, ArchiveError::Unsupported(_)));
        assert!(err.to_string().contains("RAR"), "{err}");

        let cab = dir.path().join("a.cab");
        std::fs::write(&cab, b"MSCF\x00\x00\x00\x00").unwrap();
        assert!(matches!(open(&cab), Err(ArchiveError::Unsupported(_))));

        let plain = dir.path().join("notes.txt");
        std::fs::write(&plain, b"just text").unwrap();
        let err = match open(&plain) {
            Ok(_) => panic!("plain text must not open"),
            Err(err) => err,
        };
        assert!(matches!(err, ArchiveError::Unsupported(_)));

        assert!(matches!(
            open(&dir.path().join("missing.zip")),
            Err(ArchiveError::Io { .. })
        ));
    }

    #[test]
    fn open_reports_the_backend_kind() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("payload.txt");
        std::fs::write(&src, b"data").unwrap();

        let zip = dir.path().join("a.zip");
        create(&zip, std::slice::from_ref(&src), &PackOptions::default()).unwrap();
        assert_eq!(open(&zip).unwrap().kind(), ArchiveKind::Zip);

        let tgz = dir.path().join("a.tar.gz");
        create(&tgz, &[src], &PackOptions::default()).unwrap();
        assert_eq!(open(&tgz).unwrap().kind(), ArchiveKind::TarGz);
    }
}

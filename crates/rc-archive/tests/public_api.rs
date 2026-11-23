//! Integration tests for the public API surface of `rc-archive`.
//!
//! These live outside the crate on purpose: they only see what
//! `rc-archive` actually exports, which is exactly the view the file panel has.

use std::io::Write;
use std::path::{Path, PathBuf};

use rc_archive::{
    ArchiveDir, ArchiveEntry, ArchiveError, ArchiveKind, ArchiveRead, ExtractOutcome, PackOptions,
    PackOutcome, TestReport, add_to_zip, create, describe, detect, detect_bytes, extensions,
    extract_all, extract_member_to, extract_members, is_archive_path, is_dir, list_dir, open,
    remove_from_zip, supported_for_reading, supported_for_writing, test,
};
use rc_core::ops::CancelToken;

struct Tree {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Tree {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        Tree { _dir: dir, root }
    }

    fn file(&self, name: &str, data: &[u8]) -> PathBuf {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, data).unwrap();
        path
    }
}

#[test]
fn the_whole_panel_workflow_works_through_the_public_api() {
    let tree = Tree::new();
    let docs = tree.root.join("docs");
    std::fs::create_dir_all(docs.join("nested")).unwrap();
    std::fs::write(docs.join("readme.txt"), b"read me").unwrap();
    std::fs::write(docs.join("nested").join("žluťoučký.txt"), "kůň".as_bytes()).unwrap();

    let archive_path = tree.root.join("docs.zip");
    let packed: PackOutcome = create(
        &archive_path,
        std::slice::from_ref(&docs),
        &PackOptions::default(),
    )
    .unwrap();
    assert!(packed.added >= 3 && packed.archive_size > 0);

    // Detection and the kinds the GUI may offer.
    assert_eq!(detect(&archive_path), ArchiveKind::Zip);
    assert!(is_archive_path(&archive_path));
    assert!(supported_for_reading().contains(&ArchiveKind::Zip));
    assert!(supported_for_writing().contains(&ArchiveKind::Zip));
    assert!(extensions().contains(&"zip"));

    // Enter the archive and browse it like a directory.
    assert!(is_dir(&archive_path, "docs").unwrap());
    let root_listing: Vec<rc_core::entry::Entry> = list_dir(&archive_path, "").unwrap();
    assert_eq!(root_listing.len(), 1);
    assert!(root_listing[0].is_dir());
    let docs_listing = list_dir(&archive_path, "docs").unwrap();
    let names: Vec<&str> = docs_listing.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["nested", "readme.txt"]);
    let dir_model: ArchiveDir = ArchiveDir::list(&archive_path, "docs/nested").unwrap();
    assert_eq!(dir_model.entries.len(), 1);
    assert!(dir_model.parent_entry(&archive_path).is_some());

    // Read one member through the reader trait object.
    let mut reader: Box<dyn ArchiveRead> = open(&archive_path).unwrap();
    assert_eq!(reader.kind(), ArchiveKind::Zip);
    let entries: &[ArchiveEntry] = reader.entries();
    assert!(!entries.is_empty());
    assert_eq!(reader.read("docs/readme.txt").unwrap(), b"read me");
    let mut sink: Vec<u8> = Vec::new();
    assert_eq!(reader.copy_to("docs/readme.txt", &mut sink).unwrap(), 7);
    assert_eq!(sink, b"read me");

    // Extract everything, a subset, and one member to a chosen file.
    let out = tree.root.join("out");
    let outcome: ExtractOutcome = extract_all(&archive_path, &out, &CancelToken::new()).unwrap();
    assert_eq!(outcome.files, 2);
    assert_eq!(
        std::fs::read(out.join("docs/readme.txt")).unwrap(),
        b"read me"
    );

    let subset = tree.root.join("subset");
    let outcome = extract_members(
        &archive_path,
        &subset,
        &["docs/nested".to_string()],
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(outcome.files, 1);
    assert!(!subset.join("docs/readme.txt").exists());

    let single = tree.root.join("single/member.txt");
    assert_eq!(
        extract_member_to(&archive_path, "docs/readme.txt", &single).unwrap(),
        7
    );
    assert_eq!(std::fs::read(&single).unwrap(), b"read me");

    // Modify the archive.
    let extra = tree.file("extra.txt", b"added later");
    let extra_dir = tree.root.clone();
    let outcome = add_to_zip(&archive_path, &[extra], &extra_dir, 6, None).unwrap();
    assert_eq!(outcome.added, 1);
    assert_eq!(
        open(&archive_path).unwrap().read("extra.txt").unwrap(),
        b"added later"
    );
    let removed = remove_from_zip(&archive_path, &["extra.txt".to_string()]).unwrap();
    assert_eq!(removed, 1);

    // Test and describe it.
    let report: TestReport = test(&archive_path, &CancelToken::new()).unwrap();
    assert!(report.ok, "{:?}", report.errors);
    assert!(report.entries >= 3);
    let summary = describe(&archive_path);
    assert!(summary.starts_with("ZIP archive,"), "{summary}");
}

#[test]
fn errors_are_normal_std_errors_and_never_panic() {
    let tree = Tree::new();
    let broken = tree.file("broken.zip", b"PK\x03\x04 but then nothing useful");
    let err = match open(&broken) {
        Ok(_) => panic!("garbage must not open"),
        Err(err) => err,
    };
    let as_std: &dyn std::error::Error = &err;
    assert!(!as_std.to_string().is_empty());
    assert!(matches!(
        err,
        ArchiveError::Malformed(_) | ArchiveError::Unsupported(_) | ArchiveError::Io { .. }
    ));

    // Every entry point survives the same garbage file.
    let _ = list_dir(&broken, "");
    let _ = is_dir(&broken, "x");
    let _ = extract_all(&broken, &tree.root.join("nope"), &CancelToken::new());
    let _ = test(&broken, &CancelToken::new());
    assert!(!describe(&broken).is_empty());

    // A hostile member name is refused with the dedicated error variant.
    let hostile = tree.root.join("hostile.zip");
    {
        let file = std::fs::File::create(&hostile).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("../../escape.txt", options).unwrap();
        writer.write_all(b"pwned").unwrap();
        writer.finish().unwrap();
    }
    let dest = tree.root.join("sandbox");
    match extract_all(&hostile, &dest, &CancelToken::new()) {
        Err(ArchiveError::UnsafePath(message)) => {
            assert!(message.contains("escape.txt"), "{message}")
        }
        other => panic!("expected UnsafePath, got {other:?}"),
    }
    assert!(!tree.root.join("escape.txt").exists());
}

#[test]
fn detect_bytes_is_usable_without_touching_the_file_system() {
    assert_eq!(detect_bytes("a.zip", b"PK\x03\x04"), ArchiveKind::Zip);
    assert_eq!(
        detect_bytes("a.gz", &[0x1f, 0x8b, 0x08, 0x00]),
        ArchiveKind::Gz
    );
    assert_eq!(
        detect_bytes("a.tgz", &[0x1f, 0x8b, 0x08, 0x00]),
        ArchiveKind::TarGz
    );
    assert_eq!(
        detect_bytes("a.7z", b"7z\xbc\xaf\x27\x1c"),
        ArchiveKind::SevenZip
    );
    assert_eq!(detect_bytes("plain", b"nothing here"), ArchiveKind::Unknown);
    assert!(!is_archive_path(Path::new(
        "/tmp/definitely-not-an-archive"
    )));
}

#[test]
fn truncated_and_garbage_inputs_produce_errors_never_panics() {
    let tree = Tree::new();
    let payload = tree.file("payload.txt", &vec![b'p'; 4096]);
    let zip = tree.root.join("good.zip");
    create(
        &zip,
        std::slice::from_ref(&payload),
        &PackOptions::default(),
    )
    .unwrap();
    let tgz = tree.root.join("good.tar.gz");
    create(
        &tgz,
        std::slice::from_ref(&payload),
        &PackOptions::default(),
    )
    .unwrap();

    let dest_root = tempfile::tempdir().unwrap();
    for source in [&zip, &tgz] {
        let bytes = std::fs::read(source).unwrap();
        let victim = tree.root.join(format!(
            "cut.{}",
            source.extension().unwrap().to_string_lossy()
        ));
        let step = (bytes.len() / 48).max(1);
        let mut cut = 0;
        while cut <= bytes.len() {
            std::fs::write(&victim, &bytes[..cut]).unwrap();
            // Every entry point must return, not panic, whatever it finds.
            let _ = detect(&victim);
            let _ = open(&victim);
            let _ = list_dir(&victim, "");
            let _ = list_dir(&victim, "dir/deeper");
            let _ = is_dir(&victim, "payload.txt");
            let _ = test(&victim, &CancelToken::new());
            let _ = extract_all(
                &victim,
                &dest_root.path().join(format!("cut{cut}")),
                &CancelToken::new(),
            );
            assert!(!describe(&victim).is_empty());
            cut += step;
        }
    }

    // Random bytes with archive-looking extensions.
    let mut state = 0x1234_5678u64;
    let mut random = vec![0u8; 2048];
    for byte in &mut random {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *byte = (state >> 33) as u8;
    }
    for name in [
        "noise.zip",
        "noise.tar",
        "noise.7z",
        "noise.gz",
        "noise.tar.zst",
    ] {
        let path = tree.root.join(name);
        std::fs::write(&path, &random).unwrap();
        let _ = open(&path);
        let _ = list_dir(&path, "");
        let report = test(&path, &CancelToken::new()).unwrap();
        assert!(!report.ok, "{name} is not a valid archive");
        assert!(!describe(&path).is_empty());
    }
}

//! End-to-end tests of [`super::FtpClient`] against the in-process mock server.
//!
//! Every test starts a real `TcpListener` on `127.0.0.1:0`, so nothing here
//! touches the network, and the mock joins all of its threads when it is
//! dropped at the end of the test.

use std::fs;
use std::net::TcpListener;
use std::time::Duration;

use rc_core::ops::CancelToken;

use super::mock::{Behaviour, MTIME, MockFs, MockServer};
use crate::{ConnectionSpec, Protocol, RemoteError, RemoteFs, connect};

/// Timeout used for every test connection: generous, so a slow machine cannot
/// turn a passing test into a flaky one.
const TIMEOUT: Duration = Duration::from_secs(10);

/// A spec pointing at the mock server.
fn spec_for(server: &MockServer, passive: bool) -> ConnectionSpec {
    ConnectionSpec {
        name: "mock".into(),
        host: "127.0.0.1".into(),
        port: server.port(),
        user: "anonymous".into(),
        password: String::new(),
        protocol: Protocol::Ftp,
        passive,
        encoding: "utf-8".into(),
        remote_dir: String::new(),
        timeout: TIMEOUT,
        ..Default::default()
    }
}

/// Connect to the mock server in passive mode.
fn open(server: &MockServer) -> Box<dyn RemoteFs> {
    connect(&spec_for(server, true)).expect("connect to the mock server")
}

/// `unwrap_err` without requiring `Debug` on the (undebuggable) client box.
fn connect_error(spec: &ConnectionSpec) -> RemoteError {
    match connect(spec) {
        Ok(_) => panic!("expected connect to fail"),
        Err(error) => error,
    }
}

/// A file system with a small directory tree used by several tests.
fn sample_fs() -> MockFs {
    MockFs::new()
        .dir("/pub")
        .dir("/pub/sub")
        .file("/pub/a.txt", b"hello")
        .file("/pub/sub/b.txt", b"world!")
        .link("/pub/current", "/pub/a.txt")
}

#[test]
fn connects_and_logs_in_anonymously() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    assert_eq!(client.protocol(), Protocol::Ftp);
    assert_eq!(client.pwd().unwrap(), "/");
    client.cwd("/pub").unwrap();
    assert_eq!(client.pwd().unwrap(), "/pub");
    client.quit().unwrap();
}

#[test]
fn the_initial_remote_directory_is_entered_after_login() {
    let server = MockServer::start(sample_fs());
    let mut spec = spec_for(&server, true);
    spec.remote_dir = "/pub".into();
    let mut client = connect(&spec).unwrap();
    assert_eq!(client.pwd().unwrap(), "/pub");

    // A missing initial directory must not fail the login.
    let mut spec = spec_for(&server, true);
    spec.remote_dir = "/does/not/exist".into();
    let mut client = connect(&spec).unwrap();
    assert_eq!(client.pwd().unwrap(), "/");
}

#[test]
fn list_reports_the_metadata_from_mlsd() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    let mut entries = client.list(Some("/pub")).unwrap();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, vec!["a.txt", "current", "sub"]);
    assert!(server.mlsd_calls() >= 1);
    assert_eq!(
        server.list_calls(),
        0,
        "a working MLSD must not fall back to LIST"
    );

    let file = &entries[0];
    assert!(!file.is_dir && !file.is_symlink);
    assert_eq!(file.size, 5);
    assert_eq!(file.mtime, Some(MTIME));
    assert_eq!(file.permissions, Some(0o644));

    let link = &entries[1];
    assert!(link.is_symlink);
    assert_eq!(link.target.as_deref(), Some("/pub/a.txt"));

    let dir = &entries[2];
    assert!(dir.is_dir);
    assert_eq!(dir.size, 0);
    assert_eq!(dir.permissions, Some(0o755));

    // `None` means "the current directory", which is `/` after login.
    let root = client.list(None).unwrap();
    assert_eq!(root.len(), 1);
    assert_eq!(root[0].name, "pub");
    assert!(root[0].is_dir);
}

#[test]
fn list_falls_back_to_list_when_mlsd_is_refused() {
    let server = MockServer::with_behaviour(
        sample_fs(),
        Behaviour {
            no_mlsd: true,
            ..Default::default()
        },
    );
    let mut client = open(&server);
    let mut entries = client.list(Some("/pub")).unwrap();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].name, "a.txt");
    assert_eq!(entries[0].size, 5);
    assert_eq!(entries[0].permissions, Some(0o644));
    assert_eq!(entries[0].mtime, Some(MTIME));
    assert!(entries[2].is_dir);
    assert_eq!(server.mlsd_calls(), 1);
    assert!(server.list_calls() >= 1);

    // The fallback is remembered: no second MLSD attempt.
    let entries = client.list(Some("/pub")).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(server.mlsd_calls(), 1);
}

#[test]
fn list_falls_back_when_mlsd_returns_no_usable_facts() {
    let server = MockServer::with_behaviour(
        sample_fs(),
        Behaviour {
            junk_mlsd: true,
            ..Default::default()
        },
    );
    let mut client = open(&server);
    let entries = client.list(Some("/pub")).unwrap();
    assert_eq!(entries.len(), 3);
    assert!(server.list_calls() >= 1);
}

#[test]
fn listing_a_missing_directory_is_not_found_and_keeps_mlsd() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    let error = client.list(Some("/nope")).unwrap_err();
    assert!(matches!(error, RemoteError::NotFound(_)), "{error:?}");
    assert_eq!(server.list_calls(), 1, "both formats were tried");

    // Both formats failed, so the failure must not be remembered as "this
    // server has no MLSD".
    let before = server.mlsd_calls();
    let entries = client.list(Some("/pub")).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(server.mlsd_calls(), before + 1);
    assert_eq!(server.list_calls(), 1, "LIST must not be needed again");
}

#[test]
fn modification_times_survive_both_listing_formats() {
    let mtime = crate::time::timestamp_from_utc(2015, 6, 1, 12, 0, 0).unwrap();
    let file_system = MockFs::new()
        .dir("/old")
        .file_at("/old/note.txt", b"x", mtime);

    let textual = MockServer::start(file_system.clone());
    let mut client = open(&textual);
    let entry = client.list(Some("/old")).unwrap().pop().unwrap();
    assert_eq!(entry.mtime, Some(mtime), "MLSD timestamps are exact UTC");
    assert_eq!(client.mdtm("/old/note.txt").unwrap(), Some(mtime));

    let unix = MockServer::with_behaviour(
        file_system,
        Behaviour {
            no_mlsd: true,
            ..Default::default()
        },
    );
    let mut client = open(&unix);
    let entry = client.list(Some("/old")).unwrap().pop().unwrap();
    // A unix `LIST` line prints `Mon DD  YYYY` for older files and therefore
    // loses the time of day; the date must still be right. This is exactly why
    // `MLSD`/`MDTM` are preferred.
    let (year, month, day, ..) = crate::time::civil_from_timestamp(entry.mtime.unwrap());
    assert_eq!((year, month, day), (2015, 6, 1));
}

#[test]
fn cwd_tracks_the_working_directory() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    assert_eq!(client.cwd("/pub/sub").unwrap(), "/pub/sub");
    assert_eq!(client.pwd().unwrap(), "/pub/sub");
    assert_eq!(client.cwd("..").unwrap(), "/pub");
    let error = client.cwd("/missing").unwrap_err();
    assert!(matches!(error, RemoteError::NotFound(_)), "{error:?}");
    // The failed CWD must not have moved us.
    assert_eq!(client.pwd().unwrap(), "/pub");
}

#[test]
fn size_and_mdtm_come_from_the_server() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    assert_eq!(client.size("/pub/a.txt").unwrap(), 5);
    assert_eq!(client.mdtm("/pub/a.txt").unwrap(), Some(MTIME));
    let error = client.size("/pub/missing.txt").unwrap_err();
    assert!(matches!(error, RemoteError::NotFound(_)), "{error:?}");
    let error = client.mdtm("/pub/missing.txt").unwrap_err();
    assert!(matches!(error, RemoteError::NotFound(_)), "{error:?}");
}

#[test]
fn mdtm_is_none_when_the_server_has_no_mdtm() {
    let server = MockServer::with_behaviour(
        sample_fs(),
        Behaviour {
            no_mdtm: true,
            ..Default::default()
        },
    );
    let mut client = open(&server);
    assert_eq!(client.mdtm("/pub/a.txt").unwrap(), None);
}

#[test]
fn stat_reports_files_and_directories() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    let file = client.stat("/pub/a.txt").unwrap();
    assert!(!file.is_dir);
    assert_eq!(file.name, "a.txt");
    assert_eq!(file.size, 5);
    assert_eq!(file.mtime, Some(MTIME));
    assert_eq!(file.permissions, Some(0o644));

    let dir = client.stat("/pub").unwrap();
    assert!(dir.is_dir);
    assert_eq!(dir.name, "pub");

    let link = client.stat("/pub/current").unwrap();
    assert!(link.is_symlink);
    assert_eq!(link.target.as_deref(), Some("/pub/a.txt"));
    assert!(server.mlst_calls() >= 3);

    let error = client.stat("/pub/missing").unwrap_err();
    assert!(matches!(error, RemoteError::NotFound(_)), "{error:?}");
}

#[test]
fn stat_falls_back_to_size_and_cwd_without_mlst() {
    let server = MockServer::with_behaviour(
        sample_fs(),
        Behaviour {
            no_mlst: true,
            ..Default::default()
        },
    );
    let mut client = open(&server);
    let file = client.stat("/pub/a.txt").unwrap();
    assert!(!file.is_dir);
    assert_eq!(file.size, 5);
    assert_eq!(file.mtime, Some(MTIME));
    let dir = client.stat("/pub").unwrap();
    assert!(dir.is_dir);
    assert_eq!(dir.name, "pub");
    // The fallback must leave the session where it was.
    assert_eq!(client.pwd().unwrap(), "/");
}

#[test]
fn get_downloads_a_file_and_creates_the_local_directories() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("nested/dir/a.txt");
    let written = client
        .get("/pub/a.txt", &target, &CancelToken::new())
        .unwrap();
    assert_eq!(written, 5);
    assert_eq!(fs::read(&target).unwrap(), b"hello");
}

#[test]
fn get_of_a_missing_file_is_not_found() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("x.bin");
    let error = client
        .get("/pub/none.bin", &target, &CancelToken::new())
        .unwrap_err();
    assert!(matches!(error, RemoteError::NotFound(_)), "{error:?}");
    assert!(
        !target.exists(),
        "a failed download must not leave a file behind"
    );
}

#[test]
fn put_uploads_a_file() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("upload.bin");
    fs::write(&source, b"0123456789").unwrap();
    let written = client
        .put(&source, "/pub/upload.bin", &CancelToken::new())
        .unwrap();
    assert_eq!(written, 10);
    assert_eq!(
        server.file("/pub/upload.bin").as_deref(),
        Some(&b"0123456789"[..])
    );
    assert_eq!(client.size("/pub/upload.bin").unwrap(), 10);
}

#[test]
fn put_and_get_round_trip_utf8_names_with_spaces() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("příliš žluťoučký kůň.txt");
    fs::write(&source, b"utf8").unwrap();
    let remote = "/pub/příliš žluťoučký kůň.txt";
    assert_eq!(client.put(&source, remote, &CancelToken::new()).unwrap(), 4);
    assert_eq!(server.file(remote).as_deref(), Some(&b"utf8"[..]));

    let back = temp.path().join("back.txt");
    assert_eq!(client.get(remote, &back, &CancelToken::new()).unwrap(), 4);
    assert_eq!(fs::read(&back).unwrap(), b"utf8");

    let entries = client.list(Some("/pub")).unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry.name == "příliš žluťoučký kůň.txt")
    );
}

#[test]
fn mkdir_rmdir_remove_and_rename_work() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);

    client.mkdir("/pub/new").unwrap();
    assert!(server.has_dir("/pub/new"));
    let error = client.mkdir("/pub/new").unwrap_err();
    assert!(matches!(error, RemoteError::NotFound(_)), "{error:?}");

    client.rename("/pub/a.txt", "/pub/renamed.txt").unwrap();
    assert!(server.has_file("/pub/renamed.txt"));
    assert!(!server.has_file("/pub/a.txt"));

    client.remove_file("/pub/renamed.txt").unwrap();
    assert!(!server.has_file("/pub/renamed.txt"));
    let error = client.remove_file("/pub/renamed.txt").unwrap_err();
    assert!(matches!(error, RemoteError::NotFound(_)), "{error:?}");

    client.rmdir("/pub/new").unwrap();
    assert!(!server.has_dir("/pub/new"));
    let error = client.rmdir("/pub/new").unwrap_err();
    assert!(matches!(error, RemoteError::NotFound(_)), "{error:?}");
}

#[test]
fn a_wrong_password_is_an_authentication_error() {
    let server = MockServer::with_behaviour(
        MockFs::new(),
        Behaviour {
            auth_fail: true,
            ..Default::default()
        },
    );
    let error = connect_error(&spec_for(&server, true));
    assert!(matches!(error, RemoteError::Authentication(_)), "{error:?}");
    assert!(error.to_string().contains("Login incorrect"), "{error}");
}

#[test]
fn a_closed_port_reports_a_connect_error() {
    // Grab a port and release it again, so nothing is listening there.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let spec = ConnectionSpec {
        host: "127.0.0.1".into(),
        port: address.port(),
        user: "anonymous".into(),
        timeout: Duration::from_millis(800),
        ..ConnectionSpec::default()
    };
    let error = connect_error(&spec);
    assert!(matches!(error, RemoteError::Connect { .. }), "{error:?}");
    assert!(
        error.to_string().contains(&address.port().to_string()),
        "{error}"
    );
}

#[test]
fn cancelling_during_get_stops_the_transfer_and_keeps_the_session_usable() {
    let payload = vec![b'x'; 512 * 1024];
    let server = MockServer::with_behaviour(
        MockFs::new().file("/big.bin", &payload),
        Behaviour {
            throttle: true,
            ..Default::default()
        },
    );
    let mut client = open(&server);
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("big.bin");

    let cancel = CancelToken::new();
    let flag = cancel.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        flag.cancel();
    });

    let outcome = client.get("/big.bin", &target, &cancel);
    canceller.join().unwrap();
    assert!(
        matches!(outcome, Err(RemoteError::Cancelled)),
        "{outcome:?}"
    );
    assert!(!target.exists(), "the partial download must be removed");

    // The control connection must have been resynchronised, not poisoned.
    let entries = client.list(Some("/")).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "big.bin");
    assert_eq!(client.pwd().unwrap(), "/");
}

#[test]
fn a_pre_cancelled_token_stops_a_transfer_before_it_starts() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    let temp = tempfile::tempdir().unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(matches!(
        client.get("/pub/a.txt", &temp.path().join("a"), &cancel),
        Err(RemoteError::Cancelled)
    ));
    let source = temp.path().join("source.bin");
    fs::write(&source, b"abc").unwrap();
    assert!(matches!(
        client.put(&source, "/pub/a.bin", &cancel),
        Err(RemoteError::Cancelled)
    ));
    assert!(!server.has_file("/pub/a.bin"));
}

#[test]
fn get_tree_downloads_a_two_level_directory() {
    let server = MockServer::start(sample_fs().file("/pub/sub/c.txt", b"!!"));
    let mut client = open(&server);
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("copy");
    let stats = client
        .get_tree("/pub", &target, &CancelToken::new())
        .unwrap();
    // `a.txt`, `sub/b.txt`, `sub/c.txt` and the `current` symlink, whose target
    // content a real server serves as well.
    assert_eq!(stats.files, 4);
    assert_eq!(stats.bytes, 5 + 6 + 2 + 5);
    assert_eq!(stats.dirs, 1);
    assert_eq!(fs::read(target.join("a.txt")).unwrap(), b"hello");
    assert_eq!(fs::read(target.join("sub/b.txt")).unwrap(), b"world!");
    assert_eq!(fs::read(target.join("sub/c.txt")).unwrap(), b"!!");
    assert_eq!(fs::read(target.join("current")).unwrap(), b"hello");
}

#[test]
fn put_tree_uploads_a_two_level_directory() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("tree");
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("one.txt"), b"1111").unwrap();
    fs::write(root.join("nested/two.txt"), b"222222").unwrap();

    let stats = client
        .put_tree(&root, "/upload", &CancelToken::new())
        .unwrap();
    assert_eq!(stats.files, 2);
    assert_eq!(stats.bytes, 10);
    assert_eq!(stats.dirs, 1);
    assert_eq!(
        server.file("/upload/one.txt").as_deref(),
        Some(&b"1111"[..])
    );
    assert_eq!(
        server.file("/upload/nested/two.txt").as_deref(),
        Some(&b"222222"[..])
    );
}

#[test]
fn active_mode_transfers_work() {
    let server = MockServer::start(sample_fs());
    let mut client = connect(&spec_for(&server, false)).unwrap();
    let entries = client.list(Some("/pub")).unwrap();
    assert_eq!(entries.len(), 3);

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("a.txt");
    assert_eq!(
        client
            .get("/pub/a.txt", &target, &CancelToken::new())
            .unwrap(),
        5
    );
    assert_eq!(fs::read(&target).unwrap(), b"hello");

    let source = temp.path().join("up.bin");
    fs::write(&source, b"active").unwrap();
    assert_eq!(
        client
            .put(&source, "/pub/up.bin", &CancelToken::new())
            .unwrap(),
        6
    );
    assert_eq!(server.file("/pub/up.bin").as_deref(), Some(&b"active"[..]));
}

#[test]
fn quitting_ends_the_session() {
    let server = MockServer::start(sample_fs());
    let mut client = open(&server);
    assert_eq!(client.pwd().unwrap(), "/");
    client.quit().unwrap();
    // The server has closed the connection, so the next command must fail
    // rather than hang or panic.
    assert!(client.pwd().is_err());
}

#[test]
fn a_greeting_that_is_not_220_is_rejected() {
    // A server that immediately reports failure must not be mistaken for a
    // successful login.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        use std::io::{Read, Write};
        if let Ok((mut socket, _)) = listener.accept() {
            let _ = socket.write_all(b"421 Too many connections.\r\n");
            let mut buffer = [0u8; 32];
            let _ = socket.read(&mut buffer);
        }
    });
    let spec = ConnectionSpec {
        host: "127.0.0.1".into(),
        port: address.port(),
        user: "anonymous".into(),
        timeout: Duration::from_secs(2),
        ..ConnectionSpec::default()
    };
    let error = connect_error(&spec);
    assert!(matches!(error, RemoteError::Protocol(_)), "{error:?}");
    handle.join().unwrap();
}

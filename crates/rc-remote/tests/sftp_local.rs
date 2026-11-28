//! Integration test for the SFTP client against a **real** SSH server.
//!
//! Run this ignored test with a dedicated writable directory on a test server
//! and a disposable account with an authorized, unencrypted private key:
//!
//! ```bash
//! RC_SFTP_TEST_HOST=127.0.0.1 RC_SFTP_TEST_PORT=22 RC_SFTP_TEST_USER=$USER \
//! RC_SFTP_TEST_KEY=/path/to/test-key RC_SFTP_TEST_DIR=/path/to/scratch \
//!   cargo test --locked -p rc-remote --features sftp --test sftp_local -- --ignored --nocapture
//! ```
#![cfg(feature = "sftp")]

use rc_remote::{AuthMethod, ConnectionSpec, Protocol, connect};
use std::path::PathBuf;

fn spec(known_hosts_path: PathBuf) -> ConnectionSpec {
    let port = std::env::var("RC_SFTP_TEST_PORT").map_or(22, |p| {
        p.parse().expect("RC_SFTP_TEST_PORT must be a valid port")
    });
    ConnectionSpec {
        name: "local".into(),
        host: std::env::var("RC_SFTP_TEST_HOST").expect("set RC_SFTP_TEST_HOST to the test server"),
        port,
        user: std::env::var("RC_SFTP_TEST_USER")
            .expect("set RC_SFTP_TEST_USER to the test account"),
        protocol: Protocol::Sftp,
        remote_dir: std::env::var("RC_SFTP_TEST_DIR")
            .expect("set RC_SFTP_TEST_DIR to a dedicated writable directory on the test server"),
        host_key_policy: rc_core::knownhosts::HostKeyPolicy::TrustOnFirstUse,
        known_hosts_path: Some(known_hosts_path),
        auth: AuthMethod::KeyFile {
            path: std::env::var_os("RC_SFTP_TEST_KEY")
                .expect("set RC_SFTP_TEST_KEY to the test account's private key")
                .into(),
            passphrase: None,
        },
        ..Default::default()
    }
}

#[test]
#[ignore = "requires a reachable SFTP fixture and RC_SFTP_TEST_* configuration"]
fn sftp_against_a_real_server() {
    let dir = tempfile::Builder::new()
        .prefix("rcommander-sftp-probe-")
        .tempdir()
        .unwrap();
    let spec = spec(dir.path().join("known_hosts"));
    let mut client = connect(&spec).expect("connect");
    assert_eq!(client.protocol(), Protocol::Sftp);
    let pwd = client.pwd().expect("pwd");
    assert!(pwd.starts_with('/'), "pwd: {pwd}");
    let entries = client.list(None).expect("list");
    eprintln!("{} entries in {pwd}", entries.len());

    let local = dir.path().join("rcommander-sftp-probe.txt");
    let content = b"hello from rcommander\nsecond line\n";
    std::fs::write(&local, content).unwrap();
    let probe = dir.path().file_name().unwrap().to_str().unwrap();
    let remote = format!("{}/{probe}.txt", pwd.trim_end_matches('/'));
    let cancel = rc_core::ops::CancelToken::new();
    let uploaded = client.put(&local, &remote, &cancel).expect("put");
    assert_eq!(uploaded, content.len() as u64);
    assert_eq!(client.size(&remote).expect("size"), content.len() as u64);

    let back = dir.path().join("rcommander-sftp-probe.back");
    let downloaded = client.get(&remote, &back, &cancel).expect("get");
    assert_eq!(downloaded, content.len() as u64);
    assert_eq!(std::fs::read(&back).unwrap(), content);

    client.remove_file(&remote).expect("delete");
    client.quit().expect("quit");
}

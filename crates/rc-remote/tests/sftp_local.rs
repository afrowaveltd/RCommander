//! Integration test for the SFTP client against a **real** SSH server.
//!
//! It is `#[ignore]`d so the normal test suite stays offline; run it explicitly:
//!
//! ```bash
//! RC_SFTP_TEST_HOST=127.0.0.1 RC_SFTP_TEST_PORT=22 RC_SFTP_TEST_USER=$USER \
//! RC_SFTP_TEST_KEY=$HOME/.ssh/id_ed25519_rctest RC_SFTP_TEST_DIR=/home/$USER \
//!   cargo test -p rc-remote --features sftp --test sftp_local -- --ignored --nocapture
//! ```
#![cfg(feature = "sftp")]

use rc_remote::{AuthMethod, ConnectionSpec, Protocol, connect};
use std::path::PathBuf;

#[allow(unused_imports)]
use rc_remote::RemoteFs;

fn spec() -> Option<ConnectionSpec> {
    let host = std::env::var("RC_SFTP_TEST_HOST").ok()?;
    let user = std::env::var("RC_SFTP_TEST_USER").ok()?;
    let port = std::env::var("RC_SFTP_TEST_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(22);
    let key = std::env::var("RC_SFTP_TEST_KEY").map(PathBuf::from).ok();
    let remote_dir = std::env::var("RC_SFTP_TEST_DIR").unwrap_or_else(|_| "/".to_string());
    let mut spec = ConnectionSpec {
        name: "local".into(),
        host,
        port,
        user,
        protocol: Protocol::Sftp,
        remote_dir,
        ..Default::default()
    };
    spec.host_key_policy = rc_core::knownhosts::HostKeyPolicy::TrustOnFirstUse;
    spec.auth = match key {
        Some(path) => AuthMethod::KeyFile {
            path,
            passphrase: None,
        },
        None => AuthMethod::Password,
    };
    Some(spec)
}

#[test]
#[ignore = "needs a reachable SSH server; set RC_SFTP_TEST_HOST to run it"]
fn sftp_against_a_real_server() {
    let Some(spec) = spec() else {
        eprintln!("RC_SFTP_TEST_HOST is not set — skipping");
        return;
    };
    let mut client = connect(&spec).expect("connect");
    assert_eq!(client.protocol(), Protocol::Sftp);
    let pwd = client.pwd().expect("pwd");
    assert!(pwd.starts_with('/'), "pwd: {pwd}");
    let entries = client.list(None).expect("list");
    eprintln!("{} entries in {pwd}", entries.len());

    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("rcommander-sftp-probe.txt");
    let content = b"hello from rcommander\nsecond line\n";
    std::fs::write(&local, content).unwrap();
    let remote = format!("{}/rcommander-sftp-probe.txt", pwd.trim_end_matches('/'));
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

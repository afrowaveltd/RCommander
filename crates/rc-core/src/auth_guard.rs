//! Persistent, local authentication throttling. No credentials are stored.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const WINDOW: u64 = 900;
const LIMIT: usize = 5;

/// Refusal to start an authentication attempt.
#[derive(Debug)]
pub enum GuardError {
    /// Remaining lockout in seconds.
    Blocked(u64),
    /// Another instance is authenticating this account.
    Busy,
    /// The local guard could not be checked or updated.
    Storage(io::Error),
}

impl From<io::Error> for GuardError {
    fn from(value: io::Error) -> Self {
        Self::Storage(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Account {
    host: String,
    port: u16,
    user: String,
}

impl Account {
    fn new(host: &str, port: u16, user: &str) -> Self {
        let host = host
            .trim()
            .trim_end_matches('.')
            .trim_start_matches('[')
            .trim_end_matches(']');
        let host = host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| host.to_ascii_lowercase());
        Self {
            host,
            port,
            user: user.trim().to_owned(),
        }
    }

    fn key(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("serializable account");
        format!("{:x}", Sha256::digest(bytes))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Record {
    account: Account,
    failures: Vec<u64>,
    blocked_until: u64,
}

impl Record {
    fn prune(&mut self, now: u64) {
        self.failures.retain(|&at| now.saturating_sub(at) < WINDOW);
        if self.blocked_until <= now {
            self.blocked_until = 0;
        }
    }

    fn empty(&self) -> bool {
        self.failures.is_empty() && self.blocked_until == 0
    }

    fn fail(&mut self, now: u64) -> Option<u64> {
        self.prune(now);
        self.failures.push(now);
        if self.failures.len() >= LIMIT {
            self.blocked_until = now.saturating_add(WINDOW);
            self.failures.clear();
            Some(WINDOW)
        } else {
            None
        }
    }
}

fn now() -> io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .map_err(io::Error::other)
}

// Keep the lock inode stable. OS releases the lock on process exit, including
// crashes. Empty lock files contain no account information and are not records.
fn lock(path: &Path) -> Result<File, GuardError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let file = options.open(path).map_err(|error| {
        #[cfg(windows)]
        if matches!(error.raw_os_error(), Some(32 | 33)) {
            return GuardError::Busy;
        }
        GuardError::Storage(error)
    })?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: file owns the live descriptor; flock does not access memory.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = io::Error::last_os_error();
            return Err(if error.kind() == io::ErrorKind::WouldBlock {
                GuardError::Busy
            } else {
                GuardError::Storage(error)
            });
        }
    }
    #[cfg(not(any(unix, windows)))]
    return Err(GuardError::Storage(io::Error::new(
        io::ErrorKind::Unsupported,
        "account locking unsupported",
    )));
    #[cfg(any(unix, windows))]
    Ok(file)
}

fn load(path: &Path) -> io::Result<Option<Record>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn save(path: &Path, record: &Record) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| io::Error::other("missing parent"))?,
    )?;
    serde_json::to_writer(&mut file, record).map_err(io::Error::other)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn cleanup(dir: &Path, now: u64) -> io::Result<()> {
    for item in fs::read_dir(dir)? {
        let path = item?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(key) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        // Never remove a record while another process is updating that account.
        let Ok(_lock) = lock(&path.with_extension("lock")) else {
            continue;
        };
        if let Ok(Some(mut record)) = load(&path) {
            // Only prune records whose account matches the filename. Preserve
            // mismatched records so begin_at can reject them rather than
            // silently turning a damaged record into a fresh attempt.
            if record.account.key() != key {
                continue;
            }
            record.prune(now);
            if record.empty() {
                remove(&path)?;
            }
        }
    }
    Ok(())
}

/// A single authorized connection attempt; hold it until authentication returns.
#[derive(Debug)]
pub struct Attempt {
    path: PathBuf,
    record: Record,
    _lock: File,
}

impl Attempt {
    /// Check a target, clear expired records, and reserve its account before
    /// network access. The host and port are shared across protocol aliases.
    pub fn begin(dir: &Path, host: &str, port: u16, user: &str) -> Result<Self, GuardError> {
        Self::begin_at(dir, host, port, user, now()?)
    }

    fn begin_at(
        dir: &Path,
        host: &str,
        port: u16,
        user: &str,
        now: u64,
    ) -> Result<Self, GuardError> {
        fs::create_dir_all(dir)?;
        cleanup(dir, now)?;
        let account = Account::new(host, port, user);
        let path = dir.join(format!("{}.json", account.key()));
        let lock = lock(&path.with_extension("lock"))?;
        let mut record = load(&path)?.unwrap_or_else(|| Record {
            account: account.clone(),
            failures: Vec::new(),
            blocked_until: 0,
        });
        if record.account != account {
            return Err(io::Error::other("authentication record identity mismatch").into());
        }
        record.prune(now);
        if record.blocked_until > now {
            return Err(GuardError::Blocked(record.blocked_until - now));
        }
        if record.empty() {
            remove(&path)?;
        }
        // Test persistence before permitting any network request. A read-only
        // or full store is not a reason to silently disable the protection.
        save(&path, &record)?;
        Ok(Self {
            path,
            record,
            _lock: lock,
        })
    }

    /// Record a server-reported authentication failure. Success and ordinary
    /// network errors never increase or reset the rolling failure count.
    /// Returns the ban duration when this failure triggered a new ban.
    pub fn finish(self, authenticated: bool, rejected: bool) -> Result<Option<u64>, GuardError> {
        self.finish_at(authenticated, rejected, now()?)
    }

    fn finish_at(
        mut self,
        authenticated: bool,
        rejected: bool,
        now: u64,
    ) -> Result<Option<u64>, GuardError> {
        self.record.prune(now);
        if !authenticated && rejected {
            let blocked = self.record.fail(now);
            save(&self.path, &self.record)?;
            return Ok(blocked);
        }
        if self.record.empty() {
            remove(&self.path)?;
        } else {
            save(&self.path, &self.record)?;
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt(dir: &Path, time: u64) -> Attempt {
        Attempt::begin_at(dir, "EXAMPLE.test.", 22, "alice", time).unwrap()
    }

    #[test]
    fn five_failures_in_window_block_for_fifteen_minutes_across_instances() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        for i in 0..5 {
            assert_eq!(
                attempt(p, 1000 + i)
                    .finish_at(false, true, 1000 + i)
                    .unwrap(),
                if i == 4 { Some(900) } else { None }
            );
        }
        assert!(matches!(
            Attempt::begin_at(p, "example.test", 22, "alice", 1005),
            Err(GuardError::Blocked(899))
        ));
        assert!(Attempt::begin_at(p, "example.test", 22, "bob", 1005).is_ok());
        assert!(Attempt::begin_at(p, "example.test", 23, "alice", 1005).is_ok());
        assert!(matches!(
            Attempt::begin_at(p, "example.test", 22, "alice", 1903),
            Err(GuardError::Blocked(1))
        ));
        attempt(p, 1904).finish_at(true, false, 1904).unwrap();
        assert_eq!(
            fs::read_dir(p)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|s| s == "json"))
                .count(),
            0
        );
    }

    #[test]
    fn rolling_window_is_not_reset_by_success_or_network_errors() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        for i in 0..4 {
            attempt(p, 1000 + i)
                .finish_at(false, true, 1000 + i)
                .unwrap();
        }
        attempt(p, 1004).finish_at(false, false, 1004).unwrap();
        assert_eq!(attempt(p, 1005).record.failures.len(), 4);
        assert_eq!(attempt(p, 1900).record.failures.len(), 3);
        attempt(p, 1901).finish_at(true, false, 1901).unwrap();
        assert_eq!(attempt(p, 1902).record.failures.len(), 1);
        assert!(attempt(p, 1903).record.empty());
    }

    #[test]
    fn another_attempt_cannot_race_the_same_account() {
        let dir = tempfile::tempdir().unwrap();
        let first = attempt(dir.path(), 1000);
        assert!(matches!(
            Attempt::begin_at(dir.path(), "example.test", 22, "alice", 1000),
            Err(GuardError::Busy)
        ));
        drop(first);
        assert!(attempt(dir.path(), 1001).record.empty());
    }

    #[test]
    fn cleanup_preserves_mismatched_record_for_fail_closed_validation() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let path = attempt(p, 1000).path.clone();
        let mismatched = Record {
            account: Account::new("other.test", 22, "alice"),
            failures: vec![1000],
            blocked_until: 0,
        };
        save(&path, &mismatched).unwrap();

        // An expired but mismatched record is not a valid cleanup candidate.
        Attempt::begin_at(p, "unrelated.test", 22, "bob", 1900)
            .unwrap()
            .finish_at(true, false, 1900)
            .unwrap();
        assert!(path.exists());
        assert!(matches!(
            Attempt::begin_at(p, "example.test", 22, "alice", 1900),
            Err(GuardError::Storage(_))
        ));
    }

    #[test]
    fn cleanup_removes_only_expired_valid_records() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let expired = attempt(p, 1000);
        let expired_path = expired.path.clone();
        expired.finish_at(false, true, 1000).unwrap();
        let active = Attempt::begin_at(p, "example.test", 22, "bob", 1800).unwrap();
        let active_path = active.path.clone();
        active.finish_at(false, true, 1800).unwrap();

        Attempt::begin_at(p, "unrelated.test", 22, "carol", 1900)
            .unwrap()
            .finish_at(true, false, 1900)
            .unwrap();
        assert!(!expired_path.exists());
        assert!(active_path.exists());
        assert_eq!(load(&active_path).unwrap().unwrap().failures, vec![1800]);
    }

    #[test]
    fn corrupt_record_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = attempt(dir.path(), 1000).path.clone();
        fs::write(path, "broken").unwrap();
        assert!(matches!(
            Attempt::begin_at(dir.path(), "example.test", 22, "alice", 1001),
            Err(GuardError::Storage(_))
        ));
    }
}

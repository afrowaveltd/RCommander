//! Append-only audit trail for everything with side effects (Phase 16.0b/d).
//!
//! The log answers "what happened, who asked, what was decided" after the fact, which is
//! the other half of an approval: the dialog asks *before*, the log explains *after*.
//!
//! Two properties are worth the extra code:
//!
//! * **Tamper evidence.** Each line carries the hash of the previous line, so deleting or
//!   editing an entry breaks the chain and [`verify`] can say exactly where.
//! * **Redaction.** Values the caller marks as secret are replaced before anything is
//!   written; the log never becomes a second place where a password lives.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

/// What was decided about one request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Allowed,
    NeedsApproval,
    Approved,
    Refused,
    Denied,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Allowed => "allowed",
            Verdict::NeedsApproval => "needs-approval",
            Verdict::Approved => "approved",
            Verdict::Refused => "refused",
            Verdict::Denied => "denied",
        }
    }
}

/// One line of the audit log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Unix seconds.
    pub time: i64,
    /// Who asked: `agent`, `ui`, a tool name, a plugin id.
    pub origin: String,
    /// Operation name (`read`, `write`, `execute`, …).
    pub operation: String,
    /// The resource, already canonicalised and already redacted.
    pub target: String,
    /// Where the result went, when that is not the target.
    pub destination: String,
    pub verdict: Verdict,
    /// Grant that covered it, or the reason it did not.
    pub note: String,
    /// Hash of the previous line, so the chain can be verified.
    pub previous: String,
}

impl Event {
    /// The hash of this event, used as the `previous` of the next one.
    pub fn chain_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.time.to_string().as_bytes());
        hasher.update(self.origin.as_bytes());
        hasher.update(self.operation.as_bytes());
        hasher.update(self.target.as_bytes());
        hasher.update(self.destination.as_bytes());
        hasher.update(self.verdict.as_str().as_bytes());
        hasher.update(self.note.as_bytes());
        hasher.update(self.previous.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// One log line (tab separated, so it stays greppable).
    pub fn to_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.time,
            sanitise(&self.origin),
            sanitise(&self.operation),
            sanitise(&self.target),
            sanitise(&self.destination),
            self.verdict.as_str(),
            sanitise(&self.note),
            self.previous
        )
    }

    /// Parse a line written by [`Event::to_line`].
    pub fn from_line(line: &str) -> Option<Event> {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 8 {
            return None;
        }
        Some(Event {
            time: parts[0].parse().ok()?,
            origin: parts[1].to_string(),
            operation: parts[2].to_string(),
            target: parts[3].to_string(),
            destination: parts[4].to_string(),
            verdict: match parts[5] {
                "allowed" => Verdict::Allowed,
                "needs-approval" => Verdict::NeedsApproval,
                "approved" => Verdict::Approved,
                "refused" => Verdict::Refused,
                _ => Verdict::Denied,
            },
            note: parts[6].to_string(),
            previous: parts[7].to_string(),
        })
    }
}

/// Replace tabs and newlines so one event stays one line.
fn sanitise(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// The audit trail on disk.
pub struct AuditLog {
    path: PathBuf,
    /// Hash of the last line written (empty for a fresh log).
    head: String,
    /// Disabled logs accept events and write nothing (the setting is off).
    enabled: bool,
}

impl AuditLog {
    /// A log at `path`; `enabled == false` keeps it silent.
    pub fn new(path: impl Into<PathBuf>, enabled: bool) -> AuditLog {
        let path = path.into();
        let head = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| text.lines().last().map(|line| line.to_string()))
            .and_then(|line| Event::from_line(&line))
            .map(|event| event.chain_hash())
            .unwrap_or_default();
        AuditLog {
            path,
            head,
            enabled,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Append one event; returns the written line when the log is enabled.
    pub fn record(
        &mut self,
        origin: &str,
        operation: &str,
        target: &str,
        destination: &str,
        verdict: Verdict,
        note: &str,
    ) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let event = Event {
            time: chrono::Local::now().timestamp(),
            origin: origin.to_string(),
            operation: operation.to_string(),
            target: target.to_string(),
            destination: destination.to_string(),
            verdict,
            note: note.to_string(),
            previous: self.head.clone(),
        };
        let line = event.to_line();
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            Ok(mut file) => {
                if writeln!(file, "{line}").is_ok() {
                    self.head = event.chain_hash();
                    return Some(line);
                }
                None
            }
            Err(_) => None,
        }
    }

    /// Read the whole log.
    pub fn read(&self) -> Vec<Event> {
        std::fs::read_to_string(&self.path)
            .map(|text| text.lines().filter_map(Event::from_line).collect())
            .unwrap_or_default()
    }
}

/// Result of checking a log for tampering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainState {
    /// Every line links to the one before it.
    Intact { events: usize },
    /// A line does not link correctly; `line` is one-based.
    Broken { line: usize },
    /// The log does not exist yet.
    Empty,
}

impl ChainState {
    pub fn is_intact(&self) -> bool {
        matches!(self, ChainState::Intact { .. } | ChainState::Empty)
    }
}

/// Verify the hash chain of a log file.
pub fn verify(path: &Path) -> ChainState {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ChainState::Empty;
    };
    let events: Vec<Event> = text.lines().filter_map(Event::from_line).collect();
    if events.is_empty() {
        return ChainState::Empty;
    }
    let mut previous = String::new();
    for (index, event) in events.iter().enumerate() {
        if index > 0 && event.previous != previous {
            return ChainState::Broken { line: index + 1 };
        }
        previous = event.chain_hash();
    }
    ChainState::Intact {
        events: events.len(),
    }
}

/// Replace every occurrence of `secrets` in `text` with `***`.
///
/// Callers pass the values they know are secret (a password, a token); the helper is the
/// single place that decides how they look in a log or an approval preview.
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if secret.is_empty() || secret.len() < 3 {
            continue;
        }
        out = out.replace(secret.as_str(), "***");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("{name}.log"));
        (dir, path)
    }

    #[test]
    fn events_are_written_in_order_and_the_chain_verifies() {
        let (_dir, path) = temp_log("chain");
        let mut log = AuditLog::new(&path, true);
        assert!(
            log.record(
                "agent",
                "read",
                "/tmp/a",
                "",
                Verdict::Allowed,
                "grant task"
            )
            .is_some()
        );
        log.record(
            "agent",
            "write",
            "/tmp/a",
            "/tmp/a",
            Verdict::NeedsApproval,
            "asks",
        );
        log.record(
            "ui",
            "write",
            "/tmp/a",
            "/tmp/a",
            Verdict::Approved,
            "user said yes",
        );
        let events = log.read();
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].verdict, Verdict::Approved);
        assert_eq!(events[0].previous, "", "the first line starts the chain");
        assert_eq!(verify(&path), ChainState::Intact { events: 3 });
    }

    #[test]
    fn a_removed_line_breaks_the_chain() {
        let (_dir, path) = temp_log("tamper");
        let mut log = AuditLog::new(&path, true);
        for index in 0..3 {
            log.record(
                "agent",
                "read",
                &format!("/tmp/{index}"),
                "",
                Verdict::Allowed,
                "",
            );
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        std::fs::write(&path, format!("{}\n{}", lines[0], lines[2])).unwrap();
        match verify(&path) {
            ChainState::Broken { line } => assert_eq!(line, 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_disabled_log_writes_nothing() {
        let (_dir, path) = temp_log("disabled");
        let mut log = AuditLog::new(&path, false);
        assert!(
            log.record("agent", "read", "/tmp/a", "", Verdict::Allowed, "")
                .is_none()
        );
        assert!(!path.exists());
        assert_eq!(verify(&path), ChainState::Empty);
    }

    #[test]
    fn secrets_are_redacted_before_they_reach_the_log() {
        let secret = "hunter2-super-secret".to_string();
        let text = format!("using {secret} to log in");
        let clean = redact(&text, std::slice::from_ref(&secret));
        assert!(!clean.contains("hunter2"), "{clean}");
        assert!(clean.contains("***"));
        // Very short values are not treated as secrets (a single letter would shred the text).
        assert_eq!(redact("abcdef", &["a".to_string()]), "abcdef");
    }

    #[test]
    fn tabs_and_newlines_cannot_break_the_one_line_per_event_rule() {
        let (_dir, path) = temp_log("greedy");
        let mut log = AuditLog::new(&path, true);
        log.record(
            "agent\twith\ttabs",
            "write",
            "/tmp/a\nb",
            "",
            Verdict::Denied,
            "note\there",
        );
        let events = log.read();
        assert_eq!(events.len(), 1, "the event stayed on one line");
        assert_eq!(events[0].operation, "write");
        assert_eq!(verify(&path), ChainState::Intact { events: 1 });
    }

    #[test]
    fn reopening_a_log_continues_the_chain() {
        let (_dir, path) = temp_log("reopen");
        {
            let mut log = AuditLog::new(&path, true);
            log.record("agent", "read", "/tmp/a", "", Verdict::Allowed, "");
        }
        let mut second = AuditLog::new(&path, true);
        second.record("agent", "read", "/tmp/b", "", Verdict::Denied, "no grant");
        assert_eq!(verify(&path), ChainState::Intact { events: 2 });
    }
}

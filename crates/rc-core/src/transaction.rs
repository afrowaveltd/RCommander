//! Reviewed file transactions with a verified backup (Phase 16.0c).
//!
//! The rule the panel owner insisted on: **no verified backup, no change.** Every write goes
//! through the same five steps, and a failure at any of them leaves the originals exactly as
//! they were:
//!
//! 1. **prepare** — compute the hash of the current content, reserve the backup path;
//! 2. **backup** — copy the original into the transaction directory and verify the copy
//!    (length *and* SHA-256); a mismatch aborts the whole transaction;
//! 3. **validate** — run the caller's validator on the new content (TOML parse, `systemctl
//!    cat`, a language server — whatever the operation needs);
//! 4. **apply** — write to a temporary file next to the target, then rename it into place, so
//!    a crash cannot leave a half-written file;
//! 5. **verify** — hash the result and record it in the manifest.
//!
//! The manifest is a TOML file in the transaction directory: it lists, for every file, the
//! original hash, the new hash and the backup path — which is what makes "restore" a real
//! feature instead of a promise.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// One file the transaction will change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// File to write.
    pub target: PathBuf,
    /// New content.
    pub content: Vec<u8>,
    /// True when the file does not exist yet (creation is recorded so it can be removed
    /// on rollback).
    pub is_new: bool,
}

impl Change {
    pub fn write(target: impl Into<PathBuf>, content: impl Into<Vec<u8>>) -> Change {
        Change {
            target: target.into(),
            content: content.into(),
            is_new: false,
        }
    }

    pub fn create(target: impl Into<PathBuf>, content: impl Into<Vec<u8>>) -> Change {
        Change {
            target: target.into(),
            content: content.into(),
            is_new: true,
        }
    }
}

/// What the transaction recorded about one file.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileRecord {
    pub target: PathBuf,
    /// Hash of the content before the change (`None` for a new file).
    pub original_hash: Option<String>,
    /// Hash after the change.
    pub new_hash: String,
    /// Where the backup lives.
    pub backup: Option<PathBuf>,
    /// True when the transaction created the file.
    pub created: bool,
}

/// Everything a transaction did, written to disk next to the backups.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub files: Vec<FileRecord>,
    /// Unix seconds when the transaction finished.
    pub finished: Option<i64>,
    /// Diagnostic note (what the transaction was for).
    pub label: String,
}

impl Manifest {
    pub fn to_text(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_text(text: &str) -> Manifest {
        toml::from_str(text).unwrap_or_default()
    }

    pub fn load(path: &Path) -> Manifest {
        std::fs::read_to_string(path)
            .map(|text| Manifest::from_text(&text))
            .unwrap_or_default()
    }
}

/// Checks new content before it is written (a parser, a `configtest`, a schema check).
pub type Validator = Box<dyn Fn(&Path, &[u8]) -> Result<(), String>>;

/// A failure that stops the transaction before the originals are touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionError {
    pub message: String,
}

impl TransactionError {
    fn new(message: impl Into<String>) -> Self {
        TransactionError {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TransactionError {}

/// A set of changes applied atomically enough to be undone.
pub struct Transaction {
    directory: PathBuf,
    changes: Vec<Change>,
    manifest: Manifest,
    /// Validator run against every change before anything is written.
    validator: Option<Validator>,
    applied: bool,
}

impl Transaction {
    /// A transaction whose backups (and manifest) live in `directory`.
    pub fn new(directory: impl Into<PathBuf>, label: impl Into<String>) -> Transaction {
        Transaction {
            directory: directory.into(),
            changes: Vec::new(),
            manifest: Manifest {
                label: label.into(),
                ..Manifest::default()
            },
            validator: None,
            applied: false,
        }
    }

    /// Add a file to change.
    pub fn add(&mut self, change: Change) -> &mut Self {
        self.changes.push(change);
        self
    }

    /// Validate new content before it is written (TOML parse, `configtest`, …).
    pub fn with_validator(
        mut self,
        validator: impl Fn(&Path, &[u8]) -> Result<(), String> + 'static,
    ) -> Self {
        self.validator = Some(Box::new(validator));
        self
    }

    /// The manifest as it stands (empty until the transaction is applied).
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Backup path of `target` inside the transaction directory.
    fn backup_path(&self, index: usize, target: &Path) -> PathBuf {
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());
        self.directory.join(format!("{index}-{name}"))
    }

    /// Run prepare → backup → validate. Nothing on disk changes here.
    pub fn prepare(&mut self) -> Result<(), TransactionError> {
        std::fs::create_dir_all(&self.directory).map_err(|error| {
            TransactionError::new(format!(
                "cannot create {}: {error}",
                self.directory.display()
            ))
        })?;
        let mut records = Vec::with_capacity(self.changes.len());
        for (index, change) in self.changes.iter().enumerate() {
            let exists = change.target.is_file();
            let original = if exists {
                Some(std::fs::read(&change.target).map_err(|error| {
                    TransactionError::new(format!(
                        "cannot read {}: {error}",
                        change.target.display()
                    ))
                })?)
            } else {
                None
            };
            let original_hash = original.as_ref().map(|bytes| hash_bytes(bytes));

            // The backup is the whole point: copy, then verify the copy before going on.
            let backup = match &original {
                Some(bytes) => {
                    let path = self.backup_path(index, &change.target);
                    std::fs::write(&path, bytes).map_err(|error| {
                        TransactionError::new(format!(
                            "cannot write the backup {}: {error}",
                            path.display()
                        ))
                    })?;
                    let copied = std::fs::read(&path).map_err(|error| {
                        TransactionError::new(format!(
                            "cannot read the backup {}: {error}",
                            path.display()
                        ))
                    })?;
                    if copied.len() != bytes.len()
                        || hash_bytes(&copied) != *original_hash.clone().unwrap_or_default()
                    {
                        return Err(TransactionError::new(format!(
                            "the backup of {} does not match the original; nothing was changed",
                            change.target.display()
                        )));
                    }
                    Some(path)
                }
                None => None,
            };

            // Validation happens before any write, so a broken change never lands.
            if let Some(validator) = &self.validator {
                validator(&change.target, &change.content).map_err(|message| {
                    TransactionError::new(format!(
                        "{} was rejected by the validator: {message}",
                        change.target.display()
                    ))
                })?;
            }

            records.push(FileRecord {
                target: change.target.clone(),
                original_hash,
                new_hash: hash_bytes(&change.content),
                backup,
                created: !exists,
            });
        }
        self.manifest.files = records;
        Ok(())
    }

    /// Write every change; on failure the files already written are restored.
    pub fn apply(&mut self) -> Result<&Manifest, TransactionError> {
        if self.manifest.files.len() != self.changes.len() {
            self.prepare()?;
        }
        let mut failure: Option<TransactionError> = None;
        for (index, change) in self.changes.iter().enumerate() {
            let temporary = change.target.with_extension(format!(
                "{}.rcommander-tmp",
                change
                    .target
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default()
            ));
            if let Some(parent) = change.target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(error) = std::fs::write(&temporary, &change.content) {
                failure = Some(TransactionError::new(format!(
                    "cannot write {}: {error}",
                    temporary.display()
                )));
                break;
            }
            // A rename is atomic on every platform we support, so a crash cannot leave a
            // half-written target.
            if let Err(error) = std::fs::rename(&temporary, &change.target) {
                let _ = std::fs::remove_file(&temporary);
                failure = Some(TransactionError::new(format!(
                    "cannot replace {}: {error}",
                    change.target.display()
                )));
                break;
            }
            // Verify what is on disk now.
            match std::fs::read(&change.target) {
                Ok(written) => {
                    let expected = &self.manifest.files[index].new_hash;
                    if &hash_bytes(&written) != expected {
                        failure = Some(TransactionError::new(format!(
                            "{} does not match what was written; the change was rolled back",
                            change.target.display()
                        )));
                        break;
                    }
                }
                Err(error) => {
                    failure = Some(TransactionError::new(format!(
                        "cannot read back {}: {error}",
                        change.target.display()
                    )));
                    break;
                }
            }
        }
        if let Some(failure) = failure {
            self.restore();
            return Err(failure);
        }
        self.manifest.finished = Some(chrono::Local::now().timestamp());
        let _ = std::fs::write(self.manifest_path(), self.manifest.to_text());
        self.applied = true;
        Ok(&self.manifest)
    }

    /// Path of the manifest inside the transaction directory.
    pub fn manifest_path(&self) -> PathBuf {
        self.directory.join("manifest.toml")
    }

    /// Put every original back and remove the files the transaction created.
    ///
    /// Returns how many files were restored. A file whose backup is missing or does not
    /// match its recorded hash is reported instead of being silently left changed.
    pub fn restore(&mut self) -> usize {
        let manifest = match std::fs::read_to_string(self.manifest_path()) {
            Ok(text) => Manifest::from_text(&text),
            Err(_) => self.manifest.clone(),
        };
        let mut restored = 0;
        for record in manifest.files.iter().rev() {
            if record.created {
                if record.backup.is_none() {
                    let _ = std::fs::remove_file(&record.target);
                    restored += 1;
                }
                continue;
            }
            let Some(backup) = &record.backup else {
                continue;
            };
            let Ok(bytes) = std::fs::read(backup) else {
                continue;
            };
            if record
                .original_hash
                .as_ref()
                .is_some_and(|expected| &hash_bytes(&bytes) != expected)
            {
                // The backup itself is wrong; do not pretend a restore happened.
                continue;
            }
            if std::fs::write(&record.target, &bytes).is_ok() {
                restored += 1;
            }
        }
        self.applied = false;
        restored
    }

    /// True when the transaction finished and can be restored.
    pub fn is_applied(&self) -> bool {
        self.applied
    }
}

/// SHA-256 of `bytes`, hex encoded.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Hash of a file, or `None` when it does not exist.
pub fn hash_file(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|bytes| hash_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_change_is_backed_up_verified_and_applied() {
        let dir = temp();
        let target = dir.path().join("conf.toml");
        std::fs::write(&target, "old = true\n").unwrap();
        let backups = dir.path().join("tx");
        let mut transaction = Transaction::new(&backups, "edit the config");
        transaction.add(Change::write(&target, b"new = true\n".to_vec()));
        transaction.prepare().unwrap();
        let manifest = transaction.apply().unwrap().clone();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new = true\n");
        let record = &manifest.files[0];
        assert_eq!(
            record.original_hash.as_deref(),
            Some(hash_bytes(b"old = true\n").as_str())
        );
        assert_eq!(record.new_hash, hash_bytes(b"new = true\n"));
        assert!(
            record.backup.as_ref().unwrap().is_file(),
            "the backup exists"
        );
        assert!(
            backups.join("manifest.toml").is_file(),
            "the manifest is written"
        );

        // Restore puts the original back.
        assert_eq!(transaction.restore(), 1);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old = true\n");
    }

    #[test]
    fn a_validator_can_stop_the_transaction_before_anything_is_written() {
        let dir = temp();
        let target = dir.path().join("data.json");
        std::fs::write(&target, "{\"ok\": true}").unwrap();
        let mut transaction = Transaction::new(dir.path().join("tx"), "broken json")
            .with_validator(|_path, content| {
                if content.starts_with(b"{") {
                    Ok(())
                } else {
                    Err("not JSON".to_string())
                }
            });
        transaction.add(Change::write(&target, b"not json".to_vec()));
        let error = transaction.prepare().unwrap_err();
        assert!(error.message.contains("rejected"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "{\"ok\": true}",
            "the original is untouched"
        );
        // The backup of the original is still there, so nothing was lost either way.
        assert!(dir.path().join("tx").is_dir());
    }

    #[test]
    fn a_new_file_is_recorded_so_rollback_can_remove_it() {
        let dir = temp();
        let target = dir.path().join("fresh.txt");
        let mut transaction = Transaction::new(dir.path().join("tx"), "create a file");
        transaction.add(Change::create(&target, b"hello\n".to_vec()));
        transaction.prepare().unwrap();
        transaction.apply().unwrap();
        assert!(target.is_file());
        assert_eq!(transaction.restore(), 1);
        assert!(!target.exists(), "rollback removed the created file");
    }

    #[test]
    fn several_files_are_restored_when_the_second_write_fails() {
        let dir = temp();
        let first = dir.path().join("a.txt");
        let second = dir.path().join("missing-dir/b.txt");
        std::fs::write(&first, "original").unwrap();
        let mut transaction = Transaction::new(dir.path().join("tx"), "two files");
        transaction.add(Change::write(&first, b"changed".to_vec()));
        // A directory where a file is expected makes the second write fail.
        std::fs::create_dir_all(second.parent().unwrap()).unwrap();
        transaction.add(Change::write(&second, b"other".to_vec()));
        transaction.prepare().unwrap();
        // Make the second target unusable by turning it into a directory.
        std::fs::create_dir_all(&second).unwrap();
        let result = transaction.apply();
        assert!(result.is_err(), "the write must fail");
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            "original",
            "the first file was rolled back"
        );
    }

    #[test]
    fn a_tampered_backup_is_not_pretended_to_be_a_restore() {
        let dir = temp();
        let target = dir.path().join("f.txt");
        std::fs::write(&target, "original").unwrap();
        let mut transaction = Transaction::new(dir.path().join("tx"), "tamper");
        transaction.add(Change::write(&target, b"changed".to_vec()));
        transaction.prepare().unwrap();
        transaction.apply().unwrap();
        // Corrupt the backup behind the transaction's back.
        let backup = transaction.manifest().files[0].backup.clone().unwrap();
        std::fs::write(&backup, "evil").unwrap();
        assert_eq!(transaction.restore(), 0, "no restore is claimed");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "changed");
    }

    #[test]
    fn the_manifest_round_trips_through_disk() {
        let dir = temp();
        let target = dir.path().join("x.toml");
        std::fs::write(&target, "a = 1\n").unwrap();
        let mut transaction = Transaction::new(dir.path().join("tx"), "manifest");
        transaction.add(Change::write(&target, b"a = 2\n".to_vec()));
        transaction.prepare().unwrap();
        transaction.apply().unwrap();
        let loaded = Manifest::load(&transaction.manifest_path());
        assert_eq!(loaded.label, "manifest");
        assert_eq!(loaded.files.len(), 1);
        assert!(loaded.finished.is_some());
    }

    #[test]
    fn hashing_is_stable_and_file_hashes_match_byte_hashes() {
        let dir = temp();
        let path = dir.path().join("h.txt");
        std::fs::write(&path, "abc").unwrap();
        assert_eq!(hash_file(&path), Some(hash_bytes(b"abc")));
        assert_eq!(hash_file(&dir.path().join("nope")), None);
        assert_eq!(hash_bytes(b"").len(), 64);
    }
}

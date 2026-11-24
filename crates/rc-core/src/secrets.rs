//! Credential storage for remote connections.
//!
//! Passwords and tokens must never end up in `settings.toml`, which is a plain,
//! world-readable file that users also edit by hand. This module provides an
//! encrypted store with a small, explicit envelope:
//!
//! ```text
//! magic  b"RCSECRET"     8 bytes
//! version 1              1 byte
//! salt                   16 bytes   (Argon2id input, per file)
//! nonce                  12 bytes   (AES-256-GCM, per write)
//! ciphertext + tag       n + 16 bytes
//! ```
//!
//! The key is derived from the user's master passphrase with Argon2id
//! (19 MiB, 2 iterations, 1 lane — the OWASP baseline). The plaintext is a small
//! TOML document mapping an account id (`protocol://user@host:port`) to a secret.
//!
//! A real keyring is preferred when one is available; this store is the portable
//! fallback and the tested reference implementation of the envelope.

use crate::error::{CoreError, CoreResult};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// File magic of the encrypted store.
pub const MAGIC: &[u8; 8] = b"RCSECRET";
/// Container version written by this implementation.
pub const VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

/// Where a secret comes from, so the UI can tell the user honestly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretSource {
    /// An OS keyring / credential manager answered.
    Keyring,
    /// The encrypted portable file answered.
    EncryptedFile,
    /// Nothing is stored for this account.
    Missing,
}

/// Anything that can hold secrets: a keyring or the encrypted file store.
///
/// The trait is `Send + Sync` because the MCP server holds the model behind a
/// mutex on its own thread and must be able to look a credential up there.
pub trait SecretStore: Send + Sync {
    /// Look a secret up.
    fn get(&self, account: &str) -> CoreResult<Option<String>>;
    /// Store (or replace) a secret.
    fn set(&mut self, account: &str, secret: &str) -> CoreResult<()>;
    /// Remove a secret; removing a missing secret is not an error.
    fn delete(&mut self, account: &str) -> CoreResult<()>;
    /// Which backend this store represents.
    fn source(&self) -> SecretSource;
    /// Account ids currently stored (unsorted is fine); used to re-serialise and
    /// to report how many secrets the store holds.
    fn accounts(&self) -> Vec<String>;
}

/// The in-memory secret map used inside [`EncryptedStore`].
type Vault = BTreeMap<String, String>;

/// An encrypted, passphrase-protected secret file.
///
/// The passphrase is supplied by the caller and kept in memory only for the
/// duration of the operation; the derived key is not cached anywhere.
pub struct EncryptedStore {
    path: PathBuf,
    vault: Vault,
    dirty: bool,
}

impl std::fmt::Debug for EncryptedStore {
    /// Never prints the secrets, only the path and how many entries are stored.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedStore")
            .field("path", &self.path)
            .field("entries", &self.vault.len())
            .finish()
    }
}

impl EncryptedStore {
    /// Create a new, empty store at `path` (nothing is written until [`Self::save`]).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        EncryptedStore {
            path: path.into(),
            vault: Vault::new(),
            dirty: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// True when a store file already exists on disk.
    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Number of stored secrets.
    pub fn len(&self) -> usize {
        self.vault.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vault.is_empty()
    }

    pub fn contains(&self, account: &str) -> bool {
        self.vault.contains_key(account)
    }

    /// Account ids currently stored, sorted (the trait method of the same name
    /// returns them unsorted).
    pub fn accounts_list(&self) -> Vec<String> {
        self.vault.keys().cloned().collect()
    }

    /// Load and decrypt the store with `passphrase`.
    ///
    /// A wrong passphrase and a tampered file both fail with an error; the two are
    /// deliberately indistinguishable to the caller so a wrong passphrase cannot be
    /// used to probe the file's contents.
    pub fn load(path: impl Into<PathBuf>, passphrase: &str) -> CoreResult<Self> {
        let path = path.into();
        let bytes = std::fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
        let vault = decrypt(&bytes, passphrase)?;
        Ok(EncryptedStore {
            path,
            vault,
            dirty: false,
        })
    }

    /// Encrypt the store and write it with owner-only permissions.
    pub fn save(&mut self, passphrase: &str) -> CoreResult<()> {
        let bytes = encrypt(&self.vault, passphrase)?;
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }
        std::fs::write(&self.path, bytes).map_err(|e| CoreError::io(&self.path, e))?;
        restrict_permissions(&self.path)?;
        self.dirty = false;
        Ok(())
    }

    /// Store a secret in memory; call [`Self::save`] to persist it.
    pub fn insert(&mut self, account: impl Into<String>, secret: impl Into<String>) {
        self.vault.insert(account.into(), secret.into());
        self.dirty = true;
    }

    pub fn get_secret(&self, account: &str) -> Option<&str> {
        self.vault.get(account).map(|s| s.as_str())
    }

    pub fn remove(&mut self, account: &str) -> bool {
        let removed = self.vault.remove(account).is_some();
        self.dirty |= removed;
        removed
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

impl SecretStore for EncryptedStore {
    fn accounts(&self) -> Vec<String> {
        self.accounts_list()
    }

    fn get(&self, account: &str) -> CoreResult<Option<String>> {
        Ok(self.get_secret(account).map(|s| s.to_string()))
    }

    fn set(&mut self, account: &str, secret: &str) -> CoreResult<()> {
        self.insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&mut self, account: &str) -> CoreResult<()> {
        self.remove(account);
        Ok(())
    }

    fn source(&self) -> SecretSource {
        SecretSource::EncryptedFile
    }
}

/// In-memory store used when no passphrase has been supplied yet and by tests.
#[derive(Default, Debug)]
pub struct MemoryStore {
    vault: Vault,
}

impl MemoryStore {
    pub fn new() -> Self {
        MemoryStore::default()
    }
}

impl SecretStore for MemoryStore {
    fn accounts(&self) -> Vec<String> {
        self.vault.keys().cloned().collect()
    }

    fn get(&self, account: &str) -> CoreResult<Option<String>> {
        Ok(self.vault.get(account).cloned())
    }

    fn set(&mut self, account: &str, secret: &str) -> CoreResult<()> {
        self.vault.insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&mut self, account: &str) -> CoreResult<()> {
        self.vault.remove(account);
        Ok(())
    }

    fn source(&self) -> SecretSource {
        SecretSource::EncryptedFile
    }
}

/// Build the account id used as the key of a stored secret.
///
/// Format: `protocol://user@host:port`. The password itself is never part of the
/// key, so changing it does not create a second entry.
pub fn account_id(protocol: &str, user: &str, host: &str, port: u16) -> String {
    let user = if user.is_empty() { "anonymous" } else { user };
    format!(
        "{}://{}@{}:{}",
        protocol.to_ascii_lowercase(),
        user,
        host,
        port
    )
}

fn encrypt(vault: &Vault, passphrase: &str) -> CoreResult<Vec<u8>> {
    use aes_gcm::aead::{Aead, KeyInit, OsRng};
    use aes_gcm::{Aes256Gcm, Key, Nonce};
    use argon2::Argon2;
    use rand::RngCore;

    if passphrase.is_empty() {
        return Err(CoreError::Invalid(
            "the master passphrase must not be empty".to_string(),
        ));
    }
    let plaintext = toml::to_string(vault).map_err(|e| CoreError::Config(e.to_string()))?;
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| CoreError::Invalid("encryption failed".to_string()))?;
    let mut out = Vec::with_capacity(MAGIC.len() + 1 + SALT_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    // Keep the Argon2 import used in the signature check below.
    let _ = Argon2::default();
    Ok(out)
}

fn decrypt(bytes: &[u8], passphrase: &str) -> CoreResult<Vault> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};

    let header = MAGIC.len() + 1 + SALT_LEN + NONCE_LEN;
    if bytes.len() < header + 16 {
        return Err(CoreError::Invalid(
            "not a RCommander secret store".to_string(),
        ));
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(CoreError::Invalid(
            "not a RCommander secret store".to_string(),
        ));
    }
    if bytes[MAGIC.len()] != VERSION {
        return Err(CoreError::Invalid(format!(
            "unsupported secret store version {}",
            bytes[MAGIC.len()]
        )));
    }
    if passphrase.is_empty() {
        return Err(CoreError::Invalid(
            "the master passphrase must not be empty".to_string(),
        ));
    }
    let salt = &bytes[MAGIC.len() + 1..MAGIC.len() + 1 + SALT_LEN];
    let nonce_bytes = &bytes[MAGIC.len() + 1 + SALT_LEN..header];
    let ciphertext = &bytes[header..];
    let key = derive_key(passphrase, salt)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        CoreError::Invalid("wrong passphrase or corrupted secret store".to_string())
    })?;
    let text = String::from_utf8(plaintext)
        .map_err(|_| CoreError::Invalid("secret store is not UTF-8".to_string()))?;
    toml::from_str(&text).map_err(|e| CoreError::Config(e.to_string()))
}

fn derive_key(passphrase: &str, salt: &[u8]) -> CoreResult<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(19 * 1024, 2, 1, Some(32))
        .map_err(|e| CoreError::Invalid(format!("argon2 parameters: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| CoreError::Invalid(format!("key derivation failed: {e}")))?;
    Ok(key)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> CoreResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .map_err(|e| CoreError::io(path, e))?
        .permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(path, permissions).map_err(|e| CoreError::io(path, e))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> CoreResult<()> {
    // Windows inherits the ACL of the directory; the file lives in the per-user
    // configuration directory, which is already user-private.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_store_round_trips_through_the_encrypted_file() {
        let dir = dir();
        let path = dir.path().join("secrets.enc");
        let mut store = EncryptedStore::new(path.clone());
        store.insert(account_id("ftp", "bob", "example.com", 21), "hunter2");
        store.insert(account_id("sftp", "alice", "git.example.com", 22), "s3cret");
        assert!(store.is_dirty());
        store.save("correct horse battery staple").unwrap();
        assert!(!store.is_dirty());
        assert!(path.exists());

        let loaded = EncryptedStore::load(path, "correct horse battery staple").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(
            loaded.get_secret(&account_id("ftp", "bob", "example.com", 21)),
            Some("hunter2")
        );
        assert_eq!(
            loaded.get_secret(&account_id("sftp", "alice", "git.example.com", 22)),
            Some("s3cret")
        );
        assert!(
            loaded
                .accounts()
                .iter()
                .any(|account| account.contains("bob@example.com:21"))
        );
    }

    #[test]
    fn the_file_never_contains_the_plaintext_secret() {
        let dir = dir();
        let path = dir.path().join("secrets.enc");
        let mut store = EncryptedStore::new(path.clone());
        store.insert("ftp://bob@example.com:21", "VERY-SECRET-PASSWORD");
        store.save("passphrase").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // The account key (`bob@example.com`) is inside the encrypted payload too.
        let haystack = String::from_utf8_lossy(&bytes);
        assert!(
            !haystack.contains("VERY-SECRET-PASSWORD"),
            "the secret leaked into the file"
        );
        assert!(
            !haystack.contains("bob@example.com"),
            "the account key leaked into the file"
        );
        assert!(
            bytes.starts_with(MAGIC),
            "the file carries a versioned header"
        );
        assert_eq!(bytes[MAGIC.len()], VERSION);
    }

    #[test]
    fn a_wrong_passphrase_is_rejected() {
        let dir = dir();
        let path = dir.path().join("secrets.enc");
        let mut store = EncryptedStore::new(path.clone());
        store.insert("a", "b");
        store.save("right").unwrap();
        let error = EncryptedStore::load(&path, "wrong").unwrap_err();
        assert!(
            error.to_string().contains("wrong passphrase"),
            "error: {error}"
        );
    }

    #[test]
    fn a_tampered_file_is_rejected() {
        let dir = dir();
        let path = dir.path().join("secrets.enc");
        let mut store = EncryptedStore::new(path.clone());
        store.insert("account", "value");
        store.save("passphrase").unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            EncryptedStore::load(&path, "passphrase").is_err(),
            "a flipped bit must be detected"
        );
    }

    #[test]
    fn garbage_and_empty_passphrases_are_refused() {
        let dir = dir();
        let path = dir.path().join("secrets.enc");
        std::fs::write(&path, b"not a store at all, just some text").unwrap();
        assert!(EncryptedStore::load(&path, "x").is_err());
        let mut store = EncryptedStore::new(dir.path().join("other.enc"));
        store.insert("a", "b");
        assert!(
            store.save("").is_err(),
            "an empty passphrase must be refused"
        );
        assert!(!EncryptedStore::new(dir.path().join("missing.enc")).exists());
        assert!(EncryptedStore::load(dir.path().join("missing.enc"), "x").is_err());
    }

    #[test]
    fn deleting_a_secret_and_saving_removes_it() {
        let dir = dir();
        let path = dir.path().join("secrets.enc");
        let mut store = EncryptedStore::new(path.clone());
        store.insert("keep", "1");
        store.insert("drop", "2");
        store.save("p").unwrap();
        assert!(store.remove("drop"));
        assert!(!store.remove("drop"), "removing twice reports nothing");
        assert!(store.is_dirty());
        store.save("p").unwrap();
        let loaded = EncryptedStore::load(&path, "p").unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded.get_secret("keep").is_some());
        assert!(loaded.get_secret("drop").is_none());
    }

    #[test]
    fn the_file_is_owner_only_on_unix() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = dir();
            let path = dir.path().join("secrets.enc");
            let mut store = EncryptedStore::new(path.clone());
            store.insert("a", "b");
            store.save("p").unwrap();
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "the secret file must not be world readable");
        }
    }

    #[test]
    fn account_ids_are_stable_and_hide_the_password() {
        assert_eq!(account_id("FTP", "bob", "host", 21), "ftp://bob@host:21");
        assert_eq!(
            account_id("sftp", "", "host", 22),
            "sftp://anonymous@host:22"
        );
        // Different protocols and ports are different accounts.
        assert_ne!(
            account_id("ftp", "u", "h", 21),
            account_id("sftp", "u", "h", 22)
        );
    }

    #[test]
    fn the_memory_store_implements_the_same_trait() {
        let mut store = MemoryStore::new();
        store.set("acct", "value").unwrap();
        assert_eq!(store.get("acct").unwrap().as_deref(), Some("value"));
        store.delete("acct").unwrap();
        assert_eq!(store.get("acct").unwrap(), None);
        assert_eq!(store.source(), SecretSource::EncryptedFile);
    }
}

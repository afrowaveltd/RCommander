//! The protocol-independent interface the GUI drives a remote panel with.

use std::path::Path;

use rc_core::ops::CancelToken;

use crate::entry::RemoteEntry;
use crate::error::RemoteError;
use crate::spec::Protocol;

/// Counters returned by the recursive tree transfers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferStats {
    /// Number of regular files transferred.
    pub files: u64,
    /// Number of bytes transferred.
    pub bytes: u64,
    /// Number of directories created below the target directory.
    pub dirs: u64,
}

/// Everything a remote panel needs from a server.
///
/// The implementations ([`crate::connect`]) live in the `ftp` and `sftp`
/// modules; the trait itself is deliberately synchronous, because a panel does
/// one thing at a time and long transfers are cancelled with a
/// [`CancelToken`] from another thread rather than with async machinery.
///
/// All paths are remote paths exactly as the server spells them: absolute
/// (`/pub/incoming`), relative to the current directory (`incoming`) or `.`.
/// No client-side path normalisation happens, because servers disagree about
/// what `..` and `//` mean and the GUI offers the server's own PWD instead.
pub trait RemoteFs {
    /// Protocol this client speaks.
    fn protocol(&self) -> Protocol;

    /// Current working directory as reported by the server (`PWD`/SFTP realpath).
    fn pwd(&mut self) -> Result<String, RemoteError>;

    /// Change the working directory and return the new one.
    fn cwd(&mut self, path: &str) -> Result<String, RemoteError>;

    /// List a directory; `None` means the current working directory.
    fn list(&mut self, path: Option<&str>) -> Result<Vec<RemoteEntry>, RemoteError>;

    /// Metadata of a single path (works for directories too).
    fn stat(&mut self, path: &str) -> Result<RemoteEntry, RemoteError>;

    /// Create one directory.
    fn mkdir(&mut self, path: &str) -> Result<(), RemoteError>;

    /// Remove one (empty) directory.
    fn rmdir(&mut self, path: &str) -> Result<(), RemoteError>;

    /// Remove one file.
    fn remove_file(&mut self, path: &str) -> Result<(), RemoteError>;

    /// Rename or move a path on the server.
    fn rename(&mut self, from: &str, to: &str) -> Result<(), RemoteError>;

    /// Size of a regular file in bytes.
    fn size(&mut self, path: &str) -> Result<u64, RemoteError>;

    /// Modification time (unix seconds, UTC) when the server knows it.
    ///
    /// `Ok(None)` means "this server cannot tell me" (FTP without `MDTM`,
    /// SFTP without an mtime attribute); `Err` is a real failure.
    fn mdtm(&mut self, path: &str) -> Result<Option<i64>, RemoteError>;

    /// Download one file, returning the number of bytes written.
    fn get(&mut self, remote: &str, local: &Path, cancel: &CancelToken)
    -> Result<u64, RemoteError>;

    /// Upload one file, returning the number of bytes sent.
    fn put(&mut self, local: &Path, remote: &str, cancel: &CancelToken)
    -> Result<u64, RemoteError>;

    /// Recursive download of `remote` into `local_dir` (creates directories).
    ///
    /// `dirs` in the returned stats counts directories created *below*
    /// `local_dir`. Cancelling returns [`RemoteError::Cancelled`] and leaves the
    /// files already transferred in place.
    fn get_tree(
        &mut self,
        remote: &str,
        local_dir: &Path,
        cancel: &CancelToken,
    ) -> Result<TransferStats, RemoteError> {
        crate::tree::get_tree(self, remote, local_dir, cancel)
    }

    /// Recursive upload of `local` into the remote directory `remote_dir`.
    ///
    /// Both tree transfers have a default implementation on top of the
    /// primitive operations, so every protocol gets the same traversal, the same
    /// cancellation granularity and the same name sanitisation.
    fn put_tree(
        &mut self,
        local: &Path,
        remote_dir: &str,
        cancel: &CancelToken,
    ) -> Result<TransferStats, RemoteError> {
        crate::tree::put_tree(self, local, remote_dir, cancel)
    }

    /// Close the session politely.
    ///
    /// Errors are reported for the protocol's own farewell command but a
    /// server that simply dropped the connection is not an error here.
    fn quit(&mut self) -> Result<(), RemoteError>;
}

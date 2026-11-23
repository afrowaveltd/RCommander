//! The protocol-neutral directory entry.

use std::path::PathBuf;

use rc_core::entry::{Attrs, Entry, EntryKind};

/// One row of a remote listing, as far as the protocol could tell us.
///
/// This is the superset every protocol maps onto: FTP learns all of it from
/// `MLSD` or the unix `LIST` format, SFTP from the `SSH_FXP_READDIR` attributes.
/// Fields that a server cannot provide stay at their neutral value, so
/// `permissions == None` means "unknown", not "no permissions".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteEntry {
    /// File name without any directory part.
    pub name: String,
    /// True when the entry is a directory (a symlink *to* a directory counts
    /// only when the protocol reports it as such).
    pub is_dir: bool,
    /// True when the entry is a symbolic link.
    pub is_symlink: bool,
    /// Size in bytes; `0` for directories.
    pub size: u64,
    /// Modification time as a unix timestamp in seconds (UTC) when known.
    pub mtime: Option<i64>,
    /// Unix permission bits including the setuid/setgid/sticky bits when known.
    pub permissions: Option<u32>,
    /// Symlink target when the server sent one (`LIST` `->` arrow, `MLSD`
    /// `linktarget`/`unix.linktarget` facts).
    pub target: Option<String>,
}

impl RemoteEntry {
    /// Convert to the [`rc_core::entry::Entry`] the panels and the directory
    /// synchroniser work with.
    ///
    /// `parent` is the remote directory the entry was listed from; it becomes
    /// the entry's path. Permission bits are copied into
    /// [`rc_core::entry::Attrs::unix_mode`] and a file that nobody can write
    /// becomes `readonly`. A symlink whose direction the protocol could not
    /// determine is reported as [`EntryKind::SymlinkToFile`].
    pub fn to_core_entry(&self, parent: &str) -> Entry {
        let kind = if self.is_symlink {
            if self.is_dir {
                EntryKind::SymlinkToDir
            } else {
                EntryKind::SymlinkToFile
            }
        } else if self.is_dir {
            EntryKind::Dir
        } else {
            EntryKind::File
        };

        let mut attrs = Attrs::default();
        if let Some(mode) = self.permissions {
            attrs.unix_mode = Some(mode & 0o7777);
            attrs.readonly = mode & 0o222 == 0;
        }

        Entry {
            name: self.name.clone(),
            path: join_remote_path(parent, &self.name),
            kind,
            size: self.size,
            mtime: self.mtime,
            ctime: None,
            attrs,
            link_target: self.target.as_ref().map(PathBuf::from),
        }
    }
}

/// Join a remote directory and a name using `/`, tolerating the usual
/// variations (`""`, `"/"`, a trailing slash).
pub(crate) fn join_remote_path(parent: &str, name: &str) -> PathBuf {
    let parent = parent.trim_end_matches('/');
    if parent.is_empty() {
        return PathBuf::from(format!("/{name}"));
    }
    PathBuf::from(format!("{parent}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_entry() -> RemoteEntry {
        RemoteEntry {
            name: "docs".into(),
            is_dir: true,
            is_symlink: false,
            size: 4096,
            mtime: Some(1_704_110_400),
            permissions: Some(0o755),
            target: None,
        }
    }

    #[test]
    fn directories_become_dir_entries_with_mode_and_mtime() {
        let entry = dir_entry().to_core_entry("/pub");
        assert_eq!(entry.name, "docs");
        assert_eq!(entry.path, PathBuf::from("/pub/docs"));
        assert_eq!(entry.kind, EntryKind::Dir);
        assert!(entry.is_dir() && !entry.is_symlink());
        assert_eq!(entry.size, 4096);
        assert_eq!(entry.display_size(), 0);
        assert_eq!(entry.mtime, Some(1_704_110_400));
        assert_eq!(entry.attrs.unix_mode, Some(0o755));
        assert!(!entry.attrs.readonly);
        assert_eq!(entry.attr_string(), "drwxr-xr-x");
        assert!(entry.link_target.is_none());
    }

    #[test]
    fn files_keep_their_size_and_readonly_flag() {
        let file = RemoteEntry {
            name: "a.txt".into(),
            is_dir: false,
            is_symlink: false,
            size: 17,
            mtime: None,
            permissions: Some(0o444),
            target: None,
        };
        let entry = file.to_core_entry("/pub");
        assert_eq!(entry.kind, EntryKind::File);
        assert_eq!(entry.size, 17);
        assert_eq!(entry.display_size(), 17);
        assert!(entry.attrs.readonly);
        assert_eq!(entry.attr_string(), "r--r--r--r");
    }

    #[test]
    fn symlinks_report_their_target_and_direction() {
        let link = RemoteEntry {
            name: "latest".into(),
            is_dir: true,
            is_symlink: true,
            size: 4,
            mtime: None,
            permissions: Some(0o777),
            target: Some("/pub/v2".into()),
        };
        let entry = link.to_core_entry("/pub");
        assert_eq!(entry.kind, EntryKind::SymlinkToDir);
        assert!(entry.is_symlink() && entry.is_dir());
        assert_eq!(entry.link_target, Some(PathBuf::from("/pub/v2")));
        assert_eq!(entry.attr_string(), "lrwxrwxrwx");

        let to_file = RemoteEntry {
            is_dir: false,
            ..link
        };
        assert_eq!(to_file.to_core_entry("/pub").kind, EntryKind::SymlinkToFile);
    }

    #[test]
    fn unknown_metadata_stays_unknown() {
        let raw = RemoteEntry {
            name: "x".into(),
            ..RemoteEntry::default()
        };
        let entry = raw.to_core_entry("");
        assert_eq!(entry.kind, EntryKind::File);
        assert_eq!(entry.attrs.unix_mode, None);
        assert!(!entry.attrs.readonly);
        assert_eq!(entry.mtime, None);
    }

    #[test]
    fn paths_are_joined_with_a_single_slash() {
        assert_eq!(join_remote_path("/pub/", "a"), PathBuf::from("/pub/a"));
        assert_eq!(join_remote_path("/pub", "a"), PathBuf::from("/pub/a"));
        assert_eq!(join_remote_path("/", "a"), PathBuf::from("/a"));
        assert_eq!(join_remote_path("", "a"), PathBuf::from("/a"));
        assert_eq!(join_remote_path("/pub", ".."), PathBuf::from("/pub/.."));
    }

    #[test]
    fn device_size_is_preserved_for_special_kinds() {
        let mut e = dir_entry();
        e.is_dir = false;
        e.size = 0;
        e.permissions = None;
        let core = e.to_core_entry("/dev");
        assert_eq!(core.kind, EntryKind::File);
        assert_eq!(core.attrs.unix_mode, None);
    }
}

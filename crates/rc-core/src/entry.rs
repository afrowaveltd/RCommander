//! Directory entry model.

use std::path::{Path, PathBuf};

/// Platform-neutral view of the classic DOS/unix attribute flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attrs {
    pub readonly: bool,
    pub hidden: bool,
    pub system: bool,
    pub archive: bool,
    /// Unix permission bits (mode & 0o7777) when known.
    pub unix_mode: Option<u32>,
}

impl Attrs {
    /// Parse a DOS `attrib`-style string such as `rha` into flags.
    pub fn from_dos_string(s: &str) -> Self {
        let mut a = Attrs::default();
        for c in s.chars() {
            match c.to_ascii_lowercase() {
                'r' => a.readonly = true,
                'h' => a.hidden = true,
                's' => a.system = true,
                'a' => a.archive = true,
                _ => {}
            }
        }
        a
    }

    /// Render the DOS attribute letters (without the directory/symlink marker).
    pub fn to_dos_string(self) -> String {
        let mut s = String::new();
        if self.readonly {
            s.push('r');
        }
        if self.hidden {
            s.push('h');
        }
        if self.system {
            s.push('s');
        }
        if self.archive {
            s.push('a');
        }
        s
    }
}

/// What a directory entry actually is, with symbolic links resolved one level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    SymlinkToFile,
    SymlinkToDir,
    SymlinkBroken,
    /// Devices, sockets, fifos and Android content URIs.
    Special,
}

impl EntryKind {
    pub fn is_link(self) -> bool {
        matches!(
            self,
            EntryKind::SymlinkToFile | EntryKind::SymlinkToDir | EntryKind::SymlinkBroken
        )
    }
}

/// One row of a directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Display name; also the selection key.
    pub name: String,
    /// Full path in the parent file system.
    pub path: PathBuf,
    pub kind: EntryKind,
    pub size: u64,
    /// Modification time as a unix timestamp in seconds.
    pub mtime: Option<i64>,
    /// Creation/change time as a unix timestamp in seconds when known.
    pub ctime: Option<i64>,
    pub attrs: Attrs,
    /// Symlink target path, if the entry is a link.
    pub link_target: Option<PathBuf>,
}

impl Entry {
    /// Build an entry for `path` from already obtained metadata.
    pub fn new(name: impl Into<String>, path: PathBuf, kind: EntryKind, size: u64) -> Self {
        Entry {
            name: name.into(),
            path,
            kind,
            size,
            mtime: None,
            ctime: None,
            attrs: Attrs::default(),
            link_target: None,
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self.kind, EntryKind::Dir | EntryKind::SymlinkToDir)
    }

    pub fn is_file(&self) -> bool {
        matches!(self.kind, EntryKind::File | EntryKind::SymlinkToFile)
    }

    pub fn is_symlink(&self) -> bool {
        self.kind.is_link()
    }

    /// The `..` pseudo entry.
    pub fn parent_entry(path: PathBuf) -> Self {
        Entry {
            name: "..".to_string(),
            path,
            kind: EntryKind::Dir,
            size: 0,
            mtime: None,
            ctime: None,
            attrs: Attrs::default(),
            link_target: None,
        }
    }

    /// Lower-case extension without the dot ("" when there is none).
    pub fn extension(&self) -> String {
        Path::new(&self.name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    /// Name without the extension.
    pub fn stem(&self) -> String {
        Path::new(&self.name)
            .file_stem()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| self.name.clone())
    }

    /// Unix-style "dotfile" hiding, plus the platform hidden attribute.
    pub fn is_hidden(&self) -> bool {
        self.attrs.hidden || self.name.starts_with('.')
    }

    /// The attribute column text; `d` for directories, `l` for links.
    pub fn attr_string(&self) -> String {
        let mut s = String::new();
        if self.kind.is_link() {
            s.push('l');
        } else if self.is_dir() {
            s.push('d');
        }
        if let Some(mode) = self.attrs.unix_mode {
            s.push_str(&unix_mode_string(mode));
        }
        s.push_str(&self.attrs.to_dos_string());
        s
    }

    /// Size as shown in listings: directories always report zero.
    pub fn display_size(&self) -> u64 {
        if self.is_dir() { 0 } else { self.size }
    }
}

/// Render unix permission bits like `rwxr-xr-x`.
pub fn unix_mode_string(mode: u32) -> String {
    let mut s = String::with_capacity(9);
    let bits = [
        (0o400u32, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    for (bit, ch) in bits {
        s.push(if mode & bit != 0 { ch } else { '-' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, kind: EntryKind) -> Entry {
        Entry::new(name, PathBuf::from("/tmp").join(name), kind, 0)
    }

    #[test]
    fn kinds_classify_dirs_files_and_links() {
        let d = e("a", EntryKind::Dir);
        let f = e("b", EntryKind::File);
        let l = e("c", EntryKind::SymlinkToDir);
        assert!(d.is_dir() && !d.is_file());
        assert!(f.is_file() && !f.is_dir());
        assert!(l.is_dir() && l.is_symlink());
        assert!(EntryKind::SymlinkBroken.is_link());
        assert!(EntryKind::SymlinkBroken.is_link());
    }

    #[test]
    fn extension_and_stem_ignore_leading_dots() {
        assert_eq!(e("archive.tar.gz", EntryKind::File).extension(), "gz");
        assert_eq!(e("archive.tar.gz", EntryKind::File).stem(), "archive.tar");
        assert_eq!(e(".bashrc", EntryKind::File).extension(), "");
        assert_eq!(e("noext", EntryKind::File).extension(), "");
        assert_eq!(e("NOEXT.TXT", EntryKind::File).extension(), "txt");
    }

    #[test]
    fn dotfiles_and_hidden_flag_are_hidden() {
        assert!(e(".git", EntryKind::Dir).is_hidden());
        let mut x = e("file.txt", EntryKind::File);
        assert!(!x.is_hidden());
        x.attrs.hidden = true;
        assert!(x.is_hidden());
    }

    #[test]
    fn directory_size_displays_as_zero() {
        let mut d = e("dir", EntryKind::Dir);
        d.size = 4096;
        assert_eq!(d.display_size(), 0);
        let mut f = e("f", EntryKind::File);
        f.size = 4096;
        assert_eq!(f.display_size(), 4096);
    }

    #[test]
    fn attribute_column_marks_dirs_links_and_flags() {
        let mut d = e("dir", EntryKind::Dir);
        assert_eq!(d.attr_string(), "d");
        d.attrs.readonly = true;
        assert_eq!(d.attr_string(), "dr");
        let mut l = e("link", EntryKind::SymlinkToFile);
        l.attrs.hidden = true;
        assert_eq!(l.attr_string(), "lh");
        let mut f = e("f", EntryKind::File);
        f.attrs = Attrs {
            readonly: true,
            hidden: true,
            system: true,
            archive: true,
            unix_mode: Some(0o644),
        };
        assert_eq!(f.attr_string(), "rw-r--r--rhsa");
    }

    #[test]
    fn dos_attribute_strings_round_trip() {
        let a = Attrs::from_dos_string("rhs");
        assert!(a.readonly && a.hidden && a.system && !a.archive);
        assert_eq!(a.to_dos_string(), "rhs");
        assert_eq!(Attrs::default().to_dos_string(), "");
    }

    #[test]
    fn unix_modes_render_like_ls() {
        assert_eq!(unix_mode_string(0o755), "rwxr-xr-x");
        assert_eq!(unix_mode_string(0o644), "rw-r--r--");
        assert_eq!(unix_mode_string(0o000), "---------");
    }

    #[test]
    fn parent_entry_is_a_directory_named_dotdot() {
        let p = Entry::parent_entry(PathBuf::from("/tmp/x"));
        assert_eq!(p.name, "..");
        assert!(p.is_dir());
    }
}

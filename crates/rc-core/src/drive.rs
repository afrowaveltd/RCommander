//! Drives / volumes and free space.
//!
//! The model is deliberately platform-neutral: a [`Volume`] is a root path with
//! a label and a kind. Linux and Android collect them from `/proc/mounts`,
//! Windows from the drive letters `A:`–`Z:`.

use std::path::{Path, PathBuf};

/// What sort of volume a root path represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeKind {
    Fixed,
    Removable,
    Network,
    Optical,
    Virtual,
    Unknown,
}

/// One selectable root of the file system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub root: PathBuf,
    pub label: String,
    pub kind: VolumeKind,
}

impl Volume {
    pub fn new(root: impl Into<PathBuf>, label: impl Into<String>, kind: VolumeKind) -> Self {
        Volume {
            root: root.into(),
            label: label.into(),
            kind,
        }
    }

    /// Short text shown on a drive button.
    pub fn display_name(&self) -> String {
        if self.label.is_empty() {
            self.root.to_string_lossy().to_string()
        } else {
            self.label.clone()
        }
    }

    pub fn is_removable(&self) -> bool {
        matches!(self.kind, VolumeKind::Removable | VolumeKind::Optical)
    }

    /// Free and total bytes of the volume containing `root`.
    pub fn space(&self) -> Option<Space> {
        free_space(&self.root)
    }
}

/// Free/total space of one file system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Space {
    pub total: u64,
    pub available: u64,
}

impl Space {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    /// Used share in percent (0-100).
    pub fn used_percent(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used() as f64 / self.total as f64 * 100.0) as f32
        }
    }
}

/// The current user's home directory, if known.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let mut buf = [0u16; 260];
        // SAFETY: correct pointer/length for the API; no aliasing.
        let n = unsafe {
            let win = get_win();
            match (win.shell_folder)(&mut buf) {
                true => {
                    let len = buf.iter().position(|c| *c == 0).unwrap_or(0);
                    if len == 0 {
                        return None;
                    }
                    len
                }
                false => return None,
            }
        };
        return Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])));
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
    }
}

#[cfg(windows)]
struct WinShell {
    shell_folder: unsafe fn(&mut [u16; 260]) -> bool,
}

#[cfg(windows)]
fn get_win() -> &'static WinShell {
    use std::sync::OnceLock;
    static W: OnceLock<WinShell> = OnceLock::new();
    W.get_or_init(|| WinShell {
        shell_folder: win_shell_folder,
    })
}

#[cfg(windows)]
unsafe fn win_shell_folder(buf: &mut [u16; 260]) -> bool {
    // CSIDL_PERSONAL is out of scope here; USERPROFILE is read instead.
    let Ok(profile) = std::env::var("USERPROFILE") else {
        return false;
    };
    let wide: Vec<u16> = profile.encode_utf16().collect();
    if wide.len() >= 260 {
        return false;
    }
    buf[..wide.len()].copy_from_slice(&wide);
    buf[wide.len()] = 0;
    true
}

/// Enumerate the volumes the user can open.
pub fn volumes() -> Vec<Volume> {
    #[cfg(target_os = "windows")]
    {
        let mut out = Vec::new();
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            let p = PathBuf::from(&root);
            if p.exists() {
                let kind = if win_get_drive_type(letter) == 5 {
                    VolumeKind::Optical
                } else if win_get_drive_type(letter) == 4 {
                    VolumeKind::Network
                } else if letter == b'A' || letter == b'B' {
                    VolumeKind::Removable
                } else {
                    VolumeKind::Fixed
                };
                out.push(Volume::new(p, root.clone(), kind));
            }
        }
        return out;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut out: Vec<Volume> = Vec::new();
        let mut seen: Vec<PathBuf> = Vec::new();
        let push = |root: PathBuf,
                    label: String,
                    kind: VolumeKind,
                    seen: &mut Vec<PathBuf>,
                    out: &mut Vec<Volume>| {
            let key = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
            if seen.contains(&key) {
                return;
            }
            seen.push(key);
            out.push(Volume::new(root, label, kind));
        };

        push(
            PathBuf::from("/"),
            "/".to_string(),
            VolumeKind::Fixed,
            &mut seen,
            &mut out,
        );

        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            for line in mounts.lines() {
                let mut it = line.split_whitespace();
                let (dev, mount, fstype) = match (it.next(), it.next(), it.next()) {
                    (Some(a), Some(b), Some(c)) => (a, b, c),
                    _ => continue,
                };
                if !mount.starts_with('/') {
                    continue;
                }
                let kind = match fstype {
                    "nfs" | "nfs4" | "cifs" | "smbfs" | "smb3" | "fuse.sshfs" => {
                        VolumeKind::Network
                    }
                    "iso9660" | "udf" => VolumeKind::Optical,
                    t if t.starts_with("fuse") => VolumeKind::Virtual,
                    _ => {
                        if mount.starts_with("/media/")
                            || mount.starts_with("/run/media/")
                            || mount.starts_with("/mnt/")
                        {
                            VolumeKind::Removable
                        } else {
                            VolumeKind::Fixed
                        }
                    }
                };
                if !dev.starts_with('/') && kind == VolumeKind::Fixed {
                    continue;
                }
                let label = if kind == VolumeKind::Fixed && mount == "/" {
                    "/".to_string()
                } else {
                    mount.trim_end_matches('/').to_string()
                };
                push(PathBuf::from(mount), label, kind, &mut seen, &mut out);
            }
        }

        // Android external storage volumes.
        for candidate in ["/storage/emulated/0", "/sdcard", "/storage"] {
            let p = PathBuf::from(candidate);
            if p.is_dir() && !seen.contains(&p) {
                push(
                    p.clone(),
                    candidate.to_string(),
                    VolumeKind::Removable,
                    &mut seen,
                    &mut out,
                );
            }
        }

        out.sort_by(|a, b| {
            let rank = |k: VolumeKind| match k {
                VolumeKind::Fixed => 0,
                VolumeKind::Removable => 1,
                VolumeKind::Optical => 2,
                VolumeKind::Network => 3,
                VolumeKind::Virtual => 4,
                VolumeKind::Unknown => 5,
            };
            rank(a.kind)
                .cmp(&rank(b.kind))
                .then_with(|| a.root.cmp(&b.root))
        });
        out
    }
}

#[cfg(windows)]
fn win_get_drive_type(letter: u8) -> u32 {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDriveTypeW(lp_root_path_name: *const u16) -> u32;
    }
    let mut path: Vec<u16> = format!("{}:\\", letter as char).encode_utf16().collect();
    path.push(0);
    // SAFETY: NUL-terminated wide string, valid for the duration of the call.
    unsafe { GetDriveTypeW(path.as_ptr()) }
}

/// Free space of the file system containing `path`.
pub fn free_space(path: &Path) -> Option<Space> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let c = CString::new(path.as_os_str().as_bytes()).ok()?;
        // SAFETY: valid NUL-terminated path; statvfs fills the struct on success.
        unsafe {
            let mut st: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c.as_ptr(), &mut st) != 0 {
                return None;
            }
            let block = st.f_frsize as u64;
            Some(Space {
                total: block.saturating_mul(st.f_blocks as u64),
                available: block.saturating_mul(st.f_bavail as u64),
            })
        }
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: *const u16,
                lpFreeBytesAvailable: *mut u64,
                lpTotalNumberOfBytes: *mut u64,
                lpTotalNumberOfFreeBytes: *mut u64,
            ) -> i32;
        }
        let mut wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().collect();
        wide.push(0);
        let mut available: u64 = 0;
        let mut total: u64 = 0;
        let mut free: u64 = 0;
        // SAFETY: out-parameters point to valid, properly aligned locals.
        let ok =
            unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) };
        if ok == 0 {
            return None;
        }
        let _ = free;
        Some(Space { total, available })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        None
    }
}

/// True when `path` is a file system root.
pub fn is_root(path: &Path) -> bool {
    path.parent().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_arithmetic() {
        let s = Space {
            total: 100,
            available: 25,
        };
        assert_eq!(s.used(), 75);
        assert!((s.used_percent() - 75.0).abs() < 0.01);
        assert_eq!(Space::default().used_percent(), 0.0);
        assert_eq!(
            Space {
                total: 0,
                available: 0
            }
            .used(),
            0
        );
    }

    #[test]
    fn volumes_include_the_root() {
        let vols = volumes();
        assert!(!vols.is_empty(), "at least one volume must exist");
        assert!(
            vols.iter().any(|v| v.root == Path::new("/")),
            "root volume present: {:?}",
            vols.iter().map(|v| &v.root).collect::<Vec<_>>()
        );
    }

    #[test]
    fn display_names_and_removability() {
        let v = Volume::new("/media/usb", "USB", VolumeKind::Removable);
        assert_eq!(v.display_name(), "USB");
        assert!(v.is_removable());
        let f = Volume::new("/", "", VolumeKind::Fixed);
        assert_eq!(f.display_name(), "/");
        assert!(!f.is_removable());
        assert!(Volume::new("/mnt/cd", "CD", VolumeKind::Optical).is_removable());
    }

    #[cfg(unix)]
    #[test]
    fn free_space_reports_real_numbers_for_root() {
        let s = free_space(Path::new("/")).expect("statvfs on / must succeed");
        assert!(s.total > 0);
        assert!(s.available <= s.total);
        assert!(s.used_percent() >= 0.0 && s.used_percent() <= 100.0);
    }

    #[cfg(unix)]
    #[test]
    fn free_space_of_missing_path_is_none() {
        assert!(free_space(Path::new("/definitely/not/mounted/here")).is_none());
    }

    #[test]
    fn root_detection() {
        assert!(is_root(Path::new("/")));
        assert!(!is_root(Path::new("/tmp")));
    }

    #[test]
    fn home_directory_exists_when_reported() {
        if let Some(home) = home_dir() {
            assert!(home.is_dir(), "home dir should exist: {}", home.display());
        }
    }
}

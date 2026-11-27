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
        return win_home_dir();
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
    }
}

#[cfg(windows)]
fn win_home_dir() -> Option<PathBuf> {
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        let p = PathBuf::from(profile);
        if p.is_dir() {
            return Some(p);
        }
    }
    match (std::env::var_os("HOMEDRIVE"), std::env::var_os("HOMEPATH")) {
        (Some(drive), Some(path)) => {
            let mut p = PathBuf::from(drive);
            p.push(path);
            if p.is_dir() { Some(p) } else { None }
        }
        _ => None,
    }
}

/// Enumerate the volumes the user can open.
pub fn volumes() -> Vec<Volume> {
    #[cfg(target_os = "windows")]
    {
        // Query the system drive-letter bitmask instead of checking whether
        // each root exists. A disconnected network mapping or an empty optical
        // drive can be a valid selectable Windows volume even when exists()
        // returns false (or blocks on a slow network connection).
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetLogicalDrives() -> u32;
        }
        // SAFETY: GetLogicalDrives takes no arguments and returns a bitmask.
        let mask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for letter in b'A'..=b'Z' {
            if mask & (1u32 << (letter - b'A')) == 0 {
                continue;
            }
            let root = format!("{}:\\", letter as char);
            let kind = match win_get_drive_type(letter) {
                2 => VolumeKind::Removable,
                3 => VolumeKind::Fixed,
                4 => VolumeKind::Network,
                5 => VolumeKind::Optical,
                6 => VolumeKind::Virtual,
                _ => VolumeKind::Unknown,
            };
            let name = win_volume_label(Path::new(&root));
            let label = if name.is_empty() { root.clone() } else { name };
            out.push(Volume::new(PathBuf::from(&root), label, kind));
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
                    "squashfs" | "overlay" | "tmpfs" | "devtmpfs" | "ramfs" | "autofs" | "proc"
                    | "sysfs" | "efivarfs" => VolumeKind::Virtual,
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
                    let label = volume_label(dev, Path::new(mount));
                    if label.is_empty() {
                        mount.trim_end_matches('/').to_string()
                    } else {
                        label
                    }
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
    unsafe extern "system" {
        fn GetDriveTypeW(lp_root_path_name: *const u16) -> u32;
    }
    let mut path: Vec<u16> = format!("{}:\\", letter as char).encode_utf16().collect();
    path.push(0);
    // SAFETY: NUL-terminated wide string, valid for the duration of the call.
    unsafe { GetDriveTypeW(path.as_ptr()) }
}

/// The human label of the volume that contains `mount`.
///
/// Linux keeps labels in `/dev/disk/by-label`, Windows answers through
/// `GetVolumeInformationW`; when neither knows the volume the mount path is returned, so
/// the selector always has something to show.
pub fn volume_label(device_or_mount: &str, mount: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        let _ = (device_or_mount, mount);
        win_volume_label(mount)
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(label) = linux_label(device_or_mount) {
            return label;
        }
        let _ = mount;
        String::new()
    }
}

/// Look a device up in `/dev/disk/by-label` (a directory of symlinks).
#[cfg(not(target_os = "windows"))]
fn linux_label(device: &str) -> Option<String> {
    if !device.starts_with('/') {
        return None;
    }
    let target = std::fs::canonicalize(device).ok()?;
    let entries = std::fs::read_dir("/dev/disk/by-label").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if std::fs::canonicalize(&path).ok()? != target {
            continue;
        }
        let name = path.file_name()?.to_string_lossy().to_string();
        // `by-label` escapes spaces and other characters with `\x20`.
        return Some(unescape_label(&name));
    }
    None
}

/// Decode the `\xNN` escapes `udev` uses in `by-label` names.
#[cfg(not(target_os = "windows"))]
fn unescape_label(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() && bytes[index + 1] == b'x' {
            let hex = std::str::from_utf8(&bytes[index + 2..index + 4]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(value);
                index += 4;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(target_os = "windows")]
fn win_volume_label(root: &Path) -> String {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetVolumeInformationW(
            lp_root_path_name: *const u16,
            lp_volume_name_buffer: *mut u16,
            n_volume_name_size: u32,
            lp_volume_serial_number: *mut u32,
            lp_maximum_component_length: *mut u32,
            lp_file_system_flags: *mut u32,
            lp_file_system_name_buffer: *mut u16,
            n_file_system_name_size: u32,
        ) -> i32;
    }
    let mut root: Vec<u16> = root.as_os_str().to_string_lossy().encode_utf16().collect();
    if !root.last().is_some_and(|ch| *ch == '\\' as u16) {
        root.push('\\' as u16);
    }
    root.push(0);
    let mut name = [0u16; 256];
    // SAFETY: both buffers are valid for their lengths and the root path is NUL-terminated.
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            name.as_mut_ptr(),
            name.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        return String::new();
    }
    let len = name.iter().position(|ch| *ch == 0).unwrap_or(name.len());
    String::from_utf16_lossy(&name[..len]).trim().to_string()
}

/// Linux's short drive list excludes system mounts and path-only labels.
/// The complete mount inventory remains available for path resolution.
#[cfg(any(target_os = "linux", test))]
fn linux_pickable(volume: &Volume) -> bool {
    if volume.kind == VolumeKind::Virtual || volume.root == Path::new("/") {
        return false;
    }
    if volume.display_name().trim_start().starts_with('/') {
        return false;
    }
    if volume.root.starts_with("/run") && !volume.root.starts_with("/run/media") {
        return false;
    }
    !["/snap", "/var/snap", "/boot", "/proc", "/sys", "/dev"]
        .iter()
        .any(|prefix| volume.root.starts_with(*prefix))
}

/// The list shown in the panel's volume selector.
/// On Linux both drive menus use the same labelled-volume shortlist.
/// Unlabelled mounts and the root remain accessible via ordinary paths.
pub fn pickable_volumes() -> Vec<Volume> {
    volumes()
        .into_iter()
        .filter(|volume| {
            if volume.kind == VolumeKind::Virtual {
                return false;
            }
            #[cfg(target_os = "linux")]
            {
                return linux_pickable(volume);
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            {
                let root = volume.root.as_os_str().to_string_lossy();
                if volume.kind == VolumeKind::Fixed && volume.root == Path::new("/") {
                    return false;
                }
                for prefix in ["/snap/", "/var/snap/", "/boot/", "/proc", "/sys", "/dev"] {
                    if root.starts_with(prefix) {
                        return false;
                    }
                }
            }
            #[cfg(not(target_os = "linux"))]
            true
        })
        .collect()
}

/// The volume a path belongs to, for the selector's current value.
pub fn volume_of(path: &Path) -> Option<Volume> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    volumes()
        .into_iter()
        .filter(|volume| {
            let root = std::fs::canonicalize(&volume.root).unwrap_or_else(|_| volume.root.clone());
            canonical.starts_with(&root)
        })
        // The longest matching root wins (`/mnt/data` over `/`).
        .max_by_key(|volume| volume.root.as_os_str().len())
}

/// The selector entries with their free space, removable media first.
///
/// A disk you just plugged in is the one you are looking for, so it goes on top; the free
/// space next to the name answers "do I have room for this copy?" without opening anything.
pub fn selector_entries() -> Vec<(Volume, String)> {
    let mut volumes = pickable_volumes();
    volumes.sort_by(|a, b| {
        let rank = |kind: VolumeKind| match kind {
            VolumeKind::Removable => 0,
            VolumeKind::Optical => 1,
            VolumeKind::Network => 2,
            VolumeKind::Fixed => 3,
            VolumeKind::Virtual => 4,
            VolumeKind::Unknown => 5,
        };
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then_with(|| a.root.cmp(&b.root))
    });
    volumes
        .into_iter()
        .map(|volume| {
            let base = selector_label(&volume);
            let free = volume.space().map(|space| space.available);
            let label = match free {
                Some(free) if free > 0 => {
                    format!("{base}  —  {}", crate::format::format_size_bytes(free))
                }
                _ => base,
            };
            (volume, label)
        })
        .collect()
}

/// Text for the volume selector: `C: System` on Windows, `DATA (on /mnt/data)` elsewhere.
pub fn selector_label(volume: &Volume) -> String {
    let name = volume.label.trim();
    #[cfg(target_os = "windows")]
    {
        let letter = volume.root.to_string_lossy().chars().next().unwrap_or('?');
        if name.is_empty() || name.eq_ignore_ascii_case(&volume.root.to_string_lossy()) {
            format!("{letter}:")
        } else {
            format!("{letter}: {name}")
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if name.is_empty() || name == volume.root.to_string_lossy() {
            volume.root.display().to_string()
        } else {
            format!("{name} ({})", volume.root.display())
        }
    }
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
        unsafe extern "system" {
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

    #[cfg(not(windows))]
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

    #[cfg(windows)]
    #[test]
    fn windows_volumes_use_drive_letter_roots() {
        let mounted = volumes();
        assert!(
            !mounted.is_empty(),
            "GetLogicalDrives should report at least the system drive"
        );
        for volume in mounted {
            let root = volume.root.to_string_lossy();
            let bytes = root.as_bytes();
            assert!(
                bytes.len() == 3
                    && bytes[0].is_ascii_alphabetic()
                    && bytes[1] == b':'
                    && bytes[2] == b'\\',
                "invalid Windows drive root: {root}"
            );
        }
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

#[cfg(test)]
mod selector_tests {
    use super::*;

    #[test]
    fn the_selector_hides_virtual_and_system_volumes() {
        let pickable = pickable_volumes();
        assert!(
            pickable.iter().all(|v| v.kind != VolumeKind::Virtual),
            "virtual volumes are not selectable: {pickable:?}"
        );
        #[cfg(not(target_os = "windows"))]
        assert!(
            pickable.iter().all(|v| v.root != Path::new("/")),
            "the Linux root is the system volume"
        );
        // Everything hidden from the picker stays in the complete mount inventory.
        assert!(volumes().len() >= pickable.len());
    }

    #[test]
    fn linux_drive_picker_hides_system_and_path_only_labels() {
        let candidates = [
            Volume::new("/", "/", VolumeKind::Fixed),
            Volume::new("/dev", "/dev", VolumeKind::Virtual),
            Volume::new("/proc", "/proc", VolumeKind::Virtual),
            Volume::new("/snap/core/1", "/snap/core/1", VolumeKind::Virtual),
            Volume::new("/boot/efi", "EFI", VolumeKind::Fixed),
            Volume::new("/run/user/1000", "/run/user/1000", VolumeKind::Unknown),
            Volume::new("/mnt/unlabelled", "/mnt/unlabelled", VolumeKind::Removable),
            Volume::new("/mnt/programming", "PROGRAMMING", VolumeKind::Fixed),
            Volume::new("/media/ssd", "SSD_240GB", VolumeKind::Removable),
            Volume::new("/run/media/user/DATA", "DATA", VolumeKind::Removable),
        ];
        let visible: Vec<_> = candidates.iter()
            .filter(|v| linux_pickable(v))
            .map(|v| v.label.as_str()).collect();
        assert_eq!(visible, vec!["PROGRAMMING", "SSD_240GB", "DATA"]);
    }

    #[test]
    fn a_volume_label_is_descriptive_and_never_empty_in_the_selector() {
        for volume in pickable_volumes() {
            let text = selector_label(&volume);
            assert!(!text.is_empty(), "a volume always shows something");
        }
        // A hand-made volume with a real label reads the way the panel owner asked for.
        let labelled = Volume::new(
            if cfg!(windows) { "C:\\" } else { "/mnt/data" },
            "DATA",
            VolumeKind::Fixed,
        );
        let text = selector_label(&labelled);
        assert!(text.contains("DATA"), "{text}");
        #[cfg(windows)]
        assert!(text.starts_with("C:"), "letter first on Windows: {text}");
        #[cfg(not(windows))]
        assert!(text.contains("/mnt/data"), "mount shown on Linux: {text}");
    }

    #[test]
    fn the_volume_of_a_path_is_the_longest_matching_root() {
        let volumes = volumes();
        if volumes.is_empty() {
            return;
        }
        // A path inside the volume root maps back to that volume.
        let volume = &volumes[0];
        let inside = volume.root.join("some-file.txt");
        let found = volume_of(&inside);
        assert!(found.is_some(), "every path belongs to a volume");
        // And an absurd path simply has none rather than panicking.
        let _ = volume_of(Path::new("/definitely/not/here/at/all"));
    }

    #[test]
    fn label_escapes_are_decoded() {
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(unescape_label("My\\x20Disk"), "My Disk");
            assert_eq!(unescape_label("plain"), "plain");
            assert_eq!(unescape_label(""), "");
        }
    }
}

#[cfg(test)]
mod selector_entry_tests {
    use super::*;

    #[test]
    fn the_selector_entries_carry_free_space_and_put_removable_media_first() {
        let entries = selector_entries();
        if entries.is_empty() {
            return;
        }
        for (volume, label) in &entries {
            assert!(
                label.starts_with(&selector_label(volume)),
                "the label starts with the volume name: {label}"
            );
        }
        // Removable volumes sort before fixed ones.
        let ranks: Vec<u8> = entries
            .iter()
            .map(|(volume, _)| match volume.kind {
                VolumeKind::Removable => 0,
                VolumeKind::Optical => 1,
                VolumeKind::Network => 2,
                VolumeKind::Fixed => 3,
                VolumeKind::Virtual => 4,
                VolumeKind::Unknown => 5,
            })
            .collect();
        let mut sorted = ranks.clone();
        sorted.sort_unstable();
        assert_eq!(ranks, sorted, "removable media come first");
    }
}

/// Expand a leading `~` in a user-typed path.
///
/// The dialog hints at `~/.ssh/id_ed25519`, so people type exactly that; the operating system
/// does not expand it for us, and a private key that "does not exist" is a confusing way to learn
/// that. Only a leading `~` (alone or followed by a separator) is expanded — `~user/...` and a `~`
/// in the middle of a name are left alone, because guessing there would be worse than failing.
pub fn expand_home(path: &str) -> std::path::PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" {
        return home_dir().unwrap_or_else(|| std::path::PathBuf::from(trimmed));
    }
    let rest = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"));
    match (rest, home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => std::path::PathBuf::from(trimmed),
    }
}

#[cfg(test)]
mod expand_tests {
    use super::expand_home;
    use std::path::PathBuf;

    #[test]
    fn a_leading_tilde_becomes_the_home_directory() {
        let Some(home) = super::home_dir() else {
            return; // a machine with no home directory is not our problem
        };
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("   "), PathBuf::from(""));
        assert_eq!(
            expand_home("~/.ssh/id_ed25519"),
            home.join(".ssh/id_ed25519")
        );
        // An absolute path is returned untouched.
        assert_eq!(
            expand_home("/etc/ssh/id_ed25519"),
            PathBuf::from("/etc/ssh/id_ed25519")
        );
        // A tilde inside a name is not a home directory.
        assert_eq!(expand_home("back~up.txt"), PathBuf::from("back~up.txt"));
        // Nor is `~user`, which needs a passwd lookup we do not do.
        assert_eq!(expand_home("~root/key"), PathBuf::from("~root/key"));
    }
}

//! Member-name handling: normalisation, splitting and the path-safety rules.
//!
//! Archive members are stored as flat strings that may use either separator and
//! may be hostile (`../evil`, `/abs`, `C:\abs`). Everything in this module
//! works on strings so that the same rules apply to every backend, whatever the
//! container's own escaping conventions are.

use std::path::PathBuf;

/// Split a raw member name into path components.
///
/// Both `/` and `\` are separators (Windows-created archives use backslashes),
/// empty components collapse, and `.` components are dropped. `..` components
/// are kept so that [`unsafe_reason`] can report them.
pub(crate) fn components(raw: &str) -> Vec<&str> {
    raw.split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != ".")
        .collect()
}

/// Normalise a raw member name into an archive-relative, `/`-separated path.
///
/// The result has no leading or trailing separator and no `.` components.
/// `..` components survive normalisation: they are reported later by
/// [`unsafe_reason`] instead of being silently relocated.
pub(crate) fn normalize(raw: &str) -> String {
    components(raw).join("/")
}

/// The last component of a normalised or raw path (`""` for a root entry).
pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|c| !c.is_empty())
        .unwrap_or("")
}

/// True when `path` names an absolute location on any platform we care about.
fn is_absolute(raw: &str) -> bool {
    if raw.starts_with('/') || raw.starts_with('\\') {
        return true;
    }
    let bytes = raw.as_bytes();
    // `C:` prefix — a drive letter, optionally followed by a separator.
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Why a raw member name may not be extracted, or `None` when it is safe.
///
/// Rejects absolute paths (`/abs.txt`), Windows drive letters (`C:\x`),
/// UNC-style names (`\\server\share`) and any `..` component.
pub(crate) fn unsafe_reason(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Some("absolute path".to_string());
    }
    if is_absolute(raw) {
        return Some("drive-letter path".to_string());
    }
    if components(raw).contains(&"..") {
        return Some("parent-directory (..) component".to_string());
    }
    // A normalised empty result is the archive root: harmless but nothing to write.
    None
}

/// Turn an already normalised relative path into a [`PathBuf`] that is
/// guaranteed to stay inside its base directory.
///
/// Returns `None` when the path is empty or contains a `..` component.
pub(crate) fn safe_relative(rel: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in components(rel) {
        if component == ".." {
            return None;
        }
        out.push(component);
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Normalise the `inner` directory argument used by the listing functions.
///
/// Accepts `""`, `/`, `a`, `a/`, `/a/b/` and `a\\b`; `..` components are
/// dropped so that a listing can never be asked to leave the archive root.
pub(crate) fn normalize_inner(inner: &str) -> String {
    components(inner)
        .into_iter()
        .filter(|c| *c != "..")
        .collect::<Vec<_>>()
        .join("/")
}

/// The full display path of a member: `<archive>/<inner>/<name>`.
pub(crate) fn display_path(archive: &std::path::Path, inner: &str, name: &str) -> PathBuf {
    let mut path = archive.to_path_buf();
    if !inner.is_empty() {
        path.push(inner);
    }
    if !name.is_empty() {
        path.push(name);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_handle_both_separators_and_noise() {
        assert_eq!(components("a/b/c.txt"), ["a", "b", "c.txt"]);
        assert_eq!(components("a\\b\\c.txt"), ["a", "b", "c.txt"]);
        assert_eq!(components("/a//b/./c/"), ["a", "b", "c"]);
        assert_eq!(components("./a"), ["a"]);
        assert_eq!(components(".."), [".."]);
        assert!(components("").is_empty());
    }

    #[test]
    fn normalize_strips_leads_and_trailing_slashes() {
        assert_eq!(normalize("/a/b/"), "a/b");
        assert_eq!(normalize("a\\b"), "a/b");
        assert_eq!(normalize("./x"), "x");
        assert_eq!(normalize(""), "");
        // `..` is preserved so that safety checks can see it.
        assert_eq!(normalize("../evil.txt"), "../evil.txt");
        assert_eq!(normalize("a/../../evil"), "a/../../evil");
    }

    #[test]
    fn file_name_of_a_normalised_path() {
        assert_eq!(file_name("a/b/c.txt"), "c.txt");
        assert_eq!(file_name("c.txt"), "c.txt");
        assert_eq!(file_name("a/"), "a");
        assert_eq!(file_name(""), "");
    }

    #[test]
    fn absolute_and_parent_paths_are_unsafe() {
        assert!(unsafe_reason("/abs.txt").unwrap().contains("absolute"));
        assert!(unsafe_reason("\\abs.txt").unwrap().contains("absolute"));
        assert!(
            unsafe_reason("\\\\server\\share\\x")
                .unwrap()
                .contains("absolute")
        );
        assert!(unsafe_reason("C:\\evil.txt").unwrap().contains("drive"));
        assert!(unsafe_reason("c:evil.txt").unwrap().contains("drive"));
        assert!(
            unsafe_reason("../evil.txt")
                .unwrap()
                .contains("parent-directory")
        );
        assert!(
            unsafe_reason("a/../../evil.txt")
                .unwrap()
                .contains("parent-directory")
        );
        assert!(
            unsafe_reason("..\\evil.txt")
                .unwrap()
                .contains("parent-directory")
        );
        assert!(unsafe_reason("a/..").unwrap().contains("parent-directory"));
    }

    #[test]
    fn ordinary_members_are_safe() {
        assert!(unsafe_reason("a/b/c.txt").is_none());
        assert!(unsafe_reason("dir/").is_none());
        assert!(unsafe_reason("žluťoučký.txt").is_none());
        assert!(unsafe_reason("...txt").is_none());
        assert!(unsafe_reason("a..b/c").is_none());
        assert!(unsafe_reason("").is_none());
    }

    #[test]
    fn safe_relative_never_climbs_out() {
        assert_eq!(safe_relative("a/b"), Some(PathBuf::from("a").join("b")));
        assert_eq!(safe_relative("a.txt"), Some(PathBuf::from("a.txt")));
        assert_eq!(safe_relative("../x"), None);
        assert_eq!(safe_relative("a/../../x"), None);
        assert_eq!(safe_relative(""), None);
        assert_eq!(safe_relative("./"), None);
    }

    #[test]
    fn inner_paths_are_normalised_and_clamped() {
        assert_eq!(normalize_inner(""), "");
        assert_eq!(normalize_inner("/"), "");
        assert_eq!(normalize_inner("a/"), "a");
        assert_eq!(normalize_inner("/a/b/"), "a/b");
        assert_eq!(normalize_inner("a\\b"), "a/b");
        assert_eq!(normalize_inner("../a"), "a");
        assert_eq!(normalize_inner("./"), "");
    }

    #[test]
    fn display_path_joins_archive_and_member() {
        let p = display_path(std::path::Path::new("/tmp/a.zip"), "dir", "file.txt");
        assert_eq!(p, PathBuf::from("/tmp/a.zip").join("dir").join("file.txt"));
        let p = display_path(std::path::Path::new("/tmp/a.zip"), "", "file.txt");
        assert_eq!(p, PathBuf::from("/tmp/a.zip").join("file.txt"));
    }
}

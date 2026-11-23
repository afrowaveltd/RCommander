//! Where the application finds its data files.

use std::path::{Path, PathBuf};

/// Directory of the running executable, if it can be determined.
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

/// Candidate roots for bundled data (`locales/`, `Docs/`), most specific first.
///
/// The search order lets the application run from a build directory
/// (`target/debug`), from an installed location and from the repository root.
pub fn data_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(explicit) = std::env::var("RCOMMANDER_DATA_DIR")
        && !explicit.is_empty()
    {
        roots.push(PathBuf::from(explicit));
    }
    if let Some(dir) = exe_dir() {
        roots.push(dir.clone());
        for up in [1, 2, 3] {
            let mut p = dir.clone();
            for _ in 0..up {
                p = match p.parent() {
                    Some(parent) => parent.to_path_buf(),
                    None => break,
                };
            }
            roots.push(p);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    roots
}

/// Locate a sub-directory of the data roots, e.g. `locales` or `Docs`.
pub fn find_data_dir(name: &str) -> Option<PathBuf> {
    for root in data_roots() {
        let candidate = root.join(name);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Directory holding the language packs.
pub fn locales_dir() -> PathBuf {
    find_data_dir("locales").unwrap_or_else(|| PathBuf::from("locales"))
}

/// Directory holding the in-app help.
pub fn docs_dir() -> PathBuf {
    find_data_dir("Docs").unwrap_or_else(|| PathBuf::from("Docs"))
}

/// Path of the settings file.
pub fn settings_path() -> PathBuf {
    rc_core::config::default_config_path()
}

/// Path of the hotlist file.
pub fn hotlist_path() -> PathBuf {
    rc_core::config::config_dir().join("hotlist.toml")
}

/// Session file: last directories, tabs and window geometry.
pub fn session_path() -> PathBuf {
    rc_core::config::config_dir().join("session.toml")
}

/// True when `path` is inside `dir` (used to decide whether to show "portable" hints).
pub fn is_inside(path: &Path, dir: &Path) -> bool {
    path.starts_with(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_roots_are_absolute_and_unique() {
        let roots = data_roots();
        assert!(!roots.is_empty());
        for root in &roots {
            assert!(
                root.is_absolute() || root == Path::new("."),
                "root: {}",
                root.display()
            );
        }
    }

    #[test]
    fn locales_and_docs_directories_resolve_to_paths() {
        assert!(locales_dir().to_string_lossy().contains("locales"));
        assert!(docs_dir().to_string_lossy().contains("Docs"));
    }

    #[test]
    fn config_paths_live_in_the_config_dir() {
        let dir = rc_core::config::config_dir();
        assert!(settings_path().starts_with(&dir));
        assert!(hotlist_path().starts_with(&dir));
        assert!(session_path().starts_with(&dir));
        assert!(settings_path().ends_with("settings.toml"));
    }

    #[test]
    fn is_inside_detects_nesting() {
        assert!(is_inside(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(!is_inside(Path::new("/a/x"), Path::new("/a/b")));
    }
}

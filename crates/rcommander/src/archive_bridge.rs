//! Thin integration layer over `rc-archive`, kept separate so the model does not
//! depend on the archive crate and can be tested with plain directories.

use rc_core::entry::Entry;
use rc_core::location::Location;
use rc_core::ops::CancelToken;
use std::path::{Path, PathBuf};

/// List a directory inside an archive as panel entries.
pub fn list_archive(container: &Path, inner: &str) -> Result<Vec<Entry>, String> {
    rc_archive::list_dir(container, inner).map_err(|e| e.to_string())
}

/// Read a location that may be an archive.
pub fn read_archive_listing(location: &Location) -> Option<Result<Vec<Entry>, String>> {
    location
        .archive_parts()
        .map(|(container, inner)| list_archive(container, inner))
}

/// Ensure `name` ends with the extension implied by `format`.
pub fn with_format_extension(name: &str, format: &str) -> String {
    let suffix = match format {
        "tar.gz" => ".tar.gz",
        "tar" => ".tar",
        _ => ".zip",
    };
    let lower = name.to_ascii_lowercase();
    // Trust a name the user typed with a compatible extension.
    let compatible = lower.ends_with(suffix)
        || (format == "tar.gz" && lower.ends_with(".tgz"))
        || lower.ends_with(".zip")
        || lower.ends_with(".tar");
    if compatible {
        name.to_string()
    } else {
        format!("{name}{suffix}")
    }
}

/// Create an archive from explicit paths. The container follows the extension
/// of `out` (`zip` and `tar.gz` are supported by the backend).
pub fn create_archive(
    out: &Path,
    sources: &[PathBuf],
    format: &str,
    level: u8,
) -> Result<u64, String> {
    let out = Path::new(&with_format_extension(&out.to_string_lossy(), format)).to_path_buf();
    let options = rc_archive::PackOptions {
        level,
        ..Default::default()
    };
    rc_archive::create(&out, sources, &options)
        .map(|outcome| outcome.added)
        .map_err(|e| e.to_string())
}

/// Extract a whole archive into `dest`, returning the number of files written.
pub fn extract_archive(archive: &Path, dest: &Path, cancel: &CancelToken) -> Result<u64, String> {
    rc_archive::extract_all(archive, dest, cancel)
        .map(|outcome| outcome.files)
        .map_err(|e| e.to_string())
}

/// Test an archive for integrity; returns a human readable summary.
pub fn test_archive(path: &Path, cancel: &CancelToken) -> Result<String, String> {
    rc_archive::test(path, cancel)
        .map_err(|e| e.to_string())
        .map(|report| {
            if report.ok {
                format!(
                    "{} entries, {} bytes read — OK",
                    report.entries, report.bytes_read
                )
            } else {
                format!(
                    "{} entries, problems: {}",
                    report.entries,
                    report.errors.join("; ")
                )
            }
        })
}

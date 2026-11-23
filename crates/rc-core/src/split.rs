//! Splitting a file into numbered parts and combining them again.
//!
//! Part names follow the classic scheme: `report.pdf` becomes `report.001`,
//! `report.002`, … and combining appends the parts in numeric order.

use crate::error::{CoreError, CoreResult};
use crate::ops::CancelToken;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// Default part size: 100 MiB.
pub const DEFAULT_PART_SIZE: u64 = 100 * 1024 * 1024;

/// Path of part number `index` (1-based) for `original`.
pub fn part_path(original: &Path, index: usize) -> PathBuf {
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "part".to_string());
    let parent = original
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    parent.join(format!("{stem}.{index:03}"))
}

/// All part paths that a split of `original` into `count` parts would produce.
pub fn part_paths(original: &Path, count: usize) -> Vec<PathBuf> {
    (1..=count)
        .map(|index| part_path(original, index))
        .collect()
}

/// Split `source` into parts of `part_size` bytes in `destination`.
///
/// Returns the created part paths. The source is left untouched.
pub fn split_file(
    source: &Path,
    part_size: u64,
    destination: Option<&Path>,
    cancel: &CancelToken,
) -> CoreResult<Vec<PathBuf>> {
    if part_size == 0 {
        return Err(CoreError::Invalid(
            "part size must be greater than zero".to_string(),
        ));
    }
    let metadata = std::fs::metadata(source).map_err(|e| CoreError::io(source, e))?;
    if metadata.is_dir() {
        return Err(CoreError::Invalid(format!(
            "{} is a directory",
            source.display()
        )));
    }
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "part".to_string());
    let target_dir = destination
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| source.parent().map(|p| p.to_path_buf()).unwrap_or_default());
    if !target_dir.as_os_str().is_empty() {
        std::fs::create_dir_all(&target_dir).map_err(|e| CoreError::io(&target_dir, e))?;
    }
    let base = target_dir.join(name);
    let mut input = File::open(source).map_err(|e| CoreError::io(source, e))?;
    let mut parts = Vec::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut index = 1usize;
    loop {
        cancel.check()?;
        let path = part_path(&base, index);
        let mut output = BufWriter::new(File::create(&path).map_err(|e| CoreError::io(&path, e))?);
        let mut written = 0u64;
        while written < part_size {
            cancel.check()?;
            let want = ((part_size - written) as usize).min(buffer.len());
            let read = input
                .read(&mut buffer[..want])
                .map_err(|e| CoreError::io(source, e))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|e| CoreError::io(&path, e))?;
            written += read as u64;
        }
        output.flush().map_err(|e| CoreError::io(&path, e))?;
        if written > 0 {
            parts.push(path);
            index += 1;
        }
        if written < part_size {
            break;
        }
    }
    if parts.is_empty() {
        // An empty source still produces one (empty) part.
        let path = part_path(&base, 1);
        File::create(&path).map_err(|e| CoreError::io(&path, e))?;
        parts.push(path);
    }
    Ok(parts)
}

/// Combine `parts` into `destination`, returning the number of bytes written.
pub fn combine_files(
    parts: &[PathBuf],
    destination: &Path,
    cancel: &CancelToken,
) -> CoreResult<u64> {
    if parts.is_empty() {
        return Err(CoreError::Invalid("no parts to combine".to_string()));
    }
    if let Some(parent) = destination.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    }
    let mut output =
        BufWriter::new(File::create(destination).map_err(|e| CoreError::io(destination, e))?);
    let mut buffer = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    for part in parts {
        cancel.check()?;
        let mut input = File::open(part).map_err(|e| CoreError::io(part, e))?;
        loop {
            cancel.check()?;
            let read = input
                .read(&mut buffer)
                .map_err(|e| CoreError::io(part, e))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|e| CoreError::io(destination, e))?;
            total += read as u64;
        }
    }
    output.flush().map_err(|e| CoreError::io(destination, e))?;
    Ok(total)
}

/// Find the parts that belong to `first` in its directory, in numeric order.
///
/// Gaps stop the scan, so an incomplete set is detected instead of silently
/// producing a truncated file.
pub fn discover_parts(first: &Path) -> CoreResult<Vec<PathBuf>> {
    let stem = first
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| CoreError::Invalid("no file stem".to_string()))?;
    let parent = first.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut parts = Vec::new();
    let mut index = 1usize;
    loop {
        let candidate = parent.join(format!("{stem}.{index:03}"));
        if candidate.is_file() {
            parts.push(candidate);
            index += 1;
        } else {
            break;
        }
    }
    if parts.is_empty() {
        return Err(CoreError::Invalid(format!(
            "no parts found for {}",
            first.display()
        )));
    }
    Ok(parts)
}

/// Destination name for a combined file: the part stem.
pub fn combined_name(part: &Path) -> PathBuf {
    part.with_extension("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn part_names_are_numbered_from_one() {
        let original = Path::new("/tmp/report.pdf");
        assert_eq!(part_path(original, 1), PathBuf::from("/tmp/report.001"));
        assert_eq!(part_path(original, 12), PathBuf::from("/tmp/report.012"));
        assert_eq!(part_paths(original, 3).len(), 3);
        assert_eq!(
            combined_name(Path::new("/tmp/report.003")),
            PathBuf::from("/tmp/report")
        );
    }

    #[test]
    fn splitting_and_combining_restores_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("data.bin");
        let content: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        write(&source, &content);
        let cancel = CancelToken::new();
        let parts = split_file(&source, 4096, None, &cancel).unwrap();
        assert_eq!(parts.len(), 3, "10000 bytes in 4096 byte parts is 3 parts");
        assert_eq!(fs::metadata(&parts[0]).unwrap().len(), 4096);
        assert_eq!(fs::metadata(&parts[1]).unwrap().len(), 4096);
        assert_eq!(fs::metadata(&parts[2]).unwrap().len(), 1808);
        let joined = dir.path().join("joined.bin");
        let bytes = combine_files(&parts, &joined, &cancel).unwrap();
        assert_eq!(bytes, 10_000);
        assert_eq!(fs::read(&joined).unwrap(), content);
        // Discovering the parts finds the same list.
        assert_eq!(discover_parts(&parts[0]).unwrap(), parts);
    }

    #[test]
    fn split_into_a_destination_directory() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.txt");
        write(&source, b"0123456789");
        let out_dir = dir.path().join("parts");
        let cancel = CancelToken::new();
        let parts = split_file(&source, 4, Some(&out_dir), &cancel).unwrap();
        assert_eq!(parts.len(), 3);
        assert!(out_dir.is_dir());
        assert_eq!(fs::read(&parts[0]).unwrap(), b"0123");
    }

    #[test]
    fn an_empty_file_yields_one_empty_part() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("empty.bin");
        write(&source, b"");
        let cancel = CancelToken::new();
        let parts = split_file(&source, 1024, None, &cancel).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(fs::metadata(&parts[0]).unwrap().len(), 0);
    }

    #[test]
    fn invalid_requests_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.txt");
        write(&source, b"x");
        let cancel = CancelToken::new();
        assert!(split_file(&source, 0, None, &cancel).is_err());
        assert!(split_file(dir.path(), 10, None, &cancel).is_err());
        assert!(combine_files(&[], &dir.path().join("x"), &cancel).is_err());
        assert!(discover_parts(&dir.path().join("nothing.001")).is_err());
    }

    #[test]
    fn a_gap_in_the_numbering_stops_discovery() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("x.001"), b"a");
        write(&dir.path().join("x.003"), b"c");
        let parts = discover_parts(&dir.path().join("x.001")).unwrap();
        assert_eq!(parts.len(), 1, "the sequence stops at the missing 002");
    }

    #[test]
    fn cancellation_stops_splitting() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("big.bin");
        write(&source, &vec![1u8; 300_000]);
        let cancel = CancelToken::new();
        cancel.cancel();
        let error = split_file(&source, 1024, None, &cancel).unwrap_err();
        assert!(error.is_cancelled());
    }

    #[test]
    fn the_default_part_size_is_a_hundred_megabytes() {
        assert_eq!(DEFAULT_PART_SIZE, 104_857_600);
    }
}

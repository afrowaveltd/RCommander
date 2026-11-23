//! Creating archives: the packer dialog's engine.
//!
//! `create` can write zip (stored at level 0, deflated otherwise) and tar.gz. A
//! new archive is built in a temporary file next to the destination and then
//! renamed over it, so replacing an existing archive is atomic: either the old
//! file is still there, or the complete new one is.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use tar::{Builder, EntryType, Header};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::backend::zip::zip_error_for;
use crate::error::{ArchiveError, Result};
use crate::kind::{ArchiveKind, kind_from_name};
use crate::timeutil;

/// Options for [`create`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackOptions {
    /// Compression level, `0..=9`: `0` stores members uncompressed, `1..=9`
    /// deflate them. Values above `9` are clamped.
    pub level: u8,
    /// Password to encrypt with.
    ///
    /// **Not supported yet**: any non-empty password makes [`create`] return
    /// [`ArchiveError::Unsupported`] rather than silently writing a plain
    /// archive.
    pub password: Option<String>,
    /// Base directory that stored member names are relative to.
    ///
    /// When `None`, each item's own parent is used, so packing `/data/proj`
    /// stores `proj/...` and packing `/data/proj/a.txt` stores `a.txt`.
    pub store_paths_relative_to: Option<PathBuf>,
}

impl Default for PackOptions {
    fn default() -> Self {
        PackOptions {
            level: 6,
            password: None,
            store_paths_relative_to: None,
        }
    }
}

/// What [`create`] (and the zip rewriting helpers) produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackOutcome {
    /// Members added (files plus directory entries).
    pub added: u64,
    /// Total uncompressed bytes fed into the archive.
    pub bytes_in: u64,
    /// Final size of the archive file.
    pub archive_size: u64,
}

/// One thing that will be stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// A directory member.
    Dir { name: String },
    /// A regular file member.
    File {
        src: PathBuf,
        name: String,
        size: u64,
    },
}

impl Plan {
    /// The member name this plan stores.
    pub(crate) fn name(&self) -> &str {
        match self {
            Plan::Dir { name } => name,
            Plan::File { name, .. } => name,
        }
    }
}

/// Compression level as a deflate level, or `None` when storing.
fn deflate_level(level: u8) -> Option<i64> {
    if level == 0 {
        None
    } else {
        Some(i64::from(level.min(9)))
    }
}

/// Create `out` from `items` (files and/or directories, recursing into dirs).
///
/// The container follows the extension of `out`: `.zip` produces a zip and
/// `.tar.gz`/`.tgz` a gzip-compressed tar. Every other extension is refused with
/// [`ArchiveError::Unsupported`].
///
/// Stored names are `/`-separated and relative to
/// [`PackOptions::store_paths_relative_to`] (or to each item's own parent when
/// that is `None`). Symbolic links are skipped, and so are sockets, devices and
/// fifos, so a pack never hangs on a pipe.
///
/// Replacing an existing archive is atomic: the archive is written to a
/// temporary file in the same directory and renamed into place only when it is
/// complete.
pub fn create(out: &Path, items: &[PathBuf], opts: &PackOptions) -> Result<PackOutcome> {
    reject_password(opts.password.as_deref())?;
    let kind = kind_from_name(
        &out.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    if !matches!(kind, ArchiveKind::Zip | ArchiveKind::TarGz) {
        return Err(ArchiveError::unsupported(format!(
            "cannot create {}: only .zip, .tar.gz and .tgz are supported for writing",
            out.display()
        )));
    }

    let plans = collect_plans(items, opts.store_paths_relative_to.as_deref())?;
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| ArchiveError::io(parent, e))?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| ArchiveError::io(parent, e))?;

    let result = match kind {
        ArchiveKind::Zip => write_zip(temp.as_file_mut(), &plans, opts.level, out),
        _ => write_tar_gz(temp.as_file_mut(), &plans, opts.level, out),
    };
    let (added, bytes_in) = result?;

    temp.as_file_mut()
        .flush()
        .map_err(|e| ArchiveError::io(out, e))?;
    let archive_size = temp
        .as_file()
        .metadata()
        .map_err(|e| ArchiveError::io(out, e))?
        .len();
    temp.persist(out)
        .map_err(|e| ArchiveError::io(out, e.error))?;
    Ok(PackOutcome {
        added,
        bytes_in,
        archive_size,
    })
}

/// Refuse a non-empty password instead of writing an unencrypted archive.
pub(crate) fn reject_password(password: Option<&str>) -> Result<()> {
    match password {
        Some(p) if !p.is_empty() => Err(ArchiveError::unsupported(
            "password-protected archives are not supported yet: omit the password to create a plain archive",
        )),
        _ => Ok(()),
    }
}

/// Expand `items` into the exact list of members to store.
///
/// `base` overrides the per-item parent directory. The result is sorted and
/// de-duplicated, so the same member name is never written twice.
pub(crate) fn collect_plans(items: &[PathBuf], base: Option<&Path>) -> Result<Vec<Plan>> {
    let mut plans: Vec<Plan> = Vec::new();
    for item in items {
        let default_base = item.parent().map(Path::to_path_buf).unwrap_or_default();
        let base = match base {
            Some(base) => base.to_path_buf(),
            None => default_base,
        };
        collect_one(item, &base, &mut plans)?;
    }
    plans.sort_by(|a, b| a.name().cmp(b.name()));
    plans.dedup_by(|a, b| a.name() == b.name());
    Ok(plans)
}

/// Recursively add one path (and, for directories, its contents).
fn collect_one(src: &Path, base: &Path, out: &mut Vec<Plan>) -> Result<()> {
    let metadata = fs::symlink_metadata(src).map_err(|e| ArchiveError::io(src, e))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        // Links are not followed: they may point outside the packed tree, and
        // following them can loop.
        return Ok(());
    }
    let name = member_name(src, base)?;
    if file_type.is_dir() {
        out.push(Plan::Dir { name });
        let mut children: Vec<PathBuf> = fs::read_dir(src)
            .map_err(|e| ArchiveError::io(src, e))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        children.sort();
        for child in children {
            collect_one(&child, base, out)?;
        }
    } else if file_type.is_file() {
        out.push(Plan::File {
            src: src.to_path_buf(),
            name,
            size: metadata.len(),
        });
    }
    // Sockets, fifos, devices: nothing sensible to store.
    Ok(())
}

/// The stored name for `src`, relative to `base`.
fn member_name(src: &Path, base: &Path) -> Result<String> {
    let relative =
        match src.strip_prefix(base) {
            Ok(relative) => relative.to_path_buf(),
            Err(_) => PathBuf::from(src.file_name().ok_or_else(|| {
                ArchiveError::unsupported(format!("cannot name {}", src.display()))
            })?),
        };
    let name = relative.to_string_lossy().replace('\\', "/");
    let name = name.trim_matches('/').to_string();
    if name.is_empty() || name == "." || name == ".." {
        return Err(ArchiveError::unsupported(format!(
            "cannot derive a member name for {}",
            src.display()
        )));
    }
    Ok(name)
}

/// Write `plans` into an already created [`ZipWriter`]; returns
/// `(members, bytes fed in)`.
pub(crate) fn add_plans_to_zip<W: Write + Seek>(
    writer: &mut ZipWriter<W>,
    plans: &[Plan],
    level: u8,
    out: &Path,
) -> Result<(u64, u64)> {
    let method = if level == 0 {
        CompressionMethod::Stored
    } else {
        CompressionMethod::Deflated
    };
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .compression_level(deflate_level(level))
        .last_modified_time(zip::DateTime::default_for_write());

    let mut added = 0u64;
    let mut bytes_in = 0u64;
    for plan in plans {
        match plan {
            Plan::Dir { name } => {
                writer
                    .add_directory(name.as_str(), options)
                    .map_err(|e| zip_error_for(out, None, e))?;
                added += 1;
            }
            Plan::File { src, name, .. } => {
                writer
                    .start_file(name.as_str(), options)
                    .map_err(|e| zip_error_for(out, Some(name), e))?;
                let mut file = File::open(src).map_err(|e| ArchiveError::io(src, e))?;
                let copied = io::copy(&mut file, writer).map_err(|e| ArchiveError::io(src, e))?;
                added += 1;
                bytes_in += copied;
            }
        }
    }
    Ok((added, bytes_in))
}

/// Write the whole archive as a zip.
fn write_zip<W: Write + Seek>(
    writer: &mut W,
    plans: &[Plan],
    level: u8,
    out: &Path,
) -> Result<(u64, u64)> {
    let mut zip = ZipWriter::new(writer);
    let result = add_plans_to_zip(&mut zip, plans, level, out)?;
    zip.finish().map_err(|e| zip_error_for(out, None, e))?;
    Ok(result)
}

/// Write the whole archive as a gzip-compressed tar.
fn write_tar_gz<W: Write>(
    writer: &mut W,
    plans: &[Plan],
    level: u8,
    out: &Path,
) -> Result<(u64, u64)> {
    let now = timeutil::now_unix();
    let mut encoder = GzEncoder::new(writer, Compression::new(u32::from(level.min(9))));
    let mut added = 0u64;
    let mut bytes_in = 0u64;
    {
        let mut builder = Builder::new(&mut encoder);
        builder.follow_symlinks(false);
        for plan in plans {
            match plan {
                Plan::Dir { name } => {
                    let mut header = Header::new_gnu();
                    header.set_entry_type(EntryType::Directory);
                    header.set_size(0);
                    header.set_mode(0o755);
                    header.set_mtime(now.max(0) as u64);
                    header.set_cksum();
                    builder
                        .append_data(&mut header, format!("{name}/"), io::empty())
                        .map_err(|e| ArchiveError::io(out, e))?;
                    added += 1;
                }
                Plan::File { src, name, .. } => {
                    let metadata = fs::metadata(src).map_err(|e| ArchiveError::io(src, e))?;
                    let mut header = Header::new_gnu();
                    header.set_entry_type(EntryType::Regular);
                    header.set_size(metadata.len());
                    header.set_mode(unix_mode(&metadata));
                    header.set_mtime(
                        metadata
                            .modified()
                            .ok()
                            .and_then(timeutil::system_time_to_unix)
                            .unwrap_or(now)
                            .max(0) as u64,
                    );
                    header.set_cksum();
                    let file = File::open(src).map_err(|e| ArchiveError::io(src, e))?;
                    builder
                        .append_data(&mut header, name.as_str(), file)
                        .map_err(|e| ArchiveError::io(src, e))?;
                    added += 1;
                    bytes_in += metadata.len();
                }
            }
        }
        builder.finish().map_err(|e| ArchiveError::io(out, e))?;
    }
    encoder.finish().map_err(|e| ArchiveError::io(out, e))?;
    Ok((added, bytes_in))
}

/// Unix permission bits of `metadata`, or `0o644` when they are not available.
fn unix_mode(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o7777
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0o644
    }
}

/// Names of `plans` as a set, for "replace these members" decisions.
pub(crate) fn plan_names(plans: &[Plan]) -> HashSet<String> {
    plans.iter().map(|p| p.name().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open;

    struct Tree {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Tree {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            Tree { _dir: dir, root }
        }

        fn file(&self, name: &str, data: &[u8]) -> PathBuf {
            let path = self.root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, data).unwrap();
            path
        }

        fn dir(&self, name: &str) -> PathBuf {
            let path = self.root.join(name);
            fs::create_dir_all(&path).unwrap();
            path
        }
    }

    #[test]
    fn zip_round_trip_keeps_names_sizes_and_content() {
        let tree = Tree::new();
        let a = tree.file("a.txt", b"hello");
        let dir = tree.dir("dir");
        tree.file("dir/b.txt", b"nested bytes");
        let empty = tree.file("empty.txt", b"");
        let unicode = tree.file("žluťoučký.txt", "kůň".as_bytes());

        let out = tree.root.join("out.zip");
        // `b` is inside `dir`, so packing `dir` already stores it.
        let outcome = create(
            &out,
            &[a.clone(), dir, empty, unicode],
            &PackOptions::default(),
        )
        .unwrap();
        assert_eq!(
            outcome.added, 5,
            "four files plus one directory entry, deduplicated"
        );
        assert_eq!(outcome.bytes_in, 5 + 12 + "kůň".len() as u64);
        assert!(outcome.archive_size > 0);
        assert_eq!(outcome.archive_size, fs::metadata(&out).unwrap().len());

        let mut archive = open(&out).unwrap();
        assert_eq!(archive.kind(), ArchiveKind::Zip);
        let rows: Vec<(String, u64, bool)> = archive
            .entries()
            .iter()
            .map(|e| (e.path.clone(), e.size, e.is_dir))
            .collect();
        assert!(rows.contains(&("a.txt".to_string(), 5, false)), "{rows:?}");
        assert!(rows.contains(&("dir".to_string(), 0, true)), "{rows:?}");
        assert!(
            rows.contains(&("dir/b.txt".to_string(), 12, false)),
            "{rows:?}"
        );
        assert!(
            rows.contains(&("empty.txt".to_string(), 0, false)),
            "{rows:?}"
        );
        assert!(
            rows.contains(&("žluťoučký.txt".to_string(), "kůň".len() as u64, false)),
            "{rows:?}"
        );

        assert_eq!(archive.read("a.txt").unwrap(), b"hello");
        assert_eq!(archive.read("dir/b.txt").unwrap(), b"nested bytes");
        assert!(archive.read("empty.txt").unwrap().is_empty());
        assert_eq!(archive.read("žluťoučký.txt").unwrap(), "kůň".as_bytes());
        assert!(archive.is_dir("dir"));
    }

    #[test]
    fn level_zero_stores_and_is_still_readable() {
        let tree = Tree::new();
        let payload = "ab".repeat(2048).into_bytes();
        let a = tree.file("a.bin", &payload);

        let stored = tree.root.join("stored.zip");
        create(
            &stored,
            std::slice::from_ref(&a),
            &PackOptions {
                level: 0,
                ..PackOptions::default()
            },
        )
        .unwrap();
        let deflated = tree.root.join("deflated.zip");
        create(
            &deflated,
            std::slice::from_ref(&a),
            &PackOptions {
                level: 6,
                ..PackOptions::default()
            },
        )
        .unwrap();

        // Storing cannot beat compression on repetitive data.
        assert!(fs::metadata(&stored).unwrap().len() > fs::metadata(&deflated).unwrap().len());

        for path in [&stored, &deflated] {
            let mut archive = open(path).unwrap();
            let bytes = archive.read("a.bin").unwrap();
            assert_eq!(bytes, payload);
            let compressed = archive.entry("a.bin").unwrap().compressed_size.unwrap();
            if path == &stored {
                assert_eq!(
                    compressed,
                    payload.len() as u64,
                    "level 0 stores the member uncompressed"
                );
            } else {
                assert!(compressed < payload.len() as u64, "level 6 deflates");
            }
        }
    }

    #[test]
    fn tar_gz_round_trip() {
        let tree = Tree::new();
        let a = tree.file("a.txt", b"alpha");
        let sub = tree.dir("sub");
        tree.file("sub/b.txt", b"beta");
        let out = tree.root.join("out.tar.gz");
        let outcome = create(&out, &[a, sub], &PackOptions::default()).unwrap();
        assert_eq!(outcome.added, 3, "a.txt, sub and sub/b.txt");
        assert_eq!(outcome.bytes_in, 9);

        let mut archive = open(&out).unwrap();
        assert_eq!(archive.kind(), ArchiveKind::TarGz);
        assert_eq!(archive.read("a.txt").unwrap(), b"alpha");
        assert_eq!(archive.read("sub/b.txt").unwrap(), b"beta");
        assert!(archive.is_dir("sub"));
        assert!(archive.entry("a.txt").unwrap().mtime.is_some());
    }

    #[test]
    fn tgz_alias_is_accepted_and_other_formats_are_not() {
        let tree = Tree::new();
        let a = tree.file("a.txt", b"x");
        let tgz = tree.root.join("out.tgz");
        assert!(create(&tgz, std::slice::from_ref(&a), &PackOptions::default()).is_ok());

        let sevenz = tree.root.join("out.7z");
        let err = create(&sevenz, std::slice::from_ref(&a), &PackOptions::default()).unwrap_err();
        assert!(matches!(err, ArchiveError::Unsupported(_)));
        assert!(err.to_string().contains("only .zip"), "{err}");
        assert!(!sevenz.exists());
    }

    #[test]
    fn relative_base_is_honoured() {
        let tree = Tree::new();
        let proj = tree.dir("proj");
        tree.file("proj/src/main.rs", b"fn main() {}");
        tree.file("proj/README", b"docs");

        let out = tree.root.join("proj.zip");
        create(
            &out,
            std::slice::from_ref(&proj),
            &PackOptions {
                store_paths_relative_to: Some(tree.root.clone()),
                ..PackOptions::default()
            },
        )
        .unwrap();
        let archive = open(&out).unwrap();
        let names: Vec<&str> = archive.entries().iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            names,
            ["proj", "proj/README", "proj/src", "proj/src/main.rs"]
        );

        // Without the base the item keeps its own name.
        let nested = tree.root.join("nested.zip");
        create(
            &nested,
            std::slice::from_ref(&proj),
            &PackOptions::default(),
        )
        .unwrap();
        let archive = open(&nested).unwrap();
        assert!(
            archive
                .entries()
                .iter()
                .any(|e| e.path == "proj/src/main.rs")
        );
    }

    #[test]
    fn empty_items_still_produce_a_valid_archive() {
        let tree = Tree::new();
        let zip = tree.root.join("empty.zip");
        let outcome = create(&zip, &[], &PackOptions::default()).unwrap();
        assert_eq!(outcome.added, 0);
        let archive = open(&zip).unwrap();
        assert!(archive.entries().is_empty());

        let tgz = tree.root.join("empty.tar.gz");
        create(&tgz, &[], &PackOptions::default()).unwrap();
        assert!(open(&tgz).unwrap().entries().is_empty());
    }

    #[test]
    fn replacing_an_existing_archive_is_atomic() {
        let tree = Tree::new();
        let a = tree.file("a.txt", b"first version");
        let out = tree.root.join("keep.zip");
        create(&out, &[a], &PackOptions::default()).unwrap();
        let first = fs::read(&out).unwrap();

        // A failing pack must leave the original untouched.
        let missing = tree.root.join("missing.txt");
        assert!(create(&out, &[missing], &PackOptions::default()).is_err());
        assert_eq!(fs::read(&out).unwrap(), first);
        assert_eq!(open(&out).unwrap().read("a.txt").unwrap(), b"first version");

        // A successful pack replaces it completely.
        let b = tree.file("b.txt", b"second version");
        create(&out, &[b], &PackOptions::default()).unwrap();
        let mut archive = open(&out).unwrap();
        assert!(archive.entry("a.txt").is_none());
        assert_eq!(archive.read("b.txt").unwrap(), b"second version");
    }

    #[test]
    fn passwords_are_refused_explicitly() {
        let tree = Tree::new();
        let a = tree.file("a.txt", b"x");
        let out = tree.root.join("secret.zip");
        let err = create(
            &out,
            &[a],
            &PackOptions {
                password: Some("hunter2".to_string()),
                ..PackOptions::default()
            },
        )
        .unwrap_err();
        assert!(matches!(err, ArchiveError::Unsupported(_)));
        assert!(err.to_string().contains("password"), "{err}");
        assert!(!out.exists());
    }

    #[test]
    fn symlinks_and_missing_items_are_handled() {
        let tree = Tree::new();
        tree.file("real.txt", b"real");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tree.root.join("real.txt"), tree.root.join("link.txt"))
                .unwrap();
            let out = tree.root.join("links.zip");
            let outcome =
                create(&out, &[tree.root.join("link.txt")], &PackOptions::default()).unwrap();
            assert_eq!(outcome.added, 0, "symlinks are skipped");
        }

        let out = tree.root.join("missing.zip");
        let err = create(&out, &[tree.root.join("nope.txt")], &PackOptions::default()).unwrap_err();
        assert!(matches!(err, ArchiveError::Io { .. }), "{err:?}");
        assert!(!out.exists(), "a failed pack leaves no archive behind");
    }

    #[test]
    fn nested_directories_are_walked_recursively() {
        let tree = Tree::new();
        let deep = tree.dir("a/b/c");
        tree.file("a/b/c/deep.txt", b"deep");
        let out = tree.root.join("deep.zip");
        let outcome = create(&out, std::slice::from_ref(&deep), &PackOptions::default()).unwrap();
        assert_eq!(outcome.added, 2, "the directory itself plus its file");

        let archive = open(&out).unwrap();
        let names: Vec<&str> = archive.entries().iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            names,
            ["c", "c/deep.txt"],
            "names are relative to the item's parent by default"
        );

        // Relative to the tree root the member keeps its full path; the missing
        // intermediate directories are synthesized by the listing, not stored.
        let with_base = tree.root.join("deep-base.zip");
        create(
            &with_base,
            &[deep],
            &PackOptions {
                store_paths_relative_to: Some(tree.root.clone()),
                ..PackOptions::default()
            },
        )
        .unwrap();
        let names: Vec<String> = open(&with_base)
            .unwrap()
            .entries()
            .iter()
            .map(|e| e.path.clone())
            .collect();
        assert_eq!(names, ["a/b/c", "a/b/c/deep.txt"]);
    }

    #[test]
    fn plan_helpers_are_consistent() {
        let tree = Tree::new();
        tree.file("one.txt", b"1");
        tree.file("two.txt", b"22");
        let plans = collect_plans(
            &[tree.root.join("one.txt"), tree.root.join("two.txt")],
            Some(&tree.root),
        )
        .unwrap();
        assert_eq!(plans.len(), 2);
        let names = plan_names(&plans);
        assert!(names.contains("one.txt") && names.contains("two.txt"));
        assert_eq!(deflate_level(0), None);
        assert_eq!(deflate_level(6), Some(6));
        assert_eq!(deflate_level(200), Some(9));
    }
}

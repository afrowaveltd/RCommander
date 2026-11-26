//! "Process with agent": a workspace of verified copies (Phase 16.10a).
//!
//! The idea is the panel owner's, and it is the right one: right-click a file, and the agent gets
//! to work on a **copy** in a workspace while the original stays untouched. At the lowest privilege
//! level an agent that can only break a copy is an agent you can afford to let run.
//!
//! What makes this trustworthy is not the copying but the **manifest**: every copy is hashed, the
//! hash of the original is recorded, and applying a change back is a separate, deliberate act that
//! goes through the transaction layer (verified backup, then write, then re-hash) — never a silent
//! overwrite.
//!
//! Three rules:
//!
//! * **The workspace is the whole scope.** Nothing here touches a file outside it; the policy sees
//!   the workspace root as the sandbox, which is what makes "process with agent" a safe default.
//! * **The manifest is written before the agent is invited to look.** If it cannot be written, there
//!   is no workspace, rather than a half-prepared one.
//! * **Discarding is the default end.** Closing the workspace removes it; keeping it is explicit.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::entry::Entry;
use crate::error::{CoreError, CoreResult};

/// One file the agent may work on, with the hashes that prove which version is which.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFile {
    /// The file the user pointed at (absolute).
    pub original: PathBuf,
    /// The copy the agent works on (inside the workspace).
    pub copy: PathBuf,
    /// Name inside the workspace (`report.txt`, or `sub/report.txt` for a folder).
    pub relative: String,
    /// SHA-256 of the original when the workspace was made.
    pub original_hash: String,
    /// Size in bytes, so an empty copy is visible as one.
    pub size: u64,
}

/// A prepared workspace: copies, a manifest and the directory they live in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// What the user asked for, for the title and the transcript.
    pub label: String,
    /// Where the copies live.
    pub directory: PathBuf,
    /// The copies themselves.
    pub files: Vec<WorkspaceFile>,
    /// Unix seconds when it was created.
    pub created: i64,
}

/// What a workspace cannot take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}

impl Workspace {
    /// Prepare a workspace under `directory` from the given entries.
    ///
    /// Folders are copied recursively (the agent needs to see what is in them); a file that cannot
    /// be read is reported in `skipped` rather than failing the whole workspace, because one
    /// unreadable file in a selection should not cost the user the other nine.
    pub fn prepare(
        directory: impl Into<PathBuf>,
        label: impl Into<String>,
        entries: &[Entry],
    ) -> CoreResult<(Workspace, Vec<Skipped>)> {
        let directory = directory.into();
        std::fs::create_dir_all(&directory).map_err(|error| CoreError::io(&directory, error))?;
        let mut workspace = Workspace {
            label: label.into(),
            directory: directory.clone(),
            files: Vec::new(),
            created: chrono::Local::now().timestamp(),
        };
        let mut skipped: Vec<Skipped> = Vec::new();
        for entry in entries {
            if entry.is_dir() {
                copy_directory(
                    &entry.path,
                    &directory,
                    &entry.name,
                    &mut workspace,
                    &mut skipped,
                )?;
            } else {
                match copy_one(&entry.path, &directory, &entry.name) {
                    Ok(file) => workspace.files.push(file),
                    Err(reason) => skipped.push(Skipped {
                        path: entry.path.clone(),
                        reason,
                    }),
                }
            }
        }
        if workspace.files.is_empty() {
            // An empty workspace is not a workspace; say so instead of opening a chat about nothing.
            let _ = std::fs::remove_dir_all(&directory);
            return Err(CoreError::Invalid(
                "none of the selected files could be copied".to_string(),
            ));
        }
        workspace.write_manifest()?;
        Ok((workspace, skipped))
    }

    /// Where the manifest lives.
    pub fn manifest_path(&self) -> PathBuf {
        self.directory.join("workspace.toml")
    }

    /// Write the manifest, so the workspace explains itself after a restart.
    pub fn write_manifest(&self) -> CoreResult<()> {
        let text =
            toml::to_string_pretty(self).map_err(|error| CoreError::Invalid(error.to_string()))?;
        let path = self.manifest_path();
        // A temporary file and a rename: a half-written manifest is worse than none.
        let temporary = path.with_extension("toml.tmp");
        std::fs::write(&temporary, text).map_err(|error| CoreError::io(&temporary, error))?;
        std::fs::rename(&temporary, &path).map_err(|error| CoreError::io(&path, error))?;
        Ok(())
    }

    /// Read a workspace back from its directory.
    pub fn load(directory: &Path) -> CoreResult<Workspace> {
        let path = directory.join("workspace.toml");
        let text = std::fs::read_to_string(&path).map_err(|error| CoreError::io(&path, error))?;
        toml::from_str(&text).map_err(|error| CoreError::Invalid(error.to_string()))
    }

    /// True when the copy of `relative` differs from what was prepared.
    ///
    /// This is how the application knows there is something to send back, without asking the agent.
    pub fn changed(&self, relative: &str) -> Option<bool> {
        let file = self.files.iter().find(|file| file.relative == relative)?;
        let current = crate::transaction::hash_file(&file.copy)?;
        Some(current != file.original_hash)
    }

    /// Every copy that differs from its original.
    pub fn changed_files(&self) -> Vec<&WorkspaceFile> {
        self.files
            .iter()
            .filter(|file| {
                crate::transaction::hash_file(&file.copy)
                    .is_some_and(|hash| hash != file.original_hash)
            })
            .collect()
    }

    /// Remove the workspace and everything in it.
    pub fn discard(&self) -> CoreResult<()> {
        std::fs::remove_dir_all(&self.directory)
            .map_err(|error| CoreError::io(&self.directory, error))
    }

    /// The files, as one line for the transcript: `report.txt, notes/ (3 files)`.
    pub fn summary(&self) -> String {
        let names: Vec<String> = self
            .files
            .iter()
            .take(4)
            .map(|file| file.relative.clone())
            .collect();
        let mut text = names.join(", ");
        if self.files.len() > names.len() {
            text.push_str(&format!(" (+{} more)", self.files.len() - names.len()));
        }
        text
    }
}

/// Copy one file into the workspace and hash it.
fn copy_one(source: &Path, directory: &Path, name: &str) -> Result<WorkspaceFile, String> {
    let copy = directory.join(name);
    std::fs::copy(source, &copy).map_err(|error| error.to_string())?;
    let hash = crate::transaction::hash_file(source)
        .ok_or_else(|| "cannot hash the original".to_string())?;
    let size = std::fs::metadata(&copy)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    Ok(WorkspaceFile {
        original: source.to_path_buf(),
        copy,
        relative: name.to_string(),
        original_hash: hash,
        size,
    })
}

/// Copy a directory tree, keeping the relative paths.
///
/// The tree is copied first and recorded afterwards, because the record is built from the copy:
/// what the manifest describes is what the agent will actually see.
fn copy_directory(
    source: &Path,
    directory: &Path,
    name: &str,
    workspace: &mut Workspace,
    skipped: &mut Vec<Skipped>,
) -> CoreResult<()> {
    let root = directory.join(name);
    std::fs::create_dir_all(&root).map_err(|error| CoreError::io(&root, error))?;
    // A depth limit, because a symlink loop is a real thing in a file manager and this code has no
    // business following one forever.
    const MAX_DEPTH: usize = 24;
    let mut stack: Vec<(PathBuf, usize)> = vec![(source.to_path_buf(), 0)];
    while let Some((current, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            skipped.push(Skipped {
                path: current,
                reason: "the tree is too deep (or a symlink points back at it)".to_string(),
            });
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&current) else {
            skipped.push(Skipped {
                path: current,
                reason: "cannot read this folder".to_string(),
            });
            continue;
        };
        let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        children.sort();
        for child in children {
            let Ok(relative) = child.strip_prefix(source) else {
                continue;
            };
            let target = root.join(relative);
            if child.is_dir() {
                if let Err(error) = std::fs::create_dir_all(&target) {
                    skipped.push(Skipped {
                        path: child.clone(),
                        reason: error.to_string(),
                    });
                    continue;
                }
                stack.push((child, depth + 1));
            } else if let Err(error) = std::fs::copy(&child, &target) {
                skipped.push(Skipped {
                    path: child,
                    reason: error.to_string(),
                });
            }
        }
    }
    for file in collect_files(&root) {
        let Ok(relative_to_root) = file.strip_prefix(&root) else {
            continue;
        };
        let original = source.join(relative_to_root);
        let Some(hash) = crate::transaction::hash_file(&original) else {
            skipped.push(Skipped {
                path: original,
                reason: "cannot hash the original".to_string(),
            });
            continue;
        };
        let size = std::fs::metadata(&file)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        workspace.files.push(WorkspaceFile {
            original,
            copy: file.clone(),
            relative: format!(
                "{name}/{}",
                relative_to_root.to_string_lossy().replace('\\', "/")
            ),
            original_hash: hash,
            size,
        });
    }
    Ok(())
}

/// Every regular file under `root`, in a stable order.
fn collect_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        children.sort();
        for child in children {
            if child.is_dir() {
                stack.push(child);
            } else {
                files.push(child);
            }
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryKind;

    fn entry(path: &Path) -> Entry {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let size = std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        Entry::new(name, path.to_path_buf(), EntryKind::File, size)
    }

    fn dir_entry(path: &Path) -> Entry {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        Entry::new(name, path.to_path_buf(), EntryKind::Dir, 0)
    }

    #[test]
    fn a_workspace_holds_copies_with_the_originals_untouched() {
        let originals = tempfile::tempdir().unwrap();
        let workspace_dir = tempfile::tempdir().unwrap();
        let file = originals.path().join("report.txt");
        std::fs::write(&file, b"the original").unwrap();
        let (workspace, skipped) = Workspace::prepare(
            workspace_dir.path().join("work"),
            "report.txt",
            &[entry(&file)],
        )
        .expect("a workspace is prepared");
        assert!(skipped.is_empty());
        assert_eq!(workspace.files.len(), 1);
        let copy = &workspace.files[0].copy;
        assert!(copy.is_file());
        assert_eq!(std::fs::read_to_string(copy).unwrap(), "the original");
        // The original is untouched and the hashes match.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "the original");
        assert_eq!(
            crate::transaction::hash_file(&file).unwrap(),
            workspace.files[0].original_hash
        );
        // And nothing looks changed yet.
        assert_eq!(workspace.changed("report.txt"), Some(false));
        assert!(workspace.changed_files().is_empty());
    }

    #[test]
    fn editing_the_copy_is_visible_and_the_original_stays_byte_identical() {
        let originals = tempfile::tempdir().unwrap();
        let workspace_dir = tempfile::tempdir().unwrap();
        let file = originals.path().join("settings.toml");
        std::fs::write(&file, b"a = 1\n").unwrap();
        let (workspace, _) = Workspace::prepare(
            workspace_dir.path().join("work"),
            "settings.toml",
            &[entry(&file)],
        )
        .unwrap();
        // This is what the agent does: change the copy.
        std::fs::write(&workspace.files[0].copy, b"a = 2\n").unwrap();
        assert_eq!(workspace.changed("settings.toml"), Some(true));
        assert_eq!(workspace.changed_files().len(), 1);
        // The original still says what it said.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "a = 1\n");
    }

    #[test]
    fn the_manifest_is_written_and_reads_back() {
        let originals = tempfile::tempdir().unwrap();
        let workspace_dir = tempfile::tempdir().unwrap();
        let file = originals.path().join("a.txt");
        std::fs::write(&file, b"one").unwrap();
        let (workspace, _) =
            Workspace::prepare(workspace_dir.path().join("work"), "a.txt", &[entry(&file)])
                .unwrap();
        let manifest = workspace.manifest_path();
        assert!(manifest.is_file(), "the manifest explains the workspace");
        let loaded = Workspace::load(&workspace.directory).expect("it reads back");
        assert_eq!(loaded.label, "a.txt");
        assert_eq!(loaded.files, workspace.files);
    }

    #[test]
    fn a_folder_is_copied_with_its_tree() {
        let originals = tempfile::tempdir().unwrap();
        let workspace_dir = tempfile::tempdir().unwrap();
        let folder = originals.path().join("site");
        std::fs::create_dir_all(folder.join("css")).unwrap();
        std::fs::write(folder.join("index.html"), b"<html></html>").unwrap();
        std::fs::write(folder.join("css/app.css"), b"body{}").unwrap();
        let (workspace, skipped) = Workspace::prepare(
            workspace_dir.path().join("work"),
            "site",
            &[dir_entry(&folder)],
        )
        .unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        let names: Vec<&str> = workspace
            .files
            .iter()
            .map(|file| file.relative.as_str())
            .collect();
        assert!(names.contains(&"site/index.html"), "{names:?}");
        assert!(names.contains(&"site/css/app.css"), "{names:?}");
        // The copies exist where the manifest says.
        for file in &workspace.files {
            assert!(file.copy.is_file(), "{:?}", file.copy);
        }
    }

    #[test]
    fn an_empty_selection_is_refused_rather_than_opening_a_chat_about_nothing() {
        let workspace_dir = tempfile::tempdir().unwrap();
        let empty = workspace_dir.path().join("work");
        let error = Workspace::prepare(&empty, "nothing", &[]).unwrap_err();
        assert!(
            error.to_string().contains("could be copied"),
            "the refusal explains itself: {error}"
        );
        assert!(
            !empty.exists(),
            "a workspace that failed is not left behind as an empty folder"
        );
    }

    #[test]
    fn discarding_removes_the_copies_and_leaves_the_originals() {
        let originals = tempfile::tempdir().unwrap();
        let workspace_dir = tempfile::tempdir().unwrap();
        let file = originals.path().join("a.txt");
        std::fs::write(&file, b"keep me").unwrap();
        let (workspace, _) =
            Workspace::prepare(workspace_dir.path().join("work"), "a.txt", &[entry(&file)])
                .unwrap();
        assert!(workspace.directory.exists());
        workspace.discard().unwrap();
        assert!(!workspace.directory.exists());
        assert!(file.is_file(), "the original was never in the workspace");
    }

    #[test]
    fn the_summary_names_the_copies_and_counts_the_rest() {
        let originals = tempfile::tempdir().unwrap();
        let workspace_dir = tempfile::tempdir().unwrap();
        let mut entries = Vec::new();
        for index in 0..6 {
            let file = originals.path().join(format!("f{index}.txt"));
            std::fs::write(&file, b"x").unwrap();
            entries.push(entry(&file));
        }
        let (workspace, _) =
            Workspace::prepare(workspace_dir.path().join("work"), "six", &entries).unwrap();
        let summary = workspace.summary();
        assert!(summary.contains("+2 more"), "{summary}");
    }
}

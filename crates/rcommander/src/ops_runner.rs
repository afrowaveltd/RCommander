//! Background execution of long running file operations.
//!
//! The UI must stay responsive while gigabytes are copied, so every transfer
//! runs in its own thread and reports progress through a channel. The same
//! helper is used for the desktop UI and for MCP tool calls.

use rc_core::ops::{self, CancelToken, OverwriteMode, Progress, TransferOptions};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::JoinHandle;

/// What an operation does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Copy,
    Move,
    Delete,
}

impl OpKind {
    pub fn title_key(self) -> &'static str {
        match self {
            OpKind::Copy => "status.copying",
            OpKind::Move => "cmd.move",
            OpKind::Delete => "cmd.delete",
        }
    }
}

/// A snapshot of the operation's progress.
#[derive(Clone, Debug, Default)]
pub struct OpProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub current_file: String,
}

impl OpProgress {
    pub fn fraction(&self) -> f32 {
        if self.bytes_total > 0 {
            (self.bytes_done as f32 / self.bytes_total as f32).clamp(0.0, 1.0)
        } else if self.files_total > 0 {
            (self.files_done as f32 / self.files_total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn update(&mut self, progress: &Progress) {
        self.bytes_done = progress.bytes_done;
        self.files_done = progress.files_done;
        if !progress.current.as_os_str().is_empty() {
            self.current_file = progress.current.to_string_lossy().to_string();
        }
    }
}

/// Final outcome of an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpResult {
    Finished { files: u64, bytes: u64 },
    Cancelled,
    Failed(String),
}

/// Handle to a running operation.
pub struct OpHandle {
    pub id: u64,
    pub kind: OpKind,
    pub progress: OpProgress,
    pub result: Option<OpResult>,
    cancel: CancelToken,
    receiver: Receiver<OpEvent>,
    join: Option<JoinHandle<()>>,
}

enum OpEvent {
    Progress(Progress),
    Done(OpResult),
}

impl OpHandle {
    /// Poll the channel; returns true while the operation is still running.
    pub fn poll(&mut self) -> bool {
        if self.result.is_some() {
            return false;
        }
        loop {
            match self.receiver.try_recv() {
                Ok(OpEvent::Progress(p)) => self.progress.update(&p),
                Ok(OpEvent::Done(result)) => {
                    self.result = Some(result);
                    return false;
                }
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => {
                    if self.result.is_none() {
                        self.result = Some(OpResult::Failed(
                            "operation thread ended unexpectedly".to_string(),
                        ));
                    }
                    return false;
                }
            }
        }
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn is_finished(&self) -> bool {
        self.result.is_some()
    }

    /// Block until the operation finishes (used by tests and the CLI).
    pub fn wait(mut self) -> OpResult {
        while self.poll() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        self.result.unwrap_or(OpResult::Cancelled)
    }
}

/// Plan a transfer: pair each source with its destination path.
pub fn plan_transfer(
    sources: &[PathBuf],
    destination: &std::path::Path,
) -> Vec<(PathBuf, PathBuf)> {
    sources
        .iter()
        .map(|s| {
            let name = s.file_name().map(|n| n.to_owned()).unwrap_or_default();
            (s.clone(), destination.join(name))
        })
        .collect()
}

/// Spawn a copy/move operation.
pub fn spawn_transfer(
    id: u64,
    kind: OpKind,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    overwrite: OverwriteMode,
) -> OpHandle {
    let cancel = CancelToken::new();
    let thread_cancel = cancel.clone();
    let (tx, receiver) = channel();
    let join = std::thread::spawn(move || {
        let options = TransferOptions {
            overwrite,
            ..Default::default()
        };
        let mut files = 0u64;
        let mut bytes = 0u64;
        for source in sources {
            if thread_cancel.is_cancelled() {
                let _ = tx.send(OpEvent::Done(OpResult::Cancelled));
                return;
            }
            let name = source.file_name().map(|n| n.to_owned()).unwrap_or_default();
            let target = destination.join(name);
            let tx_progress = tx.clone();
            let mut progress = |p: &Progress| {
                let _ = tx_progress.send(OpEvent::Progress(p.clone()));
            };
            let outcome = match kind {
                OpKind::Move => ops::move_path(&source, &target, &options, &thread_cancel),
                _ => ops::copy(
                    &source,
                    &target,
                    &options,
                    &thread_cancel,
                    Some(&mut progress),
                ),
            };
            match outcome {
                Ok(out) => {
                    files += out.files.max(1);
                    bytes += out.bytes;
                }
                Err(e) if e.is_cancelled() => {
                    let _ = tx.send(OpEvent::Done(OpResult::Cancelled));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(OpEvent::Done(OpResult::Failed(e.to_string())));
                    return;
                }
            }
        }
        let _ = tx.send(OpEvent::Done(OpResult::Finished { files, bytes }));
    });
    OpHandle {
        id,
        kind,
        progress: OpProgress::default(),
        result: None,
        cancel,
        receiver,
        join: Some(join),
    }
}

/// Spawn a recursive delete.
pub fn spawn_delete(id: u64, targets: Vec<PathBuf>) -> OpHandle {
    let cancel = CancelToken::new();
    let thread_cancel = cancel.clone();
    let (tx, receiver) = channel();
    let join = std::thread::spawn(move || {
        let mut removed = 0u64;
        for target in targets {
            if thread_cancel.is_cancelled() {
                let _ = tx.send(OpEvent::Done(OpResult::Cancelled));
                return;
            }
            match ops::remove(&target, &thread_cancel) {
                Ok(n) => removed += n,
                Err(e) if e.is_cancelled() => {
                    let _ = tx.send(OpEvent::Done(OpResult::Cancelled));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(OpEvent::Done(OpResult::Failed(e.to_string())));
                    return;
                }
            }
        }
        let _ = tx.send(OpEvent::Done(OpResult::Finished {
            files: removed,
            bytes: 0,
        }));
    });
    OpHandle {
        id,
        kind: OpKind::Delete,
        progress: OpProgress::default(),
        result: None,
        cancel,
        receiver,
        join: Some(join),
    }
}

/// Total bytes of a set of paths, used to show a meaningful progress bar.
pub fn total_bytes(paths: &[PathBuf]) -> u64 {
    let cancel = CancelToken::new();
    paths
        .iter()
        .map(|p| ops::measure(p, &cancel).map(|(b, _)| b).unwrap_or(0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn plan_transfer_pairs_names_with_the_destination() {
        let sources = vec![PathBuf::from("/a/one.txt"), PathBuf::from("/a/dir")];
        let plan = plan_transfer(&sources, &PathBuf::from("/dest"));
        assert_eq!(plan[0].1, PathBuf::from("/dest/one.txt"));
        assert_eq!(plan[1].1, PathBuf::from("/dest/dir"));
    }

    #[test]
    fn copy_operation_reports_progress_and_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src/a.bin");
        write(&src, &vec![1u8; 300_000]);
        let dest = dir.path().join("out");
        fs::create_dir_all(&dest).unwrap();
        let handle = spawn_transfer(
            1,
            OpKind::Copy,
            vec![src],
            dest.clone(),
            OverwriteMode::Overwrite,
        );
        let result = handle.wait();
        assert!(
            matches!(result, OpResult::Finished { files: 1, .. }),
            "result: {result:?}"
        );
        assert!(dest.join("a.bin").exists());
    }

    #[test]
    fn move_operation_uses_the_move_kind() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src/a.txt");
        write(&src, b"x");
        let dest = dir.path().join("out");
        fs::create_dir_all(&dest).unwrap();
        let result = spawn_transfer(
            2,
            OpKind::Move,
            vec![src.clone()],
            dest.clone(),
            OverwriteMode::Overwrite,
        )
        .wait();
        assert!(matches!(result, OpResult::Finished { .. }));
        assert!(!src.exists());
        assert!(dest.join("a.txt").exists());
        assert_eq!(OpKind::Move.title_key(), "cmd.move");
    }

    #[test]
    fn delete_operation_removes_the_tree() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        write(&tree.join("a.txt"), b"1");
        write(&tree.join("sub/b.txt"), b"2");
        let result = spawn_delete(3, vec![tree.clone()]).wait();
        assert!(
            matches!(result, OpResult::Finished { files, .. } if files >= 3),
            "result: {result:?}"
        );
        assert!(!tree.exists());
    }

    #[test]
    fn failing_source_reports_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = spawn_transfer(
            4,
            OpKind::Copy,
            vec![dir.path().join("missing.txt")],
            dir.path().to_path_buf(),
            OverwriteMode::Overwrite,
        )
        .wait();
        assert!(matches!(result, OpResult::Failed(_)), "result: {result:?}");
    }

    #[test]
    fn cancel_marks_the_result_as_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("big.bin");
        write(&src, &vec![7u8; 2_000_000]);
        let handle = spawn_transfer(
            5,
            OpKind::Copy,
            vec![src],
            dir.path().to_path_buf(),
            OverwriteMode::Overwrite,
        );
        handle.cancel();
        let result = handle.wait();
        assert!(
            matches!(result, OpResult::Cancelled | OpResult::Finished { .. }),
            "result: {result:?}"
        );
    }

    #[test]
    fn total_bytes_sums_the_tree() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a"), b"12345");
        write(&dir.path().join("s/b"), b"123");
        assert_eq!(total_bytes(&[dir.path().to_path_buf()]), 8);
        assert_eq!(total_bytes(&[dir.path().join("nope")]), 0);
    }

    #[test]
    fn progress_fraction_falls_back_to_files() {
        let p = OpProgress {
            files_done: 1,
            files_total: 4,
            ..Default::default()
        };
        assert!((p.fraction() - 0.25).abs() < 1e-6);
        assert_eq!(OpProgress::default().fraction(), 0.0);
        let q = OpProgress {
            bytes_done: 1,
            bytes_total: 4,
            ..Default::default()
        };
        assert!((q.fraction() - 0.25).abs() < 1e-6);
    }
}

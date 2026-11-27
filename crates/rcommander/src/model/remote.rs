//! Remote panels (FTP/SFTP) on top of the `rc-remote` engine.
//!
//! The model talks to remote file systems through [`RemoteBackend`] so that the
//! navigation, copy and delete logic can be exercised in tests with an
//! in-memory backend, while the application uses the real client through
//! [`RcRemoteBackend`].

use crate::model::{Model, Side};
use rc_core::entry::{Attrs, Entry, EntryKind};
use rc_core::error::{CoreError, CoreResult};
use rc_core::location::Location;
use std::path::{Path, PathBuf};

/// A connected remote file system, as far as the model needs it.
pub trait RemoteBackend: Send {
    /// Current directory.
    fn pwd(&mut self) -> Result<String, String>;
    /// Change directory and return the new one.
    fn cwd(&mut self, path: &str) -> Result<String, String>;
    /// List `path` (None = current directory) as panel entries.
    fn list(&mut self, path: Option<&str>) -> Result<Vec<Entry>, String>;
    /// Download `remote` into `local`.
    fn download(
        &mut self,
        remote: &str,
        local: &Path,
        cancel: &rc_core::ops::CancelToken,
    ) -> Result<u64, String>;
    /// Upload `local` to `remote`.
    fn upload(
        &mut self,
        local: &Path,
        remote: &str,
        cancel: &rc_core::ops::CancelToken,
    ) -> Result<u64, String>;
    fn mkdir(&mut self, path: &str) -> Result<(), String>;
    fn remove_file(&mut self, path: &str) -> Result<(), String>;
    fn remove_dir(&mut self, path: &str) -> Result<(), String>;
    fn rename(&mut self, from: &str, to: &str) -> Result<(), String>;
    /// Human readable description of the session (`ftp://user@host:21`).
    fn describe(&self) -> String;
}

/// Production backend: the tested `rc-remote` client.
pub struct RcRemoteBackend {
    inner: Box<dyn rc_remote::RemoteFs + Send>,
    description: String,
}

impl RcRemoteBackend {
    /// Connect with `spec` and return a backend for the panel.
    pub fn connect(spec: &rc_remote::ConnectionSpec) -> Result<Self, String> {
        Self::connect_reporting(spec, &mut |_stage| {})
    }

    /// Connect, reporting each step of the attempt.
    pub fn connect_reporting(
        spec: &rc_remote::ConnectionSpec,
        on_stage: rc_remote::StageReporter<'_>,
    ) -> Result<Self, String> {
        let description = rc_remote::describe(spec);
        let inner = rc_remote::connect_send_reporting(spec, on_stage).map_err(|e| e.to_string())?;
        Ok(RcRemoteBackend { inner, description })
    }
}

fn to_entry(item: &rc_remote::RemoteEntry, parent: &str) -> Entry {
    let path = if parent.ends_with('/') {
        format!("{parent}{}", item.name)
    } else {
        format!("{parent}/{}", item.name)
    };
    let kind = if item.is_symlink {
        if item.is_dir {
            EntryKind::SymlinkToDir
        } else {
            EntryKind::SymlinkToFile
        }
    } else if item.is_dir {
        EntryKind::Dir
    } else {
        EntryKind::File
    };
    let attrs = Attrs {
        unix_mode: item.permissions,
        ..Attrs::default()
    };
    Entry {
        name: item.name.clone(),
        path: PathBuf::from(path),
        kind,
        size: item.size,
        mtime: item.mtime,
        ctime: None,
        attrs,
        link_target: item.target.as_ref().map(PathBuf::from),
    }
}

impl RemoteBackend for RcRemoteBackend {
    fn pwd(&mut self) -> Result<String, String> {
        self.inner.pwd().map_err(|e| e.to_string())
    }

    fn cwd(&mut self, path: &str) -> Result<String, String> {
        self.inner.cwd(path).map_err(|e| e.to_string())
    }

    fn list(&mut self, path: Option<&str>) -> Result<Vec<Entry>, String> {
        let parent = match path {
            Some(p) => p.to_string(),
            None => self.inner.pwd().map_err(|e| e.to_string())?,
        };
        let items = self.inner.list(path).map_err(|e| e.to_string())?;
        Ok(items.iter().map(|item| to_entry(item, &parent)).collect())
    }

    fn download(
        &mut self,
        remote: &str,
        local: &Path,
        cancel: &rc_core::ops::CancelToken,
    ) -> Result<u64, String> {
        self.inner
            .get(remote, local, cancel)
            .map_err(|e| e.to_string())
    }

    fn upload(
        &mut self,
        local: &Path,
        remote: &str,
        cancel: &rc_core::ops::CancelToken,
    ) -> Result<u64, String> {
        self.inner
            .put(local, remote, cancel)
            .map_err(|e| e.to_string())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        self.inner.mkdir(path).map_err(|e| e.to_string())
    }

    fn remove_file(&mut self, path: &str) -> Result<(), String> {
        self.inner.remove_file(path).map_err(|e| e.to_string())
    }

    fn remove_dir(&mut self, path: &str) -> Result<(), String> {
        self.inner.rmdir(path).map_err(|e| e.to_string())
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.inner.rename(from, to).map_err(|e| e.to_string())
    }

    fn describe(&self) -> String {
        self.description.clone()
    }
}

/// One connected remote panel.
pub struct RemoteSession {
    pub backend: Box<dyn RemoteBackend>,
    pub location: Location,
}

/// Build a connection spec from a saved connection, filling in the password from
/// the credential store and mapping the saved authentication and host-key policy.
pub fn connection_spec(
    connection: &rc_core::config::SavedConnection,
    password: Option<String>,
) -> rc_remote::ConnectionSpec {
    let mut saved = connection.clone();
    if let Some(password) = password {
        saved.password = password;
    }
    let mut spec = rc_remote::ConnectionSpec::from_saved(&saved);
    // `from_saved` maps the key path; the passphrase travels in the spec only for
    // this connection attempt and is never stored on disk.
    if let rc_remote::AuthMethod::KeyFile { path, .. } = &mut spec.auth
        && path.as_os_str().is_empty()
    {
        *path = std::path::PathBuf::from(connection.key_path.trim());
    }
    spec
}

impl RemoteSession {
    pub fn new(backend: Box<dyn RemoteBackend>, location: Location) -> Self {
        RemoteSession { backend, location }
    }
}

impl Model {
    /// Connect a panel to a remote server; the panel then lists the server's
    /// initial directory.
    pub fn connect_remote(
        &mut self,
        side: Side,
        backend: Box<dyn RemoteBackend>,
        location: Location,
    ) -> CoreResult<()> {
        let mut backend = backend;
        let target = match &location {
            Location::Remote { path, .. } => path.clone(),
            other => {
                return Err(CoreError::Invalid(format!(
                    "not a remote location: {}",
                    other.display()
                )));
            }
        };
        // Do not replace an existing session until the new directory can be
        // listed. A failed login/listing must not redirect the old panel.
        backend.cwd(&target).map_err(CoreError::Invalid)?;
        let entries = backend.list(None).map_err(CoreError::Invalid)?;
        let keep = self.panel(side).cursor_entry().map(|e| e.name.clone());
        self.remote_sessions[side as usize] = Some(RemoteSession::new(backend, location.clone()));
        self.set_remote_listing(side, location, entries);
        if let Some(name) = keep
            && let Some(index) = self.panel(side).listing.find(&name)
        {
            self.panel_mut(side).set_cursor(index);
        }
        Ok(())
    }

    /// Disconnect a panel from a remote server and fall back to the home dir.
    pub fn disconnect_remote(&mut self, side: Side) -> CoreResult<()> {
        self.remote_sessions[side as usize] = None;
        let home = rc_core::drive::home_dir()
            .map(Location::local)
            .unwrap_or_else(|| Location::local("/"));
        self.open_in(side, home)
    }

    pub fn remote_session(&self, side: Side) -> Option<&RemoteSession> {
        self.remote_sessions[side as usize].as_ref()
    }

    /// True when the panel currently shows a remote directory.
    pub fn is_remote(&self, side: Side) -> bool {
        self.panel(side).location().is_remote()
    }

    /// List a remote directory into a panel. Called by `open_in` through
    /// [`Model::open_remote`].
    pub fn open_remote(&mut self, side: Side, location: Location) -> CoreResult<()> {
        let path = match &location {
            Location::Remote { path, .. } => path.clone(),
            other => {
                return Err(CoreError::Invalid(format!(
                    "not remote: {}",
                    other.display()
                )));
            }
        };
        let session = self.remote_sessions[side as usize]
            .as_mut()
            .ok_or_else(|| {
                CoreError::Unsupported("no remote session for this panel".to_string())
            })?;
        session.backend.cwd(&path).map_err(CoreError::Invalid)?;
        session.location = location.clone();
        let entries = session.backend.list(None).map_err(CoreError::Invalid)?;
        self.set_remote_listing(side, location, entries);
        Ok(())
    }

    fn set_remote_listing(&mut self, side: Side, location: Location, entries: Vec<Entry>) {
        let settings = self.settings.clone();
        let panel = self.panel_mut(side);
        let read_options = panel.read_options(&settings);
        let sort = panel.sort();
        // Remote history needs session-bound identities; until that is wired,
        // do not leave stale local or remote targets in the navigation buttons.
        panel.clear_navigation_history();
        panel.listing =
            rc_core::listing::Listing::from_entries(location.clone(), entries, &read_options, sort);
        panel.selection.clear();
        panel.tabs.set_active_location(location);
    }

    /// Download the marked (or cursor) remote entries into `local_dir`.
    pub fn download_selected(&mut self, local_dir: &Path) -> CoreResult<u64> {
        let side = self.focus;
        let base = match self.panel(side).location() {
            Location::Remote { path, .. } => path.clone(),
            other => {
                return Err(CoreError::Invalid(format!(
                    "not remote: {}",
                    other.display()
                )));
            }
        };
        let targets: Vec<(String, PathBuf)> = self
            .panel(side)
            .operation_targets()
            .iter()
            .map(|entry| {
                let remote = if base.ends_with('/') {
                    format!("{base}{}", entry.name)
                } else {
                    format!("{base}/{}", entry.name)
                };
                (remote, local_dir.join(&entry.name))
            })
            .collect();
        let cancel = rc_core::ops::CancelToken::new();
        let Some(session) = self.remote_sessions[side as usize].as_mut() else {
            return Err(CoreError::Unsupported("no remote session".to_string()));
        };
        let mut bytes = 0u64;
        for (remote, local) in targets {
            bytes += session
                .backend
                .download(&remote, &local, &cancel)
                .map_err(CoreError::Invalid)?;
        }
        Ok(bytes)
    }

    /// Upload local paths into the current remote directory.
    pub fn upload_paths(&mut self, local_paths: &[PathBuf]) -> CoreResult<u64> {
        let side = self.focus;
        let base = match self.panel(side).location() {
            Location::Remote { path, .. } => path.clone(),
            other => {
                return Err(CoreError::Invalid(format!(
                    "not remote: {}",
                    other.display()
                )));
            }
        };
        let cancel = rc_core::ops::CancelToken::new();
        let Some(session) = self.remote_sessions[side as usize].as_mut() else {
            return Err(CoreError::Unsupported("no remote session".to_string()));
        };
        let mut bytes = 0u64;
        for local in local_paths {
            let name = local
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let remote = if base.ends_with('/') {
                format!("{base}{name}")
            } else {
                format!("{base}/{name}")
            };
            bytes += session
                .backend
                .upload(local, &remote, &cancel)
                .map_err(CoreError::Invalid)?;
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    /// In-memory backend standing in for a server.
    struct FakeBackend {
        cwd: String,
        files: Vec<(String, u64)>,
        dirs: Vec<String>,
        downloaded: Vec<(String, PathBuf)>,
        uploaded: Vec<(PathBuf, String)>,
    }

    impl FakeBackend {
        fn new(root: &str) -> Self {
            FakeBackend {
                cwd: root.to_string(),
                files: vec![
                    ("remote.txt".to_string(), 42),
                    ("logo.png".to_string(), 1024),
                ],
                dirs: vec!["sub".to_string()],
                downloaded: Vec::new(),
                uploaded: Vec::new(),
            }
        }
    }

    impl RemoteBackend for FakeBackend {
        fn pwd(&mut self) -> Result<String, String> {
            Ok(self.cwd.clone())
        }

        fn cwd(&mut self, path: &str) -> Result<String, String> {
            if path.ends_with("missing") {
                return Err("no such directory".to_string());
            }
            self.cwd = path.to_string();
            Ok(self.cwd.clone())
        }

        fn list(&mut self, _path: Option<&str>) -> Result<Vec<Entry>, String> {
            if self.cwd == "/unlistable" {
                return Err("listing refused".to_string());
            }
            let mut out = Vec::new();
            for dir in &self.dirs {
                out.push(Entry::new(
                    dir.clone(),
                    PathBuf::from(format!("{}/{}", self.cwd, dir)),
                    EntryKind::Dir,
                    0,
                ));
            }
            for (name, size) in &self.files {
                out.push(Entry::new(
                    name.clone(),
                    PathBuf::from(format!("{}/{}", self.cwd, name)),
                    EntryKind::File,
                    *size,
                ));
            }
            Ok(out)
        }

        fn download(
            &mut self,
            remote: &str,
            local: &Path,
            _cancel: &rc_core::ops::CancelToken,
        ) -> Result<u64, String> {
            self.downloaded
                .push((remote.to_string(), local.to_path_buf()));
            Ok(42)
        }

        fn upload(
            &mut self,
            local: &Path,
            remote: &str,
            _cancel: &rc_core::ops::CancelToken,
        ) -> Result<u64, String> {
            self.uploaded
                .push((local.to_path_buf(), remote.to_string()));
            Ok(7)
        }

        fn mkdir(&mut self, _path: &str) -> Result<(), String> {
            Ok(())
        }

        fn remove_file(&mut self, _path: &str) -> Result<(), String> {
            Ok(())
        }

        fn remove_dir(&mut self, _path: &str) -> Result<(), String> {
            Ok(())
        }

        fn rename(&mut self, _from: &str, _to: &str) -> Result<(), String> {
            Ok(())
        }

        fn describe(&self) -> String {
            format!("fake://{}", self.cwd)
        }
    }

    fn model() -> Model {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().to_path_buf();
        // Leak the temp dir: the model keeps the path and the test ends quickly.
        std::mem::forget(dir);
        Model::for_test(
            Settings::default(),
            Location::local(left.clone()),
            Location::local("/tmp"),
        )
    }

    #[test]
    fn connecting_lists_the_remote_directory() {
        let mut model = model();
        let location = Location::remote("ftp", "example.com", Some("bob".into()), "/pub");
        model
            .connect_remote(
                Side::Left,
                Box::new(FakeBackend::new("/pub")),
                location.clone(),
            )
            .expect("connect");
        assert!(model.is_remote(Side::Left));
        assert_eq!(model.left.location().display(), "ftp://bob@example.com/pub");
        let names: Vec<String> = model
            .left
            .entries()
            .iter()
            .map(|e| e.name.clone())
            .collect();
        assert!(names.contains(&"remote.txt".to_string()));
        assert!(names.contains(&"sub".to_string()));
        assert_eq!(
            model.remote_session(Side::Left).unwrap().backend.describe(),
            "fake:///pub"
        );
    }

    #[test]
    fn failed_replacement_preserves_the_existing_remote_panel() {
        let mut model = model();
        let original = Location::remote("ftp", "old.example.com", None, "/pub");
        model
            .connect_remote(
                Side::Left,
                Box::new(FakeBackend::new("/pub")),
                original.clone(),
            )
            .unwrap();
        let replacement = Location::remote("ftp", "new.example.com", None, "/unlistable");
        assert!(
            model
                .connect_remote(Side::Left, Box::new(FakeBackend::new("/")), replacement)
                .is_err()
        );
        assert_eq!(model.left.location(), &original);
        let session = model.remote_session(Side::Left).unwrap();
        assert_eq!(session.location, original);
        assert_eq!(session.backend.describe(), "fake:///pub");
        assert!(
            model
                .left
                .entries()
                .iter()
                .any(|entry| entry.name == "remote.txt")
        );
    }

    #[test]
    fn failed_initial_listing_leaves_local_panel_unchanged() {
        let mut model = model();
        let original = model.left.location().clone();
        let replacement = Location::remote("ftp", "new.example.com", None, "/unlistable");
        assert!(
            model
                .connect_remote(Side::Left, Box::new(FakeBackend::new("/")), replacement)
                .is_err()
        );
        assert_eq!(model.left.location(), &original);
        assert!(model.remote_session(Side::Left).is_none());
    }

    #[test]
    fn navigating_a_remote_panel_updates_the_listing() {
        let mut model = model();
        let location = Location::remote("ftp", "example.com", None, "/pub");
        model
            .connect_remote(Side::Left, Box::new(FakeBackend::new("/pub")), location)
            .unwrap();
        let child = model.left.location().child("sub").unwrap();
        model.open_in(Side::Left, child).unwrap();
        assert_eq!(model.left.location().display(), "ftp://example.com/pub/sub");
    }

    #[test]
    fn a_missing_remote_directory_reports_an_error() {
        let mut model = model();
        let location = Location::remote("ftp", "example.com", None, "/pub");
        model
            .connect_remote(Side::Left, Box::new(FakeBackend::new("/pub")), location)
            .unwrap();
        let missing = Location::remote("ftp", "example.com", None, "/missing");
        let error = model.open_in(Side::Left, missing).unwrap_err();
        assert!(
            error.to_string().contains("no such directory"),
            "error: {error}"
        );
    }

    #[test]
    fn a_panel_without_a_session_cannot_open_a_remote_directory() {
        let mut model = model();
        let location = Location::remote("ftp", "example.com", None, "/pub");
        let error = model.open_in(Side::Left, location).unwrap_err();
        assert!(matches!(error, CoreError::Unsupported(_)));
    }

    #[test]
    fn downloading_marked_entries_uses_the_backend() {
        let mut model = model();
        let location = Location::remote("ftp", "example.com", None, "/pub");
        model
            .connect_remote(Side::Left, Box::new(FakeBackend::new("/pub")), location)
            .unwrap();
        model.left.selection.set(["remote.txt"]);
        let local = tempfile::tempdir().unwrap();
        let bytes = model.download_selected(local.path()).unwrap();
        assert_eq!(bytes, 42);
        let session = model.remote_session(Side::Left).unwrap();
        let backend = session.backend.describe();
        assert_eq!(backend, "fake:///pub");
    }

    #[test]
    fn uploading_sends_local_files_to_the_current_directory() {
        let mut model = model();
        let location = Location::remote("ftp", "example.com", None, "/pub");
        model
            .connect_remote(Side::Left, Box::new(FakeBackend::new("/pub")), location)
            .unwrap();
        let bytes = model
            .upload_paths(&[PathBuf::from("/tmp/local.txt")])
            .unwrap();
        assert_eq!(bytes, 7);
    }

    #[test]
    fn disconnecting_returns_to_a_local_directory() {
        let mut model = model();
        let location = Location::remote("ftp", "example.com", None, "/pub");
        model
            .connect_remote(Side::Left, Box::new(FakeBackend::new("/pub")), location)
            .unwrap();
        model.disconnect_remote(Side::Left).unwrap();
        assert!(!model.is_remote(Side::Left));
        assert!(model.remote_session(Side::Left).is_none());
    }

    #[test]
    fn remote_entries_map_to_panel_entry_kinds() {
        let item = rc_remote::RemoteEntry {
            name: "link".into(),
            is_dir: true,
            is_symlink: true,
            size: 0,
            mtime: None,
            permissions: Some(0o755),
            target: Some("../elsewhere".into()),
        };
        let entry = to_entry(&item, "/pub");
        assert_eq!(entry.name, "link");
        assert_eq!(entry.kind, EntryKind::SymlinkToDir);
        assert_eq!(entry.path, PathBuf::from("/pub/link"));
        assert_eq!(entry.link_target, Some(PathBuf::from("../elsewhere")));
        let plain = rc_remote::RemoteEntry {
            name: "f".into(),
            is_dir: false,
            is_symlink: false,
            size: 10,
            mtime: Some(5),
            permissions: None,
            target: None,
        };
        let entry = to_entry(&plain, "/pub/");
        assert_eq!(entry.kind, EntryKind::File);
        assert_eq!(entry.path, PathBuf::from("/pub/f"));
        assert_eq!(entry.mtime, Some(5));
    }
}

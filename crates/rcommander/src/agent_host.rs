//! MCP host adapter: exposes the file manager through `rc-agent`.

use crate::model::Model;
use std::sync::{Arc, Mutex};

/// Shared model handed to the MCP server.
pub type SharedModel = Arc<Mutex<Model>>;

/// Serve the Model Context Protocol on stdin/stdout.
pub fn serve_stdio(model: Model) -> Result<(), String> {
    let shared: SharedModel = Arc::new(Mutex::new(model));
    let host = Arc::new(HostBridge {
        model: shared.clone(),
    });
    let config = rc_agent::ServerConfig {
        name: "rcommander".to_string(),
        version: crate::VERSION.to_string(),
        ..Default::default()
    };
    // The bridge also switches the UI language, which publishes `set_language`.
    let language_host: Arc<dyn rc_agent::LanguageHost> = host.clone();
    let git_host: Arc<dyn rc_agent::GitHost> = host.clone();
    let server = rc_agent::McpServer::new(host, config)
        .with_language_host(language_host)
        .with_git_host(git_host);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    server
        .serve_stdio(stdin.lock(), stdout.lock())
        .map_err(|e| e.to_string())
}

/// Bridge between the MCP tool calls and the application model.
pub struct HostBridge {
    pub model: SharedModel,
}

fn lock(model: &SharedModel) -> std::sync::MutexGuard<'_, Model> {
    model
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl HostBridge {
    fn side(side: rc_agent::PanelSide) -> crate::model::Side {
        match side {
            rc_agent::PanelSide::Left => crate::model::Side::Left,
            rc_agent::PanelSide::Right => crate::model::Side::Right,
        }
    }

    fn panel(model: &Model, side: Option<rc_agent::PanelSide>) -> crate::model::Side {
        match side {
            Some(side) => Self::side(side),
            None => model.focus,
        }
    }
}

impl rc_agent::FileManagerHost for HostBridge {
    fn panels(&self) -> Vec<rc_agent::HostPanel> {
        let model = lock(&self.model);
        [crate::model::Side::Left, crate::model::Side::Right]
            .into_iter()
            .map(|side| {
                let panel = model.panel(side);
                rc_agent::HostPanel {
                    side: match side {
                        crate::model::Side::Left => rc_agent::PanelSide::Left,
                        crate::model::Side::Right => rc_agent::PanelSide::Right,
                    },
                    path: panel.location().display(),
                    active: model.focus == side,
                    view_mode: panel.view_mode.as_str().to_string(),
                    selected: panel.selection.len(),
                    entries: panel.entries().len(),
                }
            })
            .collect()
    }

    fn active_panel(&self) -> rc_agent::HostPanel {
        self.panels()
            .into_iter()
            .find(|p| p.active)
            .unwrap_or_else(|| rc_agent::HostPanel {
                side: rc_agent::PanelSide::Left,
                path: String::new(),
                active: true,
                view_mode: "full".to_string(),
                selected: 0,
                entries: 0,
            })
    }

    fn list_directory(
        &self,
        panel: Option<rc_agent::PanelSide>,
        path: Option<&str>,
    ) -> Result<Vec<rc_core::entry::Entry>, String> {
        let mut model = lock(&self.model);
        let side = Self::panel(&model, panel);
        if let Some(path) = path {
            let location = rc_core::location::Location::parse(path).map_err(|e| e.to_string())?;
            model.open_in(side, location).map_err(|e| e.to_string())?;
        }
        Ok(model.panel(side).entries().to_vec())
    }

    fn open_directory(&self, panel: rc_agent::PanelSide, path: &str) -> Result<(), String> {
        let mut model = lock(&self.model);
        let location = rc_core::location::Location::parse(path).map_err(|e| e.to_string())?;
        model
            .open_in(Self::side(panel), location)
            .map_err(|e| e.to_string())
    }

    fn selection(&self, panel: Option<rc_agent::PanelSide>) -> Result<Vec<String>, String> {
        let model = lock(&self.model);
        let side = Self::panel(&model, panel);
        Ok(model.panel(side).selected_names())
    }

    fn select(
        &self,
        panel: Option<rc_agent::PanelSide>,
        mask: &str,
        mode: rc_agent::SelectMode,
    ) -> Result<usize, String> {
        let mut model = lock(&self.model);
        let side = Self::panel(&model, panel);
        let entries: Vec<rc_core::entry::Entry> = model.panel(side).entries().to_vec();
        let mask_set = rc_core::mask::MaskSet::parse_or_all(mask).map_err(|e| e.to_string())?;
        let count = match mode {
            rc_agent::SelectMode::Set => {
                model.panel_mut(side).selection.clear();
                model
                    .panel_mut(side)
                    .selection
                    .select_by_mask(&entries, &mask_set, true)
            }
            rc_agent::SelectMode::Add => model
                .panel_mut(side)
                .selection
                .select_by_mask(&entries, &mask_set, true),
            rc_agent::SelectMode::Remove => model
                .panel_mut(side)
                .selection
                .deselect_by_mask(&entries, &mask_set, true),
            rc_agent::SelectMode::InvertAll => {
                model.panel_mut(side).selection.invert(&entries, true)
            }
        };
        Ok(count)
    }

    fn copy(
        &self,
        sources: &[String],
        dest: &str,
        move_it: bool,
        overwrite: bool,
    ) -> Result<rc_agent::HostOpResult, String> {
        let mut model = lock(&self.model);
        let state = crate::model::dialogs::TransferDialogState {
            ask_overwrite: false,
            overwrite: if overwrite {
                rc_core::ops::OverwriteMode::Overwrite
            } else {
                rc_core::ops::OverwriteMode::Fail
            },
            ..crate::model::dialogs::TransferDialogState::new(
                move_it,
                sources.to_vec(),
                dest.to_string(),
            )
        };
        model.run_transfer(&state);
        Ok(rc_agent::HostOpResult {
            files: sources.len() as u64,
            dirs: 0,
            bytes: 0,
            skipped: 0,
        })
    }

    fn delete(&self, paths: &[String], use_trash: bool) -> Result<rc_agent::HostOpResult, String> {
        let mut model = lock(&self.model);
        let state = crate::model::dialogs::DeleteState::new(paths.to_vec(), !use_trash);
        model.run_delete(&state);
        Ok(rc_agent::HostOpResult {
            files: paths.len() as u64,
            dirs: 0,
            bytes: 0,
            skipped: 0,
        })
    }

    fn create_directory(&self, path: &str) -> Result<(), String> {
        rc_core::ops::create_dir(std::path::Path::new(path)).map_err(|e| e.to_string())
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        rc_core::ops::rename(std::path::Path::new(from), std::path::Path::new(to))
            .map_err(|e| e.to_string())
    }

    fn read_text(&self, path: &str, max_bytes: usize) -> Result<String, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let slice = &bytes[..bytes.len().min(max_bytes)];
        Ok(String::from_utf8_lossy(slice).to_string())
    }

    fn write_text(&self, path: &str, content: &str) -> Result<u64, String> {
        std::fs::write(path, content.as_bytes()).map_err(|e| e.to_string())?;
        Ok(content.len() as u64)
    }

    fn disk_space(&self, path: &str) -> Result<Option<rc_core::drive::Space>, String> {
        Ok(rc_core::drive::free_space(std::path::Path::new(path)))
    }

    fn search(&self, request: &rc_agent::SearchRequest) -> Result<Vec<String>, String> {
        let mask =
            rc_core::mask::MaskSet::parse_or_all(&request.name_mask).map_err(|e| e.to_string())?;
        let mut results = Vec::new();
        let root = std::path::Path::new(&request.root);
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if results.len() >= request.max_results {
                    return Ok(results);
                }
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir {
                    if request.recursive {
                        stack.push(path);
                    }
                    continue;
                }
                if !mask.is_match(&name) {
                    continue;
                }
                if let Some(needle) = &request.content {
                    if let Ok(bytes) = std::fs::read(&path) {
                        let text = String::from_utf8_lossy(&bytes);
                        let (haystack, needle_cmp) = if request.case_sensitive {
                            (text.to_string(), needle.clone())
                        } else {
                            (text.to_lowercase(), needle.to_lowercase())
                        };
                        if !haystack.contains(&needle_cmp) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                results.push(path.to_string_lossy().to_string());
            }
        }
        Ok(results)
    }

    fn help_topics(&self) -> Vec<(String, String)> {
        let index = rc_help::HelpIndex::load(&crate::paths::docs_dir());
        match index {
            Ok(index) => index
                .topics()
                .iter()
                .map(|t| {
                    (
                        t.id.clone(),
                        t.title_for("en", "en").unwrap_or(&t.id).to_string(),
                    )
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn help_body(&self, topic: &str, language: &str) -> Option<String> {
        let index = rc_help::HelpIndex::load(&crate::paths::docs_dir()).ok()?;
        let document = index.body(topic, language, "en")?;
        Some(rc_help::to_plain_text(&document.blocks))
    }

    fn current_language(&self) -> String {
        lock(&self.model).settings.language.clone()
    }

    fn available_languages(&self) -> Vec<String> {
        rc_i18n::Translator::load_dir(&crate::paths::locales_dir())
            .map(|locales| locales.iter().map(|l| l.code().to_string()).collect())
            .unwrap_or_else(|_| vec!["en".to_string()])
    }
}

impl rc_agent::LanguageHost for HostBridge {
    fn set_language(&self, language: &str) -> Result<(), String> {
        let available = rc_agent::FileManagerHost::available_languages(self);
        if !available.iter().any(|code| code == language) {
            return Err(format!(
                "unknown language {language:?}; available: {}",
                available.join(", ")
            ));
        }
        let mut model = lock(&self.model);
        model.set_language(language);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rc_agent::FileManagerHost;

    fn fixture() -> (tempfile::TempDir, HostBridge) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("b.log"), "world").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let settings = rc_core::config::Settings::default();
        let model = Model::for_test(
            settings,
            rc_core::location::Location::local(dir.path()),
            rc_core::location::Location::local("/tmp"),
        );
        (
            dir,
            HostBridge {
                model: Arc::new(Mutex::new(model)),
            },
        )
    }

    #[test]
    fn panels_and_active_panel_are_reported() {
        let (_dir, host) = fixture();
        let panels = host.panels();
        assert_eq!(panels.len(), 2);
        assert!(panels[0].active);
        assert_eq!(host.active_panel().side, rc_agent::PanelSide::Left);
    }

    #[test]
    fn listing_and_selection_tools_work() {
        let (_dir, host) = fixture();
        let entries = host.list_directory(None, None).unwrap();
        assert!(entries.iter().any(|e| e.name == "a.txt"));
        let selected = host
            .select(None, "*.txt", rc_agent::SelectMode::Set)
            .unwrap();
        assert_eq!(selected, 1);
        assert_eq!(host.selection(None).unwrap(), vec!["a.txt".to_string()]);
    }

    #[test]
    fn text_read_and_write_round_trip() {
        let (dir, host) = fixture();
        let path = dir.path().join("new.txt").to_string_lossy().to_string();
        let written = host.write_text(&path, "content").unwrap();
        assert_eq!(written, 7);
        assert_eq!(host.read_text(&path, 1024).unwrap(), "content");
        assert_eq!(host.read_text(&path, 3).unwrap(), "con");
    }

    #[test]
    fn search_finds_by_mask_and_content() {
        let (dir, host) = fixture();
        let request = rc_agent::SearchRequest {
            root: dir.path().to_string_lossy().to_string(),
            name_mask: "*.txt".into(),
            content: Some("hell".into()),
            recursive: true,
            case_sensitive: false,
            max_results: 10,
        };
        let results = host.search(&request).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].ends_with("a.txt"));
    }

    #[test]
    fn mkdir_and_rename_work() {
        let (dir, host) = fixture();
        let new_dir = dir.path().join("made");
        host.create_directory(&new_dir.to_string_lossy()).unwrap();
        assert!(new_dir.is_dir());
        let from = dir.path().join("a.txt");
        let to = dir.path().join("c.txt");
        host.rename(&from.to_string_lossy(), &to.to_string_lossy())
            .unwrap();
        assert!(to.exists() && !from.exists());
    }

    #[test]
    fn help_and_language_tools_report_something() {
        let (_dir, host) = fixture();
        assert!(!host.current_language().is_empty());
        assert!(!host.available_languages().is_empty());
        let topics = host.help_topics();
        if crate::paths::docs_dir().is_dir() {
            assert!(!topics.is_empty());
            let (id, _) = topics.first().cloned().unwrap();
            assert!(host.help_body(&id, "en").is_some());
        }
    }

    #[test]
    fn disk_space_is_reported_for_the_root() {
        let (_dir, host) = fixture();
        let space = host.disk_space("/").unwrap();
        assert!(space.map(|s| s.total > 0).unwrap_or(false));
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;
    use rc_agent::LanguageHost;

    fn host() -> HostBridge {
        let settings = rc_core::config::Settings::default();
        let model = Model::for_test(
            settings,
            rc_core::location::Location::local("/tmp"),
            rc_core::location::Location::local("/tmp"),
        );
        HostBridge {
            model: Arc::new(Mutex::new(model)),
        }
    }

    #[test]
    fn known_languages_can_be_switched() {
        let host = host();
        let available = rc_agent::FileManagerHost::available_languages(&host);
        assert!(
            available.contains(&"en".to_string()),
            "available: {available:?}"
        );
        assert!(host.set_language("en").is_ok());
        assert_eq!(rc_agent::FileManagerHost::current_language(&host), "en");
    }

    #[test]
    fn unknown_languages_are_rejected_with_the_list() {
        let host = host();
        let error = host.set_language("klingon").unwrap_err();
        assert!(error.contains("klingon"), "error: {error}");
        assert!(error.contains("available"), "error: {error}");
    }

    #[test]
    fn the_server_publishes_the_language_tool_when_the_host_supports_it() {
        let host = Arc::new(host());
        let config = rc_agent::ServerConfig {
            name: "t".into(),
            version: "0".into(),
            ..Default::default()
        };
        let without = rc_agent::McpServer::new(host.clone(), config.clone());
        let with = rc_agent::McpServer::new(host.clone(), config).with_language_host(host);
        let names_without: Vec<String> = without.tools().into_iter().map(|t| t.name).collect();
        let names_with: Vec<String> = with.tools().into_iter().map(|t| t.name).collect();
        assert!(!names_without.contains(&"set_language".to_string()));
        assert!(
            names_with.contains(&"set_language".to_string()),
            "tools: {names_with:?}"
        );
        assert_eq!(names_with.len(), names_without.len() + 1);
    }
}

/// The Git capability as the MCP server sees it.
///
/// Every mutating call goes through [`Model::evaluate`], so the agent gets the same three
/// answers a human would: allowed, refused with a reason, or "the user must approve this
/// first". Nothing here widens the policy to make a tool convenient.
impl rc_agent::GitHost for HostBridge {
    fn git_status(&self, path: &str) -> Result<Vec<rc_agent::HostGitFile>, String> {
        let repo = open_repo(path)?;
        let status = repo.status().map_err(|error| error.to_string())?;
        let mut files: Vec<rc_agent::HostGitFile> = status
            .entries
            .iter()
            .map(|(path, entry)| rc_agent::HostGitFile {
                path: path.clone(),
                codes: entry.codes(),
                state: describe_status(*entry),
            })
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    fn git_log(&self, path: &str, limit: usize) -> Result<Vec<(String, String, String)>, String> {
        let repo = open_repo(path)?;
        Ok(repo
            .commits(limit.clamp(1, 200))
            .into_iter()
            .map(|commit| (commit.short, commit.summary, commit.author))
            .collect())
    }

    fn git_branches(&self, path: &str) -> Result<(Vec<String>, String), String> {
        let repo = open_repo(path)?;
        let branches: Vec<String> = repo.branches().into_iter().map(|b| b.name).collect();
        Ok((branches, repo.head_branch().unwrap_or_default()))
    }

    fn git_stage(&self, path: &str, files: &[String]) -> Result<usize, String> {
        let repo = open_repo(path)?;
        let root = repo.root().to_path_buf();
        let mut model = lock(&self.model);
        for file in files {
            model.evaluate(&rc_core::policy::Request {
                operation: rc_core::policy::Operation::Write,
                resource: Some(root.join(file)),
                destination: Some(rc_core::policy::Destination::Local(root.join(file))),
                origin: "agent".to_string(),
            })?;
        }
        repo.stage_paths(files).map_err(|error| error.to_string())
    }

    fn git_commit(&self, path: &str, message: &str, files: &[String]) -> Result<String, String> {
        let repo = open_repo(path)?;
        let root = repo.root().to_path_buf();
        let mut model = lock(&self.model);
        // Committing is a change to the repository, so it asks like any other write.
        model.evaluate(&rc_core::policy::Request {
            operation: rc_core::policy::Operation::Write,
            resource: Some(root.clone()),
            destination: Some(rc_core::policy::Destination::Local(root.clone())),
            origin: "agent".to_string(),
        })?;
        let paths: Vec<String> = if files.is_empty() {
            repo.status()
                .map_err(|error| error.to_string())?
                .entries
                .into_iter()
                .map(|(path, _)| path)
                .collect()
        } else {
            files.to_vec()
        };
        if !paths.is_empty() {
            repo.stage_paths(&paths)
                .map_err(|error| error.to_string())?;
        }
        let identity = model
            .git_identity()
            .map(|id| (id.name.clone(), id.email.clone()));
        let commit = repo
            .commit_staged(
                message,
                identity
                    .as_ref()
                    .map(|(name, email)| (name.as_str(), email.as_str())),
            )
            .map_err(|error| error.to_string())?;
        Ok(commit.short)
    }

    fn git_checkout(&self, path: &str, branch: &str) -> Result<(), String> {
        let repo = open_repo(path)?;
        let root = repo.root().to_path_buf();
        let mut model = lock(&self.model);
        model.evaluate(&rc_core::policy::Request {
            operation: rc_core::policy::Operation::Write,
            resource: Some(root),
            destination: None,
            origin: "agent".to_string(),
        })?;
        repo.checkout_branch(branch, false)
            .map_err(|error| error.to_string())
    }

    fn git_pull(&self, path: &str) -> Result<usize, String> {
        let repo = open_repo(path)?;
        let root = repo.root().to_path_buf();
        let mut model = lock(&self.model);
        model.evaluate(&rc_core::policy::Request {
            operation: rc_core::policy::Operation::Network,
            resource: Some(root),
            destination: None,
            origin: "agent".to_string(),
        })?;
        let remote = repo
            .remotes()
            .into_iter()
            .next()
            .ok_or_else(|| "this repository has no remote".to_string())?;
        repo.fetch(&remote, None)
            .map_err(|error| error.to_string())?;
        repo.pull_fast_forward().map_err(|error| error.to_string())
    }

    fn git_push(&self, path: &str) -> Result<(), String> {
        let repo = open_repo(path)?;
        let root = repo.root().to_path_buf();
        let mut model = lock(&self.model);
        // Publishing is the one operation that always needs its own approval.
        model.evaluate(&rc_core::policy::Request {
            operation: rc_core::policy::Operation::Publish,
            resource: Some(root),
            destination: None,
            origin: "agent".to_string(),
        })?;
        let request = repo.push_request().map_err(|error| error.to_string())?;
        repo.push(&request, None).map_err(|error| error.to_string())
    }
}

/// Open the repository containing `path`.
fn open_repo(path: &str) -> Result<rc_git::GitRepo, String> {
    rc_git::GitRepo::open(path).map_err(|error| error.to_string())
}

/// A short human-readable state for a status entry.
fn describe_status(entry: rc_git::StatusEntry) -> String {
    match (entry.staged, entry.unstaged) {
        (Some(staged), Some(unstaged)) => format!("{staged:?} (staged) + {unstaged:?}"),
        (Some(staged), None) => format!("{staged:?} (staged)"),
        (None, Some(unstaged)) => format!("{unstaged:?}"),
        (None, None) => "clean".to_string(),
    }
}

#[cfg(test)]
mod git_host_tests {
    use super::*;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    /// A bridge over a model whose policy grants the temporary directory.
    fn bridge(dir: &std::path::Path, profile: &str) -> (HostBridge, std::path::PathBuf) {
        let root = dir.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let mut options = git2::RepositoryInitOptions::new();
        options.initial_head("main");
        let repo = git2::Repository::init_opts(&root, &options).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test").unwrap();
            config.set_str("user.email", "t@example.com").unwrap();
        }
        std::fs::write(root.join("code.rs"), "fn main() {}\n").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Test", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[])
            .unwrap();

        let mut model = Model::for_test(
            Settings::default(),
            Location::local(&root),
            Location::local(&root),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model.audit = rc_core::audit::AuditLog::new(dir.join("audit.log"), true);
        model.settings.agent.profile = profile.to_string();
        model.settings.agent.sandbox_roots = vec![root.to_string_lossy().to_string()];
        model.refresh_policy();
        let bridge = HostBridge {
            model: Arc::new(Mutex::new(model)),
        };
        (bridge, root)
    }

    #[test]
    fn the_read_tools_work_without_any_grant() {
        let dir = tempfile::tempdir().unwrap();
        let (bridge, root) = bridge(dir.path(), "sandbox");
        let path = root.to_string_lossy().to_string();
        let branches = rc_agent::GitHost::git_branches(&bridge, &path).unwrap();
        assert_eq!(branches.1, "main");
        let log = rc_agent::GitHost::git_log(&bridge, &path, 5).unwrap();
        assert_eq!(log[0].1, "first");
        // A clean tree reports no changes.
        assert!(
            rc_agent::GitHost::git_status(&bridge, &path)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_commit_inside_the_sandbox_goes_through_the_policy_and_the_audit() {
        let dir = tempfile::tempdir().unwrap();
        let (bridge, root) = bridge(dir.path(), "sandbox");
        let path = root.to_string_lossy().to_string();
        std::fs::write(root.join("code.rs"), "fn main() { changed }\n").unwrap();
        let id =
            rc_agent::GitHost::git_commit(&bridge, &path, "agent edit", &["code.rs".to_string()])
                .expect("the sandbox grant covers the write");
        assert_eq!(id.len(), 8);
        let model = lock(&bridge.model);
        let events = model.audit().read();
        assert!(
            events
                .iter()
                .any(|event| event.verdict == rc_core::audit::Verdict::Allowed)
        );
        assert!(model.audit_state().is_intact());
    }

    #[test]
    fn a_commit_outside_the_granted_folder_is_refused_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let (bridge, root) = bridge(dir.path(), "sandbox");
        // Narrow the sandbox to something else, so the repository is not covered.
        {
            let mut model = lock(&bridge.model);
            model.settings.agent.sandbox_roots =
                vec![dir.path().join("elsewhere").to_string_lossy().to_string()];
            model.refresh_policy();
        }
        std::fs::write(root.join("code.rs"), "fn main() { changed }\n").unwrap();
        let error = rc_agent::GitHost::git_commit(
            &bridge,
            &root.to_string_lossy(),
            "should not happen",
            &["code.rs".to_string()],
        )
        .unwrap_err();
        assert!(
            error.contains("grant") || error.contains("covers"),
            "the refusal explains itself: {error}"
        );
    }

    #[test]
    fn pushing_always_asks_for_an_approval_and_never_runs_silently() {
        let dir = tempfile::tempdir().unwrap();
        let (bridge, root) = bridge(dir.path(), "privileged");
        let error = rc_agent::GitHost::git_push(&bridge, &root.to_string_lossy()).unwrap_err();
        assert!(error.contains("approval"), "{error}");
        // And the attempt is in the audit trail.
        let model = lock(&bridge.model);
        assert!(
            model
                .audit()
                .read()
                .iter()
                .any(|event| event.operation == "publish"),
            "the publish attempt was recorded"
        );
    }
}

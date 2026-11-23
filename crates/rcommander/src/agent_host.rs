//! MCP host adapter: exposes the file manager through `rc-agent`.

use crate::model::Model;
use std::sync::{Arc, Mutex};

/// Shared model handed to the MCP server.
pub type SharedModel = Arc<Mutex<Model>>;

/// Serve the Model Context Protocol on stdin/stdout.
pub fn serve_stdio(model: Model) -> Result<(), String> {
    let shared: SharedModel = Arc::new(Mutex::new(model));
    let host = HostBridge {
        model: shared.clone(),
    };
    let config = rc_agent::ServerConfig {
        name: "rcommander".to_string(),
        version: crate::VERSION.to_string(),
        ..Default::default()
    };
    let server = rc_agent::McpServer::new(Arc::new(host), config);
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

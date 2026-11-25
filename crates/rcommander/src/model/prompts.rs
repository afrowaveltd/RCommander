//! The prompt catalogue in the model (Phase 16.16).
//!
//! Two sources, one list: the prompts that ship with the program (`prompts/*.toml` next to the
//! data directory) and the user's own files (`<config>/prompts/*.toml`). The built-ins are
//! read-only — changing one means copying it — so an upgrade can never overwrite the user's words.
//!
//! The catalogue is text. Selecting a prompt fills the chat input and nothing else: no permission
//! is granted by a prompt, and no prompt can widen the scope of a session.

use super::Model;
use rc_core::prompt::{Catalogue, Category, Entry, Prompt, PromptFile, render, user_dir};

/// State the prompt picker needs.
#[derive(Clone, Debug, Default)]
pub struct PromptPicker {
    pub open: bool,
    /// The category tab; `None` shows everything.
    pub category: Option<Category>,
    pub search: String,
    /// Index into the visible list.
    pub selected: usize,
    /// The text after filling the variables in, shown before it is inserted.
    pub preview: String,
    /// Variables the preview could not fill.
    pub missing: Vec<String>,
}

impl Model {
    /// Where the user's own prompt files live.
    pub fn prompts_user_dir(&self) -> std::path::PathBuf {
        user_dir(&self.config_dir())
    }

    /// Load both sources. Called at boot and after the user edits something.
    pub fn load_prompts(&mut self) {
        let mut files: Vec<(std::path::PathBuf, bool)> = Vec::new();
        let builtin = crate::paths::prompts_dir();
        if let Ok(entries) = std::fs::read_dir(&builtin) {
            let mut paths: Vec<std::path::PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("toml"))
                .collect();
            paths.sort();
            for path in paths {
                files.push((path, true));
            }
        }
        let user = self.prompts_user_dir();
        if let Ok(entries) = std::fs::read_dir(&user) {
            let mut paths: Vec<std::path::PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("toml"))
                .collect();
            paths.sort();
            for path in paths {
                files.push((path, false));
            }
        }
        self.prompts = Catalogue::load(&files);
    }

    /// The catalogue (loading it on first use).
    pub fn prompts(&mut self) -> &Catalogue {
        if !self.prompts_loaded {
            self.load_prompts();
            self.prompts_loaded = true;
        }
        &self.prompts
    }

    /// The prompts the picker should show: the category filter and the search, applied.
    pub fn visible_prompts(&mut self) -> Vec<Entry> {
        let category = self.prompt_picker.category;
        let search = self.prompt_picker.search.clone();
        let catalogue = self.prompts();
        let mut entries: Vec<Entry> = catalogue
            .search(&search)
            .into_iter()
            .filter(|entry| category.is_none_or(|wanted| entry.category == wanted))
            .cloned()
            .collect();
        if entries.is_empty() && !search.trim().is_empty() {
            // Fall back to the whole catalogue so a typo does not look like an empty catalogue.
            entries = catalogue.entries().to_vec();
        }
        entries
    }

    /// Fill a prompt's variables from the current state and remember the preview.
    pub fn preview_prompt(&mut self, id: &str) -> bool {
        let Some(entry) = self.prompts().get(id).cloned() else {
            return false;
        };
        let values = self.prompt_values();
        let rendered = render(&entry, &values);
        self.prompt_picker.preview = rendered.text;
        self.prompt_picker.missing = rendered.missing;
        true
    }

    /// The variables this program can fill by itself.
    ///
    /// Deliberately few: a name, a path and a host are things the application *knows*. Anything
    /// about the content of a file is the agent's business, so there is no `{{file_content}}` that
    /// would quietly send data somewhere.
    pub fn prompt_values(&self) -> std::collections::BTreeMap<String, String> {
        let mut values = std::collections::BTreeMap::new();
        let panel = self.active();
        if let Some(path) = panel.location().as_path() {
            values.insert("cwd".to_string(), path.display().to_string());
        }
        if let Some(entry) = panel.current() {
            values.insert("selection".to_string(), entry.name.clone());
        }
        let marked = panel.selection.names();
        if marked.len() > 1 {
            values.insert("selection_count".to_string(), marked.len().to_string());
        }
        if let Some(model) = self.ai_model() {
            values.insert("model".to_string(), model.to_string());
        }
        if let rc_core::location::Location::Remote { host, .. } = panel.location() {
            values.insert("host".to_string(), host.clone());
        }
        values
    }

    /// Insert a prompt into the chat input, replacing whatever is there.
    pub fn insert_prompt(&mut self, id: &str) -> super::CommandOutcome {
        if !self.preview_prompt(id) {
            let message = self.t("prompt.not_found");
            self.report_error(&message);
            return super::CommandOutcome::Failed(message);
        }
        let text = self.prompt_picker.preview.clone();
        self.chat_session.input = text;
        self.prompt_picker.open = false;
        // A prompt with unfilled variables is still inserted: the markers are visible in the input,
        // which is the honest way to say "I could not fill this in".
        if !self.prompt_picker.missing.is_empty() {
            let names = self.prompt_picker.missing.join(", ");
            let message = self.tf("prompt.missing", &[("names", &names)]);
            self.report(message);
        }
        super::CommandOutcome::Ok
    }

    /// Save a prompt as one of the user's own, in the file of its category.
    pub fn save_prompt(&mut self, prompt: Prompt, category: Category) -> Result<(), String> {
        if prompt.id.trim().is_empty() {
            return Err(self.t("prompt.error.id").to_string());
        }
        if prompt.text.trim().is_empty() {
            return Err(self.t("prompt.error.text").to_string());
        }
        let dir = self.prompts_user_dir();
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let path = dir.join(format!("{}.toml", category.name()));
        // Read what is there, so saving one prompt does not wipe the others in the same file.
        let mut file = if path.is_file() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| toml::from_str::<PromptFile>(&text).ok())
                .unwrap_or_default()
        } else {
            PromptFile::default()
        };
        if file.meta.name.is_empty() {
            file.meta.name = category.name().to_string();
        }
        file.meta.category = category.name().to_string();
        file.prompts.retain(|existing| existing.id != prompt.id);
        file.prompts.push(prompt);
        let text = toml::to_string_pretty(&file).map_err(|error| error.to_string())?;
        std::fs::write(&path, text).map_err(|error| error.to_string())?;
        self.load_prompts();
        self.prompts_loaded = true;
        Ok(())
    }

    /// Delete one of the user's own prompts; a built-in one cannot be deleted.
    pub fn delete_prompt(&mut self, id: &str) -> Result<bool, String> {
        let Some(entry) = self.prompts().get(id).cloned() else {
            return Ok(false);
        };
        if entry.builtin {
            return Err(self.t("prompt.error.builtin").to_string());
        }
        let dir = self.prompts_user_dir();
        let mut removed = false;
        let Ok(files) = std::fs::read_dir(&dir) else {
            return Ok(false);
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut parsed) = toml::from_str::<PromptFile>(&text) else {
                continue;
            };
            let before = parsed.prompts.len();
            parsed.prompts.retain(|prompt| prompt.id != id);
            if parsed.prompts.len() != before {
                let rewritten = toml::to_string_pretty(&parsed).map_err(|e| e.to_string())?;
                std::fs::write(&path, rewritten).map_err(|e| e.to_string())?;
                removed = true;
            }
        }
        if removed {
            self.load_prompts();
            self.prompts_loaded = true;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;
    use std::path::Path;

    fn model(dir: &Path, cwd: &Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(cwd),
            Location::local(cwd),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    #[test]
    fn the_shipped_catalogue_loads_and_covers_every_category() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut model = model(dir.path(), cwd.path());
        let catalogue = model.prompts().clone();
        assert!(
            catalogue.problems().is_empty(),
            "the shipped prompts must parse: {:?}",
            catalogue.problems()
        );
        assert!(
            catalogue.entries().len() >= 20,
            "the catalogue is a starting point, not a placeholder: {}",
            catalogue.entries().len()
        );
        for category in Category::ALL {
            assert!(
                !catalogue.in_category(category).is_empty(),
                "{category:?} has no prompts"
            );
        }
        // Every prompt has a unique id and a title.
        let mut ids: Vec<&str> = catalogue
            .entries()
            .iter()
            .map(|entry| entry.prompt.id.as_str())
            .collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "ids are unique");
        for entry in catalogue.entries() {
            assert!(!entry.prompt.title.trim().is_empty(), "{:?}", entry.prompt);
            assert!(entry.builtin, "every shipped prompt is built-in");
        }
    }

    #[test]
    fn the_variables_come_from_the_panel_and_the_model() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        std::fs::write(cwd.path().join("main.rs"), b"fn main() {}").unwrap();
        let mut model = model(dir.path(), cwd.path());
        model.settings.agent.model = "qwen3:8b".to_string();
        let values = model.prompt_values();
        assert_eq!(
            values.get("cwd").map(String::as_str),
            Some(cwd.path().display().to_string().as_str())
        );
        assert_eq!(values.get("model").map(String::as_str), Some("qwen3:8b"));
        // `selection` names the file under the cursor.
        assert_eq!(values.get("selection").map(String::as_str), Some("main.rs"));
        // There is no variable for file *content*: that would quietly send data somewhere.
        assert!(!values.keys().any(|key| key.contains("content")));
    }

    #[test]
    fn a_prompt_is_rendered_with_the_current_state_and_inserted() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        std::fs::write(cwd.path().join("main.rs"), b"fn main() {}").unwrap();
        let mut model = model(dir.path(), cwd.path());
        assert_eq!(
            model.insert_prompt("files.explain"),
            crate::model::CommandOutcome::Ok
        );
        let input = model.chat_session.input.clone();
        assert!(input.contains("main.rs"), "{input}");
        assert!(!input.contains("{{"), "no marker is left: {input}");
        assert!(model.prompt_picker.missing.is_empty());
    }

    #[test]
    fn an_unfillable_variable_is_reported_and_stays_visible() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut model = model(dir.path(), cwd.path());
        // `{{host}}` is only known for a remote panel, so it cannot be filled here.
        assert_eq!(
            model.insert_prompt("remote.permissions"),
            crate::model::CommandOutcome::Ok
        );
        let input = model.chat_session.input.clone();
        assert!(
            input.contains("{{host}}"),
            "the marker stays visible: {input}"
        );
        assert_eq!(model.prompt_picker.missing, vec!["host"]);
        assert!(model.status.contains("prompt.missing"), "{}", model.status);
    }

    #[test]
    fn the_picker_filters_by_category_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut model = model(dir.path(), cwd.path());
        model.prompt_picker.category = Some(Category::Git);
        let visible = model.visible_prompts();
        assert!(!visible.is_empty());
        assert!(visible.iter().all(|entry| entry.category == Category::Git));
        model.prompt_picker.category = None;
        model.prompt_picker.search = "commit".to_string();
        let visible = model.visible_prompts();
        assert!(
            visible
                .iter()
                .any(|entry| entry.prompt.id == "git.commit-message"),
            "{:?}",
            visible.iter().map(|e| &e.prompt.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_users_own_prompt_is_saved_read_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut model = model(dir.path(), cwd.path());
        let prompt = Prompt {
            id: "mine.why".to_string(),
            title: "Why did it stop?".to_string(),
            about: "my own".to_string(),
            text: "Look at {{cwd}} and tell me why.".to_string(),
        };
        model
            .save_prompt(prompt.clone(), Category::Diagnostics)
            .expect("saving works");
        // A fresh model reads it back from the same directory.
        let mut again = model_from(dir.path(), cwd.path());
        assert!(again.prompts().get("mine.why").is_some());
        let entry = again.prompts().get("mine.why").cloned().unwrap();
        assert!(!entry.builtin, "the user's prompt is not built-in");
        assert_eq!(entry.category, Category::Diagnostics);
        // Saving one prompt keeps the others in the same file.
        let second = Prompt {
            id: "mine.other".to_string(),
            title: "Other".to_string(),
            about: String::new(),
            text: "Do the other thing.".to_string(),
        };
        model.save_prompt(second, Category::Diagnostics).unwrap();
        let mut again = model_from(dir.path(), cwd.path());
        assert!(again.prompts().get("mine.why").is_some());
        assert!(again.prompts().get("mine.other").is_some());
        // And deleting removes only that one.
        assert!(again.delete_prompt("mine.why").unwrap());
        let mut again = model_from(dir.path(), cwd.path());
        assert!(again.prompts().get("mine.why").is_none());
        assert!(again.prompts().get("mine.other").is_some());
    }

    #[test]
    fn a_built_in_prompt_cannot_be_deleted_and_empty_prompts_cannot_be_saved() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut model = model(dir.path(), cwd.path());
        let error = model.delete_prompt("files.explain").unwrap_err();
        assert!(!error.is_empty(), "the refusal explains itself");
        let empty = Prompt {
            id: String::new(),
            title: String::new(),
            about: String::new(),
            text: "x".to_string(),
        };
        assert!(model.save_prompt(empty, Category::General).is_err());
        let no_text = Prompt {
            id: "a".to_string(),
            title: "A".to_string(),
            about: String::new(),
            text: "   ".to_string(),
        };
        assert!(model.save_prompt(no_text, Category::General).is_err());
    }

    fn model_from(dir: &Path, cwd: &Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(cwd),
            Location::local(cwd),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model.load_prompts();
        model.prompts_loaded = true;
        model
    }
}

/// The editor's state on the Prompts page.
#[derive(Clone, Debug, Default)]
pub struct PromptEditor {
    /// The prompt being edited, if any.
    pub id: Option<String>,
    pub title: String,
    pub about: String,
    pub text: String,
    /// Category for a prompt that is being created (an existing one keeps its own).
    pub category: rc_core::prompt::Category,
    /// True when the edited prompt does not exist yet.
    pub creating: bool,
    /// The last thing that happened, shown under the form.
    pub status: String,
}

impl Model {
    /// Start editing a prompt: a copy of it, so cancelling changes nothing.
    pub fn edit_prompt(&mut self, id: &str) -> bool {
        let Some(entry) = self.prompts().get(id).cloned() else {
            return false;
        };
        self.prompt_editor = PromptEditor {
            id: Some(id.to_string()),
            title: entry.prompt.title.clone(),
            about: entry.prompt.about.clone(),
            text: entry.prompt.text.clone(),
            category: entry.category,
            creating: false,
            status: String::new(),
        };
        true
    }

    /// Start a new prompt of the given category.
    pub fn new_prompt(&mut self, category: rc_core::prompt::Category) {
        self.prompt_editor = PromptEditor {
            id: Some(String::new()),
            title: String::new(),
            about: String::new(),
            text: String::new(),
            category,
            creating: true,
            status: String::new(),
        };
    }

    /// Take a copy of a built-in prompt as the starting point for the user's own version.
    ///
    /// The id is kept, which is what makes the copy an *override*: the picker then shows one
    /// prompt, with the user's text, and *Reset* brings the shipped one back.
    pub fn copy_prompt_to_mine(&mut self, id: &str) -> Result<(), String> {
        let Some(entry) = self.prompts().get(id).cloned() else {
            return Err(self.t("prompt.not_found").to_string());
        };
        if !entry.builtin {
            return Ok(());
        }
        self.save_prompt(entry.prompt.clone(), entry.category)?;
        self.edit_prompt(id);
        Ok(())
    }

    /// Drop the user's version of a built-in prompt, so the shipped text applies again.
    pub fn reset_prompt(&mut self, id: &str) -> Result<bool, String> {
        if !self.prompts().is_overridden(id) {
            return Ok(false);
        }
        let removed = self.delete_prompt(id)?;
        if removed {
            self.clear_prompt_editor();
            let text = self.t("prompt.reset.done").to_string();
            self.report(text);
        }
        Ok(removed)
    }

    /// Save what the editor holds. A new prompt gets an id derived from its title.
    pub fn save_edited_prompt(&mut self) -> Result<String, String> {
        let editor = self.prompt_editor.clone();
        let id = match editor.id.as_deref() {
            Some(existing) if !existing.trim().is_empty() => existing.trim().to_string(),
            _ => slug(&editor.title, editor.category),
        };
        if editor.title.trim().is_empty() {
            return Err(self.t("prompt.error.title").to_string());
        }
        if editor.text.trim().is_empty() {
            return Err(self.t("prompt.error.text").to_string());
        }
        let prompt = rc_core::prompt::Prompt {
            id: id.clone(),
            title: editor.title.trim().to_string(),
            about: editor.about.trim().to_string(),
            text: editor.text.clone(),
        };
        // An existing prompt keeps its category; a new one takes the editor's choice.
        let category = if editor.creating {
            editor.category
        } else {
            self.prompts()
                .get(&id)
                .map(|entry| entry.category)
                .unwrap_or(editor.category)
        };
        self.save_prompt(prompt, category)?;
        self.edit_prompt(&id);
        self.prompt_editor.status = self.t("prompt.saved").to_string();
        Ok(id)
    }

    /// Leave the editor without saving.
    pub fn clear_prompt_editor(&mut self) {
        self.prompt_editor = PromptEditor::default();
    }

    /// The shipped text of the prompt being edited, when it overrides a built-in.
    pub fn prompt_reset_target(&mut self) -> Option<rc_core::prompt::Prompt> {
        let id = self.prompt_editor.id.clone()?;
        self.prompts()
            .original(&id)
            .map(|entry| entry.prompt.clone())
    }
}

/// A stable id from a title: lowercase words joined by dots, so a screenshot of the page reads
/// back as something a settings file could refer to.
fn slug(title: &str, category: rc_core::prompt::Category) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect();
    if words.is_empty() {
        return format!("{}.custom", category.name());
    }
    // Accents and non-ASCII letters survive a `to_lowercase` but not a filesystem round trip in
    // some editors, so anything outside a conservative set becomes an underscore.
    let body: Vec<String> = words
        .iter()
        .map(|word| {
            word.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect();
    format!("{}.{}", category.name(), body.join("."))
}

#[cfg(test)]
mod editor_tests {
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;
    use std::path::Path;

    fn model(dir: &Path, cwd: &Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(cwd),
            Location::local(cwd),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model.load_prompts();
        model.prompts_loaded = true;
        model
    }

    #[test]
    fn editing_a_prompt_starts_from_a_copy_and_changes_nothing_until_saved() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut model = model(dir.path(), cwd.path());
        assert!(model.edit_prompt("files.explain"));
        let original = model.prompts().get("files.explain").cloned().unwrap();
        model.prompt_editor.text = "my own wording".to_string();
        // The catalogue still has the shipped text.
        assert_eq!(
            model.prompts().get("files.explain").unwrap().prompt.text,
            original.prompt.text
        );
        // Cancelling throws the edit away.
        model.clear_prompt_editor();
        assert!(model.prompt_editor.id.is_none());
        assert_eq!(
            model.prompts().get("files.explain").unwrap().prompt.text,
            original.prompt.text
        );
    }

    #[test]
    fn saving_an_edited_built_in_creates_an_override_with_a_reset_target() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut model = model(dir.path(), cwd.path());
        let shipped = model
            .prompts()
            .get("files.explain")
            .unwrap()
            .prompt
            .text
            .clone();
        assert!(model.copy_prompt_to_mine("files.explain").is_ok());
        model.prompt_editor.text = "Explain {{selection}} in my own words.".to_string();
        let id = model.save_edited_prompt().expect("saving works");
        assert_eq!(id, "files.explain", "the id is kept, so it overrides");
        let entry = model.prompts().get("files.explain").cloned().unwrap();
        assert_eq!(entry.prompt.text, "Explain {{selection}} in my own words.");
        assert!(!entry.builtin);
        assert!(model.prompts().is_overridden("files.explain"));
        // The reset target is the shipped text.
        let target = model.prompt_reset_target().expect("a reset target");
        assert_eq!(target.text, shipped);
        // Resetting brings it back and deletes the override.
        assert!(model.reset_prompt("files.explain").unwrap());
        let entry = model.prompts().get("files.explain").cloned().unwrap();
        assert_eq!(entry.prompt.text, shipped);
        assert!(entry.builtin);
        assert!(!model.prompts().is_overridden("files.explain"));
    }

    #[test]
    fn a_new_prompt_gets_an_id_from_its_title() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut model = model(dir.path(), cwd.path());
        model.new_prompt(rc_core::prompt::Category::Diagnostics);
        model.prompt_editor.title = "Proč to spadlo?".to_string();
        model.prompt_editor.text = "Look at {{cwd}} and tell me.".to_string();
        let id = model.save_edited_prompt().expect("saving works");
        // Non-ASCII becomes underscores, so the id stays usable in a file name and a setting.
        assert!(id.starts_with("diagnostics."), "{id}");
        assert!(model.prompts().get(&id).is_some());
        let entry = model.prompts().get(&id).cloned().unwrap();
        assert_eq!(entry.category, rc_core::prompt::Category::Diagnostics);
        assert!(!entry.builtin);
        // And it survives a reload from disk.
        let mut again = model2(dir.path(), cwd.path());
        assert!(again.prompts().get(&id).is_some());
    }

    #[test]
    fn an_empty_title_or_text_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut model = model(dir.path(), cwd.path());
        model.new_prompt(rc_core::prompt::Category::General);
        model.prompt_editor.text = "text".to_string();
        assert!(model.save_edited_prompt().is_err(), "a title is needed");
        model.prompt_editor.title = "Title".to_string();
        model.prompt_editor.text = "   ".to_string();
        assert!(model.save_edited_prompt().is_err(), "text is needed");
    }

    fn model2(dir: &Path, cwd: &Path) -> Model {
        model(dir, cwd)
    }
}

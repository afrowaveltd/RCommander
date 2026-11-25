//! The prompt catalogue (Phase 16.16).
//!
//! A prompt is a piece of text with a name, a category and a few typed variables. That is all this
//! module is, and the modesty is deliberate: **a prompt is data**. Inserting one grants nothing.
//! Permissions live in `rc_core::policy`, the workspace scope decides what an agent can see, and a
//! prompt that "asks" for more access is refused exactly like one that asks for nothing. Anything
//! cleverer than text substitution belongs in the code that can be reviewed.
//!
//! The catalogue is TOML, one file per source:
//!
//! * **built-in** prompts ship with the program and are read-only — editing one means copying it,
//!   so an upgrade can never silently overwrite the user's words;
//! * **custom** prompts live in the user's configuration directory and are entirely theirs.
//!
//! Variables are `{{name}}`. An unknown variable is *left visible* rather than blanked: a prompt
//! that says "look at {{selection}}" and fills in nothing is a bug the user can see, while a prompt
//! that silently says "look at " is a bug nobody notices.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The groups the picker shows as tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// Files, folders and their contents.
    Files,
    /// Git: commits, branches, diffs, messages.
    Git,
    /// Remote connections: SFTP, FTP, SSH.
    Remote,
    /// Packages, builds, toolchains.
    Build,
    /// Logs, services, "why did this break".
    Diagnostics,
    /// Everything that does not fit above.
    #[default]
    General,
}

impl Category {
    /// Every category, in the order the picker shows them.
    pub const ALL: [Category; 6] = [
        Category::Files,
        Category::Git,
        Category::Remote,
        Category::Build,
        Category::Diagnostics,
        Category::General,
    ];

    /// Name used in the TOML file.
    pub fn name(self) -> &'static str {
        match self {
            Category::Files => "files",
            Category::Git => "git",
            Category::Remote => "remote",
            Category::Build => "build",
            Category::Diagnostics => "diagnostics",
            Category::General => "general",
        }
    }

    /// Localisation key of the tab label.
    pub fn label_key(self) -> &'static str {
        match self {
            Category::Files => "prompt.category.files",
            Category::Git => "prompt.category.git",
            Category::Remote => "prompt.category.remote",
            Category::Build => "prompt.category.build",
            Category::Diagnostics => "prompt.category.diagnostics",
            Category::General => "prompt.category.general",
        }
    }

    /// Parse the name from a file; an unknown name is [`Category::General`], not an error, so one
    /// odd file cannot take the whole catalogue down.
    pub fn parse(name: &str) -> Category {
        match name.trim().to_ascii_lowercase().as_str() {
            "files" | "file" => Category::Files,
            "git" => Category::Git,
            "remote" | "ssh" | "ftp" => Category::Remote,
            "build" | "packages" => Category::Build,
            "diagnostics" | "logs" | "system" => Category::Diagnostics,
            _ => Category::General,
        }
    }
}

/// One prompt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt {
    /// Stable id, unique in the catalogue; it is what a setting or a log refers to.
    pub id: String,
    /// Title shown in the picker; an empty title falls back to the id, so a minimal file works.
    #[serde(default)]
    pub title: String,
    /// One line of explanation, shown under the title.
    #[serde(default)]
    pub about: String,
    /// The text that is inserted into the chat input.
    pub text: String,
}

/// A prompt together with where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub prompt: Prompt,
    pub category: Category,
    /// True when the prompt ships with the program and cannot be edited in place.
    pub builtin: bool,
}

impl Entry {
    /// The variables the body uses, in the order they first appear, without duplicates.
    pub fn variables(&self) -> Vec<String> {
        variables(&self.prompt.text)
    }
}

/// The catalogue: built-in prompts plus whatever the user added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalogue {
    entries: Vec<Entry>,
    /// The built-in prompts by id, kept even when the user overrides one, so "reset" has a target.
    originals: BTreeMap<String, Entry>,
    /// Files that could not be read, so the options page can say which one and why.
    problems: Vec<String>,
}

/// A prompt body with its variables filled in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    /// The text to put into the chat input.
    pub text: String,
    /// Variables the caller did not supply; they stay visible in the text.
    pub missing: Vec<String>,
}

impl Catalogue {
    /// Every prompt, built-ins first, then the user's own, each group sorted by title.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The prompts of one category.
    pub fn in_category(&self, category: Category) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|entry| entry.category == category)
            .collect()
    }

    /// Read errors, one message per unusable file.
    pub fn problems(&self) -> &[String] {
        &self.problems
    }

    /// A prompt by id.
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.prompt.id == id)
    }

    /// Case-insensitive search across titles, descriptions and bodies.
    pub fn search(&self, needle: &str) -> Vec<&Entry> {
        let needle = needle.trim().to_lowercase();
        if needle.is_empty() {
            return self.entries.iter().collect();
        }
        self.entries
            .iter()
            .filter(|entry| {
                entry.prompt.title.to_lowercase().contains(&needle)
                    || entry.prompt.about.to_lowercase().contains(&needle)
                    || entry.prompt.text.to_lowercase().contains(&needle)
                    || entry.prompt.id.to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Build the catalogue from a list of files.
    ///
    /// A file that does not parse becomes a `problem` and is skipped: a typo in one prompt file must
    /// not cost the user every other prompt.
    pub fn load(files: &[(PathBuf, bool)]) -> Catalogue {
        let mut catalogue = Catalogue::default();
        for (path, builtin) in files {
            match load_file(path, *builtin) {
                Ok(entries) => {
                    for entry in entries {
                        if entry.builtin {
                            catalogue
                                .originals
                                .insert(entry.prompt.id.clone(), entry.clone());
                        }
                        // A later source wins: the user's own file is read after the shipped ones,
                        // so an override with the same id replaces the built-in instead of sitting
                        // beside it. That is what makes "edit a system prompt" possible without
                        // ever writing to the shipped file.
                        catalogue
                            .entries
                            .retain(|existing| existing.prompt.id != entry.prompt.id);
                        catalogue.entries.push(entry);
                    }
                }
                Err(problem) => catalogue.problems.push(problem),
            }
        }
        catalogue.sort();
        catalogue
    }

    /// The shipped prompt behind `id`, even when the user has overridden it.
    pub fn original(&self, id: &str) -> Option<&Entry> {
        self.originals.get(id)
    }

    /// True when the effective prompt with this id is the user's own version of a built-in.
    pub fn is_overridden(&self, id: &str) -> bool {
        let Some(entry) = self.get(id) else {
            return false;
        };
        !entry.builtin && self.originals.contains_key(id)
    }

    /// The effective prompt and, when it overrides a built-in, the shipped text it came from.
    pub fn origin_pair(&self, id: &str) -> (Option<&Entry>, Option<&Entry>) {
        (self.get(id), self.original(id))
    }

    /// Add one prompt in memory (used by the editor before saving).
    pub fn add(&mut self, entry: Entry) {
        // A duplicate id replaces the old entry: ids are the identity of a prompt.
        self.entries
            .retain(|existing| existing.prompt.id != entry.prompt.id);
        self.entries.push(entry);
        self.sort();
    }

    /// Remove a prompt by id; returns whether something was removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.prompt.id != id);
        self.entries.len() != before
    }

    fn sort(&mut self) {
        self.entries.sort_by(|a, b| {
            // Built-ins first inside a category, then alphabetical, then by id so the order is
            // stable no matter what order the files were read in.
            a.category
                .cmp(&b.category)
                .then_with(|| b.builtin.cmp(&a.builtin))
                .then_with(|| {
                    a.prompt
                        .title
                        .to_lowercase()
                        .cmp(&b.prompt.title.to_lowercase())
                })
                .then_with(|| a.prompt.id.cmp(&b.prompt.id))
        });
    }
}

/// Fill in `values`; unknown variables stay in the text and are reported.
pub fn render(entry: &Entry, values: &BTreeMap<String, String>) -> Rendered {
    let mut text = String::new();
    let mut missing: Vec<String> = Vec::new();
    let mut rest = entry.prompt.text.as_str();
    while let Some(start) = rest.find("{{") {
        text.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            // An unclosed `{{` stays as it is: half a variable is not a variable.
            text.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let name = after[..end].trim().to_string();
        match values.get(&name) {
            Some(value) => text.push_str(value),
            None => {
                if !missing.contains(&name) {
                    missing.push(name.clone());
                }
                // Left visible on purpose: a blank is a silent bug, a `{{name}}` is a visible one.
                text.push_str(&format!("{{{{{name}}}}}"));
            }
        }
        rest = &after[end + 2..];
    }
    text.push_str(rest);
    Rendered { text, missing }
}

/// The variables a body uses, in order of first appearance.
pub fn variables(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim().to_string();
        if !name.is_empty() && !found.contains(&name) {
            found.push(name);
        }
        rest = &after[end + 2..];
    }
    found
}

/// The file format: a list of prompts, one file per category.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptFile {
    /// `[meta]` with a human name and the category the file belongs to.
    #[serde(default)]
    pub meta: FileMeta,
    /// The prompts themselves.
    #[serde(default, rename = "prompt")]
    pub prompts: Vec<Prompt>,
}

/// `[meta]` of a prompt file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMeta {
    /// Shown in the picker as the group's origin.
    #[serde(default)]
    pub name: String,
    /// Category for every prompt in the file.
    #[serde(default)]
    pub category: String,
}

fn load_file(path: &Path, builtin: bool) -> Result<Vec<Entry>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let file: PromptFile =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    let category = Category::parse(&file.meta.category);
    let mut entries: Vec<Entry> = Vec::new();
    for prompt in file.prompts {
        if prompt.id.trim().is_empty() {
            return Err(format!(
                "{}: a prompt without an id cannot be referred to",
                path.display()
            ));
        }
        if prompt.text.trim().is_empty() {
            return Err(format!(
                "{}: prompt “{}” has no text",
                path.display(),
                prompt.id
            ));
        }
        let mut prompt = prompt;
        if prompt.title.trim().is_empty() {
            prompt.title = prompt.id.clone();
        }
        entries.push(Entry {
            prompt,
            category,
            builtin,
        });
    }
    Ok(entries)
}

/// Where the user's own prompt files live, given a configuration directory.
pub fn user_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("prompts")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, category: Category, text: &str) -> Entry {
        Entry {
            prompt: Prompt {
                id: id.to_string(),
                title: id.to_string(),
                about: String::new(),
                text: text.to_string(),
            },
            category,
            builtin: false,
        }
    }

    #[test]
    fn the_categories_have_names_and_survive_an_unknown_one() {
        for category in Category::ALL {
            assert_eq!(Category::parse(category.name()), category);
            assert!(category.label_key().starts_with("prompt.category."));
        }
        assert_eq!(Category::parse("tomfoolery"), Category::General);
        assert_eq!(Category::parse(" SSH "), Category::Remote);
    }

    #[test]
    fn variables_are_found_once_in_order() {
        let text = "Look at {{selection}} in {{cwd}}, then {{selection}} again and {{model}}.";
        assert_eq!(variables(text), vec!["selection", "cwd", "model"]);
        assert!(variables("no variables here").is_empty());
        // An unclosed marker is not a variable.
        assert!(variables("half {{open").is_empty());
    }

    #[test]
    fn rendering_fills_what_it_knows_and_leaves_the_rest_visible() {
        let prompt = entry(
            "explain",
            Category::Files,
            "Explain {{selection}} in {{cwd}}",
        );
        let mut values = BTreeMap::new();
        values.insert("cwd".to_string(), "/tmp/work".to_string());
        let rendered = render(&prompt, &values);
        assert_eq!(rendered.text, "Explain {{selection}} in /tmp/work");
        assert_eq!(rendered.missing, vec!["selection"]);
        // With everything supplied, nothing is missing.
        values.insert("selection".to_string(), "main.rs".to_string());
        let rendered = render(&prompt, &values);
        assert_eq!(rendered.text, "Explain main.rs in /tmp/work");
        assert!(rendered.missing.is_empty());
    }

    #[test]
    fn a_prompt_file_loads_with_its_category() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("git.toml");
        std::fs::write(
            &path,
            r#"
[meta]
name = "Git"
category = "git"

[[prompt]]
id = "git.commit-message"
title = "Write a commit message"
about = "Summarise the staged changes"
text = "Write a commit message for the staged changes in {{cwd}}."

[[prompt]]
id = "git.review-diff"
title = "Review the diff"
text = "Review this diff and point out the risky parts."
"#,
        )
        .unwrap();
        let catalogue = Catalogue::load(&[(path, true)]);
        assert!(
            catalogue.problems().is_empty(),
            "{:?}",
            catalogue.problems()
        );
        assert_eq!(catalogue.entries().len(), 2);
        let entry = catalogue.get("git.commit-message").unwrap();
        assert_eq!(entry.category, Category::Git);
        assert!(entry.builtin);
        assert_eq!(entry.variables(), vec!["cwd"]);
        // Sorted by title inside the category.
        assert_eq!(catalogue.entries()[0].prompt.id, "git.review-diff");
    }

    #[test]
    fn a_broken_file_is_reported_and_the_others_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("ok.toml");
        std::fs::write(
            &good,
            "[meta]\ncategory = 'files'\n[[prompt]]\nid = 'a'\ntitle = 'A'\ntext = 'do it'\n",
        )
        .unwrap();
        let broken = dir.path().join("broken.toml");
        std::fs::write(&broken, "this is not toml [[[").unwrap();
        let empty = dir.path().join("empty.toml");
        std::fs::write(
            &empty,
            "[meta]\ncategory = 'files'\n[[prompt]]\nid = 'x'\ntext = ''\n",
        )
        .unwrap();
        let catalogue = Catalogue::load(&[(good, false), (broken, false), (empty, false)]);
        assert_eq!(catalogue.entries().len(), 1, "the good file still loads");
        assert_eq!(catalogue.problems().len(), 2, "{:?}", catalogue.problems());
        assert!(
            catalogue
                .problems()
                .iter()
                .any(|p| p.contains("broken.toml")),
            "{:?}",
            catalogue.problems()
        );
        assert!(
            catalogue
                .problems()
                .iter()
                .any(|p| p.contains("has no text")),
            "{:?}",
            catalogue.problems()
        );
    }

    #[test]
    fn built_in_prompts_come_before_the_users_own() {
        let mut catalogue = Catalogue::default();
        let mut builtin = entry("builtin", Category::Files, "x");
        builtin.builtin = true;
        catalogue.add(entry("custom", Category::Files, "y"));
        catalogue.add(builtin);
        assert_eq!(catalogue.entries()[0].prompt.id, "builtin");
    }

    #[test]
    fn adding_the_same_id_replaces_the_old_prompt() {
        let mut catalogue = Catalogue::default();
        catalogue.add(entry("one", Category::General, "first"));
        catalogue.add(entry("one", Category::General, "second"));
        assert_eq!(catalogue.entries().len(), 1);
        assert_eq!(catalogue.get("one").unwrap().prompt.text, "second");
        assert!(catalogue.remove("one"));
        assert!(!catalogue.remove("one"));
        assert!(catalogue.entries().is_empty());
    }

    #[test]
    fn the_search_looks_where_a_person_would_look() {
        let mut catalogue = Catalogue::default();
        let mut a = entry("a", Category::Files, "explain this file");
        a.prompt.title = "Explain a file".to_string();
        a.prompt.about = "plain language".to_string();
        catalogue.add(a);
        catalogue.add(entry("b", Category::Git, "write a commit message"));
        assert_eq!(catalogue.search("explain").len(), 1);
        assert_eq!(catalogue.search("COMMIT").len(), 1, "case-insensitive");
        assert_eq!(
            catalogue.search("plain language").len(),
            1,
            "the about line"
        );
        assert_eq!(
            catalogue.search("").len(),
            2,
            "an empty search shows everything"
        );
        assert!(catalogue.search("nothing like this").is_empty());
    }

    #[test]
    fn the_category_filter_returns_only_its_own() {
        let mut catalogue = Catalogue::default();
        catalogue.add(entry("f", Category::Files, "x"));
        catalogue.add(entry("g", Category::Git, "y"));
        catalogue.add(entry("g2", Category::Git, "z"));
        assert_eq!(catalogue.in_category(Category::Git).len(), 2);
        assert_eq!(catalogue.in_category(Category::Files).len(), 1);
        assert!(catalogue.in_category(Category::Remote).is_empty());
    }

    #[test]
    fn the_user_directory_sits_under_the_configuration() {
        let dir = Path::new("/home/user/.config/rcommander");
        assert_eq!(user_dir(dir), dir.join("prompts"));
    }

    #[test]
    fn a_user_prompt_with_the_same_id_replaces_the_built_in_one() {
        let dir = tempfile::tempdir().unwrap();
        let builtin = dir.path().join("files.toml");
        std::fs::write(
            &builtin,
            "[meta]\ncategory = 'files'\n[[prompt]]\nid = 'files.explain'\ntitle = 'Explain'\ntext = 'shipped text'\n",
        )
        .unwrap();
        let mine = dir.path().join("mine.toml");
        std::fs::write(
            &mine,
            "[meta]\ncategory = 'files'\n[[prompt]]\nid = 'files.explain'\ntitle = 'My explain'\ntext = 'my text'\n",
        )
        .unwrap();
        let catalogue = Catalogue::load(&[(builtin, true), (mine, false)]);
        // One entry, and it is the user's.
        assert_eq!(catalogue.entries().len(), 1);
        let entry = catalogue.get("files.explain").unwrap();
        assert_eq!(entry.prompt.text, "my text");
        assert!(!entry.builtin);
        // The shipped text survives as the reset target.
        assert!(catalogue.is_overridden("files.explain"));
        let (effective, original) = catalogue.origin_pair("files.explain");
        assert_eq!(effective.unwrap().prompt.text, "my text");
        assert_eq!(original.unwrap().prompt.text, "shipped text");
        // A built-in that nobody overrode is not "overridden".
        assert!(!catalogue.is_overridden("nothing.here"));
    }

    #[test]
    fn an_ordinary_user_prompt_is_not_an_override() {
        let dir = tempfile::tempdir().unwrap();
        let mine = dir.path().join("mine.toml");
        std::fs::write(
            &mine,
            "[meta]\ncategory = 'general'\n[[prompt]]\nid = 'mine.one'\ntitle = 'Mine'\ntext = 'text'\n",
        )
        .unwrap();
        let catalogue = Catalogue::load(&[(mine, false)]);
        assert!(!catalogue.is_overridden("mine.one"));
        assert!(catalogue.original("mine.one").is_none());
    }

    #[test]
    fn a_prompt_file_round_trips_through_toml() {
        let file = PromptFile {
            meta: FileMeta {
                name: "Mine".to_string(),
                category: "diagnostics".to_string(),
            },
            prompts: vec![Prompt {
                id: "why".to_string(),
                title: "Why did it stop?".to_string(),
                about: String::new(),
                text: "Look at {{log}} and tell me why.".to_string(),
            }],
        };
        let text = toml::to_string_pretty(&file).unwrap();
        let back: PromptFile = toml::from_str(&text).unwrap();
        assert_eq!(back, file);
        assert!(text.contains("category = \"diagnostics\""), "{text}");
    }
}

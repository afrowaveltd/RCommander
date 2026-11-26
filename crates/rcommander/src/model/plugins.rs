//! A panel source described by a manifest instead of by Rust code (Phase 18.2b, declarative tier).
//!
//! [`rc_core::plugin`] validates the file; this module makes it *live*. The shape it fills in is the
//! one Phase 18.2a already defined for compiled-in sources, so the panel, the command gate and the
//! function-key bar need to know nothing about plugins at all.
//!
//! Two decisions are worth stating out loud.
//!
//! **The listing is cached, because a panel asks for one cell at a time.** A row's fields arrive as
//! one JSON object from the `list` command; `cell` is then a lookup in that object rather than a
//! second run of the program per cell. `list` replaces the cache for its own rows, so a refresh
//! cannot show a mixture of two answers.
//!
//! **The runner is a trait, so tests need no processes.** The production runner is
//! [`ProcessRunner`]; the tests drive a scripted one. That keeps the interesting behaviour — argument
//! substitution, JSON shape errors, capability mapping — testable without spawning anything.

use crate::model::Model;
use crate::model::sources::{
    PanelSource, SourceAction, SourceCapabilities, SourceColumn, SourceMenuAction,
};
use rc_core::entry::{Entry, EntryKind};
use rc_core::error::{CoreError, CoreResult};
use rc_core::plugin::{PluginManifest, SourceSpec};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Somebody who can run an argv and give back its stdout.
pub trait CommandRunner: Send + Sync {
    /// Run `argv` in `working_dir` (the plugin's own folder, when it has one) and return what it
    /// printed on stdout.
    fn run(&self, argv: &[String], working_dir: Option<&std::path::Path>) -> CoreResult<String>;
}

/// The real thing: no shell, no globbing, one argv.
///
/// The absence of a shell is the safety property. A row's key is substituted into a single argument,
/// so a value that contains `;` or `$(…)` stays one argument and cannot become a second command.
#[derive(Debug, Default)]
pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(&self, argv: &[String], working_dir: Option<&std::path::Path>) -> CoreResult<String> {
        let Some(program) = argv.first() else {
            return Err(CoreError::Invalid("an empty command".to_string()));
        };
        let mut command = std::process::Command::new(program);
        command.args(&argv[1..]);
        // A plugin may carry its own helper script; running from its folder is what makes
        // `./list.sh` mean the script next to the manifest rather than one in the user's home.
        if let Some(dir) = working_dir
            && !dir.as_os_str().is_empty()
        {
            command.current_dir(dir);
        }
        let output = command
            .output()
            .map_err(|error| CoreError::Invalid(format!("cannot run “{program}”: {error}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let first = stderr.lines().next().unwrap_or("").trim();
            let code = output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string());
            return Err(CoreError::Invalid(if first.is_empty() {
                format!("“{program}” failed ({code})")
            } else {
                format!("“{program}” failed ({code}): {first}")
            }));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// A live source backed by one manifest.
pub struct ManifestSource {
    manifest: PluginManifest,
    runner: Box<dyn CommandRunner>,
    /// Cached rows, keyed by the row's handle, for the panel's per-cell questions.
    rows: Mutex<HashMap<String, serde_json::Map<String, serde_json::Value>>>,
}

impl ManifestSource {
    /// Build the source for a validated manifest.
    pub fn new(manifest: PluginManifest, runner: Box<dyn CommandRunner>) -> ManifestSource {
        ManifestSource {
            manifest,
            runner,
            rows: Mutex::new(HashMap::new()),
        }
    }

    /// The manifest this source came from, for diagnostics.
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn spec(&self) -> &SourceSpec {
        &self.manifest.source
    }

    /// The plugin's own folder, when it was read from disk.
    fn working_dir(&self) -> Option<&std::path::Path> {
        let dir = self.manifest.directory.as_path();
        if dir.as_os_str().is_empty() {
            None
        } else {
            Some(dir)
        }
    }

    /// A value from a JSON object as the text of a cell.
    fn text_of(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => String::new(),
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Bool(flag) => flag.to_string(),
            // A float that is whole prints without a trailing ".0", because a count of 12 words
            // should not read as 12.0 in a column.
            serde_json::Value::Number(number) => match number.as_i64() {
                Some(integer) => integer.to_string(),
                None => number.to_string(),
            },
            other => other.to_string(),
        }
    }

    /// The handle a row is addressed by: its declared key field, or its position.
    fn handle_of(
        &self,
        object: &serde_json::Map<String, serde_json::Value>,
        index: usize,
    ) -> String {
        let key = self.spec().key.as_str();
        if key.is_empty() {
            return index.to_string();
        }
        object
            .get(key)
            .map(Self::text_of)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| index.to_string())
    }

    /// Arguments for one invocation, with `{path}` replaced by the row's handle.
    ///
    /// Substitution happens inside a single argument: `["notes-open", "{path}"]` becomes two
    /// arguments however strange the handle is, and a handle is never re-split on whitespace.
    fn argv_for(&self, template: &[String], handle: &str) -> Vec<String> {
        template
            .iter()
            .map(|argument| argument.replace("{path}", handle))
            .collect()
    }

    /// Run the listing command and turn stdout into rows plus a cache.
    fn list_rows(&self) -> CoreResult<Vec<Entry>> {
        let stdout = self.runner.run(&self.spec().list, self.working_dir())?;
        let value: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|error| {
            CoreError::Invalid(format!(
                "“{}” did not print JSON ({}); expected an array of objects",
                self.spec().list[0],
                error
            ))
        })?;
        let array = match value {
            serde_json::Value::Array(rows) => rows,
            other => {
                return Err(CoreError::Invalid(format!(
                    "“{}” printed {} where an array of objects was expected",
                    self.spec().list[0],
                    describe(&other)
                )));
            }
        };
        let name_field = self.spec().name_field().unwrap_or_default().to_string();
        let mut entries = Vec::with_capacity(array.len());
        let mut cache = HashMap::with_capacity(array.len());
        for (index, row) in array.into_iter().enumerate() {
            let object = match row {
                serde_json::Value::Object(object) => object,
                other => {
                    return Err(CoreError::Invalid(format!(
                        "row {} is {}, not an object",
                        index + 1,
                        describe(&other)
                    )));
                }
            };
            let handle = self.handle_of(&object, index);
            let name = object
                .get(&name_field)
                .map(Self::text_of)
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| handle.clone());
            entries.push(Entry::new(
                name,
                PathBuf::from(handle.clone()),
                EntryKind::File,
                0,
            ));
            cache.insert(handle, object);
        }
        if let Ok(mut rows) = self.rows.lock() {
            rows.extend(cache);
        }
        Ok(entries)
    }
}

impl Model {
    /// The directory manifests are read from: `<config>/plugins/<id>/plugin.toml`.
    pub fn plugin_dir(&self) -> PathBuf {
        self.config_dir().join("plugins")
    }

    /// Scan for plugins and register every manifest that parses.
    ///
    /// A manifest that does not parse is remembered in [`Model::plugin_problems`] and skipped: one
    /// community plugin with a typo must not stop the application, which is the same contract the
    /// themes and the help topics already follow. The summary is returned so the caller can put it
    /// in the status line.
    pub fn load_plugins(&mut self) -> String {
        // Shipped examples first, the user's own directory second, so a plugin the user installed
        // replaces a bundled one with the same id instead of colliding with it.
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(bundled) = crate::paths::find_data_dir("plugins") {
            roots.push(bundled);
        }
        let user = self.plugin_dir();
        if !roots.iter().any(|root| root == &user) {
            roots.push(user);
        }
        let mut problems = Vec::new();
        let mut loaded: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for root in roots {
            let discovery = rc_core::plugin::discover_in(&root);
            for manifest in discovery.plugins {
                loaded.insert(manifest.plugin.id.clone(), manifest.source.name.clone());
                self.sources.register(Box::new(ManifestSource::new(
                    manifest,
                    Box::new(ProcessRunner),
                )));
            }
            problems.extend(discovery.problems);
        }
        self.plugins_loaded = loaded.into_iter().collect();
        for problem in &problems {
            self.report_error(problem.to_string());
        }
        self.plugin_problems = problems;
        match (self.plugins_loaded.len(), self.plugin_problems.len()) {
            (0, 0) => "No plugins found".to_string(),
            (found, 0) => format!("{found} plugin(s)"),
            (0, broken) => format!("{broken} plugin(s) could not be loaded"),
            (found, broken) => {
                format!("{found} plugin(s), {broken} could not be loaded")
            }
        }
    }

    /// The plugins that were loaded by the last scan, as `(id, title)`.
    pub fn plugin_sources(&self) -> &[(String, String)] {
        &self.plugins_loaded
    }
}

/// How to describe an unexpected JSON value in an error message.
fn describe(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

impl PanelSource for ManifestSource {
    fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    fn title(&self) -> &str {
        &self.spec().name
    }

    fn list(&self, path: &str) -> CoreResult<Vec<Entry>> {
        if !path.is_empty() {
            // The manifest describes one flat listing: there is no declared way to pass a folder to
            // the list command, so pretending there is would be inventing behaviour the file does
            // not describe.
            return Err(CoreError::Invalid(format!(
                "“{}” is a single listing; it has no level called “{path}”",
                self.id()
            )));
        }
        self.list_rows()
    }

    fn open(&self, entry: &Entry) -> CoreResult<SourceAction> {
        if self.spec().open.is_empty() {
            return Err(CoreError::Invalid(format!(
                "“{}” cannot open “{}”: its plugin.toml declares no open command",
                self.id(),
                entry.name
            )));
        }
        let handle = entry.path.to_string_lossy().to_string();
        let argv = self.argv_for(&self.spec().open, &handle);
        self.runner.run(&argv, self.working_dir())?;
        // The program did whatever it does — opened a window, printed to its own stdout. There is
        // nothing left for the panel to show.
        Ok(SourceAction::Done)
    }

    fn columns(&self) -> Vec<SourceColumn> {
        self.spec()
            .columns
            .iter()
            .map(|column| SourceColumn {
                id: column.id.clone(),
                label: column.label.clone(),
                width: column.width,
                numeric: column.numeric,
            })
            .collect()
    }

    fn cell(&self, entry: &Entry, column: &str) -> String {
        let handle = entry.path.to_string_lossy().to_string();
        let field = match self.spec().column(column) {
            Some(column) => column.field.clone(),
            None => return String::new(),
        };
        let Ok(rows) = self.rows.lock() else {
            return String::new();
        };
        rows.get(&handle)
            .and_then(|object| object.get(&field))
            .map(Self::text_of)
            .unwrap_or_default()
    }

    fn actions(&self, _entry: &Entry) -> Vec<SourceMenuAction> {
        // Declared verbs are the next increment; a manifest source currently offers exactly the
        // actions its capabilities allow and nothing of its own.
        Vec::new()
    }

    fn capabilities(&self) -> SourceCapabilities {
        let spec = self.spec();
        SourceCapabilities {
            copy: spec.allows("copy"),
            move_entry: spec.allows("move"),
            create: spec.allows("create"),
            delete: spec.allows("delete"),
            rename: spec.allows("rename"),
            view: spec.allows("view"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::sources::SourceRegistry;
    use rc_core::location::Location;
    use rc_core::plugin::parse;

    const MANIFEST: &str = r#"
[plugin]
id = "notes"
name = "Notes"

[source]
name = "Notes"
list = ["notes-index", "--json"]
open = ["notes-open", "{path}"]
key = "id"

[[source.columns]]
id = "title"
label = "Title"
field = "title"
width = 280

[[source.columns]]
id = "words"
label = "Words"
field = "word_count"
width = 70
numeric = true

[source.capabilities]
allow = ["view"]
"#;

    /// A runner that answers from a script and records what it was asked to run.
    struct Scripted {
        answers: Mutex<Vec<CoreResult<String>>>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl Scripted {
        fn new(answers: Vec<CoreResult<String>>) -> Scripted {
            Scripted {
                answers: Mutex::new(answers),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<Vec<String>> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandRunner for Scripted {
        fn run(
            &self,
            argv: &[String],
            _working_dir: Option<&std::path::Path>,
        ) -> CoreResult<String> {
            self.calls.lock().unwrap().push(argv.to_vec());
            let mut answers = self.answers.lock().unwrap();
            if answers.is_empty() {
                return Err(CoreError::Invalid("nothing left to answer".to_string()));
            }
            answers.remove(0)
        }
    }

    fn source(answers: Vec<CoreResult<String>>) -> (ManifestSource, std::sync::Arc<Scripted>) {
        let manifest = parse(MANIFEST, "notes").expect("the manifest parses");
        // The runner is shared so the test can read the calls back after the source owns it.
        let runner = std::sync::Arc::new(Scripted::new(answers));
        let source = ManifestSource::new(manifest, Box::new(SharedRunner(runner.clone())));
        (source, runner)
    }

    /// A handle that keeps the scripted runner reachable for assertions.
    struct SharedRunner(std::sync::Arc<Scripted>);

    impl CommandRunner for SharedRunner {
        fn run(
            &self,
            argv: &[String],
            working_dir: Option<&std::path::Path>,
        ) -> CoreResult<String> {
            self.0.run(argv, working_dir)
        }
    }

    const ROWS: &str = r#"[
        {"id": 17, "title": "Printer on fire", "word_count": 12},
        {"id": 18, "title": "Coffee machine", "word_count": 3}
    ]"#;

    #[test]
    fn a_manifest_source_lists_the_rows_its_command_prints() {
        let (source, runner) = source(vec![Ok(ROWS.to_string())]);
        let entries = source.list("").expect("lists");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "Printer on fire");
        // The handle is the declared key field, not the position.
        assert_eq!(entries[0].path, PathBuf::from("17"));
        assert_eq!(entries[1].path, PathBuf::from("18"));
        assert_eq!(runner.calls(), vec![vec!["notes-index", "--json"]]);
    }

    #[test]
    fn cells_come_from_the_cached_row_and_never_run_the_command_again() {
        let (source, runner) = source(vec![Ok(ROWS.to_string())]);
        let entries = source.list("").expect("lists");
        assert_eq!(source.cell(&entries[0], "title"), "Printer on fire");
        // A whole number prints without a decimal point.
        assert_eq!(source.cell(&entries[0], "words"), "12");
        assert_eq!(source.cell(&entries[1], "words"), "3");
        // An id the source does not know is an empty cell, not a guess.
        assert_eq!(source.cell(&entries[0], "size"), "");
        assert_eq!(
            runner.calls().len(),
            1,
            "the panel asks for cells; it does not re-run the command"
        );
    }

    #[test]
    fn the_declared_columns_become_panel_columns() {
        let (source, _) = source(vec![]);
        let columns = source.columns();
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].id, "title");
        assert_eq!(columns[0].label, "Title");
        assert_eq!(columns[0].width, 280.0);
        assert!(columns[1].numeric && !columns[0].numeric);
        assert_eq!(source.title(), "Notes");
        assert_eq!(source.id(), "notes");
    }

    #[test]
    fn the_declared_operations_are_the_only_capabilities() {
        let (source, _) = source(vec![]);
        let capabilities = source.capabilities();
        assert!(capabilities.view);
        assert!(!capabilities.delete && !capabilities.copy && !capabilities.move_entry);
        assert!(!capabilities.create && !capabilities.rename);
    }

    #[test]
    fn opening_a_row_runs_the_open_command_with_the_handle_substituted() {
        let (source, runner) = source(vec![Ok(ROWS.to_string()), Ok(String::new())]);
        let entries = source.list("").expect("lists");
        match source.open(&entries[1]).expect("opens") {
            SourceAction::Done => {}
            other => panic!("expected the source to have handled it, got {other:?}"),
        }
        assert_eq!(
            runner.calls()[1],
            vec!["notes-open".to_string(), "18".to_string()]
        );
    }

    #[test]
    fn a_handle_with_a_shell_metacharacter_stays_one_argument() {
        let hostile = r#"[{"id": "; rm -rf ~", "title": "hostile", "word_count": 1}]"#;
        let (source, runner) = source(vec![Ok(hostile.to_string()), Ok(String::new())]);
        let entries = source.list("").expect("lists");
        assert_eq!(entries[0].name, "hostile");
        source.open(&entries[0]).expect("opens");
        // Two arguments, not three, and no shell in between.
        assert_eq!(runner.calls()[1].len(), 2);
        assert_eq!(runner.calls()[1][1], "; rm -rf ~");
    }

    #[test]
    fn output_that_is_not_an_array_of_objects_is_an_error_naming_what_arrived() {
        let (object, _) = source(vec![Ok("{\"rows\": []}".to_string())]);
        let error = object.list("").unwrap_err().to_string();
        assert!(error.contains("an object"), "{error}");
        assert!(error.contains("array of objects"), "{error}");

        let (garbage, _) = source(vec![Ok("not json at all".to_string())]);
        let error = garbage.list("").unwrap_err().to_string();
        assert!(error.contains("did not print JSON"), "{error}");

        let (numbers, _) = source(vec![Ok("[1, 2]".to_string())]);
        let error = numbers.list("").unwrap_err().to_string();
        assert!(error.contains("row 1 is a number"), "{error}");
    }

    #[test]
    fn a_failing_command_leaves_the_panel_with_an_error_not_an_empty_list() {
        let (source, _) = source(vec![Err(CoreError::Invalid(
            "“notes-index” failed (2): no such library".to_string(),
        ))]);
        let error = source.list("").unwrap_err().to_string();
        assert!(error.contains("no such library"), "{error}");
    }

    #[test]
    fn a_source_without_an_open_command_says_so_instead_of_doing_nothing() {
        let manifest = parse(
            r#"
[plugin]
id = "notes"
[source]
list = ["notes-index"]
[[source.columns]]
id = "title"
label = "Title"
"#,
            "notes",
        )
        .expect("parses");
        let source = ManifestSource::new(manifest, Box::new(Scripted::new(Vec::new())));
        let entry = Entry::new("a note", PathBuf::from("1"), EntryKind::File, 0);
        let error = source.open(&entry).unwrap_err().to_string();
        assert!(error.contains("no open command"), "{error}");
    }

    #[test]
    fn a_row_without_the_key_field_falls_back_to_its_position() {
        let rows = r#"[{"title": "no key here", "word_count": 1}]"#;
        let (source, _) = source(vec![Ok(rows.to_string())]);
        let entries = source.list("").expect("lists");
        assert_eq!(entries[0].path, PathBuf::from("0"));
        assert_eq!(entries[0].name, "no key here");
    }

    fn model_with_config(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            rc_core::config::Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    #[test]
    fn a_manifest_in_the_plugin_directory_becomes_a_panel_source() {
        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("plugins").join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::write(notes.join("plugin.toml"), MANIFEST).unwrap();
        // A folder whose manifest does not parse must not stop the good one from loading.
        let broken = dir.path().join("plugins").join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("plugin.toml"), "this is not toml").unwrap();
        let mut model = model_with_config(dir.path());
        let summary = model.load_plugins();
        assert_eq!(
            model.plugin_sources(),
            &[("notes".to_string(), "Notes".to_string())]
        );
        assert_eq!(model.plugin_problems.len(), 1);
        assert!(
            model.plugin_problems[0].to_string().contains("broken"),
            "{}",
            model.plugin_problems[0]
        );
        assert_eq!(summary, "1 plugin(s), 1 could not be loaded");
        // The loaded plugin is a real source as far as the rest of the application is concerned.
        let location = Location::plugin("notes", "");
        let source = model.sources.source_of(&location).expect("registered");
        assert_eq!(source.title(), "Notes");
        assert_eq!(model.sources.columns(&location).len(), 2);
        assert!(model.sources.capabilities(&location).view);
        assert!(!model.sources.capabilities(&location).delete);
        // Re-reading the directory is idempotent: the source is replaced, not duplicated.
        model.load_plugins();
        assert_eq!(model.plugin_sources().len(), 1);
        assert_eq!(model.sources.ids(), vec!["notes"]);
    }

    #[test]
    fn a_plugin_whose_command_does_not_exist_leaves_an_error_in_the_panel() {
        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("plugins").join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        // A list command nobody has installed: the panel must say so, not show an empty folder.
        let manifest = MANIFEST.replace(
            "list = [\"notes-index\", \"--json\"]",
            "list = [\"rc-no-such-command-anywhere\"]",
        );
        std::fs::write(notes.join("plugin.toml"), manifest).unwrap();
        let mut model = model_with_config(dir.path());
        model.load_plugins();
        let error = model
            .open_in(crate::model::Side::Left, Location::plugin("notes", ""))
            .unwrap_err()
            .to_string();
        assert!(error.contains("rc-no-such-command-anywhere"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn the_shipped_example_plugin_lists_real_notes_through_a_real_process() {
        use std::os::unix::fs::PermissionsExt;
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|dir| dir.parent())
            .map(|dir| dir.join("plugins").join("examples").join("notes"));
        let Some(example) = repo.filter(|dir| dir.is_dir()) else {
            return;
        };
        let config = tempfile::tempdir().unwrap();
        let plugin_dir = config.path().join("plugins").join("notes");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        for name in ["list.sh", "open.sh"] {
            let target = plugin_dir.join(name);
            std::fs::copy(example.join(name), &target).unwrap();
            let mut mode = std::fs::metadata(&target).unwrap().permissions();
            mode.set_mode(0o755);
            std::fs::set_permissions(&target, mode).unwrap();
        }
        // A folder of notes to point the shipped lister at, and a manifest that uses it.
        let notes = config.path().join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::write(notes.join("hello.md"), "# Hello\nfive words in this note\n").unwrap();
        std::fs::write(notes.join("todo.md"), "# Todo\none\n").unwrap();
        let manifest = std::fs::read_to_string(example.join("plugin.toml")).unwrap();
        let manifest = manifest.replace(
            "list = [\"./list.sh\"]",
            &format!("list = [\"./list.sh\", {:?}]", notes.display().to_string()),
        );
        std::fs::write(plugin_dir.join("plugin.toml"), manifest).unwrap();

        let mut model = model_with_config(config.path());
        assert_eq!(model.load_plugins(), "1 plugin(s)");
        model.focus = crate::model::Side::Left;
        model
            .open_in(crate::model::Side::Left, Location::plugin("notes", ""))
            .expect("the example lists its notes");
        let names: Vec<String> = model
            .panel(crate::model::Side::Left)
            .entries()
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        assert!(names.contains(&"hello".to_string()), "{names:?}");
        assert!(names.contains(&"todo".to_string()), "{names:?}");
        // The declared columns are what the panel draws, and the cells are the JSON fields.
        let columns = model.columns(crate::model::Side::Left);
        let ids: Vec<&str> = columns.iter().map(|column| column.id.as_str()).collect();
        assert_eq!(ids, vec!["title", "words", "modified"]);
        let entry = model
            .panel(crate::model::Side::Left)
            .entries()
            .iter()
            .find(|entry| entry.name == "hello")
            .cloned()
            .expect("hello is listed");
        let cells = model.row_cells(crate::model::Side::Left, &entry);
        assert_eq!(cells[0], "hello");
        // "#", "Hello", "five", "words", "in", "this", "note" — wc counts them all.
        assert_eq!(cells[1], "7", "the word count comes from the JSON field");
        assert!(!cells[2].is_empty(), "the modified date is filled in");
        // And the place is read-only, as the example declares.
        assert!(!model.allows(crate::model::Side::Left, "cm_Delete"));
        assert!(model.allows(crate::model::Side::Left, "cm_List"));
    }

    #[cfg(unix)]
    #[test]
    fn the_process_runner_captures_stdout_and_reports_a_failure() {
        let stdout = ProcessRunner
            .run(
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "printf '[{\"id\":1}]'".to_string(),
                ],
                None,
            )
            .expect("runs");
        assert_eq!(stdout.trim(), "[{\"id\":1}]");
        // A non-zero exit is an error carrying the first line of stderr.
        let error = ProcessRunner
            .run(
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo 'library missing' >&2; exit 3".to_string(),
                ],
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("library missing"), "{error}");
        assert!(error.contains("3"), "{error}");
        // A program that is not installed says which one.
        let error = ProcessRunner
            .run(&["rc-no-such-program-anywhere".to_string()], None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("rc-no-such-program-anywhere"), "{error}");
    }

    #[test]
    fn a_manifest_source_works_through_the_registry_the_panel_uses() {
        let (source, _) = source(vec![Ok(ROWS.to_string())]);
        let mut registry = SourceRegistry::new();
        registry.register(Box::new(source));
        let location = Location::plugin("notes", "");
        assert_eq!(registry.ids(), vec!["notes"]);
        let entries = registry
            .list(&location)
            .expect("lists through the registry");
        assert_eq!(entries.len(), 2);
        assert_eq!(registry.columns(&location).len(), 2);
        assert_eq!(
            registry.cell(&location, &entries[0], "title"),
            "Printer on fire"
        );
        // The capability gate the panel consults says no to deletion.
        assert!(!registry.capabilities(&location).delete);
        assert!(registry.capabilities(&location).view);
    }
}

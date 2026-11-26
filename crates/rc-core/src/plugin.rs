//! Plugin manifests: a panel source described by data (Phase 18.1 and the declarative tier of 18.2b).
//!
//! A `plugins/<id>/plugin.toml` file can turn a program into a panel: what to run to list the rows,
//! what to run when one is opened, which JSON field goes in which column, and which file operations
//! the place supports. No Rust code per plugin, and nothing an attacker who controls a *row* can
//! reach the arguments of a *command*.
//!
//! Three properties are deliberate and tested:
//!
//! 1. **A broken manifest is reported, never fatal.** Discovery returns the good ones plus a list of
//!    problems, so one community plugin with a typo cannot stop the application — the same contract
//!    the themes and the help topics already have.
//! 2. **Capabilities default to nothing.** A missing table means a listing you can read; nothing can
//!    delete, move or create. Declaring an operation that does not exist is a manifest error naming
//!    the operation, not a silently ignored line.
//! 3. **Arguments are fixed arrays with one substitution.** There is no shell, no globbing and no
//!    `eval`, so a row whose title is `; rm -rf ~` is a title.

use crate::error::{CoreError, CoreResult};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// What a manifest may declare about the file operations of its place.
///
/// These are names, not flags, because the manifest is data: the *application* maps them onto its
/// own capability set. An unknown name is an error with the name in it, so a manifest written for a
/// future version says so instead of quietly losing the permission.
pub const OPERATIONS: [&str; 6] = ["copy", "move", "create", "delete", "rename", "view"];

/// One column of the source's table.
#[derive(Clone, Debug, PartialEq)]
pub struct ManifestColumn {
    /// Stable id inside this source.
    pub id: String,
    /// Header text.
    pub label: String,
    /// The JSON field of a row that fills this cell.
    pub field: String,
    /// Preferred width in points; the first column flexes.
    pub width: f32,
    /// Right-align the cell.
    pub numeric: bool,
}

/// The declared shape of one source.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceSpec {
    /// Label shown in the path bar and the tab.
    pub name: String,
    /// Argv that prints a JSON array of objects. Never empty.
    pub list: Vec<String>,
    /// Argv that opens one row; `{path}` is replaced by the row's key. Empty means "cannot open".
    pub open: Vec<String>,
    /// Which JSON field identifies a row. Empty means the row's index as a string.
    pub key: String,
    /// The table's columns; at least one, and the first is the row's name.
    pub columns: Vec<ManifestColumn>,
    /// Declared operations, validated against [`OPERATIONS`].
    pub operations: Vec<String>,
}

impl SourceSpec {
    /// The operations this place actually allows, as a set for the adapter to map.
    pub fn allows(&self, operation: &str) -> bool {
        self.operations
            .iter()
            .any(|declared| declared.eq_ignore_ascii_case(operation))
    }

    /// The column a cell asks for, by id.
    pub fn column(&self, id: &str) -> Option<&ManifestColumn> {
        self.columns.iter().find(|column| column.id == id)
    }

    /// The field whose value names a row.
    pub fn name_field(&self) -> Option<&str> {
        self.columns.first().map(|column| column.field.as_str())
    }
}

/// Identity and provenance of one plugin.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginInfo {
    /// Also the URL scheme, and must equal the directory name.
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
}

/// A parsed `plugin.toml`.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
    pub source: SourceSpec,
    /// The folder the manifest was read from, so a plugin can carry its own helper script and the
    /// commands run with it as the working directory. Empty when the manifest was only parsed.
    pub directory: PathBuf,
}

/// Why a manifest was refused.
///
/// Every variant names what is wrong, because the message ends up in the status line: a user who
/// wrote `capabilities.allow = ["remove"]` needs to read "remove is not an operation".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestError {
    /// The file could not be read.
    Unreadable(String),
    /// The TOML did not parse.
    Syntax(String),
    /// The id is empty, has an illegal character, or does not match the directory name.
    BadId(String),
    /// The id is used by another plugin.
    Duplicate(String),
    /// `source.list` is missing or empty.
    NoListCommand,
    /// No columns, or a column with an empty id/label/field.
    NoColumns(String),
    /// Two columns share an id.
    DuplicateColumn(String),
    /// A column id that is not usable as a key (`ext`, `size` and `date` are reserved by the panel).
    ReservedColumn(String),
    /// An operation name nobody knows.
    UnknownOperation(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Unreadable(why) => write!(f, "cannot be read: {why}"),
            ManifestError::Syntax(why) => write!(f, "is not valid TOML: {why}"),
            ManifestError::BadId(why) => write!(f, "has an unusable id: {why}"),
            ManifestError::Duplicate(id) => write!(f, "reuses the id “{id}”"),
            ManifestError::NoListCommand => {
                write!(f, "must declare source.list: the argv that prints its rows")
            }
            ManifestError::NoColumns(why) => write!(f, "has no usable columns: {why}"),
            ManifestError::DuplicateColumn(id) => write!(f, "declares the column “{id}” twice"),
            ManifestError::ReservedColumn(id) => write!(
                f,
                "uses “{id}” as a column id, which the panel already uses for a file column"
            ),
            ManifestError::UnknownOperation(name) => write!(
                f,
                "declares the operation “{name}”, which does not exist ({})",
                OPERATIONS.join(", ")
            ),
        }
    }
}

/// Column ids the panel itself formats, so a source may not take them over.
///
/// Reusing `size` or `date` would make the panel draw the filesystem's answer over the source's
/// cell in some views, which is exactly the sort of quiet inconsistency this project refuses.
const RESERVED_COLUMNS: [&str; 8] = [
    "name", "ext", "size", "date", "attrs", "comment", "mode", "link",
];

/// The on-disk shape. Kept separate from the validated model so the parse is a pure function.
#[derive(Debug, Default, serde::Deserialize)]
struct RawManifest {
    plugin: RawPlugin,
    source: RawSource,
}

#[derive(Debug, Default, serde::Deserialize)]
struct RawPlugin {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Default, serde::Deserialize)]
struct RawSource {
    #[serde(default)]
    name: String,
    #[serde(default)]
    list: Vec<String>,
    #[serde(default)]
    open: Vec<String>,
    #[serde(default)]
    key: String,
    #[serde(default)]
    columns: Vec<RawColumn>,
    #[serde(default)]
    capabilities: RawCapabilities,
}

#[derive(Debug, Default, serde::Deserialize)]
struct RawColumn {
    #[serde(default)]
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    field: String,
    #[serde(default)]
    width: f32,
    #[serde(default)]
    numeric: bool,
}

#[derive(Debug, Default, serde::Deserialize)]
struct RawCapabilities {
    #[serde(default)]
    allow: Vec<String>,
}

/// Whether a plugin id is usable as a URL scheme: lower case, digits, `-` and `_`, starting with a
/// letter. The same shape as a locale or theme id, and safe in a `scheme://path` string.
pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    id.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Parse and validate one manifest.
///
/// `directory` is the name of the folder the manifest lives in; when it is not empty the id has to
/// match it, so a copied folder cannot impersonate another plugin's scheme.
pub fn parse(text: &str, directory: &str) -> Result<PluginManifest, ManifestError> {
    let raw: RawManifest =
        toml::from_str(text).map_err(|error| ManifestError::Syntax(error.to_string()))?;
    let id = raw.plugin.id.trim().to_string();
    if !valid_id(&id) {
        return Err(ManifestError::BadId(format!(
            "“{id}” — use lower-case letters, digits, “-” and “_”, starting with a letter"
        )));
    }
    if !directory.is_empty() && !id.eq_ignore_ascii_case(directory) {
        return Err(ManifestError::BadId(format!(
            "“{id}” does not match the folder “{directory}”"
        )));
    }
    if raw.source.list.is_empty() || raw.source.list[0].trim().is_empty() {
        return Err(ManifestError::NoListCommand);
    }
    if raw.source.columns.is_empty() {
        return Err(ManifestError::NoColumns("it declares none".to_string()));
    }
    let mut columns = Vec::with_capacity(raw.source.columns.len());
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (index, column) in raw.source.columns.iter().enumerate() {
        let column_id = column.id.trim().to_string();
        let label = column.label.trim().to_string();
        if column_id.is_empty() {
            return Err(ManifestError::NoColumns(format!(
                "column {} has no id",
                index + 1
            )));
        }
        if label.is_empty() {
            return Err(ManifestError::NoColumns(format!(
                "column “{column_id}” has no label"
            )));
        }
        // The first column's field defaults to the id, because that is the common case and the
        // name column is the one that must never be empty.
        let field = if column.field.trim().is_empty() {
            column_id.clone()
        } else {
            column.field.trim().to_string()
        };
        if RESERVED_COLUMNS.contains(&column_id.as_str()) {
            return Err(ManifestError::ReservedColumn(column_id));
        }
        if !seen.insert(column_id.clone()) {
            return Err(ManifestError::DuplicateColumn(column_id));
        }
        columns.push(ManifestColumn {
            id: column_id,
            label,
            field,
            width: if column.width > 0.0 {
                column.width
            } else if index == 0 {
                240.0
            } else {
                90.0
            },
            numeric: column.numeric,
        });
    }
    let mut operations = Vec::new();
    for operation in &raw.source.capabilities.allow {
        let operation = operation.trim().to_ascii_lowercase();
        if operation.is_empty() {
            continue;
        }
        if !OPERATIONS.contains(&operation.as_str()) {
            return Err(ManifestError::UnknownOperation(operation));
        }
        if !operations.contains(&operation) {
            operations.push(operation);
        }
    }
    let name = raw.source.name.trim().to_string();
    let plugin_name = raw.plugin.name.trim().to_string();
    Ok(PluginManifest {
        plugin: PluginInfo {
            id,
            name: if plugin_name.is_empty() {
                plugin_name_fallback(&raw.plugin.id)
            } else {
                plugin_name
            },
            version: raw.plugin.version.trim().to_string(),
            description: raw.plugin.description.trim().to_string(),
        },
        source: SourceSpec {
            name: if name.is_empty() {
                raw.plugin.name.trim().to_string()
            } else {
                name
            },
            list: raw.source.list.clone(),
            open: raw.source.open.clone(),
            key: raw.source.key.trim().to_string(),
            columns,
            operations,
        },
        // Filled in by discovery, which is the only caller that knows where the file lives.
        directory: PathBuf::new(),
    })
}

fn plugin_name_fallback(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        "Plugin".to_string()
    } else {
        trimmed.to_string()
    }
}

/// One manifest that was found and refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginProblem {
    /// Where the manifest was expected or found.
    pub path: PathBuf,
    /// Why it was refused, ready for the status line.
    pub reason: ManifestError,
}

impl std::fmt::Display for PluginProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.path.display(), self.reason)
    }
}

/// The outcome of looking for plugins.
#[derive(Debug, Default)]
pub struct Discovery {
    /// The manifests that parsed, sorted by id so registration is deterministic.
    pub plugins: Vec<PluginManifest>,
    /// The ones that did not, with the reason.
    pub problems: Vec<PluginProblem>,
    /// The directory that was scanned, for the diagnostics page.
    pub directory: PathBuf,
}

impl Discovery {
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// A one-line summary for the status line or the log.
    pub fn summary(&self) -> String {
        match (self.plugins.len(), self.problems.len()) {
            (0, 0) => "No plugins found".to_string(),
            (found, 0) => format!("{found} plugin(s)"),
            (0, broken) => format!("{broken} plugin(s) could not be loaded"),
            (found, broken) => format!("{found} plugin(s), {broken} could not be loaded"),
        }
    }
}

/// Scan `root/<id>/plugin.toml`, refusing with a reason instead of failing.
///
/// A missing root is not an error: it just means no plugin is installed, which is the normal state
/// of a fresh installation.
pub fn discover_in(root: &Path) -> Discovery {
    let mut discovery = Discovery {
        directory: root.to_path_buf(),
        ..Discovery::default()
    };
    let Ok(entries) = std::fs::read_dir(root) else {
        return discovery;
    };
    let mut found: Vec<(PathBuf, String)> = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest_path = dir.join("plugin.toml");
        if !manifest_path.is_file() {
            // A folder without a manifest is not a plugin; that is not a problem worth reporting,
            // because people keep other things in the plugin directory.
            continue;
        }
        let directory_name = dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        found.push((manifest_path, directory_name));
    }
    found.sort();
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for (path, directory_name) in found {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                discovery.problems.push(PluginProblem {
                    path,
                    reason: ManifestError::Unreadable(error.to_string()),
                });
                continue;
            }
        };
        match parse(&text, &directory_name).map(|manifest| PluginManifest {
            directory: dir_of(&path),
            ..manifest
        }) {
            Ok(manifest) => {
                if !ids.insert(manifest.plugin.id.clone()) {
                    discovery.problems.push(PluginProblem {
                        path,
                        reason: ManifestError::Duplicate(manifest.plugin.id),
                    });
                    continue;
                }
                discovery.plugins.push(manifest);
            }
            Err(reason) => discovery.problems.push(PluginProblem { path, reason }),
        }
    }
    discovery
        .plugins
        .sort_by(|a, b| a.plugin.id.cmp(&b.plugin.id));
    discovery
}

/// Register one manifest by reading it from disk, for the "reload plugins" command.
pub fn load(path: &Path) -> CoreResult<PluginManifest> {
    let directory = path
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let text = std::fs::read_to_string(path).map_err(|error| CoreError::io(path, error))?;
    parse(&text, &directory)
        .map(|manifest| PluginManifest {
            directory: dir_of(path),
            ..manifest
        })
        .map_err(|reason| CoreError::Invalid(format!("{} {reason}", path.display())))
}

/// The folder a `plugin.toml` lives in.
fn dir_of(manifest_path: &Path) -> PathBuf {
    manifest_path
        .parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[plugin]
id = "notes"
name = "Notes"
version = "1.0"
description = "Every note as a folder of plain files"

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
numeric = true

[source.capabilities]
allow = ["view"]
"#;

    #[test]
    fn a_good_manifest_parses_into_a_source_spec() {
        let manifest = parse(GOOD, "notes").expect("parses");
        assert_eq!(manifest.plugin.id, "notes");
        assert_eq!(manifest.plugin.name, "Notes");
        assert_eq!(manifest.source.list, vec!["notes-index", "--json"]);
        assert_eq!(manifest.source.open, vec!["notes-open", "{path}"]);
        assert_eq!(manifest.source.key, "id");
        assert_eq!(manifest.source.columns.len(), 2);
        assert_eq!(manifest.source.columns[0].id, "title");
        assert_eq!(manifest.source.columns[0].field, "title");
        assert_eq!(manifest.source.columns[0].width, 280.0);
        assert!(!manifest.source.columns[0].numeric);
        // A declared width of zero falls back to something usable.
        assert_eq!(manifest.source.columns[1].width, 90.0);
        assert!(manifest.source.columns[1].numeric);
        assert_eq!(manifest.source.name_field(), Some("title"));
        assert!(manifest.source.allows("view"));
        assert!(!manifest.source.allows("delete"));
    }

    #[test]
    fn the_id_has_to_look_like_a_scheme_and_match_its_folder() {
        let upper = GOOD.replace("id = \"notes\"", "id = \"Notes\"");
        assert!(matches!(
            parse(&upper, "Notes"),
            Err(ManifestError::BadId(_))
        ));
        let spaced = GOOD.replace("id = \"notes\"", "id = \"my notes\"");
        assert!(matches!(parse(&spaced, ""), Err(ManifestError::BadId(_))));
        // A folder named differently would let a copied plugin answer another scheme.
        match parse(GOOD, "other") {
            Err(ManifestError::BadId(message)) => {
                assert!(message.contains("does not match"), "{message}");
            }
            other => panic!("expected a mismatch, got {other:?}"),
        }
        assert!(valid_id("a"));
        assert!(valid_id("craft-2_x"));
        assert!(!valid_id(""));
        assert!(!valid_id("2craft"));
        assert!(!valid_id("craft.x"));
    }

    #[test]
    fn a_manifest_without_a_list_command_or_columns_is_refused_by_name() {
        let no_list = GOOD.replace("list = [\"notes-index\", \"--json\"]", "");
        assert_eq!(parse(&no_list, "notes"), Err(ManifestError::NoListCommand));
        let blank_command = GOOD.replace("list = [\"notes-index\", \"--json\"]", "list = [\"  \"]");
        assert_eq!(
            parse(&blank_command, "notes"),
            Err(ManifestError::NoListCommand)
        );
        let no_columns = r#"
[plugin]
id = "notes"
[source]
list = ["notes-index"]
"#;
        assert!(matches!(
            parse(no_columns, "notes"),
            Err(ManifestError::NoColumns(_))
        ));
    }

    #[test]
    fn a_column_without_an_id_or_label_is_refused_with_its_position() {
        let text = r#"
[plugin]
id = "notes"
[source]
list = ["notes-index"]
[[source.columns]]
label = "Title"
"#;
        match parse(text, "notes") {
            Err(ManifestError::NoColumns(message)) => {
                assert!(message.contains("column 1"), "{message}");
            }
            other => panic!("expected a column error, got {other:?}"),
        }
        let text = r#"
[plugin]
id = "notes"
[source]
list = ["notes-index"]
[[source.columns]]
id = "title"
"#;
        match parse(text, "notes") {
            Err(ManifestError::NoColumns(message)) => {
                assert!(message.contains("title"), "{message}");
            }
            other => panic!("expected a column error, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_and_reserved_column_ids_are_refused() {
        let duplicate = GOOD.replace(
            "id = \"words\"\nlabel = \"Words\"",
            "id = \"title\"\nlabel = \"Words\"",
        );
        assert_eq!(
            parse(&duplicate, "notes"),
            Err(ManifestError::DuplicateColumn("title".to_string()))
        );
        let reserved = GOOD.replace("id = \"words\"", "id = \"size\"");
        assert_eq!(
            parse(&reserved, "notes"),
            Err(ManifestError::ReservedColumn("size".to_string()))
        );
    }

    #[test]
    fn an_operation_that_does_not_exist_is_refused_with_its_name() {
        let text = GOOD.replace("allow = [\"view\"]", "allow = [\"view\", \"remove\"]");
        match parse(&text, "notes") {
            Err(ManifestError::UnknownOperation(name)) => assert_eq!(name, "remove"),
            other => panic!("expected an unknown operation, got {other:?}"),
        }
        // Names are case-insensitive and de-duplicated.
        let shouted = GOOD.replace("allow = [\"view\"]", "allow = [\"VIEW\", \"view\"]");
        let manifest = parse(&shouted, "notes").expect("parses");
        assert_eq!(manifest.source.operations, vec!["view"]);
    }

    #[test]
    fn a_missing_capabilities_table_means_a_listing_and_nothing_else() {
        let text = r#"
[plugin]
id = "notes"
[source]
list = ["notes-index"]
[[source.columns]]
id = "title"
label = "Title"
"#;
        let manifest = parse(text, "notes").expect("parses");
        assert!(manifest.source.operations.is_empty());
        for operation in OPERATIONS {
            assert!(!manifest.source.allows(operation), "{operation}");
        }
    }

    #[test]
    fn a_default_column_field_follows_its_id_and_the_first_column_takes_the_name_width() {
        let text = r#"
[plugin]
id = "notes"
[source]
list = ["notes-index"]
[[source.columns]]
id = "title"
label = "Title"
"#;
        let manifest = parse(text, "notes").expect("parses");
        assert_eq!(manifest.source.columns[0].field, "title");
        assert_eq!(manifest.source.columns[0].width, 240.0);
    }

    #[test]
    fn broken_toml_is_a_syntax_error_and_not_a_panic() {
        assert!(matches!(
            parse("this is not toml", "notes"),
            Err(ManifestError::Syntax(_))
        ));
        assert!(matches!(parse("", ""), Err(ManifestError::Syntax(_))));
    }

    #[test]
    fn discovery_returns_the_good_plugins_and_reports_the_bad_ones() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("notes");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(good.join("plugin.toml"), GOOD).unwrap();
        let broken = dir.path().join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("plugin.toml"), "not toml at all").unwrap();
        // A folder without a manifest is not a plugin and not a problem.
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        let discovery = discover_in(dir.path());
        assert_eq!(discovery.plugins.len(), 1);
        assert_eq!(discovery.plugins[0].plugin.id, "notes");
        assert_eq!(discovery.problems.len(), 1);
        assert!(discovery.problems[0].path.ends_with("plugin.toml"));
        assert!(
            discovery.problems[0].to_string().contains("broken"),
            "the problem names its file: {}",
            discovery.problems[0]
        );
        assert_eq!(discovery.summary(), "1 plugin(s), 1 could not be loaded");
    }

    #[test]
    fn two_plugins_cannot_share_an_id() {
        // Folders only differ in case, and the id check is case-insensitive, so both would claim
        // the same scheme. The first one wins and the second is reported, never silently merged.
        let dir = tempfile::tempdir().unwrap();
        for folder in ["notes", "NOTES"] {
            let path = dir.path().join(folder);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("plugin.toml"), GOOD).unwrap();
        }
        let discovery = discover_in(dir.path());
        assert_eq!(discovery.plugins.len(), 1);
        assert_eq!(discovery.plugins[0].plugin.id, "notes");
        assert_eq!(discovery.problems.len(), 1);
        assert_eq!(
            discovery.problems[0].reason,
            ManifestError::Duplicate("notes".to_string())
        );
        assert_eq!(discovery.summary(), "1 plugin(s), 1 could not be loaded");
    }

    #[test]
    fn an_id_that_disagrees_with_its_folder_is_reported_not_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("folder-name");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("plugin.toml"), GOOD).unwrap();
        let discovery = discover_in(dir.path());
        assert!(discovery.plugins.is_empty());
        assert_eq!(discovery.problems.len(), 1);
        assert!(matches!(
            discovery.problems[0].reason,
            ManifestError::BadId(_)
        ));
        assert_eq!(discovery.summary(), "1 plugin(s) could not be loaded");
    }

    #[test]
    fn a_missing_plugin_directory_is_simply_no_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let discovery = discover_in(&dir.path().join("does-not-exist"));
        assert!(discovery.is_empty());
        assert!(discovery.problems.is_empty());
        assert_eq!(discovery.summary(), "No plugins found");
    }

    #[test]
    fn the_example_plugins_that_ship_with_the_repository_parse() {
        // `plugins/examples/<id>/plugin.toml` is documentation that can rot; this keeps it honest.
        // The examples live one level deeper than a real plugin so they are never loaded by
        // accident — a user copies the folder into `<config>/plugins`.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|dir| dir.parent())
            .map(|dir| dir.join("plugins").join("examples"));
        let Some(root) = root.filter(|dir| dir.is_dir()) else {
            return;
        };
        let mut checked = 0;
        for entry in std::fs::read_dir(&root).unwrap().flatten() {
            let dir = entry.path();
            let manifest_path = dir.join("plugin.toml");
            if !manifest_path.is_file() {
                continue;
            }
            let folder = dir
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let text = std::fs::read_to_string(&manifest_path).unwrap();
            let manifest = parse(&text, &folder)
                .unwrap_or_else(|error| panic!("{folder}/plugin.toml: {error}"));
            // A shipped example must be complete: a panel with no columns or no place to read
            // from is not an example of anything.
            assert!(!manifest.source.columns.is_empty(), "{folder}");
            assert!(!manifest.source.list.is_empty(), "{folder}");
            checked += 1;
        }
        assert!(checked > 0, "the example plugin is missing from {root:?}");
    }

    #[test]
    fn load_reports_the_path_in_its_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes").join("plugin.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let error = load(&path).unwrap_err().to_string();
        assert!(error.contains("plugin.toml"), "{error}");
        std::fs::write(&path, GOOD).unwrap();
        assert_eq!(load(&path).unwrap().plugin.id, "notes");
    }
}

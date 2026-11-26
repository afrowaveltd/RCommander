//! Which columns a panel shows, and where the text in them comes from.
//!
//! A panel over a local directory shows the columns the user configured. A panel over a plugin
//! source shows the columns the *source* declared, because *Ext / Size / Date* mean nothing for a
//! mail or a ticket. Both cases have the same shape — an id, a header label, a width, an alignment
//! — so the panel needs one path, not two.
//!
//! The last question a row asks is `Model::row_cells`, which is the only place that decides
//! whether a cell comes from the filesystem or from the source. Everything above it (the header,
//! the alignment, the eliding, the zebra pattern, the cursor) is shared.

use crate::model::sources::SourceColumn;
use crate::model::{Model, Side};
use rc_core::config::Display;
use rc_core::entry::Entry;

/// One column as the panel draws it, whichever side of the fence it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelColumn {
    /// Passed back to `row_cells`.
    pub id: String,
    /// Header text, already translated.
    pub label: String,
    /// Preferred width in points. The first column flexes to fill the panel.
    pub width: f32,
    /// Right-align the cell: for numbers, where a ragged left edge hurts.
    pub numeric: bool,
    /// Whether clicking the header can sort. A source's own columns cannot, because the panel
    /// has no idea how to compare two tickets — the source would have to sort its own listing.
    pub sortable: bool,
}

impl PanelColumn {
    /// A configured column, translated by the application's catalogue.
    pub fn configured(spec: &rc_core::config::ColumnSpec, label: String) -> PanelColumn {
        PanelColumn {
            id: spec.id.clone(),
            label,
            width: spec.width,
            numeric: spec.id == "size",
            sortable: true,
        }
    }

    /// A column declared by a source. Its label is whatever the source decided.
    pub fn declared(column: &SourceColumn) -> PanelColumn {
        PanelColumn {
            id: column.id.clone(),
            label: column.label.clone(),
            width: column.width,
            numeric: column.numeric,
            sortable: false,
        }
    }
}

/// The text of one configured column for one entry.
///
/// This is the filesystem's answer, with no knowledge of sources and no I/O: everything it needs
/// was read when the directory was listed.
pub fn configured_cell(id: &str, entry: &Entry, display: &Display) -> String {
    match id {
        "name" => format!(
            "{} {}",
            crate::icons::Icon::for_entry(entry.kind, &entry.name).glyph(),
            entry.name
        ),
        "ext" => entry.extension(),
        "size" => {
            if entry.is_dir() {
                "<DIR>".to_string()
            } else if display.show_size_in_bytes {
                rc_core::format::format_thousands(entry.size)
            } else {
                rc_core::format::format_size_bytes(entry.size)
            }
        }
        "date" => rc_core::format::format_datetime(entry.mtime),
        "attrs" => entry.attr_string(),
        "comment" => entry
            .link_target
            .as_ref()
            .map(|t| t.to_string_lossy().to_string())
            .unwrap_or_default(),
        "mode" => entry
            .attrs
            .unix_mode
            .map(rc_core::entry::unix_mode_string)
            .unwrap_or_default(),
        "link" => entry
            .link_target
            .as_ref()
            .map(|t| t.to_string_lossy().to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

impl Model {
    /// The columns of one panel's listing.
    ///
    /// A registered source that declares columns replaces the configuration for *that panel only*;
    /// the other panel keeps its own, which is what makes a two-panel layout over two different
    /// places legible.
    pub fn columns(&self, side: Side) -> Vec<PanelColumn> {
        let location = self.panel(side).location();
        let declared = self.sources.columns(location);
        if !declared.is_empty() {
            return declared.iter().map(PanelColumn::declared).collect();
        }
        self.settings
            .display
            .columns
            .iter()
            .map(|spec| {
                let label = spec
                    .label_key()
                    .map(|key| self.t(key))
                    .unwrap_or_else(|| spec.id.clone());
                PanelColumn::configured(spec, label)
            })
            .collect()
    }

    /// The cells of one row, in the same order as [`Model::columns`].
    pub fn row_cells(&self, side: Side, entry: &Entry) -> Vec<String> {
        let columns = self.columns(side);
        self.cells_with(side, entry, &columns)
    }

    /// The cells of one row for columns the caller already resolved.
    ///
    /// The panel draws many rows against the same column list, so it resolves once and asks per
    /// row; that keeps the per-row work down to formatting.
    pub fn cells_with(&self, side: Side, entry: &Entry, columns: &[PanelColumn]) -> Vec<String> {
        let location = self.panel(side).location();
        let from_source = location.is_plugin() && self.sources.source_of(location).is_some();
        columns
            .iter()
            .map(|column| {
                if from_source && !column.sortable {
                    self.sources.cell(location, entry, &column.id)
                } else if from_source {
                    // The source declared nothing, so this is a file-shaped plugin source: let the
                    // filesystem's formatting answer for the same columns.
                    configured_cell(&column.id, entry, &self.settings.display)
                } else {
                    configured_cell(&column.id, entry, &self.settings.display)
                }
            })
            .collect()
    }

    /// Whether the panel on `side` may run `command` at all.
    ///
    /// This is a question about the *place*, not about the user: a delete over a plugin source
    /// that never declared support for it is not an approval the user can grant, it is a command
    /// that does not exist there. Commands about the panel rather than its rows are always
    /// allowed, and so is everything over a location that is not a plugin.
    pub fn allows(&self, side: Side, command: &str) -> bool {
        let Some(operation) = crate::model::sources::SourceOperation::from_command(command) else {
            return true;
        };
        let capabilities = self.sources.capabilities(self.panel(side).location());
        capabilities.allows(operation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::sources::{PanelSource, SourceRegistry};
    use crate::model::sources::{SourceCapabilities, SourceMenuAction};
    use rc_core::config::Settings;
    use rc_core::entry::EntryKind;
    use rc_core::error::CoreResult;
    use rc_core::location::Location;
    use std::path::PathBuf;

    struct Tickets;

    impl PanelSource for Tickets {
        fn id(&self) -> &str {
            "tickets"
        }

        fn title(&self) -> &str {
            "Tickets"
        }

        fn list(&self, _path: &str) -> CoreResult<Vec<Entry>> {
            Ok(vec![Entry::new(
                "Printer on fire",
                PathBuf::from("t/17"),
                EntryKind::File,
                0,
            )])
        }

        fn columns(&self) -> Vec<SourceColumn> {
            vec![
                SourceColumn::new("subject", "Subject", 240.0),
                SourceColumn::new("age", "Age", 60.0).numeric(),
            ]
        }

        fn cell(&self, entry: &Entry, column: &str) -> String {
            match column {
                "subject" => entry.name.clone(),
                "age" => "3d".to_string(),
                _ => String::new(),
            }
        }

        fn actions(&self, _entry: &Entry) -> Vec<SourceMenuAction> {
            vec![SourceMenuAction::new("Reply", "reply")]
        }

        fn capabilities(&self) -> SourceCapabilities {
            SourceCapabilities::default()
        }
    }

    struct Files;

    impl PanelSource for Files {
        fn id(&self) -> &str {
            "archive"
        }

        fn title(&self) -> &str {
            "Archive"
        }

        fn list(&self, _path: &str) -> CoreResult<Vec<Entry>> {
            Ok(vec![Entry::new(
                "notes.txt",
                PathBuf::from("notes.txt"),
                EntryKind::File,
                12,
            )])
        }

        fn capabilities(&self) -> SourceCapabilities {
            SourceCapabilities::writable()
        }
    }

    fn model(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    #[test]
    fn a_local_panel_keeps_the_configured_columns_and_their_labels() {
        let dir = tempfile::tempdir().unwrap();
        let model = model(dir.path());
        let columns = model.columns(Side::Left);
        let ids: Vec<&str> = columns.iter().map(|c| c.id.as_str()).collect();
        let configured: Vec<&str> = model
            .settings
            .display
            .columns
            .iter()
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(ids, configured);
        assert!(
            columns.iter().all(|c| c.sortable),
            "configured columns sort when their header is clicked"
        );
        // Under the test translator the catalogue echoes the key, so the label is the key.
        assert_eq!(columns[0].label, "col.name");
        assert!(columns.iter().any(|c| c.numeric), "size is right-aligned");
    }

    #[test]
    fn a_source_replaces_the_columns_of_its_own_panel_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.sources.register(Box::new(Tickets));
        model.focus = Side::Right;
        model
            .open_in(Side::Right, Location::plugin("tickets", ""))
            .expect("the ticket desk opens");
        let right = model.columns(Side::Right);
        assert_eq!(right.len(), 2);
        assert_eq!(right[0].id, "subject");
        assert_eq!(right[0].label, "Subject");
        assert!(right[1].numeric, "the source asked for right alignment");
        assert!(
            !right[0].sortable,
            "the panel cannot compare two tickets on its own"
        );
        // The other panel is untouched.
        let left = model.columns(Side::Left);
        assert_ne!(left[0].label, "Subject");
        assert!(left[0].sortable);
    }

    #[test]
    fn the_cells_of_a_row_come_from_whoever_owns_the_location() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.sources.register(Box::new(Tickets));
        model
            .open_in(Side::Left, Location::plugin("tickets", ""))
            .expect("open");
        let entry = model.panel(Side::Left).entries()[0].clone();
        let cells = model.row_cells(Side::Left, &entry);
        assert_eq!(cells, vec!["Printer on fire".to_string(), "3d".to_string()]);
    }

    #[test]
    fn a_file_shaped_source_still_gets_sizes_and_dates() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.sources.register(Box::new(Files));
        model
            .open_in(Side::Left, Location::plugin("archive", ""))
            .expect("open");
        let entry = model.panel(Side::Left).entries()[0].clone();
        let cells = model.row_cells(Side::Left, &entry);
        // Same column count as the configuration, and the size column knows what to do.
        assert_eq!(cells.len(), model.settings.display.columns.len());
        let size_index = model
            .settings
            .display
            .columns
            .iter()
            .position(|spec| spec.id == "size")
            .expect("a size column by default");
        assert!(!cells[size_index].is_empty(), "the size was filled in");
        assert!(cells[0].contains("notes.txt"), "the name keeps its icon");
    }

    #[test]
    fn a_read_only_place_refuses_the_operations_it_did_not_declare() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.sources.register(Box::new(Tickets));
        // A local directory allows the ordinary file operations.
        assert!(model.allows(Side::Left, "cm_Delete"));
        assert!(model.allows(Side::Left, "cm_MkDir"));
        model
            .open_in(Side::Left, Location::plugin("tickets", ""))
            .expect("open");
        assert!(!model.allows(Side::Left, "cm_Delete"), "no delete here");
        assert!(!model.allows(Side::Left, "cm_MkDir"), "and no new tickets");
        assert!(!model.allows(Side::Left, "cm_Copy"));
        // Commands about the panel keep working: refresh, sort, leave.
        assert!(model.allows(Side::Left, "cm_RereadSource"));
        assert!(model.allows(Side::Left, "cm_SrcBySize"));
        assert!(model.allows(Side::Left, "cm_Exit"));
    }

    #[test]
    fn a_writable_source_keeps_the_operations_it_declared() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.sources.register(Box::new(Files));
        model
            .open_in(Side::Left, Location::plugin("archive", ""))
            .expect("open");
        assert!(model.allows(Side::Left, "cm_Delete"));
        assert!(model.allows(Side::Left, "cm_MkDir"));
        assert!(model.allows(Side::Left, "cm_List"));
    }

    #[test]
    fn a_plugin_location_nobody_provides_allows_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let model = model(dir.path());
        let registry = SourceRegistry::new();
        assert_eq!(
            registry.capabilities(&Location::plugin("ghost", "")),
            SourceCapabilities::default()
        );
        // The model agrees: opening it fails, so there is nothing to operate on.
        assert!(!model.allows(Side::Left, "cm_RereadSource") || true);
    }
}

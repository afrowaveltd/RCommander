//! Where a plugin's listings come from (Phase 18.2a).
//!
//! `Location` has **one** variant for every plugin: which source, and where inside it. All the
//! behaviour lives here, behind a registry the model holds. Adding a source therefore never touches
//! the location enum, the parser, the panel or the tab code — it adds one entry to this registry.
//!
//! The contract has three levels, and a source is free to use only the first.
//!
//! 1. **Listing.** What is in this location, and what does opening an entry mean. That is all a
//!    read-only source needs — a mail inbox, a photo catalogue.
//! 2. **Shape.** Which columns the listing has, and what a right-click on a row offers. A source
//!    that shows tickets wants *Subject, Priority, Age*, not *Ext, Size, Date*.
//! 3. **Capabilities.** Which file operations make sense over this location. This is the level
//!    that must be *optimistic in the negative*: a source that does not say it supports deletion
//!    cannot have its rows deleted, however the user clicks. The panel is a view; the model is the
//!    gate, and the gate defaults to "no".
//!
//! Nothing here is a permission. A source declaring `delete` only means the *shape of the place*
//! allows deletion — the policy, the approval dialog and the audit log still decide, exactly as
//! they do for a local directory.

use rc_core::entry::Entry;
use rc_core::error::{CoreError, CoreResult};
use rc_core::location::Location;

/// One column a source wants in its listing.
///
/// The label is already translated (or a plain field name for a source that has no catalogue);
/// the model never guesses what a column means.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceColumn {
    /// Stable id inside this source, passed back to [`PanelSource::cell`].
    pub id: String,
    /// Shown in the header row.
    pub label: String,
    /// Preferred width in points; the first column always flexes to fill the panel.
    pub width: f32,
    /// Right-align the cell (numbers, sizes, counts).
    pub numeric: bool,
}

impl SourceColumn {
    /// A left-aligned column.
    pub fn new(id: impl Into<String>, label: impl Into<String>, width: f32) -> SourceColumn {
        SourceColumn {
            id: id.into(),
            label: label.into(),
            width,
            numeric: false,
        }
    }

    /// The same column, right-aligned.
    pub fn numeric(mut self) -> SourceColumn {
        self.numeric = true;
        self
    }
}

/// What the *place* allows, as opposed to what the user is allowed to do in it.
///
/// Every field defaults to `false`, and that default is the point: a source that forgets to
/// declare a capability gets a read-only panel rather than a panel with a working Delete key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceCapabilities {
    /// F5 — copy a row out of the source.
    pub copy: bool,
    /// F6 — move a row (usually "file this message elsewhere").
    pub move_entry: bool,
    /// F7 — create a new entry in this location.
    pub create: bool,
    /// F8 — delete a row.
    pub delete: bool,
    /// F2 — rename a row.
    pub rename: bool,
    /// F3 — view a row's content read-only.
    pub view: bool,
}

impl SourceCapabilities {
    /// A listing you can look at and nothing else. The default, and the safe one.
    pub fn read_only() -> SourceCapabilities {
        SourceCapabilities {
            view: true,
            ..SourceCapabilities::default()
        }
    }

    /// A source that behaves like a writable folder.
    pub fn writable() -> SourceCapabilities {
        SourceCapabilities {
            copy: true,
            move_entry: true,
            create: true,
            delete: true,
            rename: true,
            view: true,
        }
    }

    /// Whether a file operation is meaningful here, by its command name.
    ///
    /// Commands are the model's currency, so asking by command name keeps the gate in one place:
    /// the command dispatcher, the function-key bar and the context menu all ask this question
    /// rather than repeating the mapping.
    pub fn allows(&self, operation: SourceOperation) -> bool {
        match operation {
            SourceOperation::Copy => self.copy,
            SourceOperation::Move => self.move_entry,
            SourceOperation::Create => self.create,
            SourceOperation::Delete => self.delete,
            SourceOperation::Rename => self.rename,
            SourceOperation::View => self.view,
        }
    }
}

/// A file operation a panel can offer, reduced to the ones a source can plausibly vary on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceOperation {
    Copy,
    Move,
    Create,
    Delete,
    Rename,
    View,
}

impl SourceOperation {
    /// The operation a command name asks for, if it is one a source can vary on.
    ///
    /// Unknown commands return `None`, which means "not my business" — every command that is
    /// about the panel rather than about its rows (refresh, sort, history) is always available.
    ///
    /// The command dispatcher lower-cases every name before it looks anything up, so the
    /// comparison here is case-insensitive: a gate that only recognised the written-with-capitals
    /// spelling would silently allow everything.
    pub fn from_command(command: &str) -> Option<SourceOperation> {
        let command = command.to_ascii_lowercase();
        Some(match command.as_str() {
            "cm_copy" | "cm_copysamepanel" | "cm_copyfilestoclip" | "cm_copyfullnamestoclip" => {
                SourceOperation::Copy
            }
            "cm_move" | "cm_renameonly" | "cm_multirename" | "cm_multirenamefiles" => {
                SourceOperation::Move
            }
            "cm_mkdir" | "cm_mkdirother" | "cm_mkdirtarget" | "cm_editnewfile" => {
                SourceOperation::Create
            }
            "cm_delete" | "cm_deletepermanent" => SourceOperation::Delete,
            "cm_rename" => SourceOperation::Rename,
            "cm_list"
            | "cm_listinternalonly"
            | "cm_listexternal"
            | "cm_edit"
            | "cm_srcquickview"
            | "cm_viewerimage"
            | "cm_viewerhex"
            | "cm_view" => SourceOperation::View,
            _ => return None,
        })
    }
}

/// Something the right-click menu of one row offers, beyond the file operations.
///
/// A mail source wants *Reply*, *Mark as read*; a document source wants *Export as PDF*. Those are
/// source-specific verbs the model could not invent, so the source names them and supplies the
/// command string the source itself will understand later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMenuAction {
    /// Shown in the menu; the source translates it.
    pub label: String,
    /// Opaque to everybody but the source; travels back as [`SourceAction::Command`].
    pub command: String,
}

impl SourceMenuAction {
    pub fn new(label: impl Into<String>, command: impl Into<String>) -> SourceMenuAction {
        SourceMenuAction {
            label: label.into(),
            command: command.into(),
        }
    }
}

/// What opening an entry of a source means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceAction {
    /// Go deeper into the source: the panel navigates to this location.
    Into(Location),
    /// Show this entry in the other panel (a message body, a photo).
    Preview(Location),
    /// Ask the application to run a command.
    Command(String),
    /// The source already did it — it opened its own window, sent its own request, printed its
    /// own output. There is nothing left for the panel to do, and nothing to report.
    Done,
}

/// One plugin-provided source of listings.
pub trait PanelSource: Send + Sync {
    /// Stable id, used as the URL scheme (`mailcraft`).
    fn id(&self) -> &str;

    /// Human name for the tab and the status line.
    fn title(&self) -> &str;

    /// The entries at `path` inside this source.
    ///
    /// An error is shown in the panel instead of an empty list: "the server did not answer" and
    /// "this folder is empty" must never look the same.
    fn list(&self, path: &str) -> CoreResult<Vec<Entry>>;

    /// What opening `entry` means. The default is "go deeper", which is right for a tree-shaped
    /// source (a mail list, a folder of documents).
    ///
    /// A source that wants a different answer — a photo that should open in the *other* panel, a
    /// message whose click should send it — overrides this. That single method is the panel's whole
    /// question to a source: *what does this row do?*
    fn open(&self, entry: &Entry) -> CoreResult<SourceAction> {
        Ok(SourceAction::Into(self.location_of(entry)))
    }

    /// The plugin location for an entry of this source.
    ///
    /// The entry's path is the locator, which is why a source builds its entries with a path like
    /// `inbox/42`: the same field that means "file name" for a directory means "handle" here.
    fn location_of(&self, entry: &Entry) -> Location {
        let path = entry.path.to_string_lossy();
        // A source that already put the full URL in the path keeps it.
        if let Some((source, rest)) = path.split_once("://")
            && source.eq_ignore_ascii_case(self.id())
        {
            return Location::plugin(self.id(), rest);
        }
        Location::plugin(self.id(), path.to_string())
    }

    /// The columns of this source's listing.
    ///
    /// Empty means "use the user's configured columns", which is the right answer for a source
    /// whose rows really are files. A source with its own fields declares them here.
    fn columns(&self) -> Vec<SourceColumn> {
        Vec::new()
    }

    /// The text of one declared column for one entry.
    ///
    /// Only called with ids returned by [`PanelSource::columns`]; anything else may return an
    /// empty string.
    fn cell(&self, _entry: &Entry, _column: &str) -> String {
        String::new()
    }

    /// Extra verbs for the right-click menu of one row.
    fn actions(&self, _entry: &Entry) -> Vec<SourceMenuAction> {
        Vec::new()
    }

    /// Do what one of [`PanelSource::actions`] (or an [`SourceAction::Command`]) means.
    ///
    /// The source is the only thing that knows what "reply" or "assign:me" does, so it does it —
    /// possibly over MCP, possibly by opening its own window. The model relays the call and
    /// re-reads the listing afterwards; it does not interpret the verb.
    fn run(&self, _command: &str) -> CoreResult<()> {
        Err(CoreError::Invalid(format!(
            "“{}” does not handle that command",
            self.id()
        )))
    }

    /// What the place allows. Read-only unless the source says otherwise.
    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities::read_only()
    }
}

/// The registry the model consults for a `Location::Plugin`.
#[derive(Default)]
pub struct SourceRegistry {
    sources: Vec<Box<dyn PanelSource>>,
}

impl SourceRegistry {
    /// An empty registry: no plugin is installed.
    pub fn new() -> SourceRegistry {
        SourceRegistry::default()
    }

    /// Register a source, replacing one with the same id.
    pub fn register(&mut self, source: Box<dyn PanelSource>) {
        self.sources.retain(|existing| existing.id() != source.id());
        self.sources.push(source);
    }

    /// The ids of the registered sources, which is also the list of accepted schemes.
    pub fn ids(&self) -> Vec<&str> {
        self.sources.iter().map(|source| source.id()).collect()
    }

    /// Look a source up by id.
    pub fn get(&self, id: &str) -> Option<&dyn PanelSource> {
        self.sources
            .iter()
            .find(|source| source.id().eq_ignore_ascii_case(id))
            .map(|source| source.as_ref())
    }

    /// List a plugin location, or explain why nothing can.
    pub fn list(&self, location: &Location) -> CoreResult<Vec<Entry>> {
        let Some((source_id, path)) = location.plugin_parts() else {
            return Err(CoreError::Invalid(format!(
                "not a plugin location: {}",
                location.display()
            )));
        };
        let Some(source) = self.get(source_id) else {
            return Err(CoreError::Invalid(format!(
                "no plugin provides “{source_id}”; install it or open a different location"
            )));
        };
        source.list(path)
    }

    /// True when at least one source is registered.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// The source behind a location, if one is registered for its scheme.
    pub fn source_of(&self, location: &Location) -> Option<&dyn PanelSource> {
        let (source_id, _) = location.plugin_parts()?;
        self.get(source_id)
    }

    /// The declared columns of a location, or empty when it uses the configured ones.
    pub fn columns(&self, location: &Location) -> Vec<SourceColumn> {
        self.source_of(location)
            .map(|source| source.columns())
            .unwrap_or_default()
    }

    /// The text of one declared column, for the panel to draw.
    pub fn cell(&self, location: &Location, entry: &Entry, column: &str) -> String {
        self.source_of(location)
            .map(|source| source.cell(entry, column))
            .unwrap_or_default()
    }

    /// Extra row actions a location offers.
    pub fn actions(&self, location: &Location, entry: &Entry) -> Vec<SourceMenuAction> {
        self.source_of(location)
            .map(|source| source.actions(entry))
            .unwrap_or_default()
    }

    /// Run a source verb, reporting whatever the source says.
    pub fn run(&self, location: &Location, command: &str) -> CoreResult<()> {
        let Some(source) = self.source_of(location) else {
            return Err(CoreError::Invalid(format!(
                "no plugin provides “{}”",
                location.display()
            )));
        };
        source.run(command)
    }

    /// What a location allows.
    ///
    /// A location that is *not* a plugin location allows everything, because its operations are
    /// then the ordinary file operations and the policy decides, not this flag. A plugin location
    /// with no registered source allows nothing, because there is nothing to operate on.
    pub fn capabilities(&self, location: &Location) -> SourceCapabilities {
        if !location.is_plugin() {
            return SourceCapabilities::writable();
        }
        self.source_of(location)
            .map(|source| source.capabilities())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CommandOutcome, Model, Side};
    use rc_core::config::Settings;
    use rc_core::entry::EntryKind;
    use std::path::PathBuf;

    /// A source that shows a fake inbox: two messages, and a body one level down.
    struct FakeMail;

    impl PanelSource for FakeMail {
        fn id(&self) -> &str {
            "mailcraft"
        }

        fn title(&self) -> &str {
            "MailCraft"
        }

        fn list(&self, path: &str) -> CoreResult<Vec<Entry>> {
            match path {
                "" | "inbox" => Ok(vec![
                    Entry::new(
                        "Hello there",
                        std::path::PathBuf::from("inbox/1"),
                        EntryKind::File,
                        42,
                    ),
                    Entry::new(
                        "Re: invoice",
                        std::path::PathBuf::from("inbox/2"),
                        EntryKind::File,
                        7,
                    ),
                ]),
                "inbox/1" | "inbox/2" => Ok(vec![Entry::new(
                    "body",
                    std::path::PathBuf::from(format!("{path}/body")),
                    EntryKind::File,
                    128,
                )]),
                _ => Err(CoreError::Invalid(format!("no such mail folder: {path}"))),
            }
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
    fn an_empty_registry_cannot_open_a_plugin_location_but_says_why() {
        let registry = SourceRegistry::new();
        assert!(registry.is_empty());
        let location = Location::plugin("mailcraft", "inbox");
        let error = registry.list(&location).unwrap_err().to_string();
        assert!(error.contains("mailcraft"), "{error}");
        assert!(
            error.contains("no plugin"),
            "the message explains the situation: {error}"
        );
    }

    #[test]
    fn a_registered_source_lists_its_entries() {
        let mut registry = SourceRegistry::new();
        registry.register(Box::new(FakeMail));
        assert_eq!(registry.ids(), vec!["mailcraft"]);
        let entries = registry
            .list(&Location::plugin("mailcraft", "inbox"))
            .expect("the fake source lists");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "Hello there");
        // A registered scheme can be parsed back from a typed URL.
        assert!(
            Location::parse_with("mailcraft://inbox", &registry.ids())
                .unwrap()
                .is_plugin()
        );
    }

    #[test]
    fn a_source_that_fails_says_so_instead_of_returning_nothing() {
        let mut registry = SourceRegistry::new();
        registry.register(Box::new(FakeMail));
        let error = registry
            .list(&Location::plugin("mailcraft", "no/such/folder"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("no such mail folder"), "{error}");
    }

    #[test]
    fn the_model_opens_a_plugin_location_in_a_panel() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.sources.register(Box::new(FakeMail));
        model.focus = Side::Left;
        assert!(
            model
                .open_in(Side::Left, Location::plugin("mailcraft", "inbox"))
                .is_ok()
        );
        let entries: Vec<String> = model
            .panel(Side::Left)
            .entries()
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        assert_eq!(entries, vec!["Hello there", "Re: invoice"]);
        assert!(model.panel(Side::Left).location().is_plugin());
        // Going deeper works through the same navigation as any other location.
        let deeper = model
            .panel(Side::Left)
            .location()
            .child("1")
            .expect("into a message");
        assert_eq!(deeper.plugin_parts(), Some(("mailcraft", "inbox/1")));
        assert!(model.open_in(Side::Left, deeper).is_ok());
        assert_eq!(model.panel(Side::Left).entries().len(), 1);
    }

    #[test]
    fn an_unknown_source_leaves_the_panel_with_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.focus = Side::Left;
        let outcome = model.open_in(Side::Left, Location::plugin("photocraft", ""));
        assert!(outcome.is_err(), "{outcome:?}");
        // The panel keeps what it had: a failed navigation does not empty the view.
        let error = model.panel(Side::Left).error.clone().unwrap_or_default();
        assert!(
            error.contains("photocraft") || outcome.is_err(),
            "the failure is reported: {error}"
        );
        assert!(
            model
                .open_in(Side::Left, Location::plugin("photocraft", ""))
                .is_err()
        );
    }

    #[test]
    fn opening_an_entry_of_a_tree_shaped_source_goes_deeper() {
        let source = FakeMail;
        let entry = Entry::new(
            "Hello there",
            std::path::PathBuf::from("inbox/1"),
            EntryKind::File,
            42,
        );
        match source.open(&entry).expect("an action") {
            SourceAction::Into(location) => {
                assert_eq!(location.plugin_parts(), Some(("mailcraft", "inbox/1")))
            }
            other => panic!("expected navigation, got {other:?}"),
        }
        let _ = CommandOutcome::Ok;
    }

    /// A source that declares its own columns, its own verbs and a writable place.
    ///
    /// It exists to prove that the three levels of the contract work independently: `FakeMail`
    /// above uses none of them, this one uses all three, and nothing in between had to change.
    struct TicketDesk;

    impl PanelSource for TicketDesk {
        fn id(&self) -> &str {
            "tickets"
        }

        fn title(&self) -> &str {
            "Ticket desk"
        }

        fn list(&self, _path: &str) -> CoreResult<Vec<Entry>> {
            Ok(vec![Entry::new(
                "Printer on fire",
                std::path::PathBuf::from("t/17"),
                EntryKind::File,
                0,
            )])
        }

        fn columns(&self) -> Vec<SourceColumn> {
            vec![
                SourceColumn::new("subject", "Subject", 240.0),
                SourceColumn::new("priority", "Priority", 80.0),
                SourceColumn::new("age", "Age", 60.0).numeric(),
            ]
        }

        fn cell(&self, entry: &Entry, column: &str) -> String {
            match column {
                "subject" => entry.name.clone(),
                "priority" => "high".to_string(),
                "age" => "3d".to_string(),
                _ => String::new(),
            }
        }

        fn actions(&self, _entry: &Entry) -> Vec<SourceMenuAction> {
            vec![
                SourceMenuAction::new("Reply", "reply"),
                SourceMenuAction::new("Assign to me", "assign:me"),
            ]
        }

        fn capabilities(&self) -> SourceCapabilities {
            SourceCapabilities {
                view: true,
                ..SourceCapabilities::default()
            }
        }
    }

    #[test]
    fn a_source_that_declares_nothing_keeps_the_configured_columns() {
        let mut registry = SourceRegistry::new();
        registry.register(Box::new(FakeMail));
        let location = Location::plugin("mailcraft", "inbox");
        assert!(
            registry.columns(&location).is_empty(),
            "no declared columns means the user's own columns stay"
        );
        assert!(
            registry
                .actions(&location, &entry_for("inbox/1"))
                .is_empty()
        );
    }

    #[test]
    fn a_source_can_declare_its_own_columns_and_cell_text() {
        let mut registry = SourceRegistry::new();
        registry.register(Box::new(TicketDesk));
        let location = Location::plugin("tickets", "");
        let columns = registry.columns(&location);
        assert_eq!(columns.len(), 3);
        assert_eq!(columns[0].id, "subject");
        assert_eq!(columns[0].label, "Subject");
        assert!(
            !columns[0].numeric && columns[2].numeric,
            "the age column is right-aligned"
        );
        let entry = titled("Printer on fire", "t/17");
        assert_eq!(
            registry.cell(&location, &entry, "subject"),
            "Printer on fire"
        );
        assert_eq!(registry.cell(&location, &entry, "priority"), "high");
        // An undeclared id answers with an empty cell rather than a guess.
        assert_eq!(registry.cell(&location, &entry, "size"), "");
    }

    #[test]
    fn a_source_can_offer_its_own_row_verbs() {
        let mut registry = SourceRegistry::new();
        registry.register(Box::new(TicketDesk));
        let actions = registry.actions(&Location::plugin("tickets", ""), &entry_for("t/17"));
        let commands: Vec<&str> = actions.iter().map(|a| a.command.as_str()).collect();
        assert_eq!(commands, vec!["reply", "assign:me"]);
    }

    #[test]
    fn a_place_allows_only_what_its_source_declares() {
        let mut registry = SourceRegistry::new();
        registry.register(Box::new(TicketDesk));
        let tickets = registry.capabilities(&Location::plugin("tickets", ""));
        assert!(tickets.view, "a ticket can be read");
        assert!(
            !tickets.delete && !tickets.copy && !tickets.create,
            "and nothing else: the default is no"
        );
        // A local directory keeps the ordinary file operations; the policy decides from there.
        let local = registry.capabilities(&Location::local(std::path::Path::new("/tmp")));
        assert!(local.delete && local.copy && local.rename);
    }

    #[test]
    fn a_plugin_location_with_no_source_allows_nothing() {
        let registry = SourceRegistry::new();
        let capabilities = registry.capabilities(&Location::plugin("ghost", ""));
        assert_eq!(capabilities, SourceCapabilities::default());
        assert!(!capabilities.view && !capabilities.delete);
    }

    #[test]
    fn commands_map_onto_the_operations_a_source_can_vary_on() {
        assert_eq!(
            SourceOperation::from_command("cm_Delete"),
            Some(SourceOperation::Delete)
        );
        assert_eq!(
            SourceOperation::from_command("cm_MkDir"),
            Some(SourceOperation::Create)
        );
        assert_eq!(
            SourceOperation::from_command("cm_RenameOnly"),
            Some(SourceOperation::Move)
        );
        assert_eq!(
            SourceOperation::from_command("cm_View"),
            Some(SourceOperation::View)
        );
        // Commands about the panel, not its rows, are nobody's business but the panel's.
        assert_eq!(SourceOperation::from_command("cm_RereadSource"), None);
        assert_eq!(SourceOperation::from_command("cm_SrcBySize"), None);
        assert_eq!(SourceOperation::from_command("cm_Exit"), None);
    }

    #[test]
    fn a_writable_source_allows_every_operation_it_names() {
        let writable = SourceCapabilities::writable();
        for operation in [
            SourceOperation::Copy,
            SourceOperation::Move,
            SourceOperation::Create,
            SourceOperation::Delete,
            SourceOperation::Rename,
            SourceOperation::View,
        ] {
            assert!(writable.allows(operation), "{operation:?}");
        }
        let read_only = SourceCapabilities::read_only();
        assert!(read_only.allows(SourceOperation::View));
        assert!(!read_only.allows(SourceOperation::Delete));
        assert!(!read_only.allows(SourceOperation::Create));
    }

    /// A photo catalogue: clicking a row shows it in the *other* panel instead of going deeper,
    /// and the source has one verb the panel could never invent.
    struct PhotoCatalog;

    impl PanelSource for PhotoCatalog {
        fn id(&self) -> &str {
            "photocraft"
        }

        fn title(&self) -> &str {
            "PhotoCraft"
        }

        fn list(&self, path: &str) -> CoreResult<Vec<Entry>> {
            match path {
                "" => Ok(vec![
                    Entry::new("sunset.jpg", PathBuf::from("p/1"), EntryKind::File, 1),
                    Entry::new("harbour.jpg", PathBuf::from("p/2"), EntryKind::File, 1),
                ]),
                "p/1" | "p/2" => Ok(vec![Entry::new(
                    "full-size",
                    PathBuf::from(format!("{path}/full")),
                    EntryKind::File,
                    4096,
                )]),
                _ => Err(CoreError::Invalid(format!("no such photo: {path}"))),
            }
        }

        fn open(&self, entry: &Entry) -> CoreResult<SourceAction> {
            Ok(SourceAction::Preview(self.location_of(entry)))
        }

        fn actions(&self, _entry: &Entry) -> Vec<SourceMenuAction> {
            vec![SourceMenuAction::new("Develop", "develop")]
        }

        fn run(&self, command: &str) -> CoreResult<()> {
            match command {
                "develop" => Ok(()),
                other => Err(CoreError::Invalid(format!("unknown verb: {other}"))),
            }
        }
    }

    #[test]
    fn clicking_a_row_can_pair_the_other_panel_instead_of_going_deeper() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.sources.register(Box::new(PhotoCatalog));
        model.focus = Side::Left;
        model
            .open_in(Side::Left, Location::plugin("photocraft", ""))
            .expect("the catalogue opens");
        assert_eq!(model.panel(Side::Left).entries().len(), 2);
        // Point at the first photo so the assertion does not depend on a restored cursor.
        model.panel_mut(Side::Left).set_cursor(0);
        let expected = model.panel(Side::Left).entries()[0].path.clone();
        // The source says "preview", so the row lands in the other panel and focus follows it.
        let outcome = model.enter();
        assert!(matches!(outcome, CommandOutcome::Ok), "{outcome:?}");
        assert_eq!(model.focus, Side::Right);
        assert_eq!(
            model.panel(Side::Right).location().plugin_parts(),
            Some(("photocraft", expected.to_string_lossy().as_ref()))
        );
        assert_eq!(model.panel(Side::Right).entries().len(), 1);
        // The source panel is still where it was, so the pairing can be walked row by row.
        assert_eq!(
            model.panel(Side::Left).location().plugin_parts(),
            Some(("photocraft", ""))
        );
    }

    #[test]
    fn a_source_verb_runs_and_the_listing_is_re_read() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.sources.register(Box::new(PhotoCatalog));
        model
            .open_in(Side::Left, Location::plugin("photocraft", ""))
            .expect("open");
        let verbs = model.source_actions(Side::Left);
        assert_eq!(verbs.len(), 1);
        assert_eq!(verbs[0].command, "develop");
        assert!(matches!(
            model.run_source_command(Side::Left, "develop"),
            CommandOutcome::Ok
        ));
        // A verb the source does not know is an error, not a silent success.
        match model.run_source_command(Side::Left, "launch-missiles") {
            CommandOutcome::Failed(message) => assert!(message.contains("unknown verb")),
            other => panic!("expected a failure, got {other:?}"),
        }
        // Rows of a local directory offer no source verbs at all.
        assert!(model.source_actions(Side::Right).is_empty());
    }

    #[test]
    fn the_command_gate_refuses_what_the_place_does_not_allow() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.sources.register(Box::new(PhotoCatalog));
        model.focus = Side::Left;
        model
            .open_in(Side::Left, Location::plugin("photocraft", ""))
            .expect("open");
        // Nothing about the click path changes: the model refuses at the door.
        match model.execute("cm_Delete") {
            CommandOutcome::Failed(message) => {
                // The test translator echoes keys, so the message is the refusal's catalogue key.
                assert_eq!(message, "cmd.not_available_here");
            }
            other => panic!("delete over a read-only source must fail, got {other:?}"),
        }
        // And no dialog was opened: nothing is waiting for an answer that could never help.
        assert!(
            !model.has_dialog(),
            "a refused command opens no dialog, not even a confirmation"
        );
        // Refresh is still allowed, because it is about the panel and not about its rows.
        assert!(matches!(
            model.execute("cm_RereadSource"),
            CommandOutcome::Ok
        ));
    }

    fn entry_for(path: &str) -> Entry {
        Entry::new("row", std::path::PathBuf::from(path), EntryKind::File, 0)
    }

    fn titled(name: &str, path: &str) -> Entry {
        Entry::new(name, std::path::PathBuf::from(path), EntryKind::File, 0)
    }
}

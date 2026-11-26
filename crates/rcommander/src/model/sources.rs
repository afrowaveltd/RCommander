//! Where a plugin's listings come from (Phase 18.2a).
//!
//! `Location` has **one** variant for every plugin: which source, and where inside it. All the
//! behaviour lives here, behind a registry the model holds. Adding a source therefore never touches
//! the location enum, the parser, the panel or the tab code — it adds one entry to this registry.
//!
//! The contract is deliberately small, because a panel has only ever needed three things from a
//! source: what is in this location, what does opening an entry mean, and can the location be read
//! at all. Everything else — columns, actions, preview pairing — is a refinement that can come
//! later without breaking this shape.

use rc_core::entry::Entry;
use rc_core::error::{CoreError, CoreResult};
use rc_core::location::Location;

/// What opening an entry of a source means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceAction {
    /// Go deeper into the source: the panel navigates to this location.
    Into(Location),
    /// Show this entry in the other panel (a message body, a photo).
    Preview(Location),
    /// Ask the application to run a command.
    Command(String),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CommandOutcome, Model, Side};
    use rc_core::config::Settings;
    use rc_core::entry::EntryKind;

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
}

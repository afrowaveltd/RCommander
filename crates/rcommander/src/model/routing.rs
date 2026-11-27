//! The model registry page and discovery in the model (Phase 16.20).
//!
//! The catalogue is a settings document, so the app edits it in place like every other page of
//! the options dialog. What lives here is the part that is not drawing:
//!
//! * adding, removing and validating a provider connection;
//! * turning the egress switch on and off;
//! * asking one connection what it currently advertises and recording the answer as evidence
//!   (`rc_core::providers::observe`), which never overwrites an owner decision;
//! * a one-line summary of what the catalogue holds, for the page and the status line.
//!
//! Discovery is *not* a silent rewrite: a newly advertised model arrives switched off, a model the
//! server stopped listing is reported rather than deleted, and a profile that cannot be validated
//! is refused before anything is stored.

use super::{CommandOutcome, Model};
use rc_core::providers::{self, ObservedModel, ProviderConnection};
use rc_llm::Provider;

impl Model {
    /// Every configured connection, in the order the page lists them.
    pub fn catalogue_connections(&self) -> &[ProviderConnection] {
        &self.settings.agent.model_catalog.connections
    }

    /// Add a connection to the live catalogue, refusing an id that is already taken.
    pub fn add_catalogue_connection(
        &mut self,
        connection: ProviderConnection,
    ) -> Result<(), String> {
        let mut catalogue = std::mem::take(&mut self.settings.agent.model_catalog);
        let outcome = self.add_connection_to(&mut catalogue, connection);
        self.settings.agent.model_catalog = catalogue;
        match outcome {
            Ok(()) => {
                self.mark_settings_dirty();
                Ok(())
            }
            Err(message) => Err(message),
        }
    }

    /// Add a connection to whichever catalogue the caller holds (the options dialog edits a draft).
    pub fn add_connection_to(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        connection: ProviderConnection,
    ) -> Result<(), String> {
        if let Err(error) = connection.validate() {
            return Err(error.to_string());
        }
        if catalogue
            .connections
            .iter()
            .any(|existing| existing.id == connection.id)
        {
            let message = self.tf("ai.route.duplicate_id", &[("id", &connection.id)]);
            return Err(message);
        }
        catalogue.connections.push(connection);
        Ok(())
    }

    /// A new connection with a free id and the local default URL, ready for the page to edit.
    pub fn new_catalogue_connection(&self) -> ProviderConnection {
        self.new_connection_for(&self.settings.agent.model_catalog)
    }

    /// The same, for a catalogue the caller holds.
    pub fn new_connection_for(
        &self,
        catalogue: &rc_core::model_catalog::ModelCatalog,
    ) -> ProviderConnection {
        let mut index = catalogue.connections.len() + 1;
        loop {
            let id = format!("connection-{index}");
            if !catalogue
                .connections
                .iter()
                .any(|connection| connection.id == id)
            {
                return ProviderConnection::new(id, "http://127.0.0.1:11434");
            }
            index += 1;
        }
    }

    /// Remove a connection; models that pointed at it keep their evidence and stop resolving.
    ///
    /// The models are deliberately left in place: deleting a connection must not silently delete
    /// the owner's price, quotas or evidence for the models behind it.
    pub fn remove_catalogue_connection(&mut self, id: &str) -> bool {
        if Model::remove_connection_from(&mut self.settings.agent.model_catalog, id) {
            self.mark_settings_dirty();
            true
        } else {
            false
        }
    }

    /// Remove a connection from whichever catalogue the caller holds.
    pub fn remove_connection_from(
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        id: &str,
    ) -> bool {
        let before = catalogue.connections.len();
        catalogue
            .connections
            .retain(|connection| connection.id != id);
        catalogue.connections.len() != before
    }

    /// Turn the egress switch on or off. Only the owner may do this.
    pub fn set_allow_remote_models(&mut self, allow: bool) {
        if self.settings.agent.allow_remote_models != allow {
            self.settings.agent.allow_remote_models = allow;
            self.mark_settings_dirty();
        }
    }

    pub fn allow_remote_models(&self) -> bool {
        self.settings.agent.allow_remote_models
    }

    /// Ask one connection what it currently offers and record the answer as evidence.
    ///
    /// A provider that does not answer changes nothing: a failed list is a failed list, not an
    /// empty catalogue. Models the server stopped advertising are reported, never removed.
    pub fn discover_models(&mut self, connection_id: &str) -> CommandOutcome {
        let mut catalogue = std::mem::take(&mut self.settings.agent.model_catalog);
        let outcome = self.discover_connection_in(&mut catalogue, connection_id);
        self.settings.agent.model_catalog = catalogue;
        match outcome {
            Ok(note) => {
                self.mark_settings_dirty();
                self.report(note);
                CommandOutcome::Ok
            }
            Err(message) => {
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// Discovery for a catalogue the caller holds (the options dialog edits a draft).
    ///
    /// Returns the sentence describing the run, or the reason it did nothing.
    pub fn discover_connection_in(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        connection_id: &str,
    ) -> Result<String, String> {
        let Some(connection) = catalogue
            .connections
            .iter()
            .find(|connection| connection.id == connection_id)
            .cloned()
        else {
            return Err(self.tf("ai.route.unknown_connection", &[("id", connection_id)]));
        };
        let provider = self.provider_for_connection(&connection)?;
        let listed = provider.list_models().map_err(|error| {
            self.tf(
                "ai.route.discover_failed",
                &[("id", connection_id), ("reason", &error.to_string())],
            )
        })?;
        let observed = observed_from_provider(&listed);
        let now = chrono::Local::now().timestamp();
        let report = providers::observe(catalogue, connection_id, &observed, now)
            .map_err(|error| error.to_string())?;
        // Discovery must never leave an unsaveable profile behind.
        catalogue.validate().map_err(|error| format!("{error:?}"))?;
        let mut note = self.tf(
            "ai.route.discovered",
            &[
                ("id", connection_id),
                ("added", &report.added.len().to_string()),
                ("refreshed", &report.refreshed.len().to_string()),
                ("missing", &report.missing.len().to_string()),
            ],
        );
        if !report.unmapped.is_empty() {
            let names: Vec<&str> = report.unmapped.iter().map(String::as_str).collect();
            note.push_str(&self.tf("ai.route.unmapped", &[("names", &names.join(", "))]));
        }
        // Newly discovered models arrive switched off, which is worth saying out loud.
        if !report.added.is_empty() {
            note.push_str(&self.t("ai.route.discovered_off"));
        }
        Ok(note)
    }

    /// A provider for one configured connection: the injected one in tests, the real client
    /// otherwise. Refuses a URL the transport cannot dial instead of failing at connect time.
    pub fn provider_for_connection(
        &self,
        connection: &ProviderConnection,
    ) -> Result<std::sync::Arc<dyn Provider>, String> {
        if let Some(provider) = &self.ai_provider {
            return Ok(provider.clone());
        }
        rc_core::providers::normalize_base_url(&connection.base_url)
            .map_err(|error| error.to_string())?;
        let built = rc_runtime::LlmBackend {
            // Listing models is a small request; a dead server must not freeze the page.
            timeout: Some(std::time::Duration::from_secs(5)),
        }
        .provider_for_connection(connection);
        Ok(std::sync::Arc::from(built))
    }

    /// A short description of what the catalogue holds, for the page's summary line.
    pub fn catalogue_summary(&self) -> String {
        self.catalogue_summary_for(&self.settings.agent.model_catalog)
    }

    /// The same summary for a catalogue the caller holds (the options dialog edits a draft).
    pub fn catalogue_summary_for(
        &self,
        catalogue: &rc_core::model_catalog::ModelCatalog,
    ) -> String {
        let enabled = catalogue
            .models
            .iter()
            .filter(|model| model.enabled)
            .count();
        let local = catalogue.models.iter().filter(|model| model.local).count();
        let unmapped: usize = catalogue
            .models
            .iter()
            .filter_map(|model| model.observed.as_ref())
            .map(|observed| observed.unmapped.len())
            .sum();
        self.tf(
            "ai.route.summary",
            &[
                ("connections", &catalogue.connections.len().to_string()),
                ("models", &catalogue.models.len().to_string()),
                ("enabled", &enabled.to_string()),
                ("local", &local.to_string()),
                ("unmapped", &unmapped.to_string()),
            ],
        )
    }

    /// The validation state of the catalogue, as a message the page can show.
    pub fn catalogue_issue(&self) -> Option<String> {
        self.settings
            .agent
            .model_catalog
            .validate()
            .err()
            .map(|error| format!("{error:?}"))
    }
}

/// What a provider advertised, in the form the catalogue records.
///
/// Only the fields a provider actually reports are carried over; nothing is inferred from a model's
/// name or size.
pub fn observed_from_provider(models: &[rc_llm::Model]) -> Vec<ObservedModel> {
    models
        .iter()
        .filter(|model| !model.id.trim().is_empty())
        .map(|model| ObservedModel {
            model_id: model.id.clone(),
            capabilities: model.capabilities.iter().cloned().collect(),
            parameters: model.parameters.clone(),
            size_bytes: model.size,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rc_core::config::Settings;
    use rc_core::location::Location;
    use rc_llm::{ChatReply, ChatRequest, LlmError, Model as LlmModel};
    use std::sync::{Arc, Mutex};

    /// A provider that lists a fixed catalogue and never answers a question.
    struct ListingProvider {
        models: Vec<LlmModel>,
        error: Option<String>,
    }

    impl std::fmt::Debug for ListingProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("ListingProvider")
        }
    }

    impl Provider for ListingProvider {
        fn kind(&self) -> &'static str {
            "ollama"
        }
        fn base_url(&self) -> &str {
            "http://127.0.0.1:11434"
        }
        fn list_models(&self) -> Result<Vec<LlmModel>, LlmError> {
            match &self.error {
                Some(error) => Err(LlmError::new(error.clone())),
                None => Ok(self.models.clone()),
            }
        }
        fn chat(&self, _request: &ChatRequest) -> Result<ChatReply, LlmError> {
            Err(LlmError::new("this provider only lists models"))
        }
    }

    /// The real English pack, so a failed assertion shows the sentence a person would read.
    fn english_model(dir: &std::path::Path) -> Model {
        let locale_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../rc-i18n/locales");
        let strings = crate::i18n::TranslatorStrings::discover(&locale_dir, "en");
        Model::boot(
            Settings::default(),
            Box::new(strings),
            Location::local(dir),
            Location::local(dir),
        )
    }

    fn model_listing(models: Vec<LlmModel>) -> (Model, Mutex<Vec<String>>) {
        let dir = tempfile::tempdir().unwrap();
        let mut model = english_model(dir.path());
        model.config_dir_override = Some(dir.path().to_path_buf());
        model.settings.agent.model_catalog =
            rc_core::model_catalog::ModelCatalog::with_legacy_connection("ollama", "").unwrap();
        model.ai_provider = Some(Arc::new(ListingProvider {
            models,
            error: None,
        }));
        (model, Mutex::new(Vec::new()))
    }

    fn listed(id: &str, capabilities: &[&str]) -> LlmModel {
        LlmModel {
            id: id.into(),
            size: Some(1_024),
            parameters: Some("1B".into()),
            capabilities: capabilities.iter().map(|c| (*c).to_string()).collect(),
        }
    }

    #[test]
    fn a_connection_is_added_with_a_free_id_and_refused_when_it_collides() {
        let (mut model, _) = model_listing(Vec::new());
        let fresh = model.new_catalogue_connection();
        assert_eq!(fresh.id, "connection-2", "the legacy id is taken");
        assert!(fresh.looks_local());
        model.add_catalogue_connection(fresh.clone()).unwrap();
        assert_eq!(model.catalogue_connections().len(), 2);
        assert!(model.settings_dirty);
        let error = model.add_catalogue_connection(fresh).unwrap_err();
        assert!(error.contains("connection-2"), "message: {error}");
        // A URL the transport cannot dial is refused before it is stored.
        let mut broken = model.new_catalogue_connection();
        broken.base_url = "https://models.example.com".into();
        assert!(model.add_catalogue_connection(broken).is_err());
        assert_eq!(model.catalogue_connections().len(), 2);
    }

    #[test]
    fn discovery_records_evidence_and_switches_new_models_off() {
        let (mut model, _) = model_listing(vec![
            listed("tiny:1b", &["completion", "tools", "sparkle"]),
            listed("big:7b", &["completion"]),
        ]);
        assert!(matches!(
            model.discover_models("default"),
            CommandOutcome::Ok
        ));
        let catalogue = &model.settings.agent.model_catalog;
        assert_eq!(catalogue.models.len(), 2);
        let tiny = catalogue
            .models
            .iter()
            .find(|model| model.model_id == "tiny:1b")
            .expect("the advertised model was recorded");
        assert!(!tiny.enabled, "a discovered model waits for the owner");
        assert!(!tiny.local, "locality is the owner's decision");
        let observed = tiny.observed.as_ref().expect("evidence is dated");
        assert_eq!(
            observed.unmapped,
            ["sparkle".to_string()].into_iter().collect(),
            "an unknown capability name is kept verbatim"
        );
        assert!(model.status.contains("default"), "{}", model.status);
        assert!(model.status.contains("sparkle"), "{}", model.status);
    }

    #[test]
    fn a_failed_listing_leaves_the_catalogue_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = english_model(dir.path());
        model.config_dir_override = Some(dir.path().to_path_buf());
        model.settings.agent.model_catalog =
            rc_core::model_catalog::ModelCatalog::with_legacy_connection("ollama", "").unwrap();
        model.ai_provider = Some(Arc::new(ListingProvider {
            models: Vec::new(),
            error: Some("connection refused".into()),
        }));
        assert!(matches!(
            model.discover_models("default"),
            CommandOutcome::Failed(_)
        ));
        assert!(model.settings.agent.model_catalog.models.is_empty());
        assert!(!model.settings_dirty, "a failure is not a change to save");
        assert!(
            model
                .last_error
                .clone()
                .unwrap_or_default()
                .contains("connection refused"),
            "the reason is reported"
        );
    }

    #[test]
    fn discovery_never_touches_a_price_a_quota_or_an_owner_choice() {
        let (mut model, _) = model_listing(vec![listed("tiny:1b", &["completion"])]);
        let catalogue = &mut model.settings.agent.model_catalog;
        catalogue.models.push(rc_core::model_catalog::ModelProfile {
            id: "mine".into(),
            connection_id: "default".into(),
            model_id: "tiny:1b".into(),
            enabled: true,
            local: true,
            price_class: Some(0),
            quota_ids: Vec::new(),
            ..rc_core::model_catalog::ModelProfile::default()
        });
        model.discover_models("default");
        let mine = model
            .settings
            .agent
            .model_catalog
            .models
            .iter()
            .find(|entry| entry.id == "mine")
            .unwrap();
        assert!(mine.enabled && mine.local);
        assert_eq!(mine.price_class, Some(0));
        assert!(mine.observed.is_some(), "the evidence is refreshed");
        assert_eq!(
            model.settings.agent.model_catalog.models.len(),
            1,
            "the same binding is not added twice"
        );
    }

    #[test]
    fn a_connection_can_be_removed_without_deleting_its_models() {
        let (mut model, _) = model_listing(vec![listed("tiny:1b", &["completion"])]);
        model.discover_models("default");
        assert_eq!(model.settings.agent.model_catalog.models.len(), 1);
        assert!(model.remove_catalogue_connection("default"));
        assert!(model.catalogue_connections().is_empty());
        assert_eq!(
            model.settings.agent.model_catalog.models.len(),
            1,
            "removing a server does not delete the owner's model configuration"
        );
        assert!(!model.remove_catalogue_connection("default"));
        // A model whose connection is gone no longer resolves, and the page can say so.
        assert!(model.catalogue_issue().is_some() || !model.routing_enabled());
    }

    #[test]
    fn the_summary_counts_what_the_page_shows() {
        let (mut model, _) = model_listing(vec![listed("tiny:1b", &["completion"])]);
        model.discover_models("default");
        model.settings.agent.model_catalog.models[0].enabled = true;
        model.settings.agent.model_catalog.models[0].local = true;
        let summary = model.catalogue_summary();
        assert!(summary.contains('1'), "message: {summary}");
        assert!(!model.allow_remote_models(), "egress is off by default");
        model.set_allow_remote_models(true);
        assert!(model.allow_remote_models());
        assert!(model.settings_dirty);
    }

    #[test]
    fn an_advertised_model_becomes_an_observed_record() {
        let observed = observed_from_provider(&[listed("a:1b", &["completion", "vision"])]);
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].model_id, "a:1b");
        assert_eq!(observed[0].size_bytes, Some(1_024));
        assert_eq!(
            observed[0].capabilities,
            ["completion".to_string(), "vision".to_string()]
                .into_iter()
                .collect()
        );
        // A nameless entry is dropped rather than recorded as an unusable model.
        assert!(observed_from_provider(&[listed("  ", &[])]).is_empty());
    }
}

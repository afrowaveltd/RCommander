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

/// The editing half of the wizard: classification, limits and routes.
///
/// Every setter goes through [`Model::edit_catalogue`], which applies the change to a copy and
/// keeps it only when the whole catalogue still validates. A half-edited profile is therefore
/// impossible to leave behind: an unknown quota id, a route naming a model that is not there, or a
/// price class on a remote model is refused with its reason and nothing changes.
impl Model {
    /// Apply `edit` to `catalogue`, keeping it only when the result validates.
    pub fn edit_catalogue<F>(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        edit: F,
    ) -> Result<(), String>
    where
        F: FnOnce(&mut rc_core::model_catalog::ModelCatalog),
    {
        let mut candidate = catalogue.clone();
        edit(&mut candidate);
        match candidate.validate() {
            Ok(()) => {
                *catalogue = candidate;
                Ok(())
            }
            Err(error) => Err(self.tf("ai.route.invalid", &[("reason", &format!("{error:?}"))])),
        }
    }

    /// Enable or switch off one model.
    pub fn set_model_enabled(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        model_id: &str,
        enabled: bool,
    ) -> Result<(), String> {
        self.edit_catalogue(catalogue, |catalogue| {
            if let Some(model) = catalogue.models.iter_mut().find(|m| m.id == model_id) {
                model.enabled = enabled;
            }
        })
    }

    /// Say whether the owner hosts this model. Locality is never inferred from an address.
    pub fn set_model_local(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        model_id: &str,
        local: bool,
    ) -> Result<(), String> {
        self.edit_catalogue(catalogue, |catalogue| {
            if let Some(model) = catalogue.models.iter_mut().find(|m| m.id == model_id) {
                model.local = local;
                // Price class 0 means "local and free"; a remote model cannot keep it.
                if !local && model.price_class == Some(0) {
                    model.price_class = None;
                }
            }
        })
    }

    /// The owner's relative price class, 0 (local) to 5 (most expensive), or unclassified.
    pub fn set_model_price(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        model_id: &str,
        price: Option<u8>,
    ) -> Result<(), String> {
        self.edit_catalogue(catalogue, |catalogue| {
            if let Some(model) = catalogue.models.iter_mut().find(|m| m.id == model_id) {
                model.price_class = price;
            }
        })
    }

    /// Effective context and maximum output for one configured model.
    pub fn set_model_limits(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        model_id: &str,
        context: Option<u64>,
        max_output: Option<u64>,
    ) -> Result<(), String> {
        self.edit_catalogue(catalogue, |catalogue| {
            if let Some(model) = catalogue.models.iter_mut().find(|m| m.id == model_id) {
                model.context_tokens = context;
                model.max_output_tokens = max_output;
            }
        })
    }

    /// The quotas a model draws on, from a comma-separated list (`shared-hour, daily`).
    pub fn set_model_quotas(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        model_id: &str,
        text: &str,
    ) -> Result<(), String> {
        let ids = split_list(text);
        self.edit_catalogue(catalogue, |catalogue| {
            if let Some(model) = catalogue.models.iter_mut().find(|m| m.id == model_id) {
                model.quota_ids = ids;
            }
        })
    }

    /// Add a quota with a free id and a documented default; returns the id for the page to show.
    pub fn add_quota(&self, catalogue: &mut rc_core::model_catalog::ModelCatalog) -> String {
        let mut index = catalogue.quotas.len() + 1;
        loop {
            let id = format!("quota-{index}");
            if !catalogue.quotas.iter().any(|quota| quota.id == id) {
                let quota = rc_core::model_catalog::TokenQuota {
                    id: id.clone(),
                    tokens: 100_000,
                    period: rc_core::model_catalog::QuotaPeriod::Month,
                    window: rc_core::model_catalog::QuotaWindow::Calendar,
                };
                catalogue.quotas.push(quota);
                return id;
            }
            index += 1;
        }
    }

    /// Remove a quota and detach it from every model that used it.
    pub fn remove_quota(
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        quota_id: &str,
    ) -> bool {
        let before = catalogue.quotas.len();
        catalogue.quotas.retain(|quota| quota.id != quota_id);
        if catalogue.quotas.len() == before {
            return false;
        }
        for model in catalogue.models.iter_mut() {
            model.quota_ids.retain(|id| id != quota_id);
        }
        true
    }

    /// Set a quota's allowance and reset semantics.
    pub fn set_quota(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        quota_id: &str,
        tokens: u64,
        period: rc_core::model_catalog::QuotaPeriod,
        window: rc_core::model_catalog::QuotaWindow,
    ) -> Result<(), String> {
        self.edit_catalogue(catalogue, |catalogue| {
            if let Some(quota) = catalogue.quotas.iter_mut().find(|q| q.id == quota_id) {
                quota.tokens = tokens;
                quota.period = period;
                quota.window = window;
            }
        })
    }

    /// Add an empty route for `task`; refuses a duplicate name.
    pub fn add_route(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        task: &str,
    ) -> Result<(), String> {
        let task = task.trim().to_string();
        if task.is_empty() {
            return Err(self.t("ai.route.task_needed"));
        }
        if catalogue.routes.iter().any(|route| route.task == task) {
            return Err(self.tf("ai.route.duplicate_route", &[("task", &task)]));
        }
        self.edit_catalogue(catalogue, |catalogue| {
            catalogue
                .routes
                .push(rc_core::model_catalog::RoutingPreference {
                    task,
                    ..rc_core::model_catalog::RoutingPreference::default()
                });
        })
    }

    /// Remove a route (the models it named stay in the catalogue).
    pub fn remove_route(catalogue: &mut rc_core::model_catalog::ModelCatalog, task: &str) -> bool {
        let before = catalogue.routes.len();
        catalogue.routes.retain(|route| route.task != task);
        catalogue.routes.len() != before
    }

    /// The ordered models of a route, from a comma-separated list; the first one wins.
    pub fn set_route_models(
        &self,
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        task: &str,
        text: &str,
    ) -> Result<(), String> {
        let ids = split_list(text);
        self.edit_catalogue(catalogue, |catalogue| {
            if let Some(route) = catalogue.routes.iter_mut().find(|r| r.task == task) {
                route.model_ids = ids;
            }
        })
    }

    /// What the route does when nothing in it is eligible.
    pub fn set_route_action(
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        task: &str,
        action: rc_core::model_catalog::ExhaustedAction,
    ) {
        if let Some(route) = catalogue.routes.iter_mut().find(|r| r.task == task) {
            route.on_exhausted = action;
        }
    }

    /// Whether a verified equivalent binding may replace an unavailable primary.
    pub fn set_route_prefer_equivalent(
        catalogue: &mut rc_core::model_catalog::ModelCatalog,
        task: &str,
        prefer: bool,
    ) {
        if let Some(route) = catalogue.routes.iter_mut().find(|r| r.task == task) {
            route.prefer_equivalent = prefer;
        }
    }

    /// Ask the router the same question the chat asks, for a sample request.
    ///
    /// This is the wizard's "why": it returns the decision, including which candidates were
    /// skipped and why, so a page can show the consequence of an edit before it is saved.
    pub fn preview_route(
        &self,
        catalogue: &rc_core::model_catalog::ModelCatalog,
        ledger: Option<&rc_core::quota::QuotaLedger>,
        task: &str,
        prompt_tokens: u64,
    ) -> rc_core::routing::Decision {
        let workload = rc_core::routing::Workload {
            task: task.to_string(),
            prompt_tokens,
            max_output_tokens: rc_runtime::dispatch::DEFAULT_MAX_OUTPUT_TOKENS,
            requires: [rc_core::model_catalog::Capability::Text]
                .into_iter()
                .collect(),
            allow_remote: self.settings.agent.allow_remote_models,
        };
        match ledger {
            Some(ledger) => rc_core::routing::choose(
                catalogue,
                ledger,
                &workload,
                chrono::Local::now().timestamp(),
            ),
            None => {
                let empty = rc_core::quota::QuotaLedger::new(&catalogue.quotas);
                match empty {
                    Ok(empty) => rc_core::routing::choose(
                        catalogue,
                        &empty,
                        &workload,
                        chrono::Local::now().timestamp(),
                    ),
                    // A catalogue whose limits do not even load cannot be previewed; the caller
                    // shows the validation error instead of a made-up decision.
                    Err(_) => rc_core::routing::Decision::NoRoute {
                        task: task.to_string(),
                        fallback_task: None,
                    },
                }
            }
        }
    }
}

impl Model {
    /// One sentence describing what the router would do for `task` right now.
    ///
    /// The page shows this under every route, so an edit is visible before it is saved and a
    /// refusal is never a surprise at chat time.
    pub fn preview_line(
        &self,
        catalogue: &rc_core::model_catalog::ModelCatalog,
        ledger: Option<&rc_core::quota::QuotaLedger>,
        task: &str,
    ) -> String {
        let decision = self.preview_route(catalogue, ledger, task, 1_000);
        let locality = |local: bool| {
            if local {
                self.t("ai.route.local")
            } else {
                self.t("ai.route.remote")
            }
        };
        let price = |price: Option<u8>| match price {
            Some(price) => price.to_string(),
            None => self.t("ai.route.unpriced"),
        };
        let decision_text = match &decision {
            rc_core::routing::Decision::Chosen {
                model_id, reason, ..
            } => self.tf(
                "ai.route.preview_chosen",
                &[
                    ("model", model_id),
                    ("preference", &reason.preference.to_string()),
                    ("locality", &locality(reason.local)),
                    ("price", &price(reason.price_class)),
                ],
            ),
            rc_core::routing::Decision::NoRoute { task, .. } => {
                self.tf("ai.route.no_route", &[("task", task)])
            }
            other => {
                let mut text = self.t("ai.route.preview_none");
                let skipped = other.skipped();
                if !skipped.is_empty() {
                    let reasons: Vec<String> = skipped
                        .iter()
                        .map(|skip| {
                            format!("{} ({})", skip.model_id, self.refusal_message(&skip.reason))
                        })
                        .collect();
                    text.push_str(&self.tf("ai.route.skips", &[("reasons", &reasons.join("; "))]));
                }
                text
            }
        };
        self.tf("ai.route.preview", &[("decision", &decision_text)])
    }
}

/// Split a comma- or semicolon-separated list, dropping blanks and duplicates.
pub fn split_list(text: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    text.split([',', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter(|part| seen.insert((*part).to_string()))
        .map(str::to_string)
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
    fn classification_and_limits_are_validated_before_they_are_kept() {
        let (model, _) = model_listing(Vec::new());
        let mut catalogue =
            rc_core::model_catalog::ModelCatalog::with_legacy_connection("ollama", "").unwrap();
        catalogue.models.push(rc_core::model_catalog::ModelProfile {
            id: "mine".into(),
            connection_id: "default".into(),
            model_id: "tiny:1b".into(),
            enabled: false,
            ..rc_core::model_catalog::ModelProfile::default()
        });
        model
            .set_model_enabled(&mut catalogue, "mine", true)
            .unwrap();
        assert!(catalogue.models[0].enabled);
        model.set_model_local(&mut catalogue, "mine", true).unwrap();
        model
            .set_model_price(&mut catalogue, "mine", Some(0))
            .unwrap();
        assert_eq!(catalogue.models[0].price_class, Some(0));
        // A price class of zero belongs to a local model only.
        model
            .set_model_local(&mut catalogue, "mine", false)
            .unwrap();
        assert_eq!(
            catalogue.models[0].price_class, None,
            "a remote model cannot stay in the local price class"
        );
        // An impossible limit is refused and the catalogue keeps its previous state.
        let before = catalogue.clone();
        assert!(
            model
                .set_model_limits(&mut catalogue, "mine", Some(100), Some(1_000))
                .is_err(),
            "an output reserve larger than the context is refused"
        );
        assert_eq!(catalogue, before);
        model
            .set_model_limits(&mut catalogue, "mine", Some(8_192), Some(512))
            .unwrap();
        assert_eq!(catalogue.models[0].max_output_tokens, Some(512));
        // A quota reference that does not exist is refused too.
        assert!(
            model
                .set_model_quotas(&mut catalogue, "mine", "ghost")
                .is_err()
        );
        assert!(catalogue.models[0].quota_ids.is_empty());
    }

    #[test]
    fn quotas_can_be_added_attached_and_removed_without_leaving_references() {
        let (model, _) = model_listing(Vec::new());
        let mut catalogue =
            rc_core::model_catalog::ModelCatalog::with_legacy_connection("ollama", "").unwrap();
        catalogue.models.push(rc_core::model_catalog::ModelProfile {
            id: "mine".into(),
            connection_id: "default".into(),
            model_id: "tiny:1b".into(),
            ..rc_core::model_catalog::ModelProfile::default()
        });
        let id = model.add_quota(&mut catalogue);
        assert_eq!(id, "quota-1");
        model
            .set_model_quotas(&mut catalogue, "mine", " quota-1 ,quota-1 ")
            .unwrap();
        assert_eq!(catalogue.models[0].quota_ids, vec!["quota-1".to_string()]);
        assert_eq!(catalogue.validate(), Ok(()));
        model
            .set_quota(
                &mut catalogue,
                "quota-1",
                5_000,
                rc_core::model_catalog::QuotaPeriod::Hour,
                rc_core::model_catalog::QuotaWindow::Rolling,
            )
            .unwrap();
        assert_eq!(catalogue.quotas[0].tokens, 5_000);
        // Removing the quota detaches it, so nothing dangles.
        assert!(Model::remove_quota(&mut catalogue, "quota-1"));
        assert!(catalogue.models[0].quota_ids.is_empty());
        assert_eq!(catalogue.validate(), Ok(()));
        assert!(!Model::remove_quota(&mut catalogue, "quota-1"));
        // A zero allowance is refused by validation.
        let id = model.add_quota(&mut catalogue);
        assert!(
            model
                .set_quota(
                    &mut catalogue,
                    &id,
                    0,
                    rc_core::model_catalog::QuotaPeriod::Day,
                    rc_core::model_catalog::QuotaWindow::Calendar,
                )
                .is_err()
        );
        assert_eq!(catalogue.quotas[0].tokens, 100_000);
    }

    #[test]
    fn a_route_is_ordered_by_the_owner_and_previewed_before_it_is_saved() {
        let (model, _) = model_listing(Vec::new());
        let mut catalogue =
            rc_core::model_catalog::ModelCatalog::with_legacy_connection("ollama", "").unwrap();
        for (id, provider_model) in [("first", "a:1b"), ("second", "b:1b")] {
            catalogue.models.push(rc_core::model_catalog::ModelProfile {
                id: id.into(),
                connection_id: "default".into(),
                model_id: provider_model.into(),
                enabled: true,
                local: true,
                capabilities: [rc_core::model_catalog::Capability::Text]
                    .into_iter()
                    .collect(),
                ..rc_core::model_catalog::ModelProfile::default()
            });
        }
        model.add_route(&mut catalogue, "chat").unwrap();
        assert!(
            model.add_route(&mut catalogue, "chat").is_err(),
            "a duplicate is refused"
        );
        model
            .set_route_models(&mut catalogue, "chat", "second, first")
            .unwrap();
        assert_eq!(
            catalogue.routes[0].model_ids,
            vec!["second".to_string(), "first".to_string()]
        );
        let preview = model.preview_line(&catalogue, None, "chat");
        assert!(
            preview.contains("second"),
            "the first listed model wins: {preview}"
        );
        // A route naming a model that is not there is refused.
        let before = catalogue.clone();
        assert!(
            model
                .set_route_models(&mut catalogue, "chat", "ghost")
                .is_err()
        );
        assert_eq!(catalogue, before);
        Model::set_route_action(
            &mut catalogue,
            "chat",
            rc_core::model_catalog::ExhaustedAction::Wait,
        );
        Model::set_route_prefer_equivalent(&mut catalogue, "chat", true);
        assert_eq!(
            catalogue.routes[0].on_exhausted,
            rc_core::model_catalog::ExhaustedAction::Wait
        );
        assert!(catalogue.routes[0].prefer_equivalent);
        assert!(Model::remove_route(&mut catalogue, "chat"));
        assert!(!Model::remove_route(&mut catalogue, "chat"));
        // With no route there is nothing to preview, and the line says so.
        let preview = model.preview_line(&catalogue, None, "chat");
        assert!(
            preview.contains("no route"),
            "the line names the missing route: {preview}"
        );
    }

    #[test]
    fn a_workload_list_splits_on_commas_and_drops_repeats() {
        assert_eq!(
            split_list(" a, b ;; a ,,c "),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert!(split_list("  ").is_empty());
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

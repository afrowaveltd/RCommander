//! User-owned model catalogue and routing preferences (Phase 16.20).
//!
//! Metadata only: no network calls, credentials, quota accounting or execution. A runtime must
//! validate the catalogue and enforce privacy and budgets before dispatch — the durable token
//! ledger lives in [`crate::quota`], the admit/refuse verdict in [`crate::admission`], and
//! connections plus model discovery in [`crate::providers`].
//!
//! Two kinds of field live side by side here, and the difference matters:
//!
//! * **Owner decisions** — everything a person chose: enabled, locality, price class, quotas,
//!   equivalence group, routing order. Discovery never writes these.
//! * **Observed evidence** — [`ObservedMetadata`], recorded when a provider listed the model.
//!   It is provenance, not permission: it can widen the *interface features* a model offers, and
//!   it never implies a price, a locality, a quota or a grant.

use crate::providers::{ProviderConnection, ProviderKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Text,
    Coding,
    Reasoning,
    Translation,
    ToolCalling,
    ImageInput,
    ImageOutput,
    AudioInput,
    AudioOutput,
    Embeddings,
    StructuredOutput,
}

/// What a provider advertised about one of its models, and when.
///
/// This is evidence with a date, kept next to the owner's decisions rather than mixed into them:
/// a capability listed here may be used by admission (it is a provider's own statement about its
/// interface), while price, locality and quotas stay owner-only. An unrecognised advertised name
/// is kept verbatim so a person can see what the provider claims instead of the catalogue silently
/// rounding it away.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObservedMetadata {
    /// Unix seconds when the provider listed this model.
    pub observed_at: i64,
    /// Advertised interface features with a catalogue meaning.
    pub capabilities: BTreeSet<Capability>,
    /// Advertised names with no catalogue meaning, kept as evidence.
    pub unmapped: BTreeSet<String>,
    /// Advertised parameter count as text; never a quality score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<String>,
    /// Advertised size on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

/// Unknown means unknown, not supported and not a zero cost.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelProfile {
    /// Stable local catalogue ID; provider and model names can overlap.
    pub id: String,
    /// Reference to a configured provider connection, not a URL or secret.
    pub connection_id: String,
    pub model_id: String,
    /// Owner-verified equivalence group; never infer from names alone.
    pub equivalence_group: Option<String>,
    pub enabled: bool,
    /// Explicit owner classification; never infer locality from price.
    pub local: bool,
    /// Owner-defined relative price, 0..=5; None means not classified.
    pub price_class: Option<u8>,
    /// Effective context limit for this configured model, not parameter count.
    pub context_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub capabilities: BTreeSet<Capability>,
    /// Human-selected specialties; distinct from verified interface features.
    pub specialties: BTreeSet<String>,
    /// Multiple limits may apply; accounts can share a quota across models.
    pub quota_ids: Vec<String>,
    /// What the provider last advertised, when it was ever asked. `None` means no evidence: the
    /// entry is entirely the owner's, or the provider has not been listed yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed: Option<ObservedMetadata>,
}

impl ModelProfile {
    /// Capabilities a workload may rely on: what the owner declared plus what the provider
    /// advertised. Advertised interface features are evidence; price, locality and quotas are not.
    pub fn effective_capabilities(&self) -> BTreeSet<Capability> {
        let mut capabilities = self.capabilities.clone();
        if let Some(observed) = &self.observed {
            capabilities.extend(observed.capabilities.iter().copied());
        }
        capabilities
    }

    /// True when the provider confirmed the model offers `capability`.
    pub fn advertises(&self, capability: Capability) -> bool {
        self.observed
            .as_ref()
            .is_some_and(|observed| observed.capabilities.contains(&capability))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaPeriod {
    Hour,
    Day,
    Week,
    #[default]
    Month,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindow {
    Rolling,
    Calendar,
    #[default]
    ProviderDefined,
}

/// A configured limit, not a statement of current remaining allowance.
/// Reset rules, shared usage and provider-reported counters belong to the ledger.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TokenQuota {
    pub id: String,
    pub tokens: u64,
    pub period: QuotaPeriod,
    pub window: QuotaWindow,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExhaustedAction {
    #[default]
    Ask,
    Wait,
    Fallback,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingPreference {
    /// Workload/role identifier, e.g. "translation" or "default".
    pub task: String,
    /// Ordered catalogue IDs, never implicit cross-provider fallback.
    pub model_ids: Vec<String>,
    pub on_exhausted: ExhaustedAction,
    /// Prefer a qualified, explicitly listed equivalent binding on fallback.
    pub prefer_equivalent: bool,
    /// Optional aggregate token ceiling for temporary fallback; not a reservation.
    pub fallback_token_budget: Option<u64>,
    /// At the next safe checkpoint, prefer the original binding after recovery.
    pub return_to_primary: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelCatalog {
    /// Configured ways to reach a model server. [`crate::providers::resolve`] needs one to dial.
    ///
    /// An empty list is the pre-registry installation: settings that predate this field keep
    /// working, and a catalogue that *does* declare connections must resolve every model against
    /// one. Use [`ModelCatalog::with_legacy_connection`] to migrate.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<ProviderConnection>,
    pub models: Vec<ModelProfile>,
    pub quotas: Vec<TokenQuota>,
    pub routes: Vec<RoutingPreference>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogError {
    MissingIdentity,
    DuplicateModel,
    DuplicateQuota,
    DuplicateRoute,
    InvalidPrice,
    InvalidContext,
    InvalidQuota,
    UnknownQuota,
    UnknownModel,
    DuplicateReference,
    /// Two connections share an id.
    DuplicateConnection,
    /// A connection id is missing or padded with whitespace.
    InvalidConnection(String),
    /// A model names a connection the registry does not hold.
    UnknownConnection,
}

impl ModelCatalog {
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
            && self.models.is_empty()
            && self.quotas.is_empty()
            && self.routes.is_empty()
    }

    /// A catalogue that starts from the installation's single legacy provider.
    ///
    /// Enabling the registry must not lose the one server the owner already uses, so the legacy
    /// `provider_kind`/`provider_url` pair becomes a connection named `default`. A kind with no
    /// adapter is refused instead of being recorded as something the runtime cannot dial.
    pub fn with_legacy_connection(
        kind: &str,
        base_url: &str,
    ) -> Result<ModelCatalog, CatalogError> {
        let kind = ProviderKind::parse(kind).ok_or(CatalogError::InvalidConnection(kind.into()))?;
        let url = if base_url.trim().is_empty() {
            "http://127.0.0.1:11434".to_string()
        } else {
            base_url.trim().to_string()
        };
        let connection = ProviderConnection {
            id: "default".into(),
            kind,
            base_url: url,
            account: None,
            enabled: true,
        };
        connection
            .validate()
            .map_err(|error| CatalogError::InvalidConnection(error.to_string()))?;
        Ok(ModelCatalog {
            connections: vec![connection],
            ..ModelCatalog::default()
        })
    }

    /// Validate before any future use by a dispatcher. Does not imply authorization.
    pub fn validate(&self) -> Result<(), CatalogError> {
        let mut connections = BTreeSet::new();
        for connection in &self.connections {
            if connection.id.trim().is_empty() || connection.id.trim() != connection.id {
                return Err(CatalogError::InvalidConnection(connection.id.clone()));
            }
            connection
                .validate()
                .map_err(|error| CatalogError::InvalidConnection(error.to_string()))?;
            if !connections.insert(connection.id.as_str()) {
                return Err(CatalogError::DuplicateConnection);
            }
        }
        let registry = !self.connections.is_empty();
        let mut quotas = BTreeSet::new();
        for quota in &self.quotas {
            if quota.id.trim().is_empty() {
                return Err(CatalogError::MissingIdentity);
            }
            if quota.tokens == 0 {
                return Err(CatalogError::InvalidQuota);
            }
            if !quotas.insert(quota.id.as_str()) {
                return Err(CatalogError::DuplicateQuota);
            }
        }
        let mut models = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        for model in &self.models {
            if [
                model.id.as_str(),
                model.connection_id.as_str(),
                model.model_id.as_str(),
            ]
            .iter()
            .any(|id| id.trim().is_empty())
            {
                return Err(CatalogError::MissingIdentity);
            }
            if !models.insert(model.id.as_str())
                || !bindings.insert((model.connection_id.as_str(), model.model_id.as_str()))
            {
                return Err(CatalogError::DuplicateModel);
            }
            // Compatibility: an installation that has not migrated yet declares no connections,
            // and its single legacy provider still answers. Once a registry exists, a dangling
            // reference is a configuration error rather than something to discover at dispatch.
            if registry && !connections.contains(model.connection_id.as_str()) {
                return Err(CatalogError::UnknownConnection);
            }
            if model
                .price_class
                .is_some_and(|price| price > 5 || (price == 0 && !model.local))
            {
                return Err(CatalogError::InvalidPrice);
            }
            if model.context_tokens == Some(0)
                || model.max_output_tokens == Some(0)
                || matches!((model.context_tokens, model.max_output_tokens),
                    (Some(context), Some(output)) if output > context)
            {
                return Err(CatalogError::InvalidContext);
            }
            let mut seen = BTreeSet::new();
            for id in &model.quota_ids {
                if !quotas.contains(id.as_str()) {
                    return Err(CatalogError::UnknownQuota);
                }
                if !seen.insert(id.as_str()) {
                    return Err(CatalogError::DuplicateReference);
                }
            }
        }
        let mut tasks = BTreeSet::new();
        for route in &self.routes {
            if route.task.trim().is_empty() {
                return Err(CatalogError::MissingIdentity);
            }
            if !tasks.insert(route.task.as_str()) {
                return Err(CatalogError::DuplicateRoute);
            }
            let mut seen = BTreeSet::new();
            for id in &route.model_ids {
                if !models.contains(id.as_str()) {
                    return Err(CatalogError::UnknownModel);
                }
                if !seen.insert(id.as_str()) {
                    return Err(CatalogError::DuplicateReference);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ModelCatalog {
        ModelCatalog {
            connections: Vec::new(),
            quotas: vec![TokenQuota {
                id: "shared-hour".into(),
                tokens: 20_000,
                period: QuotaPeriod::Hour,
                window: QuotaWindow::Rolling,
            }],
            models: vec![
                ModelProfile {
                    id: "local".into(),
                    connection_id: "ollama".into(),
                    model_id: "example".into(),
                    enabled: true,
                    local: true,
                    price_class: Some(0),
                    context_tokens: Some(8192),
                    capabilities: [Capability::Text, Capability::Coding].into_iter().collect(),
                    ..Default::default()
                },
                ModelProfile {
                    id: "remote".into(),
                    connection_id: "account".into(),
                    model_id: "example".into(),
                    enabled: true,
                    price_class: Some(1),
                    quota_ids: vec!["shared-hour".into()],
                    ..Default::default()
                },
                ModelProfile {
                    id: "remote-2".into(),
                    connection_id: "account".into(),
                    model_id: "other".into(),
                    enabled: true,
                    price_class: Some(2),
                    quota_ids: vec!["shared-hour".into()],
                    ..Default::default()
                },
            ],
            routes: vec![RoutingPreference {
                task: "coding".into(),
                model_ids: vec!["local".into(), "remote".into()],
                on_exhausted: ExhaustedAction::Wait,
                ..Default::default()
            }],
        }
    }

    #[test]
    fn catalogue_round_trips_in_settings_and_accepts_shared_quotas() {
        let mut settings = crate::config::Settings::default();
        settings.agent.model_catalog = fixture();
        settings.agent.model_catalog.models[0].equivalence_group = Some("verified-example".into());
        settings.agent.model_catalog.models[1].equivalence_group = Some("verified-example".into());
        settings.agent.model_catalog.routes[0].prefer_equivalent = true;
        settings.agent.model_catalog.routes[0].fallback_token_budget = Some(20_000);
        settings.agent.model_catalog.routes[0].return_to_primary = true;
        assert_eq!(settings.agent.model_catalog.validate(), Ok(()));
        let encoded = settings.to_toml().unwrap();
        let decoded = crate::config::Settings::from_toml(&encoded).unwrap();
        assert_eq!(decoded.agent.model_catalog, settings.agent.model_catalog);
    }

    #[test]
    fn legacy_settings_remain_local_without_enabling_new_routes() {
        let settings = crate::config::Settings::from_toml("[agent]\nmodel = \"legacy\"\n").unwrap();
        assert!(settings.agent.model_catalog.is_empty());
        assert_eq!(settings.agent.provider_url, "http://127.0.0.1:11434");
        assert_eq!(settings.agent.model, "legacy");
    }

    #[test]
    fn invalid_catalogue_references_and_limits_are_refused() {
        let mut catalog = fixture();
        catalog.models[1].price_class = Some(0);
        assert_eq!(catalog.validate(), Err(CatalogError::InvalidPrice));
        catalog.models[1].price_class = Some(6);
        assert_eq!(catalog.validate(), Err(CatalogError::InvalidPrice));
        catalog.models[1].price_class = None;
        catalog.models[1].context_tokens = Some(128);
        catalog.models[1].max_output_tokens = Some(256);
        assert_eq!(catalog.validate(), Err(CatalogError::InvalidContext));
        catalog.models[1].max_output_tokens = None;
        catalog.models[1].quota_ids.push("missing".into());
        assert_eq!(catalog.validate(), Err(CatalogError::UnknownQuota));
        catalog.models[1].quota_ids.pop();
        catalog.routes[0].model_ids.push("missing".into());
        assert_eq!(catalog.validate(), Err(CatalogError::UnknownModel));
        catalog.routes[0].model_ids.pop();
        catalog.routes[0].model_ids.push("local".into());
        assert_eq!(catalog.validate(), Err(CatalogError::DuplicateReference));
    }

    #[test]
    fn a_registered_connection_is_required_once_the_registry_exists() {
        let mut catalog = fixture();
        // Before migration there is no registry and the legacy single provider still answers.
        assert_eq!(catalog.validate(), Ok(()));
        catalog.connections = vec![crate::providers::ProviderConnection::new(
            "ollama",
            "http://127.0.0.1:11434",
        )];
        // The fixture's `account` connection is not registered, so the dangling reference is an
        // error rather than a surprise at dispatch time.
        assert_eq!(catalog.validate(), Err(CatalogError::UnknownConnection));
        catalog
            .connections
            .push(crate::providers::ProviderConnection::new(
                "account",
                "http://models.example.com:11434",
            ));
        assert_eq!(catalog.validate(), Ok(()));
        // Two connections may not share an id, because ids key quotas, wallet entries and receipts.
        let duplicate = catalog.connections[0].clone();
        catalog.connections.push(duplicate);
        assert_eq!(catalog.validate(), Err(CatalogError::DuplicateConnection));
        catalog.connections.pop();
        // An unusable base URL is refused at validation time, not at connect time.
        catalog.connections[1].base_url = "https://models.example.com".into();
        assert!(matches!(
            catalog.validate(),
            Err(CatalogError::InvalidConnection(_))
        ));
    }

    #[test]
    fn the_legacy_provider_pair_migrates_into_one_connection() {
        let catalog = ModelCatalog::with_legacy_connection(" Ollama ", "").unwrap();
        assert_eq!(catalog.connections.len(), 1);
        assert_eq!(catalog.connections[0].id, "default");
        assert_eq!(catalog.connections[0].base_url, "http://127.0.0.1:11434");
        assert!(catalog.connections[0].enabled);
        assert_eq!(catalog.validate(), Ok(()));
        // A kind with no adapter is refused, and nothing is invented in its place.
        let mut unknown = ModelCatalog::with_legacy_connection("openai-compatible", "http://x");
        assert!(matches!(
            unknown,
            Err(CatalogError::InvalidConnection(kind)) if kind == "openai-compatible"
        ));
        unknown = ModelCatalog::with_legacy_connection("ollama", "http://user:secret@host");
        assert!(matches!(unknown, Err(CatalogError::InvalidConnection(_))));
    }

    #[test]
    fn connections_and_discovery_evidence_survive_the_settings_file() {
        let mut settings = crate::config::Settings::default();
        settings.agent.model_catalog = ModelCatalog {
            connections: vec![crate::providers::ProviderConnection {
                id: "default".into(),
                kind: crate::providers::ProviderKind::Ollama,
                base_url: "http://127.0.0.1:11434".into(),
                account: Some("home".into()),
                enabled: true,
            }],
            models: vec![ModelProfile {
                id: "default:tiny:1b".into(),
                connection_id: "default".into(),
                model_id: "tiny:1b".into(),
                enabled: true,
                local: true,
                price_class: Some(0),
                capabilities: [Capability::Text].into_iter().collect(),
                observed: Some(ObservedMetadata {
                    observed_at: 1_795_780_800,
                    capabilities: [Capability::Text, Capability::ToolCalling]
                        .into_iter()
                        .collect(),
                    unmapped: ["sparkle-mode".to_string()].into_iter().collect(),
                    parameters: Some("1B".into()),
                    size_bytes: Some(900 * 1024 * 1024),
                }),
                ..ModelProfile::default()
            }],
            ..ModelCatalog::default()
        };
        assert_eq!(settings.agent.model_catalog.validate(), Ok(()));
        let encoded = settings.to_toml().unwrap();
        let decoded = crate::config::Settings::from_toml(&encoded).unwrap();
        assert_eq!(decoded.agent.model_catalog, settings.agent.model_catalog);
        // An installation with nothing configured writes no catalogue block at all.
        let plain = crate::config::Settings::default();
        assert!(!plain.to_toml().unwrap().contains("model_catalog"));
    }

    #[test]
    fn duplicate_bindings_and_zero_quota_are_refused() {
        let mut catalog = fixture();
        catalog.quotas[0].tokens = 0;
        assert_eq!(catalog.validate(), Err(CatalogError::InvalidQuota));
        catalog.quotas[0].tokens = 100;
        let mut duplicate = catalog.models[0].clone();
        duplicate.id = "different-id".into();
        catalog.models.push(duplicate);
        assert_eq!(catalog.validate(), Err(CatalogError::DuplicateModel));
    }
}

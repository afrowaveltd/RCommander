//! User-owned model catalogue and routing preferences (Phase 16.20).
//! Metadata only: no network calls, credentials, quota accounting or execution.
//! A runtime must validate the catalogue and enforce privacy and budgets before dispatch.

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
    pub models: Vec<ModelProfile>,
    pub quotas: Vec<TokenQuota>,
    pub routes: Vec<RoutingPreference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

impl ModelCatalog {
    pub fn is_empty(&self) -> bool {
        self.models.is_empty() && self.quotas.is_empty() && self.routes.is_empty()
    }

    /// Validate before any future use by a dispatcher. Does not imply authorization.
    pub fn validate(&self) -> Result<(), CatalogError> {
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

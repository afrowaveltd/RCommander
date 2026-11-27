//! Provider connections, model discovery provenance and resolution (Phase 16.20, third slice).
//!
//! The catalogue says *which* models the owner configured; this module says *how to reach* one of
//! them, and where its metadata came from. Three jobs, all headless and offline:
//!
//! * [`ProviderConnection`] — a configured way to reach a model server, keyed by a stable id that
//!   the catalogue, the quota ledger and the credential wallet all refer to. It never holds a
//!   secret: a password or token lives in the wallet under the same id.
//! * [`resolve`] — the link the catalogue was missing: a model id becomes a connection, a dialable
//!   base URL and the provider-side model name, or a refusal that says why not.
//! * [`observe`] — discovery. What a provider advertises is recorded as *evidence* next to the
//!   owner's decisions, never on top of them: a newly discovered model arrives **switched off**,
//!   a provider's price, locality or quota is never invented, and a capability name nobody
//!   recognises is kept verbatim instead of being guessed at.
//!
//! What is deliberately *not* here: any network call (the caller lists models through `rc-llm` and
//! hands the result in), any secret (the wallet owns those), and any transport. `rc-llm` still
//! implements plain HTTP on the loopback interface only, so a `https` base URL is refused with
//! that reason rather than accepted and then failing at connect time.

use crate::model_catalog::{Capability, ModelCatalog, ModelProfile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A provider family the runtime can actually talk to.
///
/// The list is deliberately short: it names what ships with an adapter, not what the owner might
/// own. A second kind arrives with its adapter, so a configured connection never claims more than
/// the code can do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Ollama over plain HTTP (the loopback interface by default).
    #[default]
    Ollama,
}

impl ProviderKind {
    /// The label used in settings, the docs and the status line.
    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::Ollama => "ollama",
        }
    }

    /// Read a label back, tolerating the kinds that used to be spelled in settings.
    ///
    /// An empty label means "the default local server", which is what an older settings file
    /// implied. Anything else is unknown on purpose: guessing a kind from a name would turn a
    /// typo into a connection to somewhere unplanned.
    pub fn parse(text: &str) -> Option<ProviderKind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "" | "ollama" => Some(ProviderKind::Ollama),
            _ => None,
        }
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Where a model server runs, as configured by the owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConnection {
    /// Stable id: the catalogue's `connection_id`, a quota's account and a wallet entry all use it.
    pub id: String,
    pub kind: ProviderKind,
    /// Base URL of the server, without credentials and without a trailing slash.
    pub base_url: String,
    /// Owner label for the account this connection bills to; a label, not a proof of identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub enabled: bool,
}

impl Default for ProviderConnection {
    fn default() -> Self {
        ProviderConnection {
            id: String::new(),
            kind: ProviderKind::default(),
            base_url: String::new(),
            account: None,
            // A connection the owner just configured is meant to be used; switching it off is an
            // explicit act.
            enabled: true,
        }
    }
}

impl ProviderConnection {
    /// A connection to `base_url` with the default kind.
    pub fn new(id: impl Into<String>, base_url: impl Into<String>) -> Self {
        ProviderConnection {
            id: id.into(),
            base_url: base_url.into(),
            ..ProviderConnection::default()
        }
    }

    /// True when the URL points at this machine, which is a *hint* for the first-run wizard.
    ///
    /// It is not a locality decision: only the owner may say that a model is local, because only
    /// the owner knows what a hostname or a forwarded port really reaches.
    pub fn looks_local(&self) -> bool {
        let Some((authority, _)) = split_authority(&self.base_url) else {
            return false;
        };
        let Some(host) = authority_host(&authority) else {
            return false;
        };
        matches!(
            host.to_ascii_lowercase().as_str(),
            "127.0.0.1" | "localhost" | "::1"
        )
    }

    /// Validate this connection on its own, without the catalogue around it.
    pub fn validate(&self) -> Result<(), ConnectionError> {
        if self.id.trim().is_empty() {
            return Err(ConnectionError::NoId);
        }
        if self.id.trim() != self.id {
            return Err(ConnectionError::PaddedId(self.id.clone()));
        }
        if self.base_url.trim().is_empty() {
            return Err(ConnectionError::NoUrl(self.id.clone()));
        }
        normalize_base_url(&self.base_url).map(|_| ())
    }
}

/// Why a connection cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionError {
    NoId,
    /// Surrounding whitespace makes an id look like a different one in a file.
    PaddedId(String),
    NoUrl(String),
    /// The URL is not something the shipped transport can dial.
    BadUrl {
        url: String,
        why: String,
    },
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::NoId => f.write_str("a connection needs an id"),
            ConnectionError::PaddedId(id) => {
                write!(f, "the connection id “{id}” has surrounding whitespace")
            }
            ConnectionError::NoUrl(id) => write!(f, "the connection “{id}” has no base URL"),
            ConnectionError::BadUrl { url, why } => {
                write!(f, "the base URL “{url}” cannot be used: {why}")
            }
        }
    }
}

/// A model the provider listed, reduced to the fields the catalogue can record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedModel {
    /// The provider's own name for the model, as it must appear in a request.
    pub model_id: String,
    /// Interface features the provider advertised, verbatim.
    pub capabilities: BTreeSet<String>,
    /// Parameter count as text (`7B`), when the provider says; never a quality score.
    pub parameters: Option<String>,
    /// Size on disk, when the provider says.
    pub size_bytes: Option<u64>,
}

impl ObservedModel {
    /// One observed model with no advertised features.
    pub fn new(model_id: impl Into<String>) -> Self {
        ObservedModel {
            model_id: model_id.into(),
            capabilities: BTreeSet::new(),
            parameters: None,
            size_bytes: None,
        }
    }
}

/// What one discovery run changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoveryReport {
    /// Catalogue ids created by this run; they arrive **switched off**.
    pub added: Vec<String>,
    /// Catalogue ids whose provider-reported metadata was refreshed.
    pub refreshed: Vec<String>,
    /// Catalogue ids this connection no longer advertises. Nothing is deleted: the provider may
    /// be answering from a partially loaded server, and removing a model would silently drop the
    /// owner's price, quotas and grants.
    pub missing: Vec<String>,
    /// Provider capability names with no catalogue meaning, kept as evidence for the owner.
    pub unmapped: BTreeSet<String>,
}

impl DiscoveryReport {
    /// True when the run changed nothing.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.refreshed.is_empty() && self.missing.is_empty()
    }
}

/// A model that can be dialed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedModel {
    /// Catalogue id, the identity used by routes, quotas and receipts.
    pub model_id: String,
    pub connection_id: String,
    pub kind: ProviderKind,
    /// Base URL with the trimmed form the transport expects.
    pub base_url: String,
    /// Name to put in the request; may repeat across connections, unlike the catalogue id.
    pub provider_model: String,
    pub local: bool,
    pub quota_ids: Vec<String>,
}

/// Why a model cannot be dialed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    /// No model with this catalogue id.
    UnknownModel(String),
    /// A model (or a discovery run) names a connection the registry does not hold.
    UnknownConnection {
        model_id: Option<String>,
        connection_id: String,
    },
    /// The model is switched off; a disabled model is never dialed, even when it is the only one.
    ModelDisabled(String),
    /// The connection is switched off.
    ConnectionDisabled(String),
    /// The connection itself is unusable.
    Connection(ConnectionError),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::UnknownModel(id) => write!(f, "the model “{id}” is not in the catalogue"),
            ResolveError::UnknownConnection {
                model_id,
                connection_id,
            } => match model_id {
                Some(model_id) => write!(
                    f,
                    "the model “{model_id}” refers to the unknown connection “{connection_id}”"
                ),
                None => write!(f, "the connection “{connection_id}” is not configured"),
            },
            ResolveError::ModelDisabled(id) => write!(f, "the model “{id}” is switched off"),
            ResolveError::ConnectionDisabled(id) => {
                write!(f, "the connection “{id}” is switched off")
            }
            ResolveError::Connection(error) => write!(f, "{error}"),
        }
    }
}

/// Turn one catalogue model into something a transport can dial.
pub fn resolve(catalog: &ModelCatalog, model_id: &str) -> Result<ResolvedModel, ResolveError> {
    let model = catalog
        .models
        .iter()
        .find(|model| model.id == model_id)
        .ok_or_else(|| ResolveError::UnknownModel(model_id.to_string()))?;
    let connection = catalog
        .connections
        .iter()
        .find(|connection| connection.id == model.connection_id)
        .ok_or_else(|| ResolveError::UnknownConnection {
            model_id: Some(model.id.clone()),
            connection_id: model.connection_id.clone(),
        })?;
    let base_url = normalize_base_url(&connection.base_url).map_err(ResolveError::Connection)?;
    connection.validate().map_err(ResolveError::Connection)?;
    if !model.enabled {
        return Err(ResolveError::ModelDisabled(model.id.clone()));
    }
    if !connection.enabled {
        return Err(ResolveError::ConnectionDisabled(connection.id.clone()));
    }
    Ok(ResolvedModel {
        model_id: model.id.clone(),
        connection_id: connection.id.clone(),
        kind: connection.kind,
        base_url,
        provider_model: model.model_id.clone(),
        local: model.local,
        quota_ids: model.quota_ids.clone(),
    })
}

/// Record what `connection_id` advertised at `now`, without ever overwriting an owner decision.
///
/// New models are added **disabled** and with `local = false`: activation, locality, price,
/// quotas and equivalence are the owner's calls, and a provider naming a capability is not an
/// argument for sending data somewhere. A model the provider no longer lists is reported, not
/// removed, because a partial answer from a server must not erase the owner's configuration.
pub fn observe(
    catalog: &mut ModelCatalog,
    connection_id: &str,
    observed: &[ObservedModel],
    now: i64,
) -> Result<DiscoveryReport, ResolveError> {
    if !catalog
        .connections
        .iter()
        .any(|connection| connection.id == connection_id)
    {
        return Err(ResolveError::UnknownConnection {
            model_id: None,
            connection_id: connection_id.to_string(),
        });
    }
    let mut report = DiscoveryReport::default();
    for model in observed {
        let provider_model = model.model_id.trim();
        if provider_model.is_empty() {
            continue;
        }
        let (capabilities, unmapped) = map_capabilities(&model.capabilities);
        report.unmapped.extend(unmapped.iter().cloned());
        let observed_metadata = crate::model_catalog::ObservedMetadata {
            observed_at: now,
            capabilities,
            unmapped,
            parameters: model.parameters.clone(),
            size_bytes: model.size_bytes,
        };
        // The binding is (connection, provider model), not the local id: renaming a catalogue id
        // must not make discovery add the same server model a second time.
        let existing = catalog.models.iter_mut().find(|candidate| {
            candidate.connection_id == connection_id && candidate.model_id == provider_model
        });
        match existing {
            Some(existing) => {
                if existing.observed.as_ref() != Some(&observed_metadata) {
                    existing.observed = Some(observed_metadata);
                    report.refreshed.push(existing.id.clone());
                }
            }
            None => {
                let id = unused_model_id(catalog, connection_id, provider_model, now);
                report.added.push(id.clone());
                catalog.models.push(ModelProfile {
                    id,
                    connection_id: connection_id.to_string(),
                    model_id: provider_model.to_string(),
                    // Switched off until the owner says otherwise.
                    enabled: false,
                    observed: Some(observed_metadata),
                    ..ModelProfile::default()
                });
            }
        }
    }
    let advertised: BTreeSet<&str> = observed.iter().map(|model| model.model_id.trim()).collect();
    report.missing = catalog
        .models
        .iter()
        .filter(|model| model.connection_id == connection_id)
        .filter(|model| !advertised.contains(model.model_id.as_str()))
        .map(|model| model.id.clone())
        .collect();
    Ok(report)
}

/// Map provider-advertised feature names onto catalogue capabilities.
///
/// Only what the provider itself says is mapped, and only names this list knows: a model's name
/// or parameter count is never evidence of a capability, and an unrecognised name is reported
/// instead of guessed at.
fn map_capabilities(advertised: &BTreeSet<String>) -> (BTreeSet<Capability>, BTreeSet<String>) {
    let mut mapped = BTreeSet::new();
    let mut unmapped = BTreeSet::new();
    for name in advertised {
        match advertised_capability(name) {
            Some(capability) => {
                mapped.insert(capability);
            }
            None => {
                unmapped.insert(name.clone());
            }
        }
    }
    (mapped, unmapped)
}

fn advertised_capability(name: &str) -> Option<Capability> {
    match name.trim().to_ascii_lowercase().as_str() {
        "completion" | "text" | "generate" => Some(Capability::Text),
        "tools" | "tool_calling" | "tool-use" | "function_calling" => Some(Capability::ToolCalling),
        "vision" | "image_input" | "multimodal" => Some(Capability::ImageInput),
        "embedding" | "embeddings" => Some(Capability::Embeddings),
        "thinking" | "reasoning" => Some(Capability::Reasoning),
        _ => None,
    }
}

/// A catalogue id for a newly discovered model, unique in the catalogue.
fn unused_model_id(
    catalog: &ModelCatalog,
    connection_id: &str,
    provider_model: &str,
    now: i64,
) -> String {
    let base = format!("{connection_id}:{provider_model}");
    let taken = |id: &str| catalog.models.iter().any(|model| model.id == id);
    if !taken(&base) {
        return base;
    }
    for suffix in 2..1_000 {
        let candidate = format!("{base}#{suffix}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    // A thousand catalogue entries for one provider model is not a real case; time keeps the id
    // unique instead of panicking in a discovery run.
    format!("{base}#{now}")
}

/// Split a base URL into `(authority, path prefix)`.
///
/// Accepts the forms the shipped transport accepts: an optional `http://` scheme, a host that
/// must not be empty, an optional numeric port, and an optional path prefix.
fn split_authority(base_url: &str) -> Option<(String, String)> {
    let trimmed = base_url.trim();
    let without_scheme = match trimmed.split_once("://") {
        Some((_, rest)) => rest,
        None => trimmed,
    };
    if without_scheme.is_empty() || without_scheme.starts_with('/') {
        return None;
    }
    let (authority, path) = match without_scheme.split_once('/') {
        Some((authority, rest)) => (authority, format!("/{rest}")),
        None => (without_scheme, String::new()),
    };
    if authority.is_empty() {
        return None;
    }
    Some((authority.to_string(), path))
}

/// The bare host of an authority, without a port and without the brackets of an IPv6 literal.
fn authority_host(authority: &str) -> Option<String> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        if host.is_empty() || (!after.is_empty() && !after.starts_with(':')) {
            return None;
        }
        return Some(host.to_string());
    }
    match authority.split_once(':') {
        Some((host, _port)) => Some(host.to_string()),
        None => Some(authority.to_string()),
    }
}

/// The port of an authority, when it carries one.
fn authority_port(authority: &str) -> Option<&str> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (_host, after) = rest.split_once(']')?;
        return after.strip_prefix(':');
    }
    authority.split_once(':').map(|(_host, port)| port)
}

/// The base URL in the form the transport expects, or the reason it cannot be dialed.
pub fn normalize_base_url(base_url: &str) -> Result<String, ConnectionError> {
    let url = base_url.trim();
    if url.is_empty() {
        return Err(ConnectionError::BadUrl {
            url: String::new(),
            why: "it is empty".into(),
        });
    }
    if url.chars().any(char::is_whitespace) {
        return Err(ConnectionError::BadUrl {
            url: url.to_string(),
            why: "it contains whitespace".into(),
        });
    }
    if let Some((scheme, _)) = url.split_once("://") {
        match scheme.to_ascii_lowercase().as_str() {
            "http" => {}
            "https" => {
                return Err(ConnectionError::BadUrl {
                    url: url.to_string(),
                    why: "https needs the remote provider client, which is not built yet".into(),
                });
            }
            other => {
                return Err(ConnectionError::BadUrl {
                    url: url.to_string(),
                    why: format!("the scheme “{other}” is not http"),
                });
            }
        }
    }
    if url.contains('?') || url.contains('#') {
        return Err(ConnectionError::BadUrl {
            url: url.to_string(),
            why: "a base URL cannot carry a query or a fragment".into(),
        });
    }
    let Some((authority, _)) = split_authority(url) else {
        return Err(ConnectionError::BadUrl {
            url: url.to_string(),
            why: "there is no host".into(),
        });
    };
    // Credentials belong in the wallet, never in a URL that ends up in a settings file, an audit
    // line and a screenshot.
    if authority.contains('@') {
        return Err(ConnectionError::BadUrl {
            url: url.to_string(),
            why: "credentials do not belong in a base URL; use the credential wallet".into(),
        });
    }
    if authority_host(&authority)
        .filter(|host| !host.is_empty())
        .is_none()
    {
        let why = if authority.starts_with('[') {
            "the IPv6 host is incomplete"
        } else {
            "there is no host"
        };
        return Err(ConnectionError::BadUrl {
            url: url.to_string(),
            why: why.into(),
        });
    }
    if let Some(port) = authority_port(&authority) {
        if port == "0" {
            return Err(ConnectionError::BadUrl {
                url: url.to_string(),
                why: "port 0 is not a server".into(),
            });
        }
        port.parse::<u16>().map_err(|_| ConnectionError::BadUrl {
            url: url.to_string(),
            why: format!("the port “{port}” is not a number in 1..=65535"),
        })?;
    }
    Ok(url.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_catalog::{QuotaPeriod, QuotaWindow, RoutingPreference, TokenQuota};

    const NOON: i64 = 1_795_780_800;

    fn catalog() -> ModelCatalog {
        ModelCatalog {
            connections: vec![
                ProviderConnection::new("default", "http://127.0.0.1:11434"),
                ProviderConnection {
                    id: "remote".into(),
                    kind: ProviderKind::Ollama,
                    base_url: "http://models.example.com:11434".into(),
                    account: Some("work".into()),
                    enabled: true,
                },
            ],
            quotas: vec![TokenQuota {
                id: "pool".into(),
                tokens: 1_000,
                period: QuotaPeriod::Hour,
                window: QuotaWindow::Rolling,
            }],
            models: vec![ModelProfile {
                id: "local".into(),
                connection_id: "default".into(),
                model_id: "tiny:1b".into(),
                enabled: true,
                local: true,
                quota_ids: vec!["pool".into()],
                ..ModelProfile::default()
            }],
            routes: vec![RoutingPreference {
                task: "default".into(),
                model_ids: vec!["local".into()],
                ..RoutingPreference::default()
            }],
        }
    }

    fn observed(names: &[&str]) -> Vec<ObservedModel> {
        names
            .iter()
            .map(|name| ObservedModel {
                model_id: (*name).to_string(),
                capabilities: ["completion".to_string()].into_iter().collect(),
                ..ObservedModel::new(*name)
            })
            .collect()
    }

    #[test]
    fn a_legacy_single_server_becomes_the_default_connection() {
        let connection = ProviderConnection::new("default", "http://127.0.0.1:11434");
        assert_eq!(connection.validate(), Ok(()));
        assert_eq!(connection.kind, ProviderKind::Ollama);
        assert!(
            connection.enabled,
            "a configured connection is meant to be used"
        );
        assert!(connection.looks_local());
        assert_eq!(ProviderKind::parse(" Ollama "), Some(ProviderKind::Ollama));
        assert_eq!(ProviderKind::parse(""), Some(ProviderKind::Ollama));
        assert_eq!(
            ProviderKind::parse("openai-compatible"),
            None,
            "a kind with no adapter is not invented"
        );
        assert_eq!(ProviderKind::default().to_string(), "ollama");
    }

    #[test]
    fn a_model_resolves_to_its_connection_and_provider_name() {
        let catalog = catalog();
        let resolved = resolve(&catalog, "local").expect("a configured, enabled model resolves");
        assert_eq!(resolved.connection_id, "default");
        assert_eq!(resolved.kind, ProviderKind::Ollama);
        assert_eq!(resolved.base_url, "http://127.0.0.1:11434");
        assert_eq!(resolved.provider_model, "tiny:1b");
        assert!(resolved.local);
        assert_eq!(resolved.quota_ids, vec!["pool".to_string()]);
        assert_eq!(
            resolve(&catalog, "nope"),
            Err(ResolveError::UnknownModel("nope".into()))
        );
    }

    #[test]
    fn a_switched_off_model_or_connection_does_not_resolve() {
        let mut catalog = catalog();
        catalog.models[0].enabled = false;
        assert_eq!(
            resolve(&catalog, "local"),
            Err(ResolveError::ModelDisabled("local".into()))
        );
        catalog.models[0].enabled = true;
        catalog.connections[0].enabled = false;
        assert_eq!(
            resolve(&catalog, "local"),
            Err(ResolveError::ConnectionDisabled("default".into()))
        );
    }

    #[test]
    fn a_model_whose_connection_was_removed_is_refused_with_both_ids() {
        let mut catalog = catalog();
        catalog.connections.remove(0);
        assert_eq!(
            resolve(&catalog, "local"),
            Err(ResolveError::UnknownConnection {
                model_id: Some("local".into()),
                connection_id: "default".into(),
            })
        );
    }

    #[test]
    fn credentials_and_unreachable_schemes_are_refused_in_a_base_url() {
        let embedded = ProviderConnection::new("x", "http://user:secret@127.0.0.1:11434");
        let error = embedded.validate().unwrap_err().to_string();
        assert!(error.contains("credential wallet"), "message: {error}");
        let tls = ProviderConnection::new("x", "https://models.example.com");
        let error = tls.validate().unwrap_err().to_string();
        assert!(error.contains("not built yet"), "message: {error}");
        assert!(ProviderConnection::new("x", "http://").validate().is_err());
        assert!(
            ProviderConnection::new("x", "127.0.0.1:notaport")
                .validate()
                .is_err()
        );
        assert!(
            ProviderConnection::new("x", "http://h:0")
                .validate()
                .is_err()
        );
        assert!(
            ProviderConnection::new("x", "http://[::1")
                .validate()
                .is_err()
        );
        assert_eq!(
            ProviderConnection::new("x", "http://[::1]:11434/").validate(),
            Ok(())
        );
        assert_eq!(
            normalize_base_url("HTTP://models.example.com/ollama/"),
            Ok("HTTP://models.example.com/ollama".into())
        );
        assert_eq!(
            ProviderConnection::new(" padded", "http://h").validate(),
            Err(ConnectionError::PaddedId(" padded".into()))
        );
    }

    #[test]
    fn discovery_adds_models_switched_off_and_leaves_owner_decisions_alone() {
        let mut catalog = catalog();
        let report = observe(
            &mut catalog,
            "default",
            &observed(&["tiny:1b", "big:7b"]),
            NOON,
        )
        .unwrap();
        assert_eq!(report.added, vec!["default:big:7b".to_string()]);
        assert_eq!(report.refreshed, vec!["local".to_string()]);
        let added = catalog
            .models
            .iter()
            .find(|model| model.id == "default:big:7b")
            .expect("the advertised model was added");
        assert!(!added.enabled, "a discovered model waits for the owner");
        assert!(!added.local, "locality is never inferred from an address");
        assert!(added.price_class.is_none());
        assert!(added.quota_ids.is_empty());
        assert_eq!(
            added.observed.as_ref().map(|seen| seen.observed_at),
            Some(NOON)
        );
        // The owner's own model keeps its price, locality and quotas.
        let local = catalog
            .models
            .iter()
            .find(|model| model.id == "local")
            .unwrap();
        assert!(local.local);
        assert!(local.enabled);
        assert_eq!(local.quota_ids, vec!["pool".to_string()]);
        assert_eq!(
            local.observed.as_ref().map(|seen| seen.observed_at),
            Some(NOON),
            "the provider metadata is recorded next to the owner's decisions"
        );
    }

    #[test]
    fn discovery_keeps_unrecognised_capability_names_as_evidence() {
        let mut catalog = catalog();
        let mut model = ObservedModel::new("tiny:1b");
        model.capabilities = ["completion", "tools", "thinking", "sparkle-mode"]
            .into_iter()
            .map(str::to_string)
            .collect();
        model.parameters = Some("1B".into());
        model.size_bytes = Some(900 * 1024 * 1024);
        observe(&mut catalog, "default", &[model], NOON).unwrap();
        let local = catalog
            .models
            .iter()
            .find(|model| model.id == "local")
            .unwrap();
        let seen = local.observed.as_ref().unwrap();
        assert_eq!(
            seen.capabilities,
            [
                Capability::Text,
                Capability::ToolCalling,
                Capability::Reasoning
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(
            seen.unmapped,
            ["sparkle-mode".to_string()].into_iter().collect(),
            "an unknown name is kept verbatim, not guessed at"
        );
        assert_eq!(seen.parameters.as_deref(), Some("1B"));
        assert_eq!(seen.size_bytes, Some(900 * 1024 * 1024));
        // Advertised features become usable evidence, but never a price or a locality.
        assert_eq!(
            local.effective_capabilities(),
            [
                Capability::Text,
                Capability::ToolCalling,
                Capability::Reasoning
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(local.capabilities, BTreeSet::new(), "the owner set nothing");
        assert!(
            local.specialties.is_empty(),
            "no specialty is invented either"
        );
        assert!(local.price_class.is_none());
    }

    #[test]
    fn a_second_run_with_the_same_answer_changes_nothing() {
        let mut catalog = catalog();
        let first = observe(&mut catalog, "default", &observed(&["tiny:1b"]), NOON).unwrap();
        assert_eq!(first.refreshed, vec!["local".to_string()]);
        let second = observe(&mut catalog, "default", &observed(&["tiny:1b"]), NOON).unwrap();
        assert!(second.is_empty(), "no new evidence, no rewrite: {second:?}");
        // A later observation is recorded as newer evidence.
        let third = observe(&mut catalog, "default", &observed(&["tiny:1b"]), NOON + 60).unwrap();
        assert_eq!(third.refreshed, vec!["local".to_string()]);
        assert_eq!(
            catalog.models[0]
                .observed
                .as_ref()
                .map(|seen| seen.observed_at),
            Some(NOON + 60)
        );
    }

    #[test]
    fn a_model_the_provider_stopped_listing_is_reported_not_deleted() {
        let mut catalog = catalog();
        let report = observe(&mut catalog, "default", &observed(&["other:1b"]), NOON).unwrap();
        assert_eq!(report.missing, vec!["local".to_string()]);
        assert!(
            catalog.models.iter().any(|model| model.id == "local"),
            "an owner's configuration survives a partial model list"
        );
    }

    #[test]
    fn discovery_needs_a_configured_connection_and_never_reuses_an_id() {
        let mut catalog = catalog();
        assert_eq!(
            observe(&mut catalog, "ghost", &observed(&["x"]), NOON),
            Err(ResolveError::UnknownConnection {
                model_id: None,
                connection_id: "ghost".into(),
            })
        );
        // An owner already used the obvious id for something else.
        catalog.models.push(ModelProfile {
            id: "default:big:7b".into(),
            connection_id: "remote".into(),
            model_id: "same-name-on-another-server".into(),
            ..ModelProfile::default()
        });
        let report = observe(&mut catalog, "default", &observed(&["big:7b"]), NOON).unwrap();
        assert_eq!(report.added, vec!["default:big:7b#2".to_string()]);
        assert_eq!(catalog.validate(), Ok(()));
    }

    #[test]
    fn a_loopback_address_is_only_a_hint_about_locality() {
        let local = ProviderConnection::new("a", "http://localhost:11434");
        let v6 = ProviderConnection::new("b", "http://[::1]:11434");
        let lan = ProviderConnection::new("c", "http://192.168.1.10:11434");
        let named = ProviderConnection::new("d", "http://gpu-box:11434");
        assert!(local.looks_local() && v6.looks_local());
        assert!(!lan.looks_local() && !named.looks_local());
        // The hint never writes locality into a profile; only the owner does that.
        let mut catalog = catalog();
        observe(&mut catalog, "default", &observed(&["tiny:1b"]), NOON).unwrap();
        assert!(
            catalog
                .models
                .iter()
                .all(|model| model.id != "default:tiny:1b")
        );
    }
}

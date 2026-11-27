//! Admission control for one catalogue model (Phase 16.20, second slice).
//!
//! [`admit`] answers one question: *may this request go to this model right now, and why?* It is a
//! pure check over the owner's catalogue and the durable [`QuotaLedger`]; it changes nothing, so a
//! refusal performs no reservation, no provider call and no audit entry of its own.
//!
//! The checks run in the order a person would ask them, and the first failing one is reported:
//!
//! 1. does the model exist and is it enabled;
//! 2. does it declare every capability the workload needs;
//! 3. may this request's data leave the machine (the owner's egress decision, never inferred);
//! 4. does the context window fit the prompt plus the whole output reserve;
//! 5. is the model (or one of its quotas) currently waiting out a provider answer;
//! 6. is there enough known allowance left for the estimate plus the maximum output.
//!
//! Two rules the code enforces rather than documents: **unknown remaining is not unlimited**
//! (a provider-defined window without an observed reset is refused, not assumed free), and
//! **admission is not authorization** — an admitted request still needs the policy, approval and
//! egress decisions of the calling layer, and a more capable model never widens an OS, tool or
//! credential permission.

use crate::model_catalog::{Capability, ModelCatalog, ModelProfile};
use crate::quota::{QuotaLedger, Remaining, UnknownRemaining};
use std::collections::BTreeSet;

/// What a workload needs before it may be sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    /// Catalogue model id, never a provider model name: two providers can offer the same name.
    pub model_id: String,
    /// Workload/role identifier the routing preferences are keyed by, e.g. `coding`.
    pub task: String,
    /// Estimated prompt size in tokens; an estimate is all a client can honestly claim.
    pub prompt_tokens: u64,
    /// Maximum output the request may produce, reserved in full before dispatch.
    pub max_output_tokens: u64,
    /// Capabilities the workload requires, all of them.
    pub requires: BTreeSet<Capability>,
    /// Whether this request's data may be sent to a remote provider.
    ///
    /// A local model ignores it; a remote one is refused without it. Defaulting this to `true`
    /// anywhere would turn a privacy decision into an assumption.
    pub allow_remote: bool,
}

impl Requirement {
    /// A plain text request against one model.
    pub fn text(model_id: impl Into<String>, prompt_tokens: u64, max_output_tokens: u64) -> Self {
        Requirement {
            model_id: model_id.into(),
            task: "default".into(),
            prompt_tokens,
            max_output_tokens,
            requires: [Capability::Text].into_iter().collect(),
            allow_remote: false,
        }
    }

    /// Total tokens the request may consume: the estimate plus the whole output reserve.
    pub fn reservation_tokens(&self) -> u64 {
        self.prompt_tokens.saturating_add(self.max_output_tokens)
    }
}

/// Why a model was not allowed to run this request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    UnknownModel(String),
    Disabled(String),
    /// The model does not declare a capability the workload needs.
    MissingCapability(Capability),
    /// Sending this data to a remote provider was not authorized.
    RemoteNotAuthorized,
    /// The context window cannot hold the prompt and the output reserve.
    ContextTooSmall {
        needed: u64,
        available: u64,
    },
    /// The model scope is waiting out a provider answer.
    ModelWaiting {
        until: i64,
    },
    /// One of its quotas is waiting out a provider answer.
    QuotaWaiting {
        quota_id: String,
        until: i64,
    },
    /// A known allowance is smaller than the reservation.
    QuotaExceeded {
        quota_id: String,
        remaining: u64,
        wanted: u64,
    },
    /// The allowance cannot be computed; it is not treated as unlimited.
    UnknownRemaining {
        quota_id: String,
        why: UnknownRemaining,
    },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::UnknownModel(id) => write!(f, "the model “{id}” is not in the catalogue"),
            Refusal::Disabled(id) => write!(f, "the model “{id}” is switched off"),
            Refusal::MissingCapability(capability) => {
                write!(
                    f,
                    "the model does not declare the capability {capability:?}"
                )
            }
            Refusal::RemoteNotAuthorized => write!(
                f,
                "this data may not be sent to a remote provider, and the model is not local"
            ),
            Refusal::ContextTooSmall { needed, available } => write!(
                f,
                "the request needs {needed} tokens and the context window holds {available}"
            ),
            Refusal::ModelWaiting { until } => {
                write!(f, "the model is rate limited until {until}")
            }
            Refusal::QuotaWaiting { quota_id, until } => {
                write!(f, "the quota “{quota_id}” is rate limited until {until}")
            }
            Refusal::QuotaExceeded {
                quota_id,
                remaining,
                wanted,
            } => write!(
                f,
                "the quota “{quota_id}” has {remaining} tokens left and {wanted} are needed"
            ),
            Refusal::UnknownRemaining { quota_id, why } => match why {
                UnknownRemaining::ProviderDefinedWithoutReset => write!(
                    f,
                    "the reset of the quota “{quota_id}” is owned by the provider and was never observed"
                ),
                UnknownRemaining::NotConfigured => {
                    write!(f, "the quota “{quota_id}” is no longer configured")
                }
            },
        }
    }
}

/// What the caller may do next, with the evidence it is allowed to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub model_id: String,
    /// Configured connection the model belongs to; the caller resolves it, not this module.
    pub connection_id: String,
    /// Provider model name to put in the request.
    pub provider_model: String,
    /// Tokens to reserve before dispatch.
    pub reservation_tokens: u64,
    /// Quotas the reservation must be booked against, in catalogue order.
    pub quota_ids: Vec<String>,
    /// Earliest known rollover across the quotas, when any of them can say.
    pub reset_at: Option<i64>,
    /// True for a provider the device does not host, so a status line can say where the data went.
    pub remote: bool,
}

/// The answer of [`admit`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Admission {
    Admitted(Grant),
    Refused(Refusal),
}

impl Admission {
    pub fn is_admitted(&self) -> bool {
        matches!(self, Admission::Admitted(_))
    }

    /// The grant, when the model was admitted.
    pub fn grant(&self) -> Option<&Grant> {
        match self {
            Admission::Admitted(grant) => Some(grant),
            Admission::Refused(_) => None,
        }
    }
}

/// Scope of a model-wide wait, so the ledger can key it without knowing about models.
pub fn model_scope(model_id: &str) -> String {
    format!("model:{model_id}")
}

/// Decide whether `requirement` may be sent to its model at `now`.
///
/// Deterministic and side-effect free: the same catalogue, ledger, requirement and instant always
/// give the same answer, which is what lets the tests and the review agree about a refusal.
pub fn admit(
    catalog: &ModelCatalog,
    ledger: &QuotaLedger,
    requirement: &Requirement,
    now: i64,
) -> Admission {
    let Some(model) = catalog
        .models
        .iter()
        .find(|model| model.id == requirement.model_id)
    else {
        return Admission::Refused(Refusal::UnknownModel(requirement.model_id.clone()));
    };
    if !model.enabled {
        return Admission::Refused(Refusal::Disabled(model.id.clone()));
    }
    // Capabilities come from the owner *and* from what the provider advertised about its own
    // interface: a provider's statement about a feature is evidence, while price, locality and
    // quota are not (see `ModelProfile::effective_capabilities`).
    let offered = model.effective_capabilities();
    if let Some(missing) = requirement
        .requires
        .iter()
        .find(|capability| !offered.contains(capability))
    {
        return Admission::Refused(Refusal::MissingCapability(*missing));
    }
    if !model.local && !requirement.allow_remote {
        return Admission::Refused(Refusal::RemoteNotAuthorized);
    }
    if let Some(context) = model.context_tokens {
        let needed = requirement.reservation_tokens();
        if needed > context {
            return Admission::Refused(Refusal::ContextTooSmall {
                needed,
                available: context,
            });
        }
    }
    if let Some(until) = ledger.wait_until(&model_scope(&model.id), now) {
        return Admission::Refused(Refusal::ModelWaiting { until });
    }
    let wanted = requirement.reservation_tokens();
    let mut reset_at: Option<i64> = None;
    for quota_id in &model.quota_ids {
        if let Some(until) = ledger.wait_until(quota_id, now) {
            return Admission::Refused(Refusal::QuotaWaiting {
                quota_id: quota_id.clone(),
                until,
            });
        }
        match ledger.remaining(quota_id, now) {
            Remaining::Known {
                tokens,
                reset_at: roll,
            } => {
                if tokens < wanted {
                    return Admission::Refused(Refusal::QuotaExceeded {
                        quota_id: quota_id.clone(),
                        remaining: tokens,
                        wanted,
                    });
                }
                reset_at = match (reset_at, roll) {
                    (Some(current), Some(roll)) => Some(current.min(roll)),
                    (None, roll) => roll,
                    (current, None) => current,
                };
            }
            Remaining::Unknown(why) => {
                return Admission::Refused(Refusal::UnknownRemaining {
                    quota_id: quota_id.clone(),
                    why,
                });
            }
        }
    }
    Admission::Admitted(Grant {
        model_id: model.id.clone(),
        connection_id: model.connection_id.clone(),
        provider_model: model.model_id.clone(),
        reservation_tokens: wanted,
        quota_ids: model.quota_ids.clone(),
        reset_at,
        remote: !model.local,
    })
}

/// Why a model would be skipped, for the router's "why not this one" explanation.
///
/// A router ranks candidates; this helper only reports the first refusal per candidate so the
/// reason can be shown to the owner without replaying the admission logic.
pub fn explain_skips(
    catalog: &ModelCatalog,
    ledger: &QuotaLedger,
    requirement: &Requirement,
    now: i64,
    candidates: &[String],
) -> Vec<(String, Refusal)> {
    let mut skips = Vec::new();
    for candidate in candidates {
        let mut probe = requirement.clone();
        probe.model_id = candidate.clone();
        if let Admission::Refused(refusal) = admit(catalog, ledger, &probe, now) {
            skips.push((candidate.clone(), refusal));
        }
    }
    skips
}

/// The model's own note about where the data goes, when the caller needs it for a status line.
pub fn is_remote(model: &ModelProfile) -> bool {
    !model.local
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_catalog::{
        ExhaustedAction, ModelProfile, QuotaPeriod, QuotaWindow, RoutingPreference, TokenQuota,
    };

    const NOON: i64 = 1_795_780_800;

    fn catalog() -> ModelCatalog {
        ModelCatalog {
            connections: Vec::new(),
            quotas: vec![TokenQuota {
                id: "shared".into(),
                tokens: 1_000,
                period: QuotaPeriod::Hour,
                window: QuotaWindow::Rolling,
            }],
            models: vec![
                ModelProfile {
                    id: "local".into(),
                    connection_id: "ollama".into(),
                    model_id: "tiny:1b".into(),
                    enabled: true,
                    local: true,
                    price_class: Some(0),
                    context_tokens: Some(8_192),
                    capabilities: [Capability::Text, Capability::Coding].into_iter().collect(),
                    ..ModelProfile::default()
                },
                ModelProfile {
                    id: "remote".into(),
                    connection_id: "account".into(),
                    model_id: "big".into(),
                    enabled: true,
                    local: false,
                    price_class: Some(2),
                    context_tokens: Some(128_000),
                    capabilities: [
                        Capability::Text,
                        Capability::Coding,
                        Capability::ToolCalling,
                    ]
                    .into_iter()
                    .collect(),
                    quota_ids: vec!["shared".into()],
                    ..ModelProfile::default()
                },
                ModelProfile {
                    id: "off".into(),
                    connection_id: "ollama".into(),
                    model_id: "disabled:1b".into(),
                    enabled: false,
                    local: true,
                    ..ModelProfile::default()
                },
            ],
            routes: vec![RoutingPreference {
                task: "coding".into(),
                model_ids: vec!["local".into(), "remote".into()],
                on_exhausted: ExhaustedAction::Wait,
                ..RoutingPreference::default()
            }],
        }
    }

    fn ledger(catalog: &ModelCatalog) -> QuotaLedger {
        QuotaLedger::new(&catalog.quotas).unwrap()
    }

    #[test]
    fn a_local_model_is_admitted_with_its_reservation_and_no_quota() {
        let catalog = catalog();
        let ledger = ledger(&catalog);
        let admission = admit(
            &catalog,
            &ledger,
            &Requirement::text("local", 1_000, 500),
            NOON,
        );
        let grant = admission.grant().expect("a local model within its context");
        assert_eq!(grant.provider_model, "tiny:1b");
        assert_eq!(grant.connection_id, "ollama");
        assert_eq!(grant.reservation_tokens, 1_500);
        assert!(!grant.remote);
        assert_eq!(grant.reset_at, None);
        // Admission performed no reservation: the ledger is untouched.
        assert!(ledger.outstanding().is_empty());
    }

    #[test]
    fn a_remote_model_is_refused_without_egress_authorization() {
        let catalog = catalog();
        let ledger = ledger(&catalog);
        let refused = admit(
            &catalog,
            &ledger,
            &Requirement::text("remote", 1_000, 500),
            NOON,
        );
        assert_eq!(
            refused,
            Admission::Refused(Refusal::RemoteNotAuthorized),
            "a capability increase must never widen where the data may go"
        );
        // The quota is 1 000 tokens, so the authorized request stays inside the known allowance.
        let mut allowed = Requirement::text("remote", 300, 200);
        allowed.allow_remote = true;
        let grant = admit(&catalog, &ledger, &allowed, NOON)
            .grant()
            .expect("the owner authorized remote data")
            .clone();
        assert!(grant.remote);
        assert_eq!(grant.quota_ids, vec!["shared".to_string()]);
        assert_eq!(
            grant.reset_at, None,
            "no charge yet, so no rollover to report"
        );
    }

    #[test]
    fn insufficient_context_is_refused_before_any_quota_is_touched() {
        let catalog = catalog();
        let ledger = ledger(&catalog);
        let refused = admit(
            &catalog,
            &ledger,
            &Requirement::text("local", 8_000, 1_000),
            NOON,
        );
        assert_eq!(
            refused,
            Admission::Refused(Refusal::ContextTooSmall {
                needed: 9_000,
                available: 8_192,
            })
        );
        // Exactly at the limit is allowed: the reserve is part of the request.
        assert!(
            admit(
                &catalog,
                &ledger,
                &Requirement::text("local", 7_192, 1_000),
                NOON
            )
            .is_admitted()
        );
    }

    #[test]
    fn a_missing_capability_or_a_disabled_model_is_refused_with_the_reason() {
        let catalog = catalog();
        let ledger = ledger(&catalog);
        let mut tools = Requirement::text("local", 10, 10);
        tools.requires = [Capability::ToolCalling].into_iter().collect();
        assert_eq!(
            admit(&catalog, &ledger, &tools, NOON),
            Admission::Refused(Refusal::MissingCapability(Capability::ToolCalling))
        );
        assert_eq!(
            admit(&catalog, &ledger, &Requirement::text("off", 10, 10), NOON),
            Admission::Refused(Refusal::Disabled("off".into()))
        );
        assert_eq!(
            admit(&catalog, &ledger, &Requirement::text("nope", 10, 10), NOON),
            Admission::Refused(Refusal::UnknownModel("nope".into()))
        );
    }

    #[test]
    fn an_exhausted_quota_is_refused_with_the_reset_evidence() {
        let catalog = catalog();
        let mut ledger = ledger(&catalog);
        ledger
            .reserve("a", &["shared".to_string()], 800, NOON)
            .unwrap();
        ledger.settle("a", Some(800)).unwrap();
        let mut request = Requirement::text("remote", 1_000, 500);
        request.allow_remote = true;
        assert_eq!(
            admit(&catalog, &ledger, &request, NOON),
            Admission::Refused(Refusal::QuotaExceeded {
                quota_id: "shared".into(),
                remaining: 200,
                wanted: 1_500,
            })
        );
        // A small request still fits, and the rollover it reports is the window's own.
        let small = Requirement {
            prompt_tokens: 100,
            max_output_tokens: 100,
            ..request.clone()
        };
        let grant = admit(&catalog, &ledger, &small, NOON)
            .grant()
            .cloned()
            .unwrap();
        assert_eq!(grant.reset_at, Some(NOON + 3_600));
        // Past the window the full allowance is back, for a request that fits it.
        let full = Requirement {
            prompt_tokens: 700,
            max_output_tokens: 300,
            ..request
        };
        assert_eq!(
            admit(&catalog, &ledger, &full, NOON),
            Admission::Refused(Refusal::QuotaExceeded {
                quota_id: "shared".into(),
                remaining: 200,
                wanted: 1_000,
            })
        );
        assert!(admit(&catalog, &ledger, &full, NOON + 3_601).is_admitted());
    }

    #[test]
    fn unknown_remaining_is_not_treated_as_unlimited() {
        let mut catalog = catalog();
        catalog.quotas[0].window = QuotaWindow::ProviderDefined;
        let ledger = ledger(&catalog);
        let mut request = Requirement::text("remote", 10, 10);
        request.allow_remote = true;
        assert_eq!(
            admit(&catalog, &ledger, &request, NOON),
            Admission::Refused(Refusal::UnknownRemaining {
                quota_id: "shared".into(),
                why: UnknownRemaining::ProviderDefinedWithoutReset,
            })
        );
        // After the provider confirms a reset the same request is admitted.
        let mut ledger = ledger;
        ledger.record_provider_reset("shared", NOON).unwrap();
        assert!(admit(&catalog, &ledger, &request, NOON).is_admitted());
    }

    #[test]
    fn a_rate_limited_model_or_quota_reports_when_it_may_be_asked_again() {
        let catalog = catalog();
        let mut ledger = ledger(&catalog);
        ledger.record_rate_limit(&model_scope("local"), Some(30), NOON);
        assert_eq!(
            admit(&catalog, &ledger, &Requirement::text("local", 10, 10), NOON),
            Admission::Refused(Refusal::ModelWaiting { until: NOON + 30 })
        );
        assert!(
            admit(
                &catalog,
                &ledger,
                &Requirement::text("local", 10, 10),
                NOON + 30
            )
            .is_admitted()
        );

        let mut remote = Requirement::text("remote", 10, 10);
        remote.allow_remote = true;
        ledger.record_rate_limit("shared", Some(45), NOON);
        assert_eq!(
            admit(&catalog, &ledger, &remote, NOON),
            Admission::Refused(Refusal::QuotaWaiting {
                quota_id: "shared".into(),
                until: NOON + 45,
            })
        );
    }

    #[test]
    fn skips_are_explained_per_candidate_for_the_router_to_show() {
        let catalog = catalog();
        let ledger = ledger(&catalog);
        let candidates = vec![
            "local".to_string(),
            "remote".to_string(),
            "nope".to_string(),
        ];
        let skips = explain_skips(
            &catalog,
            &ledger,
            &Requirement::text("local", 10, 10),
            NOON,
            &candidates,
        );
        assert_eq!(
            skips,
            vec![
                ("remote".to_string(), Refusal::RemoteNotAuthorized),
                ("nope".to_string(), Refusal::UnknownModel("nope".into())),
            ],
            "the admitted candidate is not a skip"
        );
    }

    #[test]
    fn an_advertised_capability_counts_as_evidence_for_admission() {
        let mut catalog = catalog();
        // The local model declares text and coding, and the provider adds tool calling.
        catalog.models[0].observed = Some(crate::model_catalog::ObservedMetadata {
            observed_at: NOON,
            capabilities: [Capability::ToolCalling].into_iter().collect(),
            ..crate::model_catalog::ObservedMetadata::default()
        });
        let ledger = ledger(&catalog);
        let mut tools = Requirement::text("local", 10, 10);
        tools.requires = [Capability::ToolCalling].into_iter().collect();
        assert!(
            admit(&catalog, &ledger, &tools, NOON).is_admitted(),
            "a provider-advertised interface feature may satisfy a workload"
        );
        // Evidence never becomes a price or a locality: the model keeps the price class and the
        // locality the owner set, and the advertised features live beside them.
        assert!(catalog.models[0].local);
        assert_eq!(catalog.models[0].price_class, Some(0));
        assert!(
            catalog.models[0].advertises(Capability::ToolCalling),
            "the provider's statement is kept as dated evidence"
        );
        // A capability nobody advertised is still missing.
        let mut vision = Requirement::text("local", 10, 10);
        vision.requires = [Capability::ImageInput].into_iter().collect();
        assert_eq!(
            admit(&catalog, &ledger, &vision, NOON),
            Admission::Refused(Refusal::MissingCapability(Capability::ImageInput))
        );
    }

    #[test]
    fn a_refusal_reads_as_a_sentence_and_names_the_limit() {
        let refusal = Refusal::QuotaExceeded {
            quota_id: "shared".into(),
            remaining: 12,
            wanted: 40,
        };
        let text = refusal.to_string();
        assert!(text.contains("shared"), "message: {text}");
        assert!(text.contains("12"), "message: {text}");
        assert!(text.contains("40"), "message: {text}");
        assert!(
            is_remote(&catalog().models[1]),
            "a provider on someone else's machine is remote"
        );
    }
}

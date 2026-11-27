//! The deterministic router (Phase 16.20, fourth slice).
//!
//! [`admission::admit`](crate::admission::admit) answers "may *this* model run *this* request".
//! The router answers the question before it: **which** model should, given the owner's ordered
//! preferences, the catalogue's capabilities, the egress decision and the budget.
//!
//! Three rules decide everything, and all three are the owner's:
//!
//! 1. **The route order is the ranking.** A candidate appears because the owner listed it for this
//!    workload, and the first eligible one wins. Price class and locality never reorder the list:
//!    they are shown in the reason and used when the owner wrote the order, not inferred here.
//! 2. **Eligibility is admission's job.** Every candidate goes through the same checks a single
//!    model gets — enabled, capability, egress, context, waits, known allowance — so a routed
//!    request and a direct one cannot disagree about what is allowed.
//! 3. **Refusal is explained, never silent.** Each skipped candidate carries its own reason, and
//!    an exhausted route reports the earliest recovery it can honestly name plus the action the
//!    owner chose (`ask`, `wait`, `fallback`).
//!
//! Two of the catalogue's fallback fields have a meaning here and are honoured:
//! `prefer_equivalent` moves a user-verified equivalent binding to the front of the remaining
//! candidates once the primary is unavailable, and `on_exhausted` decides what happens when
//! nothing is eligible. `return_to_primary` and `fallback_token_budget` are recorded but not yet
//! enforced: they need per-route accounting (how much of the fallback ceiling this task has spent)
//! that the ledger does not keep yet, and the router does not guess at a budget.
//!
//! The router is pure: same catalogue, ledger, task and instant give the same answer, which is
//! what lets the wizard, the status line and the tests agree about a decision.

use crate::admission::{self, Admission, Grant, Refusal, Requirement};
use crate::model_catalog::{ExhaustedAction, ModelCatalog, ModelProfile, RoutingPreference};
use crate::quota::{QuotaLedger, Remaining};
use std::collections::BTreeSet;

/// What a workload asks for, before a model has been chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workload {
    /// Route key the owner's preferences are written under (`chat`, `coding`, `default`, …).
    pub task: String,
    pub prompt_tokens: u64,
    pub max_output_tokens: u64,
    pub requires: BTreeSet<crate::model_catalog::Capability>,
    /// Whether this request's data may reach a provider the device does not host.
    pub allow_remote: bool,
}

impl Workload {
    /// A plain text workload for `task`.
    pub fn text(task: impl Into<String>, prompt_tokens: u64, max_output_tokens: u64) -> Self {
        Workload {
            task: task.into(),
            prompt_tokens,
            max_output_tokens,
            requires: [crate::model_catalog::Capability::Text]
                .into_iter()
                .collect(),
            allow_remote: false,
        }
    }

    /// The tokens a chosen model must be able to reserve.
    pub fn reservation_tokens(&self) -> u64 {
        self.prompt_tokens.saturating_add(self.max_output_tokens)
    }
}

/// One candidate the router looked at and did not choose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub model_id: String,
    pub reason: Refusal,
}

/// Why this model and not another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceReason {
    /// Position in the owner's ordered list (0 is the preferred model).
    pub preference: usize,
    /// Owner's price class, shown as evidence; it did not reorder anything.
    pub price_class: Option<u8>,
    /// True when the connection is on this machine.
    pub local: bool,
    /// Set when the choice came from `prefer_equivalent`: the group that was honoured, and the
    /// model whose exhaustion caused the search.
    pub equivalent: Option<(String, String)>,
}

/// What the router decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// This model may run the request; the grant is what admission returned.
    Chosen {
        model_id: String,
        grant: Grant,
        reason: ChoiceReason,
        /// Candidates examined before this one, with their reasons.
        skipped: Vec<Skipped>,
    },
    /// Nothing in the route is eligible right now.
    ///
    /// `until` is the earliest recovery the router can honestly name (a provider wait or a known
    /// window rollover); `None` means the recovery is unknown, which the caller must not read as
    /// "soon".
    Unavailable {
        task: String,
        action: ExhaustedAction,
        until: Option<i64>,
        skipped: Vec<Skipped>,
    },
    /// The catalogue has no route for this workload, so nothing may be sent implicitly.
    NoRoute {
        task: String,
        /// The route key that was consulted after the exact match failed, when there was one.
        fallback_task: Option<String>,
    },
}

impl Decision {
    /// The chosen model, when there is one.
    pub fn model_id(&self) -> Option<&str> {
        match self {
            Decision::Chosen { model_id, .. } => Some(model_id),
            _ => None,
        }
    }

    /// Every candidate the router looked at, in the order it looked.
    pub fn skipped(&self) -> &[Skipped] {
        match self {
            Decision::Chosen { skipped, .. } | Decision::Unavailable { skipped, .. } => skipped,
            Decision::NoRoute { .. } => &[],
        }
    }
}

/// Choose the model for `workload` at `now`.
///
/// This is a decision, not a reservation: nothing is held and nothing is dialed. A caller that
/// acts on it must still reserve the grant and persist the intent (see `rc-runtime`).
pub fn choose(
    catalog: &ModelCatalog,
    ledger: &QuotaLedger,
    workload: &Workload,
    now: i64,
) -> Decision {
    let Some(route) = route_for(catalog, &workload.task) else {
        return Decision::NoRoute {
            task: workload.task.clone(),
            fallback_task: None,
        };
    };
    let order = candidate_order(catalog, route, ledger, workload, now);
    let mut skipped = Vec::new();
    let mut recoveries: Vec<i64> = Vec::new();
    for candidate in &order {
        let entry = candidate.model;
        let requirement = candidate_requirement(entry, workload);
        match admission::admit(catalog, ledger, &requirement, now) {
            Admission::Admitted(grant) => {
                return Decision::Chosen {
                    model_id: entry.id.clone(),
                    grant,
                    reason: ChoiceReason {
                        preference: candidate.preference,
                        price_class: entry.price_class,
                        local: entry.local,
                        equivalent: candidate.equivalent.clone(),
                    },
                    skipped,
                };
            }
            Admission::Refused(reason) => {
                if let Some(until) = recovery_at(&reason, entry, ledger, now) {
                    recoveries.push(until);
                }
                skipped.push(Skipped {
                    model_id: entry.id.clone(),
                    reason,
                });
            }
        }
    }
    Decision::Unavailable {
        task: workload.task.clone(),
        action: route.on_exhausted,
        until: recoveries.into_iter().min(),
        skipped,
    }
}

/// The ordered candidate list for one route, after applying `prefer_equivalent`.
struct Candidate<'a> {
    model: &'a ModelProfile,
    /// Position in the owner's list, kept even when the order is adjusted.
    preference: usize,
    /// `Some((group, exhausted model))` when this candidate was promoted for equivalence.
    equivalent: Option<(String, String)>,
}

fn candidate_order<'a>(
    catalog: &'a ModelCatalog,
    route: &RoutingPreference,
    ledger: &QuotaLedger,
    workload: &Workload,
    now: i64,
) -> Vec<Candidate<'a>> {
    let by_id = |id: &str| catalog.models.iter().find(|model| model.id == id);
    let mut order: Vec<Candidate<'a>> = route
        .model_ids
        .iter()
        .enumerate()
        .filter_map(|(preference, id)| {
            by_id(id).map(|model| Candidate {
                model,
                preference,
                equivalent: None,
            })
        })
        .collect();
    if !route.prefer_equivalent || order.len() < 2 {
        return order;
    }
    // Only the primary can trigger the equivalence rule, and only when it is unavailable: a
    // qualified binding that works needs no substitute.
    let primary = &order[0];
    let Some(group) = primary.model.equivalence_group.clone() else {
        return order;
    };
    if eligible(catalog, ledger, primary.model, workload, now) {
        return order;
    }
    let exhausted = primary.model.id.clone();
    // Stable partition: equivalents first, everything else keeps the owner's order.
    order[1..].sort_by_key(|candidate| {
        if candidate.model.equivalence_group.as_ref() == Some(&group) {
            0
        } else {
            1
        }
    });
    for candidate in order.iter_mut().skip(1) {
        if candidate.model.equivalence_group.as_ref() == Some(&group) {
            candidate.equivalent = Some((group.clone(), exhausted.clone()));
        }
    }
    order
}

/// The requirement one candidate is judged against.
///
/// The output reserve is the *model's* own limit when the catalogue states one: a workload asking
/// for at most 1 024 tokens may be served by a model that can only produce 256, and refusing it
/// for being smaller would rule out the cheapest qualified candidate. Only when the model does not
/// state a limit does the workload's ceiling stand in for it.
fn candidate_requirement(model: &ModelProfile, workload: &Workload) -> Requirement {
    let ceiling = match model.max_output_tokens {
        Some(limit) => workload.max_output_tokens.min(limit),
        None => workload.max_output_tokens,
    };
    Requirement {
        model_id: model.id.clone(),
        task: workload.task.clone(),
        prompt_tokens: workload.prompt_tokens,
        max_output_tokens: ceiling,
        requires: workload.requires.clone(),
        allow_remote: workload.allow_remote,
    }
}

/// True when this model may run the workload right now. Used only to decide whether the
/// equivalence rule applies, so the full decision is still made by `admission::admit`.
fn eligible(
    catalog: &ModelCatalog,
    ledger: &QuotaLedger,
    model: &ModelProfile,
    workload: &Workload,
    now: i64,
) -> bool {
    admission::admit(
        catalog,
        ledger,
        &candidate_requirement(model, workload),
        now,
    )
    .is_admitted()
}

/// The route the owner wrote for `task`, or the `default` route when there is no exact match.
fn route_for<'a>(catalog: &'a ModelCatalog, task: &str) -> Option<&'a RoutingPreference> {
    catalog
        .routes
        .iter()
        .find(|route| route.task == task)
        .or_else(|| catalog.routes.iter().find(|route| route.task == "default"))
}

/// The earliest honest recovery after a refusal, when there is one.
///
/// A provider wait names its own time. An exhausted window names its known rollover. Everything
/// else (an unconfigured model, a missing capability, an unknown allowance) recovers at no time
/// the router can promise, so it contributes nothing rather than a guess.
fn recovery_at(
    reason: &Refusal,
    model: &ModelProfile,
    ledger: &QuotaLedger,
    now: i64,
) -> Option<i64> {
    match reason {
        Refusal::ModelWaiting { until } | Refusal::QuotaWaiting { until, .. } => {
            Some((*until).max(now))
        }
        Refusal::QuotaExceeded { quota_id, .. } => {
            let _ = model;
            match ledger.remaining(quota_id, now) {
                Remaining::Known {
                    reset_at: Some(reset),
                    ..
                } => Some(reset),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_catalog::{Capability, ModelProfile, QuotaPeriod, QuotaWindow, TokenQuota};
    use crate::providers::{ProviderConnection, ProviderKind};

    const NOON: i64 = 1_795_780_800;

    /// A catalogue with a local small model, an equivalent remote binding and an unrelated remote
    /// model, all drawing on one hourly pool.
    fn catalog() -> ModelCatalog {
        let connection = |id: &str, url: &str| ProviderConnection {
            id: id.into(),
            kind: ProviderKind::Ollama,
            base_url: url.into(),
            account: None,
            enabled: true,
        };
        ModelCatalog {
            connections: vec![
                connection("here", "http://127.0.0.1:11434"),
                connection("there", "http://models.example.com:11434"),
            ],
            quotas: vec![TokenQuota {
                id: "pool".into(),
                tokens: 1_000,
                period: QuotaPeriod::Hour,
                window: QuotaWindow::Rolling,
            }],
            models: vec![
                ModelProfile {
                    id: "small-local".into(),
                    connection_id: "here".into(),
                    model_id: "small:1b".into(),
                    enabled: true,
                    local: true,
                    equivalence_group: Some("verified-small".into()),
                    price_class: Some(0),
                    context_tokens: Some(8_192),
                    capabilities: [Capability::Text].into_iter().collect(),
                    quota_ids: vec!["pool".into()],
                    ..ModelProfile::default()
                },
                ModelProfile {
                    id: "small-remote".into(),
                    connection_id: "there".into(),
                    model_id: "small:1b".into(),
                    enabled: true,
                    local: false,
                    equivalence_group: Some("verified-small".into()),
                    price_class: Some(1),
                    capabilities: [Capability::Text].into_iter().collect(),
                    quota_ids: vec!["pool".into()],
                    ..ModelProfile::default()
                },
                ModelProfile {
                    id: "other-remote".into(),
                    connection_id: "there".into(),
                    model_id: "other:70b".into(),
                    enabled: true,
                    local: false,
                    price_class: Some(3),
                    capabilities: [Capability::Text, Capability::ToolCalling]
                        .into_iter()
                        .collect(),
                    quota_ids: vec!["pool".into()],
                    ..ModelProfile::default()
                },
                ModelProfile {
                    id: "vision-local".into(),
                    connection_id: "here".into(),
                    model_id: "vision:2b".into(),
                    enabled: true,
                    local: true,
                    capabilities: [Capability::Text, Capability::ImageInput]
                        .into_iter()
                        .collect(),
                    ..ModelProfile::default()
                },
            ],
            routes: vec![RoutingPreference {
                task: "chat".into(),
                model_ids: vec![
                    "small-local".into(),
                    "other-remote".into(),
                    "small-remote".into(),
                ],
                on_exhausted: ExhaustedAction::Ask,
                ..RoutingPreference::default()
            }],
        }
    }

    fn ledger(catalog: &ModelCatalog) -> QuotaLedger {
        QuotaLedger::new(&catalog.quotas).unwrap()
    }

    fn chat() -> Workload {
        Workload::text("chat", 100, 100)
    }

    #[test]
    fn the_first_eligible_model_in_the_owner_order_is_chosen() {
        let catalog = catalog();
        let ledger = ledger(&catalog);
        let decision = choose(&catalog, &ledger, &chat(), NOON);
        let Decision::Chosen {
            model_id,
            reason,
            skipped,
            grant,
        } = decision.clone()
        else {
            panic!("a local model within the pool is chosen, got {decision:?}");
        };
        assert_eq!(model_id, "small-local");
        assert_eq!(reason.preference, 0);
        assert_eq!(reason.price_class, Some(0));
        assert!(reason.local);
        assert_eq!(reason.equivalent, None);
        assert!(skipped.is_empty(), "the first candidate was eligible");
        assert_eq!(grant.reservation_tokens, 200);
        assert_eq!(grant.connection_id, "here");
        // The decision is deterministic and reserves nothing.
        assert_eq!(choose(&catalog, &ledger, &chat(), NOON), decision);
        assert!(ledger.outstanding().is_empty());
    }

    #[test]
    fn a_disabled_or_incapable_model_is_skipped_with_its_reason() {
        let mut catalog = catalog();
        catalog.models[0].enabled = false;
        let quota_ledger = ledger(&catalog);
        // The owner authorised egress, so the remote candidates are eligible.
        let mut workload = chat();
        workload.allow_remote = true;
        let Decision::Chosen {
            model_id,
            skipped,
            reason,
            ..
        } = choose(&catalog, &quota_ledger, &workload, NOON)
        else {
            panic!("the next candidate runs");
        };
        assert_eq!(model_id, "other-remote");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].model_id, "small-local");
        assert_eq!(skipped[0].reason, Refusal::Disabled("small-local".into()));
        assert_eq!(reason.preference, 1, "the owner's position is reported");
        // A workload a candidate cannot satisfy is skipped for that reason. Only models the owner
        // listed are candidates, so the vision model joins the route to take part.
        let mut catalog = catalog.clone();
        catalog.models[0].enabled = true;
        catalog.routes[0].model_ids.push("vision-local".into());
        let quota_ledger = ledger(&catalog);
        let mut vision = chat();
        vision.requires = [Capability::ImageInput].into_iter().collect();
        let Decision::Chosen {
            model_id, skipped, ..
        } = choose(&catalog, &quota_ledger, &vision, NOON)
        else {
            panic!("the vision model runs");
        };
        assert_eq!(model_id, "vision-local");
        assert_eq!(skipped.len(), 3, "every unqualified candidate was examined");
        assert_eq!(
            skipped[0].reason,
            Refusal::MissingCapability(Capability::ImageInput)
        );
    }

    #[test]
    fn price_and_locality_never_reorder_the_owner_preference() {
        let mut catalog = catalog();
        let ledger = ledger(&catalog);
        // The owner listed the expensive remote model first; the router keeps that order.
        catalog.routes[0].model_ids = vec!["other-remote".into(), "small-local".into()];
        let mut workload = chat();
        workload.allow_remote = true;
        let Decision::Chosen {
            model_id, reason, ..
        } = choose(&catalog, &ledger, &workload, NOON)
        else {
            panic!("the first listed model wins");
        };
        assert_eq!(model_id, "other-remote");
        assert_eq!(reason.price_class, Some(3), "the price is shown, not used");
        assert!(!reason.local);
    }

    #[test]
    fn an_equivalent_binding_is_preferred_once_the_primary_is_unavailable() {
        let mut catalog = catalog();
        let ledger = ledger(&catalog);
        // The primary is switched off, so the search moves on — the pool still has room, which is
        // what separates this rule from an exhausted budget.
        catalog.models[0].enabled = false;
        catalog.routes[0].prefer_equivalent = true;
        let mut workload = chat();
        workload.allow_remote = true;
        let Decision::Chosen {
            model_id,
            reason,
            skipped,
            ..
        } = choose(&catalog, &ledger, &workload, NOON)
        else {
            panic!("a candidate still runs");
        };
        assert_eq!(
            model_id, "small-remote",
            "the verified equivalent is promoted over the unrelated model"
        );
        assert_eq!(
            reason.equivalent,
            Some(("verified-small".to_string(), "small-local".to_string())),
            "the choice names the group and what it substituted for"
        );
        assert_eq!(reason.preference, 2, "its owner position is still reported");
        assert!(
            skipped.iter().any(|skip| skip.model_id == "small-local"),
            "the exhausted primary is explained, not hidden"
        );
        // Without the switch the owner's order is followed instead.
        catalog.routes[0].prefer_equivalent = false;
        let Decision::Chosen { model_id, .. } = choose(&catalog, &ledger, &workload, NOON) else {
            panic!("a candidate still runs");
        };
        assert_eq!(model_id, "other-remote");
    }

    #[test]
    fn an_exhausted_route_reports_the_earliest_recovery_it_can_name() {
        let mut catalog = catalog();
        let mut ledger = ledger(&catalog);
        ledger
            .reserve("spend", &["pool".to_string()], 1_000, NOON)
            .unwrap();
        ledger.settle("spend", Some(1_000)).unwrap();
        catalog.routes[0].model_ids = vec!["small-local".into()];
        catalog.routes[0].on_exhausted = ExhaustedAction::Wait;
        let decision = choose(&catalog, &ledger, &chat(), NOON);
        let Decision::Unavailable {
            action,
            until,
            skipped,
            task,
        } = decision
        else {
            panic!("nothing is eligible, got {decision:?}");
        };
        assert_eq!(task, "chat");
        assert_eq!(action, ExhaustedAction::Wait);
        assert_eq!(
            until,
            Some(NOON + 3_600),
            "the window's own rollover is the honest answer"
        );
        assert_eq!(skipped.len(), 1);
        assert!(matches!(skipped[0].reason, Refusal::QuotaExceeded { .. }));
    }

    #[test]
    fn a_provider_wait_is_reported_as_the_recovery_time() {
        let mut catalog = catalog();
        let mut ledger = ledger(&catalog);
        ledger.record_rate_limit(&admission::model_scope("small-local"), Some(120), NOON);
        catalog.routes[0].model_ids = vec!["small-local".into()];
        let Decision::Unavailable { until, .. } = choose(&catalog, &ledger, &chat(), NOON) else {
            panic!("the only candidate is waiting");
        };
        assert_eq!(until, Some(NOON + 120));
    }

    #[test]
    fn an_unknown_recovery_is_not_invented() {
        let mut catalog = catalog();
        catalog.routes[0].model_ids = vec!["small-local".into()];
        catalog.quotas[0].window = QuotaWindow::ProviderDefined;
        let ledger = QuotaLedger::new(&catalog.quotas).unwrap();
        let Decision::Unavailable { until, skipped, .. } = choose(&catalog, &ledger, &chat(), NOON)
        else {
            panic!("an unknown allowance is not usable");
        };
        assert_eq!(until, None, "an unknown reset is not a promise");
        assert!(matches!(
            skipped[0].reason,
            Refusal::UnknownRemaining { .. }
        ));
    }

    #[test]
    fn the_owner_action_decides_what_happens_when_nothing_is_eligible() {
        let mut catalog = catalog();
        let ledger = ledger(&catalog);
        catalog.routes[0].model_ids = vec!["small-local".into()];
        catalog.models[0].enabled = false;
        for action in [
            ExhaustedAction::Ask,
            ExhaustedAction::Wait,
            ExhaustedAction::Fallback,
        ] {
            catalog.routes[0].on_exhausted = action;
            let Decision::Unavailable {
                action: decided, ..
            } = choose(&catalog, &ledger, &chat(), NOON)
            else {
                panic!("nothing is eligible");
            };
            assert_eq!(decided, action, "the owner's choice is reported unchanged");
        }
    }

    #[test]
    fn a_workload_with_no_route_gets_no_model() {
        let catalog = catalog();
        let ledger = ledger(&catalog);
        let Decision::NoRoute { task, .. } =
            choose(&catalog, &ledger, &Workload::text("coding", 10, 10), NOON)
        else {
            panic!("an unlisted workload has no candidates");
        };
        assert_eq!(task, "coding");
    }

    #[test]
    fn the_default_route_covers_a_workload_the_owner_did_not_name() {
        let mut catalog = catalog();
        let ledger = ledger(&catalog);
        catalog.routes.push(RoutingPreference {
            task: "default".into(),
            model_ids: vec!["small-local".into()],
            ..RoutingPreference::default()
        });
        let Decision::Chosen { model_id, .. } = choose(
            &catalog,
            &ledger,
            &Workload::text("translation", 10, 10),
            NOON,
        ) else {
            panic!("the default route answers for unnamed workloads");
        };
        assert_eq!(model_id, "small-local");
    }

    #[test]
    fn a_remote_candidate_is_skipped_when_egress_is_not_authorised() {
        let mut catalog = catalog();
        let ledger = ledger(&catalog);
        catalog.models[0].enabled = false;
        catalog.routes[0].model_ids = vec!["small-local".into(), "other-remote".into()];
        let Decision::Unavailable { skipped, .. } = choose(&catalog, &ledger, &chat(), NOON) else {
            panic!("a remote model is not used without permission");
        };
        assert_eq!(skipped.len(), 2);
        assert_eq!(skipped[1].reason, Refusal::RemoteNotAuthorized);
    }
}

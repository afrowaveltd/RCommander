//! Dispatch tests: the whole path, with no socket anywhere.
//!
//! Every test here answers the same question in a different way: *did anything reach a provider
//! that should not have, and was the allowance booked honestly?* The backend is a fake that records
//! every call, so "no call happened" is an assertion rather than a hope.

use super::*;
use rc_core::model_catalog::{
    ModelProfile, QuotaPeriod, QuotaWindow, RoutingPreference, TokenQuota,
};
use rc_core::providers::ProviderConnection;
use rc_core::quota::{QuotaLedger, Remaining};
use std::sync::Mutex;

/// 2026-11-27T12:00:00Z, so nothing depends on the wall clock.
const NOON: i64 = 1_795_780_800;

/// A backend that answers once and records what it was asked.
struct FakeBackend {
    reply: Mutex<Option<Result<ChatReply, LlmError>>>,
    calls: Mutex<Vec<(String, String, String)>>,
    output_limits: Mutex<Vec<Option<u64>>>,
    /// Pieces handed out by `chat_stream` before the answer itself arrives.
    chunks: Vec<String>,
}

impl FakeBackend {
    /// An answer that reports both token counts.
    fn answering(prompt_tokens: u64, answer_tokens: u64) -> FakeBackend {
        FakeBackend::with(Ok(ChatReply {
            content: "answer".into(),
            model: "tiny:1b".into(),
            done_reason: Some("stop".into()),
            prompt_tokens: Some(prompt_tokens),
            answer_tokens: Some(answer_tokens),
        }))
    }

    /// An answer that reports no usage at all.
    fn answering_without_usage() -> FakeBackend {
        FakeBackend::with(Ok(ChatReply {
            content: "answer".into(),
            model: "tiny:1b".into(),
            done_reason: Some("stop".into()),
            prompt_tokens: None,
            answer_tokens: None,
        }))
    }

    fn failing(error: LlmError) -> FakeBackend {
        FakeBackend::with(Err(error))
    }

    fn with(reply: Result<ChatReply, LlmError>) -> FakeBackend {
        FakeBackend {
            reply: Mutex::new(Some(reply)),
            calls: Mutex::new(Vec::new()),
            output_limits: Mutex::new(Vec::new()),
            chunks: Vec::new(),
        }
    }

    /// An answer that arrives in pieces, like a streaming provider.
    fn streaming(chunks: &[&str], prompt_tokens: u64, answer_tokens: u64) -> FakeBackend {
        FakeBackend {
            chunks: chunks.iter().map(|chunk| (*chunk).to_string()).collect(),
            ..FakeBackend::answering(prompt_tokens, answer_tokens)
        }
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl Backend for FakeBackend {
    fn chat(&self, resolved: &ResolvedModel, request: &ChatRequest) -> Result<ChatReply, LlmError> {
        self.output_limits
            .lock()
            .unwrap()
            .push(request.max_output_tokens);
        self.calls.lock().unwrap().push((
            resolved.connection_id.clone(),
            resolved.provider_model.clone(),
            request.model.clone(),
        ));
        match self.reply.lock().unwrap().take() {
            Some(reply) => reply,
            // A second call in one test means the dispatcher dialed twice; fail loudly.
            None => Err(LlmError::new("the fake backend was asked twice")),
        }
    }

    fn chat_stream(
        &self,
        resolved: &ResolvedModel,
        request: &ChatRequest,
        control: &rc_llm::StreamControl,
        on_event: &mut dyn FnMut(rc_llm::StreamEvent<'_>),
    ) -> Result<ChatReply, LlmError> {
        if control.is_cancelled() {
            return Err(LlmError::new("request cancelled"));
        }
        for chunk in &self.chunks {
            on_event(rc_llm::StreamEvent::Activity);
            on_event(rc_llm::StreamEvent::Text(chunk));
        }
        self.chat(resolved, request)
    }
}

fn catalog(local: bool, pool_tokens: u64) -> ModelCatalog {
    ModelCatalog {
        connections: vec![ProviderConnection {
            id: "default".into(),
            kind: ProviderKind::Ollama,
            base_url: "http://127.0.0.1:11434".into(),
            account: None,
            enabled: true,
        }],
        quotas: vec![TokenQuota {
            id: "pool".into(),
            tokens: pool_tokens,
            period: QuotaPeriod::Hour,
            window: QuotaWindow::Rolling,
        }],
        models: vec![ModelProfile {
            id: "tiny".into(),
            connection_id: "default".into(),
            model_id: "tiny:1b".into(),
            enabled: true,
            local,
            context_tokens: Some(8_192),
            capabilities: [Capability::Text].into_iter().collect(),
            quota_ids: vec!["pool".into()],
            ..ModelProfile::default()
        }],
        routes: vec![RoutingPreference {
            task: "default".into(),
            model_ids: vec!["tiny".into()],
            ..RoutingPreference::default()
        }],
    }
}

/// A ledger backed by a real file in a temporary directory.
fn ledger(dir: &tempfile::TempDir, catalog: &ModelCatalog) -> QuotaLedger {
    QuotaLedger::load(dir.path().join("quota.toml"), &catalog.quotas).unwrap()
}

fn reopened(dir: &tempfile::TempDir, catalog: &ModelCatalog) -> QuotaLedger {
    QuotaLedger::load(dir.path().join("quota.toml"), &catalog.quotas).unwrap()
}

fn request(model_id: &str) -> DispatchRequest {
    DispatchRequest::new("attempt-1", model_id, vec![Message::user("hello")], 100, 50)
}

fn remaining(ledger: &QuotaLedger) -> u64 {
    match ledger.remaining("pool", NOON) {
        Remaining::Known { tokens, .. } => tokens,
        other => panic!("the pool should be known, got {other:?}"),
    }
}

#[test]
fn an_admitted_request_is_reserved_charged_and_reported_with_its_provenance() {
    let catalog = catalog(true, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let backend = FakeBackend::answering(300, 120);
    let outcome = Dispatcher::new(backend).dispatch(&catalog, &mut ledger, &request("tiny"), NOON);
    let DispatchOutcome::Answered {
        content,
        reply_model,
        receipt,
    } = outcome
    else {
        panic!("an admitted local request answers, got {outcome:?}");
    };
    assert_eq!(content, "answer");
    assert_eq!(reply_model, "tiny:1b");
    assert_eq!(receipt.model_id, "tiny");
    assert_eq!(receipt.connection_id, "default");
    assert_eq!(receipt.provider_model, "tiny:1b");
    assert!(!receipt.remote, "a local model keeps the data here");
    assert_eq!(
        receipt.reserved, 150,
        "the estimate plus the whole output reserve"
    );
    assert_eq!(
        receipt.charged,
        Some(420),
        "the provider's own numbers are booked"
    );
    assert_eq!(receipt.reported_tokens, Some(420));
    assert!(!receipt.usage_missing);
    assert_eq!(receipt.save_error, None);
    // The receipt survives a restart, because it was written to the ledger file.
    let reopened = reopened(&dir, &catalog);
    assert_eq!(remaining(&reopened), 580);
    assert!(
        reopened.outstanding().is_empty(),
        "the reservation was reconciled"
    );
    assert_eq!(reopened.charges("pool").len(), 1);
}

#[test]
fn a_remote_model_without_egress_authorization_never_reaches_the_backend() {
    let catalog = catalog(false, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let dispatcher = Dispatcher::new(FakeBackend::answering(10, 10));
    let outcome = dispatcher.dispatch(&catalog, &mut ledger, &request("tiny"), NOON);
    assert_eq!(
        outcome,
        DispatchOutcome::Refused {
            refusal: Refusal::RemoteNotAuthorized
        }
    );
    assert_eq!(
        dispatcher.backend().call_count(),
        0,
        "an unauthorized egress never becomes a request"
    );
    assert_eq!(remaining(&ledger), 1_000, "a refusal costs no allowance");
    assert!(ledger.outstanding().is_empty());
    assert!(
        !dir.path().join("quota.toml").exists(),
        "a refusal writes nothing at all"
    );
}

#[test]
fn a_model_that_cannot_be_resolved_is_not_dialed() {
    let catalog = catalog(true, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let dispatcher = Dispatcher::new(FakeBackend::answering(1, 1));
    let outcome = dispatcher.dispatch(&catalog, &mut ledger, &request("ghost"), NOON);
    assert_eq!(
        outcome,
        DispatchOutcome::Unresolvable {
            error: ResolveError::UnknownModel("ghost".into())
        }
    );
    assert_eq!(dispatcher.backend().call_count(), 0);
}

#[test]
fn a_waiting_pool_refuses_before_the_call_and_proceeds_once_it_expires() {
    let catalog = catalog(true, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    ledger.record_rate_limit("pool", Some(60), NOON);
    let dispatcher = Dispatcher::new(FakeBackend::answering(10, 10));
    let outcome = dispatcher.dispatch(&catalog, &mut ledger, &request("tiny"), NOON);
    assert_eq!(
        outcome,
        DispatchOutcome::Refused {
            refusal: Refusal::QuotaWaiting {
                quota_id: "pool".into(),
                until: NOON + 60,
            }
        }
    );
    assert_eq!(
        dispatcher.backend().call_count(),
        0,
        "waiting means nobody is asked"
    );
    // The wait expires on its own; the next attempt goes through.
    let dispatcher = Dispatcher::new(FakeBackend::answering(10, 10));
    let outcome = dispatcher.dispatch(&catalog, &mut ledger, &request("tiny"), NOON + 60);
    assert!(outcome.is_answered(), "after the wait: {outcome:?}");
    assert_eq!(dispatcher.backend().call_count(), 1);
}

#[test]
fn a_rate_limit_releases_the_allowance_and_blocks_the_next_attempt() {
    let catalog = catalog(true, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let mut error = LlmError::new("the server answered 429 for /api/chat");
    error.notice = Some(ProviderNotice::new(
        429,
        Some(90),
        "{\"error\":\"slow down\"}",
    ));
    let dispatcher = Dispatcher::new(FakeBackend::failing(error));
    let outcome = dispatcher.dispatch(&catalog, &mut ledger, &request("tiny"), NOON);
    let DispatchOutcome::RateLimited {
        scope,
        until,
        notice,
        receipt,
    } = outcome
    else {
        panic!("a 429 is reported as a wait, got {outcome:?}");
    };
    assert_eq!(scope, "model:tiny");
    assert_eq!(until, NOON + 90);
    assert_eq!(notice.status, 429);
    assert_eq!(notice.retry_after_seconds, Some(90));
    assert_eq!(
        receipt.charged,
        Some(0),
        "a rate limit generates nothing, so it costs nothing"
    );
    assert_eq!(dispatcher.backend().call_count(), 1);
    // The allowance came back and the wait is durable for both scopes.
    let mut reopened = reopened(&dir, &catalog);
    assert_eq!(remaining(&reopened), 1_000);
    assert!(reopened.outstanding().is_empty());
    assert_eq!(reopened.wait_until("model:tiny", NOON), Some(NOON + 90));
    assert_eq!(
        reopened.wait_until("pool", NOON),
        Some(NOON + 90),
        "the shared pool waits too: patience is cheaper than a retry storm"
    );
    // A second attempt inside the wait never reaches a backend, even after a restart.
    let dispatcher = Dispatcher::new(FakeBackend::answering(1, 1));
    let outcome = dispatcher.dispatch(&catalog, &mut reopened, &request("tiny"), NOON + 10);
    assert_eq!(
        outcome,
        DispatchOutcome::Refused {
            refusal: Refusal::ModelWaiting { until: NOON + 90 }
        }
    );
    assert_eq!(dispatcher.backend().call_count(), 0);
}

#[test]
fn missing_usage_is_charged_in_full() {
    let catalog = catalog(true, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let dispatcher = Dispatcher::new(FakeBackend::answering_without_usage());
    let outcome = dispatcher.dispatch(&catalog, &mut ledger, &request("tiny"), NOON);
    let DispatchOutcome::Answered { receipt, .. } = outcome else {
        panic!("an answer without usage is still an answer, got {outcome:?}");
    };
    assert_eq!(
        receipt.charged,
        Some(150),
        "an unreported request is charged in full"
    );
    assert!(receipt.usage_missing);
    assert_eq!(receipt.reported_tokens, None);
}

#[test]
fn a_transport_failure_after_dispatch_is_charged_conservatively() {
    let catalog = catalog(true, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let dispatcher = Dispatcher::new(FakeBackend::failing(LlmError::new("connection refused")));
    let outcome = dispatcher.dispatch(&catalog, &mut ledger, &request("tiny"), NOON);
    let DispatchOutcome::Failed { error, receipt, .. } = outcome else {
        panic!("a transport failure is reported as such, got {outcome:?}");
    };
    assert!(error.contains("connection refused"), "message: {error}");
    assert_eq!(
        receipt.charged,
        Some(150),
        "the request may have been processed, so it is not free"
    );
    assert!(receipt.usage_missing);
    assert_eq!(remaining(&reopened(&dir, &catalog)), 850);
}

#[test]
fn an_unwritable_ledger_stops_the_request_before_any_call() {
    let catalog = catalog(true, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let blocked = dir.path().join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    ledger.set_path(&blocked);
    let dispatcher = Dispatcher::new(FakeBackend::answering(1, 1));
    let outcome = dispatcher.dispatch(&catalog, &mut ledger, &request("tiny"), NOON);
    let DispatchOutcome::NotPersisted { error } = outcome else {
        panic!("an unrecordable request is not sent, got {outcome:?}");
    };
    assert!(!error.is_empty());
    assert_eq!(
        dispatcher.backend().call_count(),
        0,
        "nothing may leave the machine when its cost cannot be recorded"
    );
    assert!(
        ledger.outstanding().is_empty(),
        "the allowance was released"
    );
    assert_eq!(remaining(&ledger), 1_000);
}

#[test]
fn two_models_sharing_one_account_cannot_overspend_the_same_pool() {
    let mut catalog = catalog(true, 1_000);
    catalog.models.push(ModelProfile {
        id: "big".into(),
        connection_id: "default".into(),
        model_id: "big:7b".into(),
        enabled: true,
        local: true,
        capabilities: [Capability::Text].into_iter().collect(),
        quota_ids: vec!["pool".into()],
        ..ModelProfile::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let dispatcher = Dispatcher::new(FakeBackend::answering(500, 400));
    let first = dispatcher.dispatch(&catalog, &mut ledger, &request("tiny"), NOON);
    assert_eq!(
        first.receipt().and_then(|receipt| receipt.charged),
        Some(900)
    );
    assert_eq!(remaining(&ledger), 100);
    // The other model draws from the same pool, and the pool is now too small.
    let second = Dispatcher::new(FakeBackend::answering(10, 10));
    let outcome = second.dispatch(&catalog, &mut ledger, &request("big"), NOON);
    assert_eq!(
        outcome,
        DispatchOutcome::Refused {
            refusal: Refusal::QuotaExceeded {
                quota_id: "pool".into(),
                remaining: 100,
                wanted: 150,
            }
        }
    );
    assert_eq!(second.backend().call_count(), 0);
}

#[test]
fn a_streamed_answer_is_forwarded_in_order_and_charged_once() {
    let catalog = catalog(true, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let dispatcher = Dispatcher::new(FakeBackend::streaming(&["an", "swer"], 300, 120));
    let control = rc_llm::StreamControl::default();
    let mut seen: Vec<String> = Vec::new();
    let mut chunks = 0;
    let outcome = dispatcher.dispatch_stream(
        &catalog,
        &mut ledger,
        &request("tiny"),
        &control,
        &mut |event| match event {
            rc_llm::StreamEvent::Text(text) => {
                chunks += 1;
                seen.push(text.to_string());
            }
            rc_llm::StreamEvent::Activity | rc_llm::StreamEvent::LoadDuration(_) => {}
        },
        NOON,
    );
    assert_eq!(seen, vec!["an".to_string(), "swer".to_string()]);
    assert_eq!(chunks, 2, "each piece reaches the caller as it arrives");
    let DispatchOutcome::Answered {
        content, receipt, ..
    } = outcome
    else {
        panic!("a streamed answer still answers, got {outcome:?}");
    };
    assert_eq!(content, "answer");
    assert_eq!(receipt.charged, Some(420));
    assert_eq!(remaining(&reopened(&dir, &catalog)), 580);
}

#[test]
fn a_cancelled_stream_is_charged_because_the_request_was_already_sent() {
    let catalog = catalog(true, 1_000);
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = ledger(&dir, &catalog);
    let dispatcher = Dispatcher::new(FakeBackend::streaming(&["half"], 300, 120));
    let control = rc_llm::StreamControl::default();
    control.cancel();
    let outcome = dispatcher.dispatch_stream(
        &catalog,
        &mut ledger,
        &request("tiny"),
        &control,
        &mut |_| {},
        NOON,
    );
    let DispatchOutcome::Failed { error, receipt, .. } = outcome else {
        panic!("a cancelled request is a failure, got {outcome:?}");
    };
    assert!(error.contains("cancelled"), "message: {error}");
    assert_eq!(
        receipt.charged,
        Some(150),
        "the request was on the wire, so the allowance is not silently returned"
    );
    assert_eq!(dispatcher.backend().call_count(), 0, "nothing was streamed");
}

#[test]
fn the_real_backend_builds_the_provider_from_the_resolved_connection() {
    let catalog = catalog(true, 1_000);
    let resolved = rc_core::providers::resolve(&catalog, "tiny").unwrap();
    let provider = LlmBackend::default().provider(&resolved);
    assert_eq!(provider.kind(), "ollama");
    assert_eq!(provider.base_url(), "http://127.0.0.1:11434");
}

#[test]
fn dispatch_passes_the_reserved_output_limit_to_the_backend() {
    for streaming in [false, true] {
        let catalog = catalog(true, 1000);
        let dir = tempfile::tempdir().unwrap();
        let mut ledger = ledger(&dir, &catalog);
        let dispatcher = Dispatcher::new(FakeBackend::answering(100, 30));
        let req = request("tiny");
        let result = if streaming {
            dispatcher.dispatch_stream(
                &catalog,
                &mut ledger,
                &req,
                &rc_llm::StreamControl::default(),
                &mut |_| {},
                NOON,
            )
        } else {
            dispatcher.dispatch(&catalog, &mut ledger, &req, NOON)
        };
        assert!(matches!(result, DispatchOutcome::Answered { .. }));
        assert_eq!(
            *dispatcher.backend().output_limits.lock().unwrap(),
            vec![Some(50)]
        );
    }
}

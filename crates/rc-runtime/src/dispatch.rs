//! Admitted, ledger-backed model dispatch.
//!
//! This is the first vertical of the headless runtime: one request, checked against the owner's
//! catalogue, admitted against the owner's budget, reserved in the durable ledger, dispatched
//! through a [`Backend`], and reconciled against what the provider reported. The backend is a
//! trait so the whole path is testable with no socket, and `rc-llm` supplies the real one.

use rc_core::admission::{self, Admission, Refusal, Requirement};
use rc_core::model_catalog::{Capability, ModelCatalog};
use rc_core::providers::{self, ProviderKind, ResolveError, ResolvedModel};
use rc_core::quota::{LedgerError, QuotaLedger};
use rc_llm::{ChatReply, ChatRequest, LlmError, Message, ProviderNotice};
use std::collections::BTreeSet;

/// Output reserve used when the catalogue does not state one for the model.
///
/// A reservation is estimated input plus *the whole possible output*, and a client that does not
/// know the limit has to name something; this is the documented default rather than a promise
/// about any provider.
pub const DEFAULT_MAX_OUTPUT_TOKENS: u64 = 1024;

/// One request for a model, before anything has been decided about it.
#[derive(Clone, Debug, PartialEq)]
pub struct DispatchRequest {
    /// Stable id of this attempt; it names the reservation and therefore the receipt. A retry uses
    /// a new id, because the ledger must not confuse two attempts.
    pub request_id: String,
    /// Catalogue model id, never a provider model name.
    pub model_id: String,
    pub messages: Vec<Message>,
    /// The caller's estimate of the prompt; an estimate is all a client can honestly claim.
    pub prompt_tokens: u64,
    /// Maximum output the answer may use, reserved in full before dispatch.
    pub max_output_tokens: u64,
    pub temperature: Option<f32>,
    /// Workload/role the owner's routing preferences are keyed by.
    pub task: String,
    /// Capabilities the workload requires.
    pub requires: BTreeSet<Capability>,
    /// Whether this request's data may be sent to a provider the device does not host.
    pub allow_remote: bool,
}

impl DispatchRequest {
    /// A plain text request. Local data stays local until the caller says otherwise.
    pub fn new(
        request_id: impl Into<String>,
        model_id: impl Into<String>,
        messages: Vec<Message>,
        prompt_tokens: u64,
        max_output_tokens: u64,
    ) -> DispatchRequest {
        DispatchRequest {
            request_id: request_id.into(),
            model_id: model_id.into(),
            messages,
            prompt_tokens,
            max_output_tokens,
            temperature: None,
            task: "default".into(),
            requires: [Capability::Text].into_iter().collect(),
            allow_remote: false,
        }
    }

    /// The tokens this request may consume: the estimate plus the whole output reserve.
    pub fn reservation_tokens(&self) -> u64 {
        self.prompt_tokens.saturating_add(self.max_output_tokens)
    }
}

/// What happened to the allowance and where the answer came from.
///
/// This is the evidence the status line, the audit trail and the review show: which connection
/// answered, whether that was somebody else's machine, how much was held and what was booked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub request_id: String,
    pub model_id: String,
    pub connection_id: String,
    pub provider_model: String,
    /// True when the connection is not the device itself, so the data left the machine.
    pub remote: bool,
    /// Tokens held before dispatch.
    pub reserved: u64,
    /// Tokens booked afterwards; `None` when nothing was written to the ledger.
    pub charged: Option<u64>,
    /// True when the provider reported no usable usage and the reservation was charged in full.
    pub usage_missing: bool,
    /// Usage the provider reported, when it reported all of it.
    pub reported_tokens: Option<u64>,
    /// Set when the ledger could not be written after the call: the answer is real, the accounting
    /// of it is not durable yet.
    pub save_error: Option<String>,
}

impl Receipt {
    fn new(request: &DispatchRequest, resolved: &ResolvedModel, reserved: u64) -> Receipt {
        Receipt {
            request_id: request.request_id.clone(),
            model_id: resolved.model_id.clone(),
            connection_id: resolved.connection_id.clone(),
            provider_model: resolved.provider_model.clone(),
            remote: !resolved.local,
            reserved,
            charged: None,
            usage_missing: false,
            reported_tokens: None,
            save_error: None,
        }
    }
}

/// The answer of [`Dispatcher::dispatch`].
#[derive(Clone, Debug, PartialEq)]
pub enum DispatchOutcome {
    /// The model answered. `receipt` says what it cost and which connection produced it.
    Answered {
        content: String,
        reply_model: String,
        receipt: Receipt,
    },
    /// Admission refused, so nothing was reserved and no backend was called.
    Refused { refusal: Refusal },
    /// The catalogue model could not be turned into a dialable connection.
    Unresolvable { error: ResolveError },
    /// A limit refused the reservation itself. Admission normally catches this first; the variant
    /// exists so a race can never be silently ignored.
    NotReserved { error: LedgerError },
    /// The dispatch intent could not be persisted, so the request was never sent.
    NotPersisted { error: String },
    /// The provider said "later". Nothing was generated, so nothing is charged, and the announced
    /// delay is recorded per scope so the next attempt waits instead of retrying into the wall.
    RateLimited {
        scope: String,
        until: i64,
        notice: ProviderNotice,
        receipt: Receipt,
    },
    /// The request may have been processed: the reservation is charged in full.
    Failed {
        error: String,
        notice: Option<ProviderNotice>,
        receipt: Receipt,
    },
}

impl DispatchOutcome {
    pub fn is_answered(&self) -> bool {
        matches!(self, DispatchOutcome::Answered { .. })
    }

    /// The answer text, when there is one.
    pub fn content(&self) -> Option<&str> {
        match self {
            DispatchOutcome::Answered { content, .. } => Some(content),
            _ => None,
        }
    }

    /// The evidence, whenever a model was resolved.
    pub fn receipt(&self) -> Option<&Receipt> {
        match self {
            DispatchOutcome::Answered { receipt, .. }
            | DispatchOutcome::RateLimited { receipt, .. }
            | DispatchOutcome::Failed { receipt, .. } => Some(receipt),
            _ => None,
        }
    }
}

/// The only thing the dispatcher needs from a model client.
///
/// A trait rather than a concrete provider, so the whole path — refusal, reservation, crash
/// reconciliation, rate-limit handling — is testable without a socket, and so a future transport
/// (a different provider family, an external runtime) plugs in here.
pub trait Backend: Send + Sync {
    /// Ask `resolved` for an answer. The provider model name is already in `request`.
    fn chat(&self, resolved: &ResolvedModel, request: &ChatRequest) -> Result<ChatReply, LlmError>;

    /// Ask for an answer and hand every piece of it to `on_event` as it arrives.
    ///
    /// The default falls back to [`Self::chat`] and reports the whole answer as one chunk, so a
    /// backend that cannot stream still works — the caller just sees one big update. A cancelled
    /// `control` must stop the work and report a failure: the request is already on the wire, so
    /// the dispatcher charges it as it would charge any failure.
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
        let reply = self.chat(resolved, request)?;
        on_event(rc_llm::StreamEvent::Activity);
        on_event(rc_llm::StreamEvent::Text(&reply.content));
        Ok(reply)
    }
}

/// A backend shared by several workers: the app keeps one handle and hands clones to threads.
impl Backend for std::sync::Arc<dyn Backend> {
    fn chat(&self, resolved: &ResolvedModel, request: &ChatRequest) -> Result<ChatReply, LlmError> {
        (**self).chat(resolved, request)
    }

    fn chat_stream(
        &self,
        resolved: &ResolvedModel,
        request: &ChatRequest,
        control: &rc_llm::StreamControl,
        on_event: &mut dyn FnMut(rc_llm::StreamEvent<'_>),
    ) -> Result<ChatReply, LlmError> {
        (**self).chat_stream(resolved, request, control, on_event)
    }
}

/// The real backend: the plain-HTTP client from `rc-llm`, chosen by the connection's kind.
#[derive(Clone, Debug, Default)]
pub struct LlmBackend {
    /// Per-request timeout, when the caller has an opinion.
    pub timeout: Option<std::time::Duration>,
}

impl LlmBackend {
    /// Build the provider the connection asks for.
    ///
    /// `ProviderKind` only lists families `rc-llm` implements, so this match cannot claim a
    /// transport that does not exist.
    pub fn provider(&self, resolved: &ResolvedModel) -> Box<dyn rc_llm::Provider> {
        match resolved.kind {
            ProviderKind::Ollama => {
                let provider = rc_llm::OllamaProvider::new(resolved.base_url.clone());
                match self.timeout {
                    Some(timeout) => Box::new(provider.with_timeout(timeout)),
                    None => Box::new(provider),
                }
            }
        }
    }
}

impl Backend for LlmBackend {
    fn chat(&self, resolved: &ResolvedModel, request: &ChatRequest) -> Result<ChatReply, LlmError> {
        self.provider(resolved).chat(request)
    }

    fn chat_stream(
        &self,
        resolved: &ResolvedModel,
        request: &ChatRequest,
        control: &rc_llm::StreamControl,
        on_event: &mut dyn FnMut(rc_llm::StreamEvent<'_>),
    ) -> Result<ChatReply, LlmError> {
        self.provider(resolved)
            .chat_stream_controlled(request, control, on_event)
    }
}

/// Runs one request through resolve → admit → reserve → persist → call → reconcile.
#[derive(Clone, Debug)]
pub struct Dispatcher<B: Backend> {
    backend: B,
}

impl<B: Backend> Dispatcher<B> {
    pub fn new(backend: B) -> Dispatcher<B> {
        Dispatcher { backend }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Dispatch one request.
    ///
    /// `now` is Unix seconds, supplied by the caller so the whole path is deterministic. The ledger
    /// must have a path: its dispatch intent is persisted **before** the call, because an
    /// allowance that was taken and then lost to a crash must remain visible.
    pub fn dispatch(
        &self,
        catalog: &ModelCatalog,
        ledger: &mut QuotaLedger,
        request: &DispatchRequest,
        now: i64,
    ) -> DispatchOutcome {
        self.run(catalog, ledger, request, now, &mut |resolved, chat| {
            self.backend.chat(resolved, chat)
        })
    }

    /// Dispatch one request and forward the answer as it arrives.
    ///
    /// Everything except the transport is identical to [`Self::dispatch`]: the same admission, the
    /// same reservation, the same conservative reconciliation. A cancelled stream is a failure for
    /// accounting purposes, because the request was already on the wire and may have produced
    /// output the provider will still bill.
    pub fn dispatch_stream(
        &self,
        catalog: &ModelCatalog,
        ledger: &mut QuotaLedger,
        request: &DispatchRequest,
        control: &rc_llm::StreamControl,
        on_event: &mut dyn FnMut(rc_llm::StreamEvent<'_>),
        now: i64,
    ) -> DispatchOutcome {
        let backend = &self.backend;
        self.run(catalog, ledger, request, now, &mut |resolved, chat| {
            backend.chat_stream(resolved, chat, control, on_event)
        })
    }

    /// The shared path: resolve, admit, reserve, persist, call, reconcile.
    fn run(
        &self,
        catalog: &ModelCatalog,
        ledger: &mut QuotaLedger,
        request: &DispatchRequest,
        now: i64,
        call: &mut dyn FnMut(&ResolvedModel, &ChatRequest) -> Result<ChatReply, LlmError>,
    ) -> DispatchOutcome {
        let resolved = match providers::resolve(catalog, &request.model_id) {
            Ok(resolved) => resolved,
            Err(error) => return DispatchOutcome::Unresolvable { error },
        };
        let requirement = Requirement {
            model_id: request.model_id.clone(),
            task: request.task.clone(),
            prompt_tokens: request.prompt_tokens,
            max_output_tokens: request.max_output_tokens,
            requires: request.requires.clone(),
            allow_remote: request.allow_remote,
        };
        let grant = match admission::admit(catalog, ledger, &requirement, now) {
            Admission::Admitted(grant) => grant,
            Admission::Refused(refusal) => return DispatchOutcome::Refused { refusal },
        };
        let reserved = grant.reservation_tokens;
        let mut receipt = Receipt::new(request, &resolved, reserved);
        if let Err(error) = ledger.reserve(&request.request_id, &grant.quota_ids, reserved, now) {
            return DispatchOutcome::NotReserved { error };
        }
        // Persist the intent before anything leaves the machine. If that fails, nothing is sent:
        // a request whose cost cannot be recorded is not worth sending.
        let persisted = ledger
            .mark_dispatched(&request.request_id)
            .map_err(|error| error.to_string())
            .and_then(|()| ledger.save().map_err(|error| error.to_string()));
        if let Err(error) = persisted {
            // Nothing was sent, and we know it: the intent itself could not be recorded.
            let _ = ledger.abandon(&request.request_id);
            return DispatchOutcome::NotPersisted { error };
        }
        let mut chat = ChatRequest::new(resolved.provider_model.clone(), request.messages.clone());
        if let Some(temperature) = request.temperature {
            chat = chat.temperature(temperature);
        }
        match call(&resolved, &chat) {
            Ok(reply) => {
                // Verified metadata only: both counts, or none.
                let reported = match (reply.prompt_tokens, reply.answer_tokens) {
                    (Some(prompt), Some(answer)) => Some(prompt.saturating_add(answer)),
                    _ => None,
                };
                settle_and_save(ledger, &request.request_id, reported, &mut receipt);
                DispatchOutcome::Answered {
                    content: reply.content,
                    reply_model: reply.model,
                    receipt,
                }
            }
            Err(error) => {
                // A rate limit is an answer, not usage: the provider generated nothing, so the
                // allowance goes back and only the delay is recorded.
                if let Some(notice) = error
                    .notice
                    .clone()
                    .filter(|notice| notice.is_rate_limited())
                {
                    // The provider refused the request, which is evidence that it generated
                    // nothing; charging for a rejection would burn the allowance on every 429.
                    let _ = ledger.abandon(&request.request_id);
                    let scope = admission::model_scope(&request.model_id);
                    ledger.record_rate_limit(&scope, notice.retry_after_seconds, now);
                    for quota_id in &grant.quota_ids {
                        // A 429 does not prove a token quota is spent, but blocking the shared pool
                        // for the announced delay is the conservative reading and costs nothing
                        // except patience.
                        ledger.record_rate_limit(quota_id, notice.retry_after_seconds, now);
                    }
                    if let Err(save) = ledger.save() {
                        receipt.save_error = Some(save.to_string());
                    }
                    let until = ledger.wait_until(&scope, now).unwrap_or(now);
                    receipt.charged = Some(0);
                    return DispatchOutcome::RateLimited {
                        scope,
                        until,
                        notice,
                        receipt,
                    };
                }
                // Anything else may have been processed before it failed; charge the reservation.
                settle_and_save(ledger, &request.request_id, None, &mut receipt);
                DispatchOutcome::Failed {
                    error: error.to_string(),
                    notice: error.notice,
                    receipt,
                }
            }
        }
    }
}

/// Reconcile the reservation, persisting the result, and fill in the receipt.
fn settle_and_save(
    ledger: &mut QuotaLedger,
    request_id: &str,
    reported: Option<u64>,
    receipt: &mut Receipt,
) {
    match ledger.settle(request_id, reported) {
        Ok(report) => {
            receipt.charged = Some(report.charged);
            receipt.usage_missing = report.usage_missing;
            receipt.reported_tokens = reported;
        }
        Err(error) => {
            // Reconciliation failed; the reservation is gone from the ledger's point of view, so
            // say so rather than pretending the cost is known.
            receipt.charged = None;
            receipt.save_error = Some(error.to_string());
            return;
        }
    }
    if let Err(error) = ledger.save() {
        receipt.save_error = Some(error.to_string());
    }
}

#[cfg(test)]
mod tests;

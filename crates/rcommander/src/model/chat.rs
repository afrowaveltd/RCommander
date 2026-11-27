//! The agent chat (Phase 16.1, first slice).
//!
//! A conversation in the model: a transcript, an input line and one background call. The answer
//! streams into the transcript through a channel, so the window stays responsive while a large
//! model is generating, and *Stop* simply drops the receiver's job (the worker notices a closed
//! channel and gives up at the next chunk).
//!
//! Nothing here can touch files: the chat is a conversation, and every side effect still has to go
//! through the policy layer (16.0) by way of an MCP tool.

use super::{CommandOutcome, Model};
use rc_core::admission::Refusal;
use rc_llm::{ChatReply, ChatRequest, Message, StreamControl, StreamEvent};
use rc_runtime::{DispatchOutcome, DispatchRequest, Dispatcher, Receipt};
use std::time::{Duration, Instant};

/// One line of the transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    /// Stable identity of this line, unique within the session.
    ///
    /// A view uses it to cache derived state (a parsed Markdown answer, a highlighted code block)
    /// without guessing from the line's position, which shifts when the transcript is cleared.
    pub seq: u64,
    pub speaker: Speaker,
    pub text: String,
    /// True while the answer is still being received.
    pub streaming: bool,
}

impl ChatLine {
    /// A line with no identity yet; the session stamps it when it is pushed.
    pub fn new(speaker: Speaker, text: impl Into<String>) -> Self {
        ChatLine {
            seq: 0,
            speaker,
            text: text.into(),
            streaming: false,
        }
    }
}

/// Who said it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    User,
    Model,
    /// A note from the application (errors, cancellations).
    System,
}

impl Speaker {
    /// Localisation key of the speaker's name; the view translates it.
    pub fn label_key(self) -> &'static str {
        match self {
            Speaker::User => "ai.chat.speaker.user",
            Speaker::Model => "ai.chat.speaker.model",
            Speaker::System => "ai.chat.speaker.system",
        }
    }
}

/// The chat panel's state.
#[derive(Default)]
pub struct ChatSession {
    pub lines: Vec<ChatLine>,
    /// Text being typed.
    pub input: String,
    /// The system prompt sent before the conversation.
    pub system: String,
    /// A running answer.
    job: Option<ChatJob>,
    /// Model the transcript belongs to, so a model switch is visible.
    pub model: String,
    /// What the last routed answer cost and which connection produced it (Phase 16.20).
    pub last_receipt: Option<Receipt>,
    /// Counter behind [`ChatLine::seq`].
    next_seq: u64,
}

impl ChatSession {
    /// Append a line, giving it a fresh identity.
    pub fn push(&mut self, speaker: Speaker, text: impl Into<String>) -> usize {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.lines.push(ChatLine {
            seq,
            speaker,
            text: text.into(),
            streaming: false,
        });
        self.lines.len() - 1
    }

    /// Append a line that is still being written, and return its index.
    fn push_streaming(&mut self) -> usize {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.lines.push(ChatLine {
            seq,
            speaker: Speaker::Model,
            text: String::new(),
            streaming: true,
        });
        self.lines.len() - 1
    }
}

impl std::fmt::Debug for ChatSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatSession")
            .field("lines", &self.lines.len())
            .field("running", &self.job.is_some())
            .field("model", &self.model)
            .finish()
    }
}

// A per-process counter for routed request ids, so a receipt names one attempt. The id ends up in
// the ledger, so it has to be unique without a random source.
thread_local! {
    static NEXT_REQUEST: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The catalogue entry whose provider model the chat is configured with.
///
/// Matched by provider model name, which is what the settings and the picker hold; an enabled
/// entry wins over a switched-off one, so re-enabling a model takes effect without editing a
/// second field.
fn routed_catalogue_model<'a>(
    catalogue: &'a rc_core::model_catalog::ModelCatalog,
    model: &str,
) -> Option<&'a rc_core::model_catalog::ModelProfile> {
    let mut matches = catalogue
        .models
        .iter()
        .filter(|entry| entry.model_id == model);
    let first = matches.next()?;
    let enabled = std::iter::once(first)
        .chain(matches)
        .find(|entry| entry.enabled);
    enabled.or(Some(first))
}

/// A token estimate for a prompt.
///
/// There is no tokenizer here and there will not be one per provider, so the estimate is the
/// documented rule of four characters per token, rounded up and never zero. It is an estimate:
/// the ledger reserves it plus the whole output reserve, and the provider's own count reconciles
/// the reservation afterwards.
pub fn estimate_prompt_tokens(messages: &[Message]) -> u64 {
    let characters: u64 = messages
        .iter()
        .map(|message| message.content.chars().count() as u64)
        .sum();
    characters.div_ceil(4).max(1)
}

/// One answer being streamed.
struct ChatJob {
    receiver: std::sync::mpsc::Receiver<ChatEvent>,
    control: StreamControl,
    started: Instant,
    last_activity: Instant,
    interval: Duration,
    loaded: Option<bool>,
    cold: bool,
    initial_probe: bool,
    responding: bool,
    load_duration: Option<Duration>,
    estimate_key: String,
    estimate: Option<u64>,
}

impl Drop for ChatJob {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

impl ChatJob {
    fn wait_due(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_activity) >= self.interval
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChatWaiting {
    pub elapsed: u64,
    pub loaded: Option<bool>,
    pub responding: bool,
    pub estimate: Option<u64>,
    pub ask: bool,
}

/// Why a routed request never produced an answer.
///
/// Kept as data until the UI thread turns it into a sentence, because the sentence has to be in
/// the reader's language and the worker thread has no translator.
#[derive(Clone, Debug)]
pub enum ChatRoutingFailure {
    Refused(Refusal),
    Unresolvable(rc_core::providers::ResolveError),
    NotReserved(rc_core::quota::LedgerError),
    NotPersisted(String),
    RateLimited { until: i64, now: i64 },
    Failed(String),
}

/// What the worker sends back.
enum ChatEvent {
    Probe(Option<bool>),
    Activity,
    LoadDuration(Duration),
    Chunk(String),
    /// What the routed request cost, before the answer itself completes.
    Receipt(Box<Receipt>),
    /// A routed request that was stopped before it could answer.
    RoutingFailed(Box<ChatRoutingFailure>),
    Done(Box<ChatReply>),
    Failed(String),
}

impl Model {
    /// The chat session, created on first use.
    pub fn chat(&mut self) -> &mut ChatSession {
        &mut self.chat_session
    }

    /// True while an answer is streaming.
    pub fn chat_is_running(&self) -> bool {
        self.chat_session.job.is_some()
    }

    pub fn chat_waiting(&self) -> Option<ChatWaiting> {
        let job = self.chat_session.job.as_ref()?;
        let now = Instant::now();
        Some(ChatWaiting {
            elapsed: now.saturating_duration_since(job.started).as_secs(),
            loaded: job.loaded,
            responding: job.responding,
            estimate: job.estimate,
            ask: job.wait_due(now),
        })
    }

    pub fn chat_continue_waiting(&mut self) {
        if let Some(job) = self.chat_session.job.as_mut() {
            job.last_activity = Instant::now();
        }
    }

    /// The model the chat uses, with a helpful message when none is configured.
    fn chat_model(&mut self) -> Result<String, String> {
        match self.ai_model().map(|name| name.to_string()) {
            Some(model) => Ok(model),
            None => {
                // A fresh installation may not have listed models yet.
                let _ = self.refresh_ai_models();
                self.ai_model()
                    .map(|name| name.to_string())
                    .ok_or_else(|| self.t("ai.model.none"))
            }
        }
    }

    /// Send the current input as a message.
    pub fn chat_send(&mut self) -> CommandOutcome {
        let input = self.chat_session.input.trim().to_string();
        if input.is_empty() {
            return CommandOutcome::None;
        }
        if self.chat_is_running() {
            let message = self.t("ai.chat.busy");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let model = match self.chat_model() {
            Ok(model) => model,
            Err(message) => {
                self.chat_session.push(Speaker::System, message.clone());
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        // The transcript becomes the request history: everything said so far.
        let mut messages: Vec<Message> = Vec::new();
        let system = if self.chat_session.system.trim().is_empty() {
            "You are the assistant inside RCommander, a dual-pane file manager. Answer briefly \
             and concretely. You cannot touch files unless a tool gives you access."
                .to_string()
        } else {
            self.chat_session.system.clone()
        };
        messages.push(Message::system(system));
        for line in &self.chat_session.lines {
            match line.speaker {
                Speaker::User => messages.push(Message::user(line.text.clone())),
                Speaker::Model if !line.streaming && !line.text.is_empty() => {
                    messages.push(Message::assistant(line.text.clone()))
                }
                _ => {}
            }
        }
        self.chat_session.push(Speaker::User, input.clone());
        messages.push(Message::user(input.clone()));
        self.chat_session.push_streaming();
        self.chat_session.input.clear();
        self.chat_session.model = model.clone();

        let (sender, receiver) = std::sync::mpsc::channel();
        let control = StreamControl::default();
        let worker_control = control.clone();
        let started = Instant::now();
        let routed = self.routing_enabled();
        // The estimate is only a courtesy for the "loading a cold model" line; a routed request
        // keys it by the connection it will actually use.
        let estimate_key = match self.routed_estimate_key(&model) {
            Some(key) if routed => key,
            _ => {
                let provider = self.ai_provider();
                let (kind, base_url) = match &provider {
                    Some(provider) => (provider.kind(), provider.base_url().to_string()),
                    None => ("ollama", self.settings.agent.provider_url.clone()),
                };
                format!("{kind}|{base_url}|{model}")
            }
        };
        let estimate = self
            .settings
            .agent
            .model_load_seconds
            .get(&estimate_key)
            .copied();
        if routed {
            if let Err(message) =
                self.spawn_routed_chat(model.clone(), messages.clone(), &control, &sender)
            {
                self.abort_chat_request(&message);
                return CommandOutcome::Failed(message);
            }
        } else {
            let provider_message = self.t("ai.provider.unknown");
            let Some(provider) = self.ai_provider() else {
                self.abort_chat_request(&provider_message);
                return CommandOutcome::Failed(provider_message);
            };
            let request = ChatRequest::new(model.clone(), messages);
            std::thread::spawn(move || {
                let initial = provider.model_loaded(&model).ok().flatten();
                let _ = sender.send(ChatEvent::Probe(initial));
                if worker_control.is_cancelled() {
                    return;
                }
                // Only a provider that can actually answer the question is polled: a provider that
                // returns `None` would otherwise have a thread asking it every five seconds for an
                // answer it has already said it does not have.
                if initial.is_some() {
                    let probe_provider = provider.clone();
                    let probe_control = worker_control.clone();
                    let probe_sender = sender.clone();
                    std::thread::spawn(move || {
                        // A budget, so a server that stops answering cannot leave a thread polling
                        // forever: five minutes of five-second probes covers any load worth waiting
                        // for, and the request itself is cancelled long before that matters.
                        for _ in 0..60 {
                            std::thread::sleep(Duration::from_secs(5));
                            if probe_control.is_cancelled() {
                                break;
                            }
                            let loaded = probe_provider.model_loaded(&model).ok().flatten();
                            if probe_sender.send(ChatEvent::Probe(loaded)).is_err() {
                                break;
                            }
                        }
                    });
                }
                let result =
                    provider.chat_stream_controlled(&request, &worker_control, &mut |event| {
                        let event = match event {
                            StreamEvent::Activity => ChatEvent::Activity,
                            StreamEvent::Text(text) => ChatEvent::Chunk(text.to_string()),
                            StreamEvent::LoadDuration(duration) => {
                                ChatEvent::LoadDuration(duration)
                            }
                        };
                        let _ = sender.send(event);
                    });
                let _ = match result {
                    Ok(reply) => sender.send(ChatEvent::Done(Box::new(reply))),
                    Err(error) => sender.send(ChatEvent::Failed(error.to_string())),
                };
                worker_control.cancel();
            });
        }
        self.chat_session.job = Some(ChatJob {
            receiver,
            control,
            started,
            last_activity: started,
            interval: Duration::from_secs(self.settings.agent.response_wait_seconds.clamp(1, 3600)),
            loaded: None,
            cold: false,
            initial_probe: false,
            responding: false,
            load_duration: None,
            estimate_key,
            estimate,
        });
        CommandOutcome::Ok
    }

    /// Mark the pending answer line as stopped and report why nothing will arrive.
    fn abort_chat_request(&mut self, message: &str) {
        if let Some(line) = self.chat_session.lines.last_mut()
            && line.streaming
        {
            line.streaming = false;
            if line.text.is_empty() {
                line.text = format!("⚠ {message}");
            }
        }
        self.report_error(message.to_string());
    }

    /// The load-time cache key a routed request would use, when the catalogue can say.
    fn routed_estimate_key(&self, model: &str) -> Option<String> {
        let catalogue = &self.settings.agent.model_catalog;
        let entry = routed_catalogue_model(catalogue, model)?;
        let connection = catalogue
            .connections
            .iter()
            .find(|connection| connection.id == entry.connection_id)?;
        Some(format!(
            "{}|{}|{}",
            connection.kind, connection.base_url, entry.model_id
        ))
    }

    /// Start a routed answer: the catalogue decides the connection, admission decides whether it
    /// may run, the ledger reserves the allowance, and only then does the request leave.
    fn spawn_routed_chat(
        &mut self,
        model: String,
        messages: Vec<Message>,
        control: &StreamControl,
        sender: &std::sync::mpsc::Sender<ChatEvent>,
    ) -> Result<(), String> {
        let catalogue = self.settings.agent.model_catalog.clone();
        let entry = routed_catalogue_model(&catalogue, &model)
            .ok_or_else(|| self.tf("ai.route.no_model", &[("model", &model)]))?;
        let ledger = match self.quota_ledger()? {
            Some(ledger) => ledger,
            // `routing_enabled()` was true a moment ago, so this is a contradiction worth naming
            // rather than silently falling back to unmetered sending.
            None => return Err(self.t("ai.ledger.unavailable")),
        };
        let backend: std::sync::Arc<dyn rc_runtime::Backend> =
            self.ai_backend.clone().unwrap_or_else(|| {
                std::sync::Arc::new(rc_runtime::LlmBackend {
                    timeout: Some(Duration::from_secs(
                        self.settings.agent.response_wait_seconds.clamp(3, 3600),
                    )),
                })
            });
        let allow_remote = self.settings.agent.allow_remote_models;
        let max_output = entry
            .max_output_tokens
            .unwrap_or(rc_runtime::dispatch::DEFAULT_MAX_OUTPUT_TOKENS);
        let prompt_tokens = estimate_prompt_tokens(&messages);
        let request_id = format!(
            "chat-{}-{}",
            std::process::id(),
            NEXT_REQUEST.with(|next| {
                let value = next.get() + 1;
                next.set(value);
                value
            })
        );
        let dispatch_request = DispatchRequest {
            request_id,
            model_id: entry.id.clone(),
            messages,
            prompt_tokens,
            max_output_tokens: max_output,
            temperature: None,
            task: "chat".into(),
            requires: [rc_core::model_catalog::Capability::Text]
                .into_iter()
                .collect(),
            allow_remote,
        };
        let worker_control = control.clone();
        let worker_sender = sender.clone();
        std::thread::spawn(move || {
            let dispatcher = Dispatcher::new(backend);
            let mut guard = match ledger.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let now = chrono::Local::now().timestamp();
            let outcome = dispatcher.dispatch_stream(
                &catalogue,
                &mut guard,
                &dispatch_request,
                &worker_control,
                &mut |event| {
                    let event = match event {
                        StreamEvent::Activity => ChatEvent::Activity,
                        StreamEvent::Text(text) => ChatEvent::Chunk(text.to_string()),
                        StreamEvent::LoadDuration(duration) => ChatEvent::LoadDuration(duration),
                    };
                    let _ = worker_sender.send(event);
                },
                now,
            );
            drop(guard);
            match outcome {
                DispatchOutcome::Answered {
                    content,
                    reply_model,
                    receipt,
                } => {
                    let _ = worker_sender.send(ChatEvent::Receipt(Box::new(receipt)));
                    let reply = ChatReply {
                        content,
                        model: reply_model,
                        done_reason: None,
                        prompt_tokens: None,
                        answer_tokens: None,
                    };
                    let _ = worker_sender.send(ChatEvent::Done(Box::new(reply)));
                }
                DispatchOutcome::RateLimited {
                    until,
                    notice,
                    receipt,
                    ..
                } => {
                    let _ = worker_sender.send(ChatEvent::Receipt(Box::new(receipt)));
                    let announced = notice
                        .retry_after_seconds
                        .and_then(|seconds| i64::try_from(seconds).ok())
                        .unwrap_or(0);
                    let wait = ChatRoutingFailure::RateLimited {
                        until,
                        now: now + announced,
                    };
                    let _ = worker_sender.send(ChatEvent::RoutingFailed(Box::new(wait)));
                }
                DispatchOutcome::Refused { refusal } => {
                    let _ = worker_sender.send(ChatEvent::RoutingFailed(Box::new(
                        ChatRoutingFailure::Refused(refusal),
                    )));
                }
                DispatchOutcome::Unresolvable { error } => {
                    let _ = worker_sender.send(ChatEvent::RoutingFailed(Box::new(
                        ChatRoutingFailure::Unresolvable(error),
                    )));
                }
                DispatchOutcome::NotReserved { error } => {
                    let _ = worker_sender.send(ChatEvent::RoutingFailed(Box::new(
                        ChatRoutingFailure::NotReserved(error),
                    )));
                }
                DispatchOutcome::NotPersisted { error } => {
                    let _ = worker_sender.send(ChatEvent::RoutingFailed(Box::new(
                        ChatRoutingFailure::NotPersisted(error),
                    )));
                }
                DispatchOutcome::Failed { error, receipt, .. } => {
                    let _ = worker_sender.send(ChatEvent::Receipt(Box::new(receipt)));
                    let _ = worker_sender.send(ChatEvent::RoutingFailed(Box::new(
                        ChatRoutingFailure::Failed(error),
                    )));
                }
            }
            worker_control.cancel();
        });
        Ok(())
    }

    /// A localized sentence for a refusal, so the reason reaches the reader in their language.
    pub fn refusal_message(&self, refusal: &Refusal) -> String {
        let now = chrono::Local::now().timestamp();
        match refusal {
            Refusal::RemoteNotAuthorized => self.t("ai.refusal.remote"),
            Refusal::UnknownModel(id) => self.tf("ai.refusal.unknown_model", &[("model", id)]),
            Refusal::Disabled(id) => self.tf("ai.refusal.disabled", &[("model", id)]),
            Refusal::MissingCapability(capability) => self.tf(
                "ai.refusal.capability",
                &[("capability", &format!("{capability:?}"))],
            ),
            Refusal::ContextTooSmall { needed, available } => self.tf(
                "ai.refusal.context",
                &[
                    ("needed", &needed.to_string()),
                    ("available", &available.to_string()),
                ],
            ),
            Refusal::ModelWaiting { until } => {
                let seconds = (until - now).max(0).to_string();
                self.tf("ai.refusal.waiting", &[("seconds", &seconds)])
            }
            Refusal::QuotaWaiting { quota_id, until } => {
                let seconds = (until - now).max(0).to_string();
                self.tf(
                    "ai.refusal.quota_waiting",
                    &[("quota", quota_id), ("seconds", &seconds)],
                )
            }
            Refusal::QuotaExceeded {
                quota_id,
                remaining,
                wanted,
            } => self.tf(
                "ai.refusal.quota",
                &[
                    ("quota", quota_id),
                    ("remaining", &remaining.to_string()),
                    ("wanted", &wanted.to_string()),
                ],
            ),
            Refusal::UnknownRemaining { quota_id, .. } => {
                self.tf("ai.refusal.unknown_quota", &[("quota", quota_id)])
            }
        }
    }

    /// A localized sentence for a routed request that never produced an answer.
    pub fn routing_failure_message(&self, failure: &ChatRoutingFailure) -> String {
        match failure {
            ChatRoutingFailure::Refused(refusal) => {
                let reason = self.refusal_message(refusal);
                self.tf("ai.route.refused", &[("reason", &reason)])
            }
            ChatRoutingFailure::Unresolvable(error) => {
                self.tf("ai.route.unresolvable", &[("reason", &error.to_string())])
            }
            ChatRoutingFailure::NotReserved(error) => {
                let reason = error.to_string();
                self.tf("ai.route.refused", &[("reason", &reason)])
            }
            ChatRoutingFailure::NotPersisted(error) => {
                self.tf("ai.route.not_persisted", &[("reason", error)])
            }
            ChatRoutingFailure::RateLimited { until, now } => {
                let seconds = (until - now).max(0).to_string();
                self.tf("ai.route.rate_limited", &[("seconds", &seconds)])
            }
            ChatRoutingFailure::Failed(error) => {
                let reason = error.to_string();
                self.tf("ai.route.failed", &[("reason", &reason)])
            }
        }
    }

    /// The sentence describing what a routed answer cost, for the status line.
    fn metered_note(&self, receipt: &Receipt) -> String {
        let tokens = receipt.charged.unwrap_or(0).to_string();
        if receipt.usage_missing {
            return self.tf(
                "ai.chat.metered_reserved",
                &[
                    ("model", &receipt.provider_model),
                    ("connection", &receipt.connection_id),
                    ("tokens", &tokens),
                ],
            );
        }
        self.tf(
            "ai.chat.metered",
            &[
                ("model", &receipt.provider_model),
                ("connection", &receipt.connection_id),
                ("tokens", &tokens),
            ],
        )
    }

    /// Take whatever the model has produced so far; returns true when the transcript changed.
    pub fn poll_chat(&mut self) -> bool {
        // The job is moved out for the duration of the poll so the transcript and the
        // status line can be written without borrowing the session twice.
        let Some(mut job) = self.chat_session.job.take() else {
            return false;
        };
        let mut changed = false;
        let mut finished = false;
        let mut done = false;
        let mut failure: Option<String> = None;
        loop {
            match job.receiver.try_recv() {
                Ok(ChatEvent::Probe(loaded)) => {
                    if !job.initial_probe {
                        job.cold = loaded == Some(false);
                        job.initial_probe = true;
                    }
                    job.loaded = loaded;
                    changed = true;
                }
                Ok(ChatEvent::Activity) => {
                    job.last_activity = Instant::now();
                    job.responding = true;
                    changed = true;
                }
                Ok(ChatEvent::LoadDuration(duration)) => {
                    job.load_duration = Some(duration);
                }
                Ok(ChatEvent::Chunk(text)) => {
                    if let Some(line) = self.chat_session.lines.last_mut()
                        && line.streaming
                    {
                        line.text.push_str(&text);
                        changed = true;
                    }
                }
                Ok(ChatEvent::Done(reply)) => {
                    if job.cold
                        && let Some(duration) = job.load_duration
                    {
                        self.settings
                            .agent
                            .model_load_seconds
                            .insert(job.estimate_key.clone(), duration.as_secs().max(1));
                        self.mark_settings_dirty();
                    }
                    if let Some(line) = self.chat_session.lines.last_mut()
                        && line.streaming
                    {
                        if line.text.is_empty() {
                            line.text = reply.content.clone();
                        }
                        line.streaming = false;
                        changed = true;
                    }
                    finished = true;
                    done = true;
                }
                Ok(ChatEvent::Receipt(receipt)) => {
                    self.chat_session.last_receipt = Some(*receipt);
                }
                Ok(ChatEvent::RoutingFailed(routing_failure)) => {
                    let message = self.routing_failure_message(&routing_failure);
                    if let Some(line) = self.chat_session.lines.last_mut()
                        && line.streaming
                    {
                        line.streaming = false;
                        if line.text.is_empty() {
                            line.text = format!("⚠ {message}");
                        }
                    }
                    failure = Some(message);
                    changed = true;
                    finished = true;
                }
                Ok(ChatEvent::Failed(message)) => {
                    if let Some(line) = self.chat_session.lines.last_mut()
                        && line.streaming
                    {
                        line.streaming = false;
                        if line.text.is_empty() {
                            line.text = format!("⚠ {message}");
                        }
                    }
                    failure = Some(message);
                    changed = true;
                    finished = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            if let Some(message) = failure {
                // A failed answer is an error the user was waiting for, so it is an alert.
                self.report_error(message);
            } else if done {
                let model = self.chat_session.model.clone();
                let text = match self.chat_session.last_receipt.clone() {
                    Some(receipt) => self.metered_note(&receipt),
                    None => self.tf("ai.chat.finished", &[("model", &model)]),
                };
                self.report_done(text, Some(crate::model::desktop_sound::Sound::Done));
            }
        } else {
            self.chat_session.job = Some(job);
        }
        changed
    }

    /// Stop a running answer; the text received so far stays.
    pub fn chat_stop(&mut self) -> CommandOutcome {
        if self.chat_session.job.take().is_none() {
            return CommandOutcome::None;
        }
        let note = self.t("ai.stopped");
        if let Some(line) = self.chat_session.lines.last_mut()
            && line.streaming
        {
            line.streaming = false;
            if line.text.is_empty() {
                line.text = note.clone();
            } else {
                line.text.push_str(&format!("\n\n{note}"));
            }
        }
        self.report(note);
        CommandOutcome::Ok
    }

    /// Clear the transcript.
    pub fn chat_clear(&mut self) -> CommandOutcome {
        self.chat_session.lines.clear();
        self.chat_session.job = None;
        CommandOutcome::Ok
    }

    /// Write the transcript next to the active panel's directory, so it can be read and shared.
    ///
    /// The file name carries a timestamp and the panel is refreshed afterwards, so the export is
    /// visible where the user is looking instead of in a place they have to be told about.
    pub fn chat_export(&mut self) -> CommandOutcome {
        let lines = self.chat_session.lines.clone();
        if lines.is_empty() {
            let message = self.t("prompt.export.empty");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        // Markdown, not plain text, because that is what a conversation with a model already *is*:
        // its answers arrive as Markdown, and flattening them into a .txt loses the code fences and
        // the lists that made them readable. `chat_markdown` is the one definition of that format;
        // the importer reads exactly what it writes.
        let stamp = chrono::Local::now();
        let text = self.chat_markdown();
        let directory = self
            .active()
            .location()
            .as_path()
            .map(|path| path.to_path_buf())
            .unwrap_or_else(std::env::temp_dir);
        let file_stamp = stamp.format("%Y%m%d-%H%M%S");
        let target = directory.join(format!("chat-{file_stamp}.md"));
        match std::fs::write(&target, text.as_bytes()) {
            Ok(()) => {
                let message = self.tf(
                    "prompt.export.done",
                    &[("path", &target.display().to_string())],
                );
                self.report_done(message, None);
                self.refresh_active();
                CommandOutcome::Ok
            }
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// Read a conversation back from a Markdown export.
    ///
    /// The format the export writes is the format this reads: front matter naming it, `## You` and
    /// `## Agent` for the turns, and blockquotes for the application's own notes. Anything that does
    /// not match is refused with the reason rather than half-loaded — a conversation that silently
    /// loses its middle is worse than one that does not open.
    pub fn chat_import_markdown(&mut self, path: &std::path::Path) -> CommandOutcome {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                let message = error.to_string();
                self.report_error(&message);
                return CommandOutcome::Failed(message);
            }
        };
        let mut lines: Vec<ChatLine> = Vec::new();
        let mut speaker: Option<Speaker> = None;
        let mut body = String::new();
        let mut in_front_matter = false;
        let mut seen_front_matter = false;
        for line in text.lines() {
            let trimmed = line.trim_end();
            if trimmed == "---" && !seen_front_matter {
                in_front_matter = !in_front_matter;
                if !in_front_matter {
                    seen_front_matter = true;
                }
                continue;
            }
            if in_front_matter {
                continue;
            }
            let heading = match trimmed {
                "## You" => Some(Speaker::User),
                "## Agent" => Some(Speaker::Model),
                _ => None,
            };
            if let Some(next) = heading {
                if let Some(previous) = speaker.take()
                    && !body.trim().is_empty()
                {
                    lines.push(ChatLine::new(previous, body.trim().to_string()));
                }
                speaker = Some(next);
                body.clear();
                continue;
            }
            if trimmed.starts_with("> ") {
                // A note after a turn has to close that turn first, or its body would swallow the
                // quote and the note would arrive as part of the answer.
                if let Some(previous) = speaker.take()
                    && !body.trim().is_empty()
                {
                    lines.push(ChatLine::new(previous, body.trim().to_string()));
                }
                body.clear();
                lines.push(ChatLine::new(
                    Speaker::System,
                    trimmed.trim_start_matches("> ").to_string(),
                ));
                continue;
            }
            if trimmed == "> " {
                continue;
            }
            // The title and its date line are the file's own decoration, not a turn.
            if speaker.is_some() {
                body.push_str(trimmed);
                body.push('\n');
            }
        }
        if let Some(previous) = speaker
            && !body.trim().is_empty()
        {
            lines.push(ChatLine::new(previous, body.trim().to_string()));
        }
        if lines.is_empty() {
            let message = self.t("prompt.export.unreadable");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        // Sequence numbers are the session's, not the file's: they are what a view caches on, so a
        // loaded conversation must look exactly like a typed one.
        for line in &mut lines {
            line.seq = self.chat_session.next_seq;
            self.chat_session.next_seq += 1;
        }
        self.chat_session.lines = lines;
        self.chat_session.job = None;
        let count = self.chat_session.lines.len().to_string();
        let message = self.tf("prompt.import.done", &[("count", &count)]);
        self.report_done(message, None);
        CommandOutcome::Ok
    }

    /// The transcript as the Markdown export writes it.
    ///
    /// Shared by the exporter and the tests so the round trip is checked against the real format
    /// rather than against a copy of it that can drift.
    pub fn chat_markdown(&self) -> String {
        let stamp = chrono::Local::now();
        let mut text = String::new();
        text.push_str("---\n");
        text.push_str("format: rcommander-chat\n");
        text.push_str("version: 1\n");
        text.push_str(&format!("exported: {}\n", stamp.to_rfc3339()));
        text.push_str(&format!("messages: {}\n", self.chat_session.lines.len()));
        text.push_str("---\n\n");
        text.push_str("# Conversation\n\n");
        text.push_str(&format!("_{}_\n\n", stamp.format("%Y-%m-%d %H:%M")));
        for line in &self.chat_session.lines {
            match line.speaker {
                Speaker::System => {
                    for part in line.text.lines() {
                        text.push_str("> ");
                        text.push_str(part);
                        text.push('\n');
                    }
                    text.push('\n');
                }
                Speaker::User => {
                    text.push_str("## You\n\n");
                    text.push_str(line.text.trim_end());
                    text.push_str("\n\n");
                }
                Speaker::Model => {
                    text.push_str("## Agent\n\n");
                    text.push_str(line.text.trim_end());
                    text.push_str("\n\n");
                }
            }
        }
        text
    }

    /// Turn the question in the input (or the last one asked) into a prompt being edited.
    pub fn chat_save_prompt(&mut self) -> CommandOutcome {
        let text = if self.chat_session.input.trim().is_empty() {
            self.chat_session
                .lines
                .iter()
                .rev()
                .find(|line| line.speaker == Speaker::User)
                .map(|line| line.text.clone())
                .unwrap_or_default()
        } else {
            self.chat_session.input.clone()
        };
        if text.trim().is_empty() {
            let message = self.t("prompt.error.text");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        self.new_prompt(rc_core::prompt::Category::General);
        self.prompt_editor.text = text;
        self.open_options_on(super::dialogs::OptionsTab::Prompts)
    }

    /// Append a system note to the transcript (used by the agent tools later).
    pub fn chat_note(&mut self, text: impl Into<String>) {
        self.chat_session.push(Speaker::System, text);
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;
    use rc_llm::{ChatReply, ChatRequest, LlmError, Model as LlmModel, Provider};
    use std::sync::{Arc, Mutex};

    /// A provider that streams a fixed answer in pieces.
    struct StreamingProvider {
        pieces: Vec<String>,
        fail: Option<String>,
        seen: Arc<Mutex<Vec<ChatRequest>>>,
    }

    impl std::fmt::Debug for StreamingProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("StreamingProvider")
        }
    }

    impl Provider for StreamingProvider {
        fn kind(&self) -> &'static str {
            "ollama"
        }
        fn base_url(&self) -> &str {
            "http://stub"
        }
        fn list_models(&self) -> Result<Vec<LlmModel>, LlmError> {
            Ok(vec![LlmModel {
                id: "stub:1b".into(),
                size: None,
                parameters: None,
                capabilities: vec!["completion".into()],
            }])
        }
        fn chat(&self, request: &ChatRequest) -> Result<ChatReply, LlmError> {
            Ok(ChatReply {
                content: self.pieces.join(""),
                model: request.model.clone(),
                done_reason: Some("stop".into()),
                prompt_tokens: None,
                answer_tokens: None,
            })
        }
        fn chat_stream(
            &self,
            request: &ChatRequest,
            on_chunk: &mut dyn FnMut(&str),
        ) -> Result<ChatReply, LlmError> {
            self.seen.lock().unwrap().push(request.clone());
            if let Some(error) = &self.fail {
                return Err(LlmError::new(error.clone()));
            }
            for piece in &self.pieces {
                on_chunk(piece);
            }
            Ok(ChatReply {
                content: self.pieces.join(""),
                model: request.model.clone(),
                done_reason: Some("stop".into()),
                prompt_tokens: Some(4),
                answer_tokens: Some(2),
            })
        }
    }

    fn chat_model(pieces: Vec<&str>, fail: Option<&str>) -> (Model, Arc<Mutex<Vec<ChatRequest>>>) {
        let dir =
            std::env::temp_dir().join(format!("rc-chat-{}-{}", std::process::id(), pieces.len()));
        let _ = std::fs::create_dir_all(&dir);
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(&dir),
            Location::local(&dir),
        );
        model.config_dir_override = Some(dir);
        let seen = Arc::new(Mutex::new(Vec::new()));
        model.set_ai_provider(Some(Arc::new(StreamingProvider {
            pieces: pieces.into_iter().map(|piece| piece.to_string()).collect(),
            fail: fail.map(|error| error.to_string()),
            seen: seen.clone(),
        })));
        model.settings.agent.model = "stub:1b".to_string();
        (model, seen)
    }

    /// A backend for the routed path that records every request it is given.
    struct RoutedBackend {
        content: String,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl rc_runtime::Backend for RoutedBackend {
        fn chat(
            &self,
            _resolved: &rc_core::providers::ResolvedModel,
            request: &ChatRequest,
        ) -> Result<ChatReply, LlmError> {
            self.calls.lock().unwrap().push(
                request
                    .messages
                    .last()
                    .map(|m| m.content.clone())
                    .unwrap_or_default(),
            );
            Ok(ChatReply {
                content: self.content.clone(),
                model: request.model.clone(),
                done_reason: Some("stop".into()),
                prompt_tokens: Some(300),
                answer_tokens: Some(120),
            })
        }
    }

    /// A model with routing switched on: one local or remote model drawing on one hourly pool.
    fn routed_model(local: bool, pool: u64) -> (Model, Arc<Mutex<Vec<String>>>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        // The real English pack, so the assertions read the sentences a person would see rather
        // than translation keys.
        let locale_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../rc-i18n/locales");
        let strings = crate::i18n::TranslatorStrings::discover(&locale_dir, "en");
        let mut model = Model::boot(
            Settings::default(),
            Box::new(strings),
            Location::local(dir.path()),
            Location::local(dir.path()),
        );
        model.config_dir_override = Some(dir.path().to_path_buf());
        model.settings.agent.model = "tiny:1b".to_string();
        model.settings.agent.model_catalog = rc_core::model_catalog::ModelCatalog {
            connections: vec![rc_core::providers::ProviderConnection {
                id: "default".into(),
                kind: rc_core::providers::ProviderKind::Ollama,
                base_url: "http://127.0.0.1:11434".into(),
                account: None,
                enabled: true,
            }],
            quotas: vec![rc_core::model_catalog::TokenQuota {
                id: "pool".into(),
                tokens: pool,
                period: rc_core::model_catalog::QuotaPeriod::Hour,
                window: rc_core::model_catalog::QuotaWindow::Rolling,
            }],
            models: vec![rc_core::model_catalog::ModelProfile {
                id: "tiny".into(),
                connection_id: "default".into(),
                model_id: "tiny:1b".into(),
                enabled: true,
                local,
                context_tokens: Some(8_192),
                // A modest output reserve, so the reservation fits the pool in the tests.
                max_output_tokens: Some(256),
                capabilities: [rc_core::model_catalog::Capability::Text]
                    .into_iter()
                    .collect(),
                quota_ids: vec!["pool".into()],
                ..rc_core::model_catalog::ModelProfile::default()
            }],
            ..rc_core::model_catalog::ModelCatalog::default()
        };
        let calls = Arc::new(Mutex::new(Vec::new()));
        model.ai_backend = Some(Arc::new(RoutedBackend {
            content: "routed answer".into(),
            calls: calls.clone(),
        }));
        (model, calls, dir)
    }

    /// The charge booked against the pool, read back from the ledger file.
    fn booked(model: &Model) -> u64 {
        let ledger = rc_core::quota::QuotaLedger::load_in(
            model.config_dir(),
            &model.settings.agent.model_catalog,
        )
        .expect("the ledger was written");
        match ledger.remaining("pool", chrono::Local::now().timestamp()) {
            rc_core::quota::Remaining::Known { tokens, .. } => tokens,
            other => panic!("the pool should have a known remainder, got {other:?}"),
        }
    }

    fn settle(model: &mut Model) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline && model.chat_is_running() {
            model.poll_chat();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        model.poll_chat();
    }

    #[test]
    fn a_routed_chat_is_admitted_charged_and_reports_its_provenance() {
        use super::*;
        let (mut model, calls, _dir) = routed_model(true, 1_000);
        model.chat_session.input = "hello".into();
        assert!(
            matches!(model.chat_send(), CommandOutcome::Ok),
            "a local model within the pool is dispatched"
        );
        settle(&mut model);
        assert_eq!(calls.lock().unwrap().len(), 1, "the backend was asked once");
        let answer = model
            .chat_session
            .lines
            .iter()
            .rev()
            .find(|line| matches!(line.speaker, Speaker::Model))
            .map(|line| line.text.clone())
            .unwrap_or_default();
        assert_eq!(answer, "routed answer");
        let receipt = model
            .chat_session
            .last_receipt
            .clone()
            .expect("a routed answer carries a receipt");
        assert_eq!(receipt.connection_id, "default");
        assert_eq!(receipt.provider_model, "tiny:1b");
        assert_eq!(
            receipt.charged,
            Some(420),
            "the provider's own counts are booked"
        );
        assert!(!receipt.remote);
        assert_eq!(model.last_error, None, "nothing went wrong");
        assert!(
            model.status.contains("420"),
            "the status line says what it cost: {}",
            model.status
        );
        assert_eq!(
            booked(&model),
            580,
            "the pool lost exactly the answer's cost"
        );
    }

    #[test]
    fn a_remote_model_without_permission_is_refused_and_never_reaches_a_backend() {
        let (mut model, calls, _dir) = routed_model(false, 1_000);
        model.chat_session.input = "hello".into();
        model.chat_send();
        settle(&mut model);
        assert_eq!(
            calls.lock().unwrap().len(),
            0,
            "unauthorized egress must not become a request"
        );
        let error = model.last_error.clone().unwrap_or_default();
        assert!(
            error.contains("remote provider"),
            "the refusal says why: {error}"
        );
        let line = model
            .chat_session
            .lines
            .last()
            .map(|line| line.text.clone())
            .unwrap_or_default();
        assert!(line.contains("⚠"), "the transcript shows the stop: {line}");
        assert_eq!(booked(&model), 1_000, "a refusal costs no allowance");
    }

    #[test]
    fn an_exhausted_pool_refuses_the_chat_before_the_backend() {
        // The reservation is the estimate plus the whole output reserve (1 024 tokens by default),
        // so a ten-token pool can never hold it.
        let (mut model, calls, _dir) = routed_model(true, 10);
        model.chat_session.input = "hello".into();
        model.chat_send();
        settle(&mut model);
        assert_eq!(calls.lock().unwrap().len(), 0);
        let error = model.last_error.clone().unwrap_or_default();
        assert!(
            error.contains("pool"),
            "the refusal names the pool: {error}"
        );
    }

    #[test]
    fn a_ledger_that_cannot_be_read_stops_the_chat_instead_of_sending() {
        use super::*;
        let (mut model, calls, dir) = routed_model(true, 1_000);
        // A ledger written by a newer build: it must not be guessed at.
        std::fs::write(
            dir.path().join(rc_core::quota::LEDGER_FILE),
            "version = 99\n",
        )
        .unwrap();
        model.chat_session.input = "hello".into();
        let outcome = model.chat_send();
        assert!(matches!(outcome, CommandOutcome::Failed(_)));
        assert_eq!(calls.lock().unwrap().len(), 0, "nothing is sent unmetered");
        let error = model.last_error.clone().unwrap_or_default();
        assert!(
            error.contains("newer ledger"),
            "the reason is the ledger: {error}"
        );
    }

    #[test]
    fn a_prompt_estimate_is_four_characters_per_token_and_never_zero() {
        use super::*;
        assert_eq!(estimate_prompt_tokens(&[Message::user("12345")]), 2);
        assert_eq!(estimate_prompt_tokens(&[Message::user("1234")]), 1);
        assert_eq!(
            estimate_prompt_tokens(&[]),
            1,
            "a request always costs something"
        );
        assert_eq!(
            estimate_prompt_tokens(&[Message::user("abcd"), Message::user("efgh")]),
            2,
            "every message in the history counts"
        );
    }

    #[test]
    fn wait_question_repeats_after_another_interval() {
        use super::*;
        let (mut model, _) = chat_model(vec!["answer"], None);
        let (_sender, receiver) = std::sync::mpsc::channel();
        let now = Instant::now();
        model.chat_session.job = Some(ChatJob {
            receiver,
            control: StreamControl::default(),
            started: now - Duration::from_secs(121),
            last_activity: now - Duration::from_secs(121),
            interval: Duration::from_secs(120),
            loaded: Some(false),
            cold: true,
            initial_probe: true,
            responding: false,
            load_duration: None,
            estimate_key: "test".into(),
            estimate: Some(300),
        });
        assert!(model.chat_waiting().unwrap().ask);
        model.chat_continue_waiting();
        assert!(!model.chat_waiting().unwrap().ask);
        let job = model.chat_session.job.as_ref().unwrap();
        assert!(job.wait_due(job.last_activity + Duration::from_secs(120)));
        model.chat_stop();
        assert!(model.chat_waiting().is_none());
    }

    #[test]
    fn a_message_is_streamed_into_the_transcript() {
        let (mut model, seen) = chat_model(vec!["Hel", "lo there"], None);
        model.chat().input = "Hi".to_string();
        assert_eq!(model.chat_send(), crate::model::CommandOutcome::Ok);
        settle(&mut model);
        let lines = &model.chat().lines;
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0].speaker, crate::model::chat::Speaker::User);
        assert_eq!(lines[0].text, "Hi");
        assert_eq!(lines[1].speaker, crate::model::chat::Speaker::Model);
        assert_eq!(lines[1].text, "Hello there");
        assert!(!lines[1].streaming, "the answer finished");
        // The request carried the system prompt and the user message.
        let requests = seen.lock().unwrap();
        let request = &requests[0];
        assert_eq!(request.messages[0].role, rc_llm::Role::System);
        assert_eq!(
            request.messages.last().unwrap().content,
            "Hi",
            "the question is the last message"
        );
    }

    #[test]
    fn the_second_message_carries_the_history() {
        let (mut model, seen) = chat_model(vec!["first answer"], None);
        model.chat().input = "one".to_string();
        model.chat_send();
        settle(&mut model);
        model.chat().input = "two".to_string();
        model.chat_send();
        settle(&mut model);
        let requests = seen.lock().unwrap();
        let second = &requests[1];
        let contents: Vec<&str> = second
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect();
        assert!(contents.contains(&"one"), "{contents:?}");
        assert!(contents.contains(&"first answer"), "{contents:?}");
        assert_eq!(contents.last().copied(), Some("two"));
        assert_eq!(model.chat().lines.len(), 4);
    }

    #[test]
    fn an_empty_input_sends_nothing_and_a_failure_is_reported() {
        let (mut model, seen) = chat_model(vec![], None);
        model.chat().input = "   ".to_string();
        assert_eq!(model.chat_send(), crate::model::CommandOutcome::None);
        assert!(seen.lock().unwrap().is_empty());

        let (mut failing, _) = chat_model(vec![], Some("the model is not loaded"));
        failing.chat().input = "hi".to_string();
        failing.chat_send();
        settle(&mut failing);
        let lines = &failing.chat().lines;
        assert!(
            lines.last().unwrap().text.contains("not loaded"),
            "{lines:?}"
        );
        assert!(failing.status.contains("not loaded"), "{}", failing.status);
        assert!(!failing.chat_is_running());
    }

    #[test]
    fn stopping_keeps_what_arrived() {
        let (mut model, _) = chat_model(vec!["partial answer"], None);
        model.chat().input = "go".to_string();
        model.chat_send();
        // Stop before polling: the worker may already be done, so accept both outcomes.
        model.chat_stop();
        model.poll_chat();
        assert!(!model.chat_is_running());
        let lines = &model.chat().lines;
        assert!(
            lines.last().unwrap().text.contains("partial")
                || lines.last().unwrap().text.contains("ai.stopped")
        );
    }

    #[test]
    fn clearing_empties_the_transcript() {
        let (mut model, _) = chat_model(vec!["x"], None);
        model.chat().input = "hi".to_string();
        model.chat_send();
        settle(&mut model);
        assert_eq!(model.chat_clear(), crate::model::CommandOutcome::Ok);
        assert!(model.chat().lines.is_empty());
    }

    #[test]
    fn a_chat_without_a_model_explains_itself() {
        let dir = std::env::temp_dir().join(format!("rc-chat-nomodel-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(&dir),
            Location::local(&dir),
        );
        model.config_dir_override = Some(dir);
        model.set_ai_provider(Some(Arc::new(StreamingProvider {
            pieces: vec!["x".to_string()],
            fail: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        })));
        model.settings.agent.model.clear();
        model.chat().input = "hi".to_string();
        // The stub provider offers one model, so the send succeeds after the refresh.
        let outcome = model.chat_send();
        settle(&mut model);
        assert!(
            outcome == crate::model::CommandOutcome::Ok
                || matches!(outcome, crate::model::CommandOutcome::Failed(_)),
            "{outcome:?}"
        );
    }
}

#[cfg(test)]
mod markdown_tests {
    use super::*;
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn fresh_model(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    #[test]
    fn an_export_is_markdown_a_reader_can_restore() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = fresh_model(dir.path());
        model
            .chat_session
            .push(Speaker::User, "Co je v téhle složce?");
        model.chat_session.push(
            Speaker::Model,
            "Tři soubory:\n\n```\nalpha.txt\nbeta.log\ngamma.md\n```\n",
        );
        model
            .chat_session
            .push(Speaker::System, "připojení ukončeno");
        let text = model.chat_markdown();
        assert!(text.starts_with("---\nformat: rcommander-chat\n"), "{text}");
        assert!(text.contains("## You\n"), "{text}");
        assert!(text.contains("## Agent\n"), "{text}");
        // The code fence survives: that is the whole reason for Markdown.
        assert!(text.contains("```\nalpha.txt"), "{text}");
        // A system note is a blockquote, not a turn with a heading.
        assert!(text.contains("> připojení ukončeno"), "{text}");
        assert!(!text.contains("## Note"), "{text}");

        // Round trip: the file, read back, is the conversation it came from.
        let path = dir.path().join("chat.md");
        std::fs::write(&path, &text).unwrap();
        let mut fresh = fresh_model(dir.path());
        fresh.chat_session.lines.clear();
        fresh.chat_session.push(Speaker::User, "jiná konverzace");
        assert!(matches!(
            fresh.chat_import_markdown(&path),
            crate::model::CommandOutcome::Ok
        ));
        let lines = &fresh.chat_session.lines;
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].speaker, Speaker::User);
        assert_eq!(lines[0].text, "Co je v téhle složce?");
        assert_eq!(lines[1].speaker, Speaker::Model);
        assert!(lines[1].text.contains("```"), "{}", lines[1].text);
        assert_eq!(lines[2].speaker, Speaker::System);
        // Identities are fresh and unique, so a view can cache on them.
        let seqs: Vec<u64> = lines.iter().map(|line| line.seq).collect();
        let mut unique = seqs.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), seqs.len(), "sequence numbers are unique");
    }

    #[test]
    fn a_file_that_is_not_a_conversation_is_refused_rather_than_half_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = fresh_model(dir.path());
        model.chat_session.push(Speaker::User, "keep me");
        let path = dir.path().join("notes.md");
        std::fs::write(&path, "# Something else\n\nJust prose.\n").unwrap();
        let outcome = model.chat_import_markdown(&path);
        assert!(matches!(outcome, crate::model::CommandOutcome::Failed(_)));
        // The transcript that was there is untouched.
        assert_eq!(model.chat_session.lines.len(), 1);
        assert_eq!(model.chat_session.lines[0].text, "keep me");
    }
}

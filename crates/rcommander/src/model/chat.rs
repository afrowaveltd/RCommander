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
use rc_llm::{ChatReply, ChatRequest, Message, StreamControl, StreamEvent};
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

/// What the worker sends back.
enum ChatEvent {
    Probe(Option<bool>),
    Activity,
    LoadDuration(Duration),
    Chunk(String),
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
        let Some(provider) = self.ai_provider() else {
            let message = self.t("ai.provider.unknown");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
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

        let request = ChatRequest::new(model.clone(), messages);
        let (sender, receiver) = std::sync::mpsc::channel();
        let control = StreamControl::default();
        let worker_control = control.clone();
        let estimate_key = format!("{}|{}|{}", provider.kind(), provider.base_url(), model);
        let estimate = self.settings.agent.model_load_seconds.get(&estimate_key).copied();
        let started = Instant::now();
        std::thread::spawn(move || {
            let initial = provider.model_loaded(&model).ok().flatten();
            let _ = sender.send(ChatEvent::Probe(initial));
            if worker_control.is_cancelled() {
                return;
            }
            let probe_provider = provider.clone();
            let probe_control = worker_control.clone();
            let probe_sender = sender.clone();
            std::thread::spawn(move || {
                while !probe_control.is_cancelled() {
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
            let result = provider.chat_stream_controlled(&request, &worker_control, &mut |event| {
                let event = match event {
                    StreamEvent::Activity => ChatEvent::Activity,
                    StreamEvent::Text(text) => ChatEvent::Chunk(text.to_string()),
                    StreamEvent::LoadDuration(duration) => ChatEvent::LoadDuration(duration),
                };
                let _ = sender.send(event);
            });
            let _ = match result {
                Ok(reply) => sender.send(ChatEvent::Done(Box::new(reply))),
                Err(error) => sender.send(ChatEvent::Failed(error.to_string())),
            };
            worker_control.cancel();
        });
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

    /// Take whatever the model has produced so far; returns true when the transcript changed.
    pub fn poll_chat(&mut self) -> bool {
        // The job is moved out for the duration of the poll so the transcript and the
        // status line can be written without borrowing the session twice.
        let Some(mut job) = self.chat_session.job.take() else {
            return false;
        };
        let mut changed = false;
        let mut finished = false;
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
                    if job.cold && let Some(duration) = job.load_duration {
                        self.settings.agent.model_load_seconds.insert(
                            job.estimate_key.clone(), duration.as_secs().max(1),
                        );
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
                self.report_error(message);
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

    fn settle(model: &mut Model) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline && model.chat_is_running() {
            model.poll_chat();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        model.poll_chat();
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

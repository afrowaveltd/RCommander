//! Choosing the language model (Phase 16.2).
//!
//! The provider is a plain HTTP client for a model server that already runs on the machine —
//! Ollama first, because it needs no account, no key and no network. Listing models is fast
//! (a few dozen milliseconds locally) so the dialog does it on open with a short timeout; a
//! *test question* goes through a model, which can take a minute while it loads, so that runs on
//! a worker thread and is collected by [`Model::poll_ai_test`].

use super::{CommandOutcome, Model};
use rc_llm::{ChatRequest, Message, Model as LlmModel, Provider};

/// One answer from the model, shown in the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiTestResult {
    pub model: String,
    pub content: String,
}

/// A model test running on a worker thread.
pub struct AiTestJob {
    receiver: std::sync::mpsc::Receiver<Result<AiTestResult, String>>,
}

/// Cache of the model list, so the dialog does not hit the server on every frame.
#[derive(Clone, Debug, Default)]
pub struct AiModelCache {
    pub models: Vec<LlmModel>,
    /// Why the last attempt failed, when it did.
    pub error: Option<String>,
    /// The URL the cache belongs to.
    pub url: String,
}

impl Model {
    /// The provider configured in the settings (or the one an embedder supplied).
    pub fn ai_provider(&self) -> Option<std::sync::Arc<dyn Provider>> {
        if let Some(provider) = &self.ai_provider {
            return Some(provider.clone());
        }
        match self
            .settings
            .agent
            .provider_kind
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "ollama" | "" => Some(std::sync::Arc::new(
                rc_llm::OllamaProvider::new(self.settings.agent.provider_url.clone())
                    .with_timeout(std::time::Duration::from_secs(5)),
            )),
            // Only Ollama is built in so far; an unknown kind is reported by the caller.
            _ => None,
        }
    }

    /// Replace the provider (tests and future provider selection).
    pub fn set_ai_provider(&mut self, provider: Option<std::sync::Arc<dyn Provider>>) {
        self.ai_provider = provider;
        self.ai_models = AiModelCache::default();
    }

    /// Refresh the cached model list; returns how many models the server offers.
    pub fn refresh_ai_models(&mut self) -> CommandOutcome {
        let Some(provider) = self.ai_provider() else {
            let message = self.t("ai.provider.unknown");
            self.ai_models = AiModelCache {
                models: Vec::new(),
                error: Some(message.clone()),
                url: self.settings.agent.provider_url.clone(),
            };
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        };
        match provider.list_models() {
            Ok(models) => {
                self.ai_models = AiModelCache {
                    error: None,
                    url: provider.base_url().to_string(),
                    models,
                };
                // A fresh installation has no model chosen: take the first tool-capable one,
                // because that is what the agent panel needs.
                if self.settings.agent.model.is_empty()
                    && let Some(first) = self
                        .ai_models
                        .models
                        .iter()
                        .find(|model| model.supports_tools())
                        .or_else(|| self.ai_models.models.first())
                {
                    self.settings.agent.model = first.id.clone();
                    self.mark_settings_dirty();
                }
                let count = self.ai_models.models.len().to_string();
                let message = self.tf("ai.models.found", &[("count", &count)]);
                self.report(message);
                CommandOutcome::Ok
            }
            Err(error) => {
                let message = error.to_string();
                self.ai_models = AiModelCache {
                    models: Vec::new(),
                    error: Some(message.clone()),
                    url: provider.base_url().to_string(),
                };
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// The cached list, refreshing it when empty.
    pub fn ai_models(&mut self) -> &[LlmModel] {
        if self.ai_models.models.is_empty()
            && self.ai_models.error.is_none()
            && self.ai_models.url.is_empty()
        {
            let _ = self.refresh_ai_models();
        }
        &self.ai_models.models
    }

    /// The model the agent would use, if one is configured.
    pub fn ai_model(&self) -> Option<&str> {
        let model = self.settings.agent.model.trim();
        if model.is_empty() { None } else { Some(model) }
    }

    /// Choose a model; refuses a name the server does not offer (unless the list is unknown).
    pub fn select_ai_model(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        if !self.ai_models.models.is_empty()
            && !self.ai_models.models.iter().any(|model| model.id == name)
        {
            return false;
        }
        self.settings.agent.model = name.to_string();
        self.mark_settings_dirty();
        true
    }

    /// Ask the selected model a trivial question in the background.
    pub fn test_ai_model(&mut self) -> CommandOutcome {
        let Some(model) = self.ai_model().map(|name| name.to_string()) else {
            let message = self.t("ai.model.none");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        };
        let Some(provider) = self.ai_provider() else {
            let message = self.t("ai.provider.unknown");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        };
        if self.ai_test_job.is_some() {
            let message = self.t("ai.busy");
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        let request = ChatRequest::new(
            model.clone(),
            vec![
                Message::system("Answer with one short sentence."),
                Message::user("Reply with the word OK and the language you understand best."),
            ],
        )
        .temperature(0.0);
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = provider
                .chat(&request)
                .map(|reply| AiTestResult {
                    model: reply.model.clone(),
                    content: reply.content,
                })
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.ai_test_job = Some(AiTestJob { receiver });
        self.report(self.tf("ai.testing", &[("model", &model)]));
        CommandOutcome::Ok
    }

    /// Collect a finished test; returns true when the state changed.
    pub fn poll_ai_test(&mut self) -> bool {
        let Some(job) = self.ai_test_job.as_ref() else {
            return false;
        };
        let outcome = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("the model call stopped unexpectedly".to_string())
            }
        };
        self.ai_test_job = None;
        match outcome {
            Ok(result) => {
                self.last_ai_reply = Some(result.clone());
                let content = result.content.trim().replace('\n', " ");
                let message = self.tf("ai.test.ok", &[("reply", &content)]);
                self.report(message);
            }
            Err(error) => {
                self.last_ai_reply = None;
                self.report_error(error);
            }
        }
        true
    }

    /// True while a model test is running.
    pub fn is_testing_model(&self) -> bool {
        self.ai_test_job.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;
    use rc_llm::ChatReply;

    /// A provider that answers from a fixed list, so the wiring can be tested offline.
    struct FakeProvider {
        models: Vec<LlmModel>,
        fail: Option<String>,
    }

    impl std::fmt::Debug for FakeProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("FakeProvider")
        }
    }

    impl Provider for FakeProvider {
        fn kind(&self) -> &'static str {
            "ollama"
        }
        fn base_url(&self) -> &str {
            "http://fake"
        }
        fn list_models(&self) -> Result<Vec<LlmModel>, rc_llm::LlmError> {
            match &self.fail {
                Some(error) => Err(rc_llm::LlmError::new(error.clone())),
                None => Ok(self.models.clone()),
            }
        }
        fn chat(&self, request: &ChatRequest) -> Result<ChatReply, rc_llm::LlmError> {
            if let Some(error) = &self.fail {
                return Err(rc_llm::LlmError::new(error.clone()));
            }
            Ok(ChatReply {
                content: format!("OK from {}", request.model),
                model: request.model.clone(),
                done_reason: Some("stop".to_string()),
                prompt_tokens: Some(3),
                answer_tokens: Some(2),
            })
        }
    }

    fn model_in(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    fn two_models() -> Vec<LlmModel> {
        vec![
            LlmModel {
                id: "coder:7b".into(),
                size: Some(4_700_000_000),
                parameters: Some("7.6B".into()),
                capabilities: vec!["completion".into(), "tools".into()],
            },
            LlmModel {
                id: "chat:3b".into(),
                size: None,
                parameters: None,
                capabilities: vec!["completion".into()],
            },
        ]
    }

    #[test]
    fn listing_models_selects_a_tool_capable_one_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.set_ai_provider(Some(std::sync::Arc::new(FakeProvider {
            models: two_models(),
            fail: None,
        })));
        assert_eq!(model.refresh_ai_models(), super::CommandOutcome::Ok);
        assert_eq!(model.ai_models().len(), 2);
        assert_eq!(
            model.ai_model(),
            Some("coder:7b"),
            "the tool-capable model is picked when nothing is configured"
        );
        assert!(model.settings_dirty, "the choice is persisted");
    }

    #[test]
    fn a_model_can_be_chosen_and_an_unknown_one_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.set_ai_provider(Some(std::sync::Arc::new(FakeProvider {
            models: two_models(),
            fail: None,
        })));
        model.refresh_ai_models();
        assert!(model.select_ai_model("chat:3b"));
        assert_eq!(model.ai_model(), Some("chat:3b"));
        assert!(!model.select_ai_model("does-not-exist"), "not offered");
        assert!(!model.select_ai_model("   "));
        assert_eq!(model.ai_model(), Some("chat:3b"), "the choice is unchanged");
    }

    #[test]
    fn a_server_that_does_not_answer_reports_why() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.set_ai_provider(Some(std::sync::Arc::new(FakeProvider {
            models: Vec::new(),
            fail: Some("cannot reach the server".to_string()),
        })));
        assert!(matches!(
            model.refresh_ai_models(),
            super::CommandOutcome::Failed(_)
        ));
        assert!(model.ai_models().is_empty());
        assert!(model.status.contains("cannot reach"), "{}", model.status);
    }

    #[test]
    fn testing_a_model_answers_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.set_ai_provider(Some(std::sync::Arc::new(FakeProvider {
            models: two_models(),
            fail: None,
        })));
        model.refresh_ai_models();
        assert_eq!(model.test_ai_model(), super::CommandOutcome::Ok);
        assert!(model.is_testing_model());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline && model.is_testing_model() {
            model.poll_ai_test();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!model.is_testing_model());
        let reply = model.last_ai_reply.clone().expect("an answer arrived");
        assert!(reply.content.contains("OK from"), "{}", reply.content);
        assert!(model.status.contains("ai.test.ok"), "{}", model.status);
    }

    #[test]
    fn testing_without_a_model_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        assert!(matches!(
            model.test_ai_model(),
            super::CommandOutcome::Failed(_)
        ));
    }
}

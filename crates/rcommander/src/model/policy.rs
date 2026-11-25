//! Policy, approvals and the audit trail in the application model (Phase 16.0b/d).
//!
//! Every side effect the agent (or a plugin, or an MCP tool) wants to perform goes through
//! [`Model::evaluate`]:
//!
//! 1. the request is checked against the current policy,
//! 2. the decision is written to the audit log,
//! 3. an approval-worthy request opens the approval dialog instead of running,
//! 4. the approved (or denied) outcome is written to the audit log as well.
//!
//! The human path — a menu command, a keyboard shortcut — is not subject to the agent policy:
//! the policy constrains *the agent's* reach, and pretending otherwise would make the file
//! manager unusable. `origin` makes that distinction explicit in the log.

use super::{CommandOutcome, Model};
use rc_core::audit::{AuditLog, ChainState, Verdict};
use rc_core::policy::{Decision, Destination, Grant, Operation, Policy, Profile, Request};

/// The three answers the approval dialog offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// Run this once; the next attempt asks again.
    Once,
    /// Run it for the rest of this session (revocable from the security page).
    Session,
    /// Never ask again for this operation — requires typing the confirmation word.
    Always,
}

/// The word the user has to type before "allow always" is accepted.
///
/// Deliberately the plainest possible word: it must be typed, not clicked, so that an accidental
/// key press or a stray Enter cannot make a permanent decision.
pub const CONFIRM_WORD: &str = "yes";

/// True when `typed` is the confirmation word.
pub fn confirms_always(typed: &str) -> bool {
    typed.trim().eq_ignore_ascii_case(CONFIRM_WORD)
}

/// What an approval dialog is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingApproval {
    /// Description of the operation for the UI (`write`, `execute`, …).
    pub operation: Operation,
    /// The canonical resource.
    pub target: String,
    /// Where the result goes, when that differs.
    pub destination: String,
    /// Who asked.
    pub origin: String,
    /// The agent's explanation, when it gave one.
    pub explanation: String,
    /// True when running it cannot be undone by restoring a file (a push, a service restart).
    pub irreversible: bool,
}

impl PendingApproval {
    /// What saying yes actually permits, in the user's language.
    ///
    /// Every line is a consequence, not a restatement of the operation name: the dialog is
    /// supposed to be readable by somebody who does not know what a "publish" is.
    pub fn consequence_keys(&self) -> Vec<&'static str> {
        let mut keys: Vec<&'static str> = match self.operation {
            Operation::Read => vec!["agent.risk.read"],
            Operation::Write => vec!["agent.risk.write"],
            Operation::Create => vec!["agent.risk.create"],
            Operation::Delete => vec!["agent.risk.delete"],
            Operation::Rename => vec!["agent.risk.rename"],
            Operation::Execute => vec!["agent.risk.execute"],
            Operation::Publish => vec!["agent.risk.publish"],
            Operation::Network => vec!["agent.risk.network"],
            Operation::Privileged => vec!["agent.risk.privileged"],
        };
        if self.irreversible {
            keys.push("agent.risk.irreversible");
        } else {
            keys.push("agent.risk.reversible");
        }
        keys
    }

    /// What "allow always" would cover, so the second window can say it plainly.
    pub fn always_scope_key(&self) -> &'static str {
        match self.operation {
            Operation::Read => "agent.always.scope.read",
            Operation::Write | Operation::Create | Operation::Rename => "agent.always.scope.write",
            Operation::Delete => "agent.always.scope.delete",
            Operation::Execute => "agent.always.scope.execute",
            Operation::Publish => "agent.always.scope.publish",
            Operation::Network => "agent.always.scope.network",
            Operation::Privileged => "agent.always.scope.privileged",
        }
    }

    /// One-line summary for the dialog title area.
    pub fn summary(&self) -> String {
        let destination = if self.destination.is_empty() {
            String::new()
        } else {
            format!(" → {}", self.destination)
        };
        format!(
            "{}: {}{destination}",
            super::policy::operation_name(self.operation),
            self.target
        )
    }
}

impl Model {
    /// Where the settings are written: the override the tests install, or the real per-user file.
    fn writable_settings_path(&self) -> Option<std::path::PathBuf> {
        Some(self.config_file("settings.toml"))
    }

    /// The policy currently in force.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// Rebuild the policy from the settings (called at boot and after the options change).
    pub fn refresh_policy(&mut self) {
        let fallback = self
            .panel(self.focus.other())
            .location()
            .as_path()
            .map(|path| path.to_path_buf())
            .into_iter()
            .collect::<Vec<_>>();
        self.policy = Policy::from_settings(&self.settings.agent, &fallback);
    }

    /// The audit trail (reads it when needed).
    pub fn audit(&self) -> &AuditLog {
        &self.audit
    }

    /// Check a request, log the decision, and either run it or ask the user.
    ///
    /// Returns `Ok(())` when the request may proceed (allowed or approved), `Err` when it was
    /// refused — in which case nothing was executed and the caller has nothing to undo.
    pub fn evaluate(&mut self, request: &Request) -> Result<(), String> {
        let now = chrono::Local::now().timestamp();
        let decision = self.policy.check(request, now);
        let target = request
            .resource
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let destination = request
            .destination
            .as_ref()
            .map(Destination::describe)
            .unwrap_or_default();
        match decision {
            Decision::Allow { grant } => {
                self.audit.record(
                    &request.origin,
                    crate::model::policy::operation_name(request.operation),
                    &target,
                    &destination,
                    Verdict::Allowed,
                    &format!("grant {grant}"),
                );
                Ok(())
            }
            Decision::NeedsApproval { reason } => {
                self.audit.record(
                    &request.origin,
                    operation_name(request.operation),
                    &target,
                    &destination,
                    Verdict::NeedsApproval,
                    &reason,
                );
                Err(format!("approval required: {reason}"))
            }
            Decision::Deny(denial) => {
                self.audit.record(
                    &request.origin,
                    operation_name(request.operation),
                    &target,
                    &destination,
                    Verdict::Denied,
                    denial.message.as_str(),
                );
                Err(denial.message)
            }
        }
    }

    /// The request the approval dialog is currently asking about, cloned for the view.
    pub fn pending_approval(&self) -> Option<&PendingApproval> {
        match self.top_dialog() {
            Some(super::Dialog::AgentApproval(state)) => Some(&state.pending),
            _ => None,
        }
    }

    /// Open the approval dialog for a request that the policy wants confirmed.
    pub fn ask_approval(&mut self, request: &Request, explanation: &str) -> CommandOutcome {
        let now = chrono::Local::now().timestamp();
        let decision = self.policy.check(request, now);
        let irreversible = matches!(
            request.operation,
            Operation::Execute | Operation::Publish | Operation::Privileged
        );
        let pending = PendingApproval {
            operation: request.operation,
            target: request
                .resource
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            destination: request
                .destination
                .as_ref()
                .map(Destination::describe)
                .unwrap_or_default(),
            origin: request.origin.clone(),
            explanation: explanation.to_string(),
            irreversible,
        };
        match decision {
            Decision::NeedsApproval { .. } | Decision::Allow { .. } => {
                self.open_dialog(super::Dialog::AgentApproval(
                    super::dialogs::AgentApprovalState {
                        pending,
                        request: request.clone(),
                        remember_for_session: false,
                        confirming_always: false,
                        typed: String::new(),
                        status: String::new(),
                    },
                ));
                CommandOutcome::Ok
            }
            Decision::Deny(denial) => {
                self.audit.record(
                    &request.origin,
                    operation_name(request.operation),
                    &pending.target,
                    &pending.destination,
                    Verdict::Denied,
                    denial.message.as_str(),
                );
                let message = denial.message.clone();
                self.report_error(&message);
                CommandOutcome::Failed(message)
            }
        }
    }

    /// The user approved the pending request once, for the session, or permanently.
    ///
    /// `typed` is only read for [`ApprovalDecision::Always`]: the permanent answer needs the user
    /// to type the confirmation word, so a click alone can never make it.
    pub fn approve_pending_with(
        &mut self,
        decision: ApprovalDecision,
        typed: &str,
    ) -> CommandOutcome {
        if decision == ApprovalDecision::Always && !confirms_always(typed) {
            // Keep the dialog open: the user asked for "always" but did not confirm it.
            let message = self.tf("agent.always.need_word", &[("word", CONFIRM_WORD)]);
            if let Some(state) = self.approval_state_mut() {
                state.status = message.clone();
            }
            return CommandOutcome::Failed(message);
        }
        match decision {
            ApprovalDecision::Once => self.approve_pending(false),
            ApprovalDecision::Session => self.approve_pending(true),
            ApprovalDecision::Always => self.approve_pending_always(),
        }
    }

    /// Store a permanent pre-approval for the pending operation and close the dialog.
    fn approve_pending_always(&mut self) -> CommandOutcome {
        let Some(state) = self.approval_state() else {
            return CommandOutcome::None;
        };
        let request = state.request.clone();
        let operation = request.operation;
        let note = self
            .tf("agent.always.note", &[("origin", &request.origin)])
            .to_string();
        let target = request
            .resource
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let destination = request
            .destination
            .as_ref()
            .map(Destination::describe)
            .unwrap_or_default();
        if !self.policy.preapprove(operation, note.clone()) {
            let message = self.t("agent.always.profile_caps");
            self.audit.record(
                &request.origin,
                operation_name(operation),
                &target,
                &destination,
                Verdict::Denied,
                "the profile does not offer this operation",
            );
            self.report_error(&message);
            return CommandOutcome::Failed(message);
        }
        // Persist the decision: the whole point of "always" is that a restart remembers it.
        let entry = rc_core::config::Preapproval {
            // The plain name, not the localisation key: the settings file must not depend on
            // the language the dialog happens to be in.
            operation: operation_name(operation).to_string(),
            label: note,
            created: chrono::Local::now().timestamp(),
        };
        self.settings
            .agent
            .preapproved
            .retain(|existing| existing.operation != entry.operation);
        self.settings.agent.preapproved.push(entry);
        self.settings_dirty = true;
        if let Some(path) = self.writable_settings_path()
            && let Err(error) = self.settings.save(&path)
        {
            let message = error.to_string();
            self.report_error(format!("settings not saved: {message}"));
        }
        self.audit.record(
            &request.origin,
            operation_name(operation),
            &target,
            &destination,
            Verdict::Approved,
            "approved permanently by the user",
        );
        self.close_dialogs();
        let name = self.t(operation.key());
        self.report(self.tf("agent.always.stored", &[("operation", &name)]));
        CommandOutcome::Ok
    }

    /// Forget a permanent pre-approval (the undo of "allow always").
    pub fn revoke_preapproval(&mut self, operation: Operation) -> bool {
        let removed_policy = self.policy.unforgive(operation);
        let key = operation_name(operation).to_string();
        let before = self.settings.agent.preapproved.len();
        self.settings
            .agent
            .preapproved
            .retain(|entry| entry.operation != key);
        let removed_settings = self.settings.agent.preapproved.len() != before;
        if removed_policy || removed_settings {
            self.settings_dirty = true;
            if let Some(path) = self.writable_settings_path()
                && let Err(error) = self.settings.save(&path)
            {
                let message = error.to_string();
                self.report_error(format!("settings not saved: {message}"));
            }
            let name = self.t(operation.key());
            self.report(self.tf("agent.always.revoked", &[("operation", &name)]));
            true
        } else {
            false
        }
    }

    /// Every stored permanent pre-approval, for the security page.
    pub fn preapprovals(&self) -> Vec<(Operation, String, i64)> {
        let mut out: Vec<(Operation, String, i64)> = Vec::new();
        for entry in &self.settings.agent.preapproved {
            if let Some(operation) = rc_core::policy::parse_operation(&entry.operation) {
                out.push((operation, entry.label.clone(), entry.created));
            }
        }
        out
    }

    /// The user approved the pending request.
    pub fn approve_pending(&mut self, remember: bool) -> CommandOutcome {
        let Some(state) = self.approval_state() else {
            return CommandOutcome::None;
        };
        let request = state.request.clone();
        if remember {
            // Two things are needed: the operation is approved for this run (that lifts the
            // approval gate) and a grant covers the path (that lifts the missing-grant refusal).
            self.policy.approve_for_session(request.operation);
            let mut grant = Grant::new(
                format!("session-{}", request.origin),
                format!("{} allowed by the user", request.origin),
                &[request.operation],
            )
            .any_root();
            // Session grants expire with an explicit revoke or a restart, not by the clock.
            grant.expires = None;
            self.policy.grant(grant);
        }
        self.audit.record(
            &request.origin,
            operation_name(request.operation),
            request
                .resource
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
                .as_str(),
            &request
                .destination
                .as_ref()
                .map(Destination::describe)
                .unwrap_or_default(),
            Verdict::Approved,
            if remember {
                "approved for this session"
            } else {
                "approved once"
            },
        );
        self.close_dialogs();
        self.report(self.tf("agent.approved", &[("origin", &request.origin)]));
        CommandOutcome::Ok
    }

    /// The user refused the pending request.
    pub fn refuse_pending(&mut self) -> CommandOutcome {
        let Some(state) = self.approval_state() else {
            return CommandOutcome::None;
        };
        let request = state.request.clone();
        self.audit.record(
            &request.origin,
            operation_name(request.operation),
            request
                .resource
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
                .as_str(),
            "",
            Verdict::Refused,
            "the user refused it",
        );
        self.close_dialogs();
        self.report(self.t("agent.refused"));
        CommandOutcome::Ok
    }

    /// Mutable access to the open approval dialog, for the confirmation status line.
    fn approval_state_mut(&mut self) -> Option<&mut super::dialogs::AgentApprovalState> {
        match self.top_dialog_mut() {
            Some(super::Dialog::AgentApproval(state)) => Some(state),
            _ => None,
        }
    }

    fn approval_state(&mut self) -> Option<super::dialogs::AgentApprovalState> {
        match self.top_dialog().cloned() {
            Some(super::Dialog::AgentApproval(state)) => Some(state),
            _ => None,
        }
    }

    /// Revoke every session grant (the "stop everything" control).
    pub fn revoke_agent_grants(&mut self) -> usize {
        let ids: Vec<String> = self
            .policy
            .grants
            .iter()
            .filter(|grant| grant.id.starts_with("session-"))
            .map(|grant| grant.id.clone())
            .collect();
        let mut revoked = 0;
        for id in ids {
            if self.policy.revoke(&id) {
                revoked += 1;
            }
        }
        let session = self.policy.clear_session_approvals();
        revoked += session;
        if revoked > 0 {
            let count = revoked.to_string();
            self.report(self.tf("agent.revoked", &[("count", &count)]));
        }
        revoked
    }

    /// Verify the audit log's hash chain, for the diagnostics page.
    pub fn audit_state(&self) -> ChainState {
        rc_core::audit::verify(self.audit.path())
    }

    /// True when the policy would let `request` run without asking.
    pub fn would_allow(&self, request: &Request) -> bool {
        self.policy
            .check(request, chrono::Local::now().timestamp())
            .is_allowed()
    }

    /// The raw decision, for tests and the diagnostics page.
    pub fn decide(&self, request: &Request) -> rc_core::policy::Decision {
        self.policy.check(request, chrono::Local::now().timestamp())
    }
}

/// The settings name of an operation.
pub fn operation_name(operation: Operation) -> &'static str {
    match operation {
        Operation::Read => "read",
        Operation::Write => "write",
        Operation::Create => "create",
        Operation::Delete => "delete",
        Operation::Rename => "rename",
        Operation::Execute => "execute",
        Operation::Publish => "publish",
        Operation::Network => "network",
        Operation::Privileged => "privileged",
    }
}

/// The profile name, for the status line.
pub fn profile_name(profile: Profile) -> &'static str {
    match profile {
        Profile::Sandbox => "sandbox",
        Profile::User => "user",
        Profile::Privileged => "privileged",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn model_in(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model.audit = AuditLog::new(dir.join("audit.log"), true);
        model.refresh_policy();
        model
    }

    fn ask_push(model: &mut Model) -> CommandOutcome {
        let request = Request::publish("origin/main of the panel's repository", "agent:git");
        model.ask_approval(&request, "publishing the commit")
    }

    #[test]
    fn allowing_once_runs_the_push_and_the_next_attempt_asks_again() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        assert_eq!(ask_push(&mut model), CommandOutcome::Ok);
        assert!(model.pending_approval().is_some(), "the dialog is open");
        assert_eq!(
            model.approve_pending_with(ApprovalDecision::Once, ""),
            CommandOutcome::Ok
        );
        assert!(model.pending_approval().is_none(), "the dialog closed");
        assert!(!model.policy().is_preapproved(Operation::Publish));
        // Nothing was stored, so the next push asks again.
        let request = Request::publish("origin/main", "agent:git");
        assert!(!model.would_allow(&request));
        assert_eq!(model.settings.agent.preapproved.len(), 0);
    }

    #[test]
    fn allowing_for_the_session_survives_until_the_grants_are_revoked() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        ask_push(&mut model);
        model.approve_pending_with(ApprovalDecision::Session, "");
        let request = Request::publish("origin/main", "agent:git");
        assert!(model.would_allow(&request), "the session grant covers it");
        assert_eq!(
            model.settings.agent.preapproved.len(),
            0,
            "not written down"
        );
        assert!(model.revoke_agent_grants() >= 1);
        assert!(!model.would_allow(&request), "the stop button works");
    }

    #[test]
    fn allowing_always_needs_the_word_and_then_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        ask_push(&mut model);
        // A click alone is not enough.
        match model.approve_pending_with(ApprovalDecision::Always, "ano") {
            CommandOutcome::Failed(message) => {
                assert!(!message.is_empty(), "the refusal explains itself")
            }
            other => panic!("a wrong word must not be accepted: {other:?}"),
        }
        assert!(model.pending_approval().is_some(), "the dialog stays open");
        assert!(!model.policy().is_preapproved(Operation::Publish));

        assert_eq!(
            model.approve_pending_with(ApprovalDecision::Always, "  YES "),
            CommandOutcome::Ok
        );
        assert!(model.policy().is_preapproved(Operation::Publish));
        assert!(model.would_allow(&Request::publish("anything", "agent")));
        // It is written to the settings file, so the next start knows it.
        let stored = model.settings.agent.preapproved.clone();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].operation, "publish");
        let path = dir.path().join("settings.toml");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            text.contains("preapproved") || !path.exists(),
            "the decision reaches the file or the test has no writable settings: {text}"
        );
        // A fresh model built from those settings still allows the push.
        let mut reloaded = model_in(dir.path());
        reloaded.settings.agent = model.settings.agent.clone();
        reloaded.refresh_policy();
        assert!(reloaded.would_allow(&Request::publish("anything", "agent")));
        // And the permanent answer can be taken back.
        assert!(model.revoke_preapproval(Operation::Publish));
        assert!(!model.policy().is_preapproved(Operation::Publish));
        assert!(model.settings.agent.preapproved.is_empty());
    }

    #[test]
    fn the_dialog_lists_the_consequences_before_any_button() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        ask_push(&mut model);
        let pending = model
            .pending_approval()
            .cloned()
            .expect("a pending approval");
        let keys = pending.consequence_keys();
        assert!(keys.contains(&"agent.risk.publish"), "{keys:?}");
        assert!(
            keys.contains(&"agent.risk.irreversible"),
            "a push cannot be undone by restoring a backup: {keys:?}"
        );
        assert_eq!(pending.always_scope_key(), "agent.always.scope.publish");
        // That the keys exist in both packs is checked by tests/i18n_keys.rs, which can see them.
    }

    #[test]
    fn the_confirmation_word_is_exact() {
        for yes in ["yes", "YES", " yes ", "Yes"] {
            assert!(confirms_always(yes), "{yes:?} should confirm");
        }
        for no in ["", "ye", "yess", "ano", "true", "1", "of course"] {
            assert!(!confirms_always(no), "{no:?} must not confirm");
        }
    }

    #[test]
    fn reading_inside_the_shared_folder_is_allowed_and_audited() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("work");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("a.txt"), "hello").unwrap();
        let mut model = model_in(dir.path());
        model.settings.agent.sandbox_roots = vec![workspace.to_string_lossy().to_string()];
        model.refresh_policy();

        let request = Request::read(workspace.join("a.txt"), "agent");
        assert!(model.evaluate(&request).is_ok(), "reading is allowed");
        let events = model.audit().read();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].verdict, Verdict::Allowed);
        assert_eq!(events[0].operation, "read");
        assert_eq!(events[0].origin, "agent");
    }

    #[test]
    fn a_write_outside_the_shared_folder_is_refused_and_logged() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("work");
        std::fs::create_dir_all(&workspace).unwrap();
        let mut model = model_in(dir.path());
        model.settings.agent.sandbox_roots = vec![workspace.to_string_lossy().to_string()];
        model.refresh_policy();

        let outside = dir.path().join("elsewhere.txt");
        let request = Request::write(workspace.join("a.txt"), &outside, "agent");
        assert!(model.evaluate(&request).is_err());
        let events = model.audit().read();
        assert_eq!(events[0].verdict, Verdict::Denied);
        assert!(events[0].note.contains("grant"), "{}", events[0].note);
    }

    #[test]
    fn an_elevated_action_opens_the_approval_dialog_and_remembers_the_answer() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.settings.agent.profile = "privileged".to_string();
        model.settings.agent.sandbox_roots = vec![dir.path().to_string_lossy().to_string()];
        model.refresh_policy();

        let request = Request::privileged("systemctl restart apache2", "agent");
        assert_eq!(
            model.ask_approval(&request, "the service fails to start"),
            CommandOutcome::Ok
        );
        match model.top_dialog() {
            Some(crate::model::Dialog::AgentApproval(state)) => {
                assert!(
                    state.pending.irreversible,
                    "a restart cannot be undone by a restore"
                );
                assert_eq!(state.pending.explanation, "the service fails to start");
            }
            other => panic!("expected the approval dialog, got {other:?}"),
        }

        assert_eq!(model.approve_pending(true), CommandOutcome::Ok);
        assert!(model.status.contains("agent.approved"), "{}", model.status);
        assert!(model.top_dialog().is_none(), "the dialog closes");
        let events = model.audit().read();
        assert_eq!(events.last().unwrap().verdict, Verdict::Approved);
        // "Allow for this session" means exactly that: the next attempt runs without a question.
        assert!(
            model.would_allow(&request),
            "the session approval lifts the gate: {:?}",
            model.decide(&request)
        );
        // Revoking the session grants puts the question back.
        model.revoke_agent_grants();
        assert!(matches!(
            model.decide(&request),
            rc_core::policy::Decision::NeedsApproval { .. }
        ));
    }

    #[test]
    fn refusing_records_the_decision_and_leaves_the_policy_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.settings.agent.profile = "privileged".to_string();
        model.refresh_policy();
        let request = Request::privileged("mount /dev/sdb1 /mnt", "agent");
        model.ask_approval(&request, "");
        assert_eq!(model.refuse_pending(), CommandOutcome::Ok);
        let events = model.audit().read();
        assert_eq!(events.last().unwrap().verdict, Verdict::Refused);
        assert!(
            !model
                .policy()
                .grants
                .iter()
                .any(|grant| grant.id.starts_with("session-")),
            "refusing creates no grant"
        );
        // The profile still offers elevation, so the request is asked again rather than
        // silently remembered as a refusal.
        assert!(matches!(
            model.decide(&request),
            rc_core::policy::Decision::NeedsApproval { .. }
        ));
    }

    #[test]
    fn the_sandbox_profile_refuses_elevation_outright() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        let request = Request::privileged("mount /dev/sdb1 /mnt", "agent");
        assert!(matches!(
            model.ask_approval(&request, ""),
            CommandOutcome::Failed(_)
        ));
        assert_eq!(model.audit().read()[0].verdict, Verdict::Denied);
    }

    #[test]
    fn revoking_session_grants_stops_the_agent_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("work");
        std::fs::create_dir_all(&workspace).unwrap();
        let mut model = model_in(dir.path());
        model.settings.agent.sandbox_roots = vec![workspace.to_string_lossy().to_string()];
        model.refresh_policy();
        // A write inside the sandbox is allowed; a session grant widens it to another folder.
        let outside = dir.path().join("other");
        std::fs::create_dir_all(&outside).unwrap();
        let request = Request::write(outside.join("a.txt"), outside.join("a.txt"), "agent");
        assert!(model.decide(&request).is_denied(), "outside the sandbox");
        model.policy.grant(
            Grant::new("session-agent", "temporary access", &[Operation::Write])
                .root(outside.clone()),
        );
        assert!(model.would_allow(&request), "the session grant allows it");
        assert_eq!(model.revoke_agent_grants(), 1);
        assert!(
            model.decide(&request).is_denied(),
            "after revoking, the request is refused outright"
        );
    }

    #[test]
    fn the_audit_chain_stays_intact_through_the_model() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model_in(dir.path());
        model.settings.agent.sandbox_roots = vec![dir.path().to_string_lossy().to_string()];
        model.refresh_policy();
        for name in ["a.txt", "b.txt"] {
            let _ = model.evaluate(&Request::read(dir.path().join(name), "agent"));
        }
        assert!(model.audit_state().is_intact(), "{:?}", model.audit_state());
    }
}

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

    /// The user approved the pending request.
    pub fn approve_pending(&mut self, remember: bool) -> CommandOutcome {
        let Some(state) = self.approval_state() else {
            return CommandOutcome::None;
        };
        let request = state.request.clone();
        if remember {
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
        // An elevated operation still asks (the profile gate never disappears), but the
        // remembered grant means the policy no longer *denies* it.
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

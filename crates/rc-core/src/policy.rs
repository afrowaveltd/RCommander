//! Capability policy for everything with side effects (Phase 16.0).
//!
//! The engine is deliberately small and deterministic: a *request* (operation + canonical
//! resource + destination) is checked against a *policy* (profile + grants) and yields one of
//! three answers — allowed, denied with a reason, or "ask the user first".
//!
//! Two rules shape the whole module:
//!
//! * **Deny wins.** A grant never overrides an explicit deny, and an expired or revoked grant
//!   fails closed.
//! * **Canonical resources only.** A grant names real paths; checking resolves symlinks and
//!   refuses `..` escapes, so a grant for `/home/me/project` cannot be reached through
//!   `/tmp/link-to-project/../..`.
//!
//! The policy is a permission scope, not an operating-system boundary: it decides what the
//! *application* lets the agent do. Real isolation is Phase 20's business.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// How much the agent may do at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Only the directories granted to the task, and only reading unless told otherwise.
    #[default]
    Sandbox,
    /// What the logged-in user may do — still through the tool layer, still audited.
    User,
    /// Everything, with every privileged operation behind an approval.
    Privileged,
}

impl Profile {
    pub fn key(self) -> &'static str {
        match self {
            Profile::Sandbox => "agent.profile.sandbox",
            Profile::User => "agent.profile.user",
            Profile::Privileged => "agent.profile.privileged",
        }
    }

    /// Parse the settings value; anything unknown falls back to the safest profile.
    pub fn parse(text: &str) -> Profile {
        match text.trim().to_ascii_lowercase().as_str() {
            "user" => Profile::User,
            "privileged" => Profile::Privileged,
            _ => Profile::Sandbox,
        }
    }
}

/// What a request wants to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Read,
    Write,
    Create,
    Delete,
    Rename,
    /// Run a program or script.
    Execute,
    /// Send data somewhere else (upload, push, HTTP POST).
    Publish,
    /// Reach the network at all (model providers, package downloads).
    Network,
    /// Do something only an administrator may do.
    Privileged,
}

impl Operation {
    pub fn key(self) -> &'static str {
        match self {
            Operation::Read => "agent.op.read",
            Operation::Write => "agent.op.write",
            Operation::Create => "agent.op.create",
            Operation::Delete => "agent.op.delete",
            Operation::Rename => "agent.op.rename",
            Operation::Execute => "agent.op.execute",
            Operation::Publish => "agent.op.publish",
            Operation::Network => "agent.op.network",
            Operation::Privileged => "agent.op.privileged",
        }
    }

    /// Operations that change something (the ones worth an approval).
    pub fn is_mutating(self) -> bool {
        matches!(
            self,
            Operation::Write
                | Operation::Create
                | Operation::Delete
                | Operation::Rename
                | Operation::Execute
                | Operation::Publish
                | Operation::Privileged
        )
    }
}

/// Where the data goes. A write inside the sandbox and an upload to a server are very
/// different risks, so the destination is part of the decision, not an afterthought.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Destination {
    /// A path on this machine.
    Local(PathBuf),
    /// A remote host and path.
    Remote { host: String, path: String },
}

impl Destination {
    pub fn describe(&self) -> String {
        match self {
            Destination::Local(path) => path.display().to_string(),
            Destination::Remote { host, path } => format!("{host}:{path}"),
        }
    }
}

/// One time-limited permission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// Stable id, so a grant can be revoked by name and followed in the audit log.
    pub id: String,
    /// Human readable purpose (`"translate locales"`).
    pub label: String,
    pub operations: BTreeSet<Operation>,
    /// Roots the grant covers. An empty list with `allow_any_root` means "everywhere", which
    /// only `User` and `Privileged` profiles may hold.
    pub roots: Vec<PathBuf>,
    /// Cover every path (used by the `User` profile's implicit read grant).
    pub allow_any_root: bool,
    /// Unix timestamp after which the grant is dead.
    pub expires: Option<i64>,
    /// Set when the user revoked it; kept so the audit log can explain the refusal.
    pub revoked: bool,
}

impl Grant {
    pub fn new(id: impl Into<String>, label: impl Into<String>, operations: &[Operation]) -> Self {
        Grant {
            id: id.into(),
            label: label.into(),
            operations: operations.iter().copied().collect(),
            roots: Vec::new(),
            allow_any_root: false,
            expires: None,
            revoked: false,
        }
    }

    pub fn root(mut self, root: impl Into<PathBuf>) -> Self {
        self.roots.push(root.into());
        self
    }

    pub fn any_root(mut self) -> Self {
        self.allow_any_root = true;
        self
    }

    pub fn until(mut self, timestamp: i64) -> Self {
        self.expires = Some(timestamp);
        self
    }

    /// True when the grant is usable at `now`.
    pub fn is_active(&self, now: i64) -> bool {
        !self.revoked && self.expires.map(|expires| now < expires).unwrap_or(true)
    }
}

/// A capability check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub operation: Operation,
    /// Resource the operation acts on (for file operations).
    pub resource: Option<PathBuf>,
    /// Where the result goes, when that differs from the resource.
    pub destination: Option<Destination>,
    /// Who asked (an agent name, a tool name, `"ui"` for a human action).
    pub origin: String,
}

impl Request {
    pub fn read(resource: impl Into<PathBuf>, origin: impl Into<String>) -> Request {
        Request {
            operation: Operation::Read,
            resource: Some(resource.into()),
            destination: None,
            origin: origin.into(),
        }
    }

    pub fn write(
        resource: impl Into<PathBuf>,
        destination: impl Into<PathBuf>,
        origin: impl Into<String>,
    ) -> Request {
        Request {
            operation: Operation::Write,
            resource: Some(resource.into()),
            destination: Some(Destination::Local(destination.into())),
            origin: origin.into(),
        }
    }

    /// Deleting a path.
    pub fn delete(resource: impl Into<PathBuf>, origin: impl Into<String>) -> Request {
        Request {
            operation: Operation::Delete,
            resource: Some(resource.into()),
            destination: None,
            origin: origin.into(),
        }
    }

    /// Publishing to somewhere outside this machine (a push, an upload); the resource is a label.
    pub fn publish(label: impl Into<String>, origin: impl Into<String>) -> Request {
        Request {
            operation: Operation::Publish,
            resource: Some(PathBuf::from(label.into())),
            destination: None,
            origin: origin.into(),
        }
    }

    pub fn execute(command: impl Into<String>, origin: impl Into<String>) -> Request {
        Request {
            operation: Operation::Execute,
            resource: Some(PathBuf::from(command.into())),
            destination: None,
            origin: origin.into(),
        }
    }

    pub fn privileged(action: impl Into<String>, origin: impl Into<String>) -> Request {
        Request {
            operation: Operation::Privileged,
            resource: Some(PathBuf::from(action.into())),
            destination: None,
            origin: origin.into(),
        }
    }
}

/// Why a request was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Denial {
    /// Machine-readable reason, for tests and the audit log.
    pub reason: DenialReason,
    /// What to show the user.
    pub message: String,
}

/// The kinds of refusal the engine distinguishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenialReason {
    /// No grant covers this operation on this resource.
    NoGrant,
    /// An explicit deny list won.
    Denied,
    /// The grant exists but has expired or was revoked.
    Expired,
    /// The path left the granted roots (symlink or `..`).
    OutsideRoot,
    /// The profile is too low for the operation.
    ProfileTooLow,
    /// The path could not be resolved at all.
    Unresolvable,
}

impl DenialReason {
    pub fn key(self) -> &'static str {
        match self {
            DenialReason::NoGrant => "agent.deny.no_grant",
            DenialReason::Denied => "agent.deny.denied",
            DenialReason::Expired => "agent.deny.expired",
            DenialReason::OutsideRoot => "agent.deny.outside_root",
            DenialReason::ProfileTooLow => "agent.deny.profile",
            DenialReason::Unresolvable => "agent.deny.unresolvable",
        }
    }
}

/// The answer to a [`Request`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow {
        /// Which grant allowed it, for the audit log.
        grant: String,
    },
    NeedsApproval {
        /// What the user would be approving.
        reason: String,
    },
    Deny(Denial),
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow { .. })
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, Decision::Deny(_))
    }
}

/// The whole policy: a profile, the grants, and the explicit deny list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub profile: Profile,
    pub grants: Vec<Grant>,
    /// Paths that are never touched, whatever a grant says (`~/.ssh`, `.git` internals, …).
    pub deny_paths: Vec<PathBuf>,
    /// Operations that always need an approval, even when a grant covers them.
    pub require_approval: BTreeSet<Operation>,
    /// Operations the human **permanently** pre-approved, with the note they gave.
    ///
    /// This is the "allow always" answer of the approval dialog: it is deliberately the only
    /// place where an approval-worthy operation can run without a question, it survives a
    /// restart, and it is visible and revocable in the security page. The deny list still wins
    /// over it and the profile still caps it — a `sandbox` policy cannot pre-approve an elevated
    /// command it would never offer.
    pub preapproved: BTreeMap<Operation, String>,
    /// Operations the user approved for the rest of **this run** ("allow for this session").
    ///
    /// In memory only: a restart asks again, which is what "this session" means. Revoking the
    /// session grants clears it, so the stop button also stops these.
    #[serde(default, skip_serializing)]
    pub approved_this_session: BTreeSet<Operation>,
}

impl Policy {
    /// The safest policy: sandbox profile, no grants, nothing runs.
    pub fn restrictive() -> Policy {
        Policy {
            profile: Profile::Sandbox,
            grants: Vec::new(),
            deny_paths: Vec::new(),
            require_approval: [
                Operation::Execute,
                Operation::Publish,
                Operation::Privileged,
            ]
            .into_iter()
            .collect(),
            preapproved: BTreeMap::new(),
            approved_this_session: BTreeSet::new(),
        }
    }

    /// Allow an operation for the rest of this run.
    pub fn approve_for_session(&mut self, operation: Operation) {
        self.approved_this_session.insert(operation);
    }

    /// Forget every session approval (the "stop everything" button).
    pub fn clear_session_approvals(&mut self) -> usize {
        let count = self.approved_this_session.len();
        self.approved_this_session.clear();
        count
    }

    /// True when an operation was approved for this run by the user.
    pub fn is_approved_this_session(&self, operation: Operation) -> bool {
        self.approved_this_session.contains(&operation)
    }

    /// Mark an operation as permanently pre-approved by the human.
    ///
    /// Returns false when the profile could never offer that operation anyway, which keeps a
    /// stored pre-approval from quietly widening the profile's ceiling.
    pub fn preapprove(&mut self, operation: Operation, note: impl Into<String>) -> bool {
        if operation == Operation::Privileged && self.profile != Profile::Privileged {
            return false;
        }
        self.preapproved.insert(operation, note.into());
        true
    }

    /// Drop a stored pre-approval.
    pub fn unforgive(&mut self, operation: Operation) -> bool {
        self.preapproved.remove(&operation).is_some()
    }

    /// True when the human pre-approved this operation permanently.
    pub fn is_preapproved(&self, operation: Operation) -> bool {
        self.preapproved.contains_key(&operation)
    }

    /// A policy for the `user` profile with the usual deny list.
    pub fn for_user_profile(home: Option<&Path>) -> Policy {
        let mut policy = Policy {
            profile: Profile::User,
            grants: vec![
                Grant::new(
                    "user-read",
                    "read anything the user can read",
                    &[Operation::Read, Operation::Network],
                )
                .any_root(),
            ],
            ..Policy::restrictive()
        };
        if let Some(home) = home {
            policy.deny_paths.push(home.join(".ssh"));
            policy.deny_paths.push(home.join(".gnupg"));
        }
        policy
    }

    /// Add or replace a grant by id.
    pub fn grant(&mut self, grant: Grant) -> &mut Self {
        self.grants.retain(|existing| existing.id != grant.id);
        self.grants.push(grant);
        self
    }

    /// Revoke a grant; it stays in the list so the audit log can name it.
    pub fn revoke(&mut self, id: &str) -> bool {
        match self.grants.iter_mut().find(|grant| grant.id == id) {
            Some(grant) => {
                grant.revoked = true;
                true
            }
            None => false,
        }
    }

    /// Check a request at `now` (unix seconds).
    pub fn check(&self, request: &Request, now: i64) -> Decision {
        // 0. A privileged operation always needs its own approval, whatever the profile —
        //    the point of the profile is to *offer* it, never to run it silently.
        if request.operation == Operation::Privileged && self.profile != Profile::Privileged {
            return Decision::Deny(Denial {
                reason: DenialReason::ProfileTooLow,
                message: format!(
                    "“{}” needs elevated privileges, which the {} profile does not offer",
                    request.origin,
                    match self.profile {
                        Profile::Sandbox => "sandbox",
                        Profile::User => "user",
                        Profile::Privileged => "privileged",
                    }
                ),
            });
        }

        // 1. Canonicalise the resource when it *is* a path. For `Execute`, `Privileged`,
        //    `Publish` and `Network` the resource may be a command or a label, so only its
        //    text is kept.
        let resource = match &request.resource {
            Some(path) if resource_is_path(request.operation) => match canonical_for_check(path) {
                Some(path) => Some(path),
                None => {
                    return Decision::Deny(Denial {
                        reason: DenialReason::Unresolvable,
                        message: format!("cannot resolve {}", path.display()),
                    });
                }
            },
            other => other.clone(),
        };

        // 1b. A local destination is a second resource: writing *into* the sandbox from
        //     outside it (or out of it) is exactly the case a grant must not silently allow.
        let destination_path = match &request.destination {
            Some(Destination::Local(path)) => match canonical_for_check(path) {
                Some(path) => Some(path),
                None => {
                    return Decision::Deny(Denial {
                        reason: DenialReason::Unresolvable,
                        message: format!("cannot resolve the destination {}", path.display()),
                    });
                }
            },
            _ => None,
        };

        // 2. The deny list wins over every grant.
        if let Some(resource) = &resource {
            for denied in &self.deny_paths {
                let denied = canonical_for_check(denied).unwrap_or_else(|| denied.clone());
                if resource.starts_with(&denied) {
                    return Decision::Deny(Denial {
                        reason: DenialReason::Denied,
                        message: format!("{} is on the deny list", resource.display()),
                    });
                }
            }
        }

        // 2b. A permanent pre-approval runs the operation without a question. It comes after the
        //     deny list (which always wins) and before the grants (a pre-approval is explicit
        //     where a grant is an assumption).
        if let Some(note) = self.preapproved.get(&request.operation) {
            return Decision::Allow {
                grant: format!("preapproved: {note}"),
            };
        }
        // "Allow for this session" is the same idea with a shorter lifetime.
        if self.approved_this_session.contains(&request.operation) {
            return Decision::Allow {
                grant: "approved for this session".to_string(),
            };
        }

        // 3. Collect the grants that could cover the operation.
        let mut covering: Vec<&Grant> = Vec::new();
        let mut expired: Option<&Grant> = None;
        for grant in &self.grants {
            if !grant.operations.contains(&request.operation) {
                continue;
            }
            if !grant.is_active(now) {
                if grant.roots.is_empty() || resource_covers(grant, resource.as_deref()) {
                    expired = Some(grant);
                }
                continue;
            }
            if resource_covers(grant, resource.as_deref())
                && (destination_path.is_none()
                    || resource_covers(grant, destination_path.as_deref()))
            {
                covering.push(grant);
            }
        }

        if covering.is_empty() {
            if let Some(grant) = expired {
                return Decision::Deny(Denial {
                    reason: DenialReason::Expired,
                    message: format!("the grant “{}” has expired or was revoked", grant.label),
                });
            }
            // An operation without a path (`publish`, `execute`, `network`) cannot be covered by
            // a path grant, so "no grant" here does not mean "refused" — it means nobody has
            // pre-allowed it, which is exactly what the human should decide. Falling through to
            // the approval gate is what makes the dialog reachable for a push at all.
            if !resource_is_path(request.operation) {
                return Decision::NeedsApproval {
                    reason: format!(
                        "{} wants to {} {}",
                        request.origin,
                        request.operation.key(),
                        resource
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "something".to_string())
                    ),
                };
            }
            // A path operation without a grant is refused — never silently widened.
            return Decision::Deny(Denial {
                reason: DenialReason::NoGrant,
                message: format!(
                    "no grant covers {} on {}",
                    request.operation.key(),
                    resource
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "this resource".to_string())
                ),
            });
        }

        // 4. Anything the policy marks as approval-worthy goes to the user first.
        if self.require_approval.contains(&request.operation)
            || (request.operation == Operation::Privileged)
        {
            return Decision::NeedsApproval {
                reason: format!(
                    "{} wants to {} {}",
                    request.origin,
                    request.operation.key(),
                    resource
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "something".to_string())
                ),
            };
        }

        Decision::Allow {
            grant: covering[0].id.clone(),
        }
    }
}

impl Policy {
    /// Build the policy from the `[agent]` settings section.
    ///
    /// `fallback_roots` is used when no sandbox root is configured (the other panel's
    /// directory), so a fresh installation starts working without a detour through the
    /// options dialog.
    pub fn from_settings(agent: &crate::config::Agent, fallback_roots: &[PathBuf]) -> Policy {
        let profile = Profile::parse(&agent.profile);
        let mut policy = Policy {
            profile,
            grants: Vec::new(),
            deny_paths: agent.deny_paths.iter().map(PathBuf::from).collect(),
            require_approval: agent
                .require_approval
                .iter()
                .filter_map(|name| parse_operation(name))
                .collect(),
            preapproved: BTreeMap::new(),
            approved_this_session: BTreeSet::new(),
        };
        for entry in &agent.preapproved {
            if let Some(operation) = parse_operation(&entry.operation) {
                policy.preapprove(operation, entry.label.clone());
            }
        }
        if policy.require_approval.is_empty() {
            // An empty list must not become "no approvals at all"; the safe default stays.
            policy.require_approval = [
                Operation::Execute,
                Operation::Publish,
                Operation::Privileged,
            ]
            .into_iter()
            .collect();
        }
        let mut roots: Vec<PathBuf> = agent.sandbox_roots.iter().map(PathBuf::from).collect();
        if roots.is_empty() {
            roots = fallback_roots.to_vec();
        }
        match profile {
            Profile::Sandbox => {
                if !roots.is_empty() {
                    policy.grant(
                        Grant::new(
                            "sandbox",
                            "the workspace the user shared",
                            &[
                                Operation::Read,
                                Operation::Write,
                                Operation::Create,
                                Operation::Delete,
                                Operation::Rename,
                            ],
                        )
                        .root(roots[0].clone()),
                    );
                }
            }
            Profile::User | Profile::Privileged => {
                policy.grant(
                    Grant::new(
                        "user-read",
                        "read anything the user can read",
                        &[Operation::Read, Operation::Network],
                    )
                    .any_root(),
                );
                for root in &roots {
                    policy.grant(
                        Grant::new(
                            format!("workspace-{}", root.display()),
                            "the workspace the user shared",
                            &[
                                Operation::Write,
                                Operation::Create,
                                Operation::Delete,
                                Operation::Rename,
                            ],
                        )
                        .root(root.clone()),
                    );
                }
                if profile == Profile::Privileged {
                    // The privileged profile *offers* elevation and publication; both stay in
                    // the approval list, so the grant only decides that the user is asked
                    // rather than refused outright.
                    policy.grant(
                        Grant::new(
                            "elevated",
                            "administration",
                            &[
                                Operation::Privileged,
                                Operation::Execute,
                                Operation::Publish,
                            ],
                        )
                        .any_root(),
                    );
                }
            }
        }
        policy
    }
}

/// Parse an operation name from the settings file.
pub fn parse_operation(name: &str) -> Option<Operation> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "read" => Operation::Read,
        "write" => Operation::Write,
        "create" => Operation::Create,
        "delete" => Operation::Delete,
        "rename" => Operation::Rename,
        "execute" | "exec" => Operation::Execute,
        "publish" => Operation::Publish,
        "network" => Operation::Network,
        "privileged" | "sudo" => Operation::Privileged,
        _ => return None,
    })
}

/// True when the operation's resource is a filesystem path.
fn resource_is_path(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::Read
            | Operation::Write
            | Operation::Create
            | Operation::Delete
            | Operation::Rename
    )
}

/// True when `grant` covers `resource`.
fn resource_covers(grant: &Grant, resource: Option<&Path>) -> bool {
    match resource {
        None => true,
        Some(resource) => {
            if grant.allow_any_root {
                return true;
            }
            grant.roots.iter().any(|root| {
                let root = canonical_for_check(root).unwrap_or_else(|| root.clone());
                resource == root.as_path() || resource.starts_with(&root)
            })
        }
    }
}

/// Resolve a path the way the filesystem would, tolerating a missing last component.
///
/// `..` is resolved by `canonicalize` itself; when the path does not exist yet, the closest
/// existing ancestor is canonicalised and the rest appended, which is what a write is about
/// to create.
pub fn canonical_for_check(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return Some(resolved);
    }
    let mut ancestor = path.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        match ancestor.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                if let Some(name) = ancestor.file_name() {
                    rest.push(name.to_os_string());
                }
                ancestor = parent.to_path_buf();
                if let Ok(resolved) = std::fs::canonicalize(&ancestor) {
                    let mut out = resolved;
                    for part in rest.iter().rev() {
                        out.push(part);
                    }
                    return Some(out);
                }
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        (dir, root)
    }

    fn sandbox(root: &Path) -> Policy {
        let mut policy = Policy::restrictive();
        policy.grant(
            Grant::new(
                "task",
                "the task's folder",
                &[Operation::Read, Operation::Write],
            )
            .root(root)
            .until(now() + 3600),
        );
        policy
    }

    fn now() -> i64 {
        chrono::Local::now().timestamp()
    }

    #[test]
    fn a_grant_covers_its_root_and_nothing_else() {
        let (_dir, root) = temp();
        let policy = sandbox(&root);
        let inside = root.join("notes.md");
        assert!(
            policy
                .check(&Request::read(&inside, "agent"), now())
                .is_allowed()
        );
        let outside = std::env::temp_dir().join("elsewhere.txt");
        let decision = policy.check(&Request::read(&outside, "agent"), now());
        assert!(decision.is_denied(), "{decision:?}");
        match decision {
            Decision::Deny(denial) => assert!(
                matches!(
                    denial.reason,
                    DenialReason::NoGrant | DenialReason::OutsideRoot
                ),
                "{denial:?}"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_symlink_cannot_leave_the_granted_root() {
        let (_dir, root) = temp();
        let outside = std::env::temp_dir().join(format!("rc-policy-secret-{}", std::process::id()));
        std::fs::write(&outside, "secret").unwrap();
        let link = root.join("shortcut");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, &link).is_ok();
        #[cfg(not(unix))]
        let made = false;
        if made {
            let policy = sandbox(&root);
            let decision = policy.check(&Request::read(&link, "agent"), now());
            assert!(
                decision.is_denied(),
                "a symlink out of the sandbox must be refused: {decision:?}"
            );
        }
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn dotdot_cannot_escape_either() {
        let (_dir, root) = temp();
        let policy = sandbox(&root);
        let sneaky = root.join("..").join("..").join("etc").join("passwd");
        assert!(
            policy
                .check(&Request::read(&sneaky, "agent"), now())
                .is_denied(),
            "…/../.. must not reach outside the root"
        );
    }

    #[test]
    fn deny_wins_over_a_grant_and_expired_grants_fail_closed() {
        let (_dir, root) = temp();
        let mut policy = sandbox(&root);
        let secrets = root.join("secrets");
        std::fs::create_dir_all(&secrets).unwrap();
        policy.deny_paths.push(secrets.clone());
        let decision = policy.check(&Request::read(secrets.join("key.pem"), "agent"), now());
        match decision {
            Decision::Deny(denial) => assert_eq!(denial.reason, DenialReason::Denied),
            other => panic!("{other:?}"),
        }

        // An expired grant refuses even inside its own root.
        let mut expired = Policy::restrictive();
        expired.grant(
            Grant::new("old", "yesterday", &[Operation::Read])
                .root(root.clone())
                .until(now() - 10),
        );
        let decision = expired.check(&Request::read(root.join("a.txt"), "agent"), now());
        match decision {
            Decision::Deny(denial) => assert_eq!(denial.reason, DenialReason::Expired),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_revoked_grant_stops_working_immediately() {
        let (_dir, root) = temp();
        let mut policy = sandbox(&root);
        assert!(
            policy
                .check(&Request::read(root.join("x"), "agent"), now())
                .is_allowed()
        );
        assert!(policy.revoke("task"));
        let decision = policy.check(&Request::read(root.join("x"), "agent"), now());
        assert!(decision.is_denied(), "{decision:?}");
    }

    #[test]
    fn destructive_and_publication_actions_need_an_approval() {
        let (_dir, root) = temp();
        let mut policy = sandbox(&root);
        // Give the agent everything, then check the approval gate still asks.
        policy.grant(
            Grant::new(
                "all",
                "everything",
                &[
                    Operation::Execute,
                    Operation::Publish,
                    Operation::Privileged,
                ],
            )
            .any_root(),
        );
        policy.profile = Profile::Privileged;
        for request in [
            Request::execute("rm -rf build", "agent"),
            Request::privileged("systemctl restart apache2", "agent"),
            Request {
                operation: Operation::Publish,
                resource: Some(root.join("index.html")),
                destination: Some(Destination::Remote {
                    host: "example.com".into(),
                    path: "/var/www/index.html".into(),
                }),
                origin: "agent".into(),
            },
        ] {
            match policy.check(&request, now()) {
                Decision::NeedsApproval { .. } => {}
                other => panic!("{request:?} → {other:?}"),
            }
        }
    }

    #[test]
    fn the_profile_caps_what_an_approval_could_unlock() {
        let (_dir, root) = temp();
        let mut policy = sandbox(&root);
        policy.grant(Grant::new("p", "elevate", &[Operation::Privileged]).any_root());
        let decision = policy.check(&Request::privileged("mount", "agent"), now());
        match decision {
            Decision::Deny(denial) => assert_eq!(denial.reason, DenialReason::ProfileTooLow),
            other => panic!("{other:?}"),
        }
        // Raising the profile turns the same request into an approval, not a silent run.
        policy.profile = Profile::Privileged;
        assert!(matches!(
            policy.check(&Request::privileged("mount", "agent"), now()),
            Decision::NeedsApproval { .. }
        ));
    }

    #[test]
    fn a_path_that_cannot_be_resolved_is_refused() {
        let policy = Policy::restrictive();
        let decision = policy.check(&Request::read("", "agent"), now());
        match decision {
            Decision::Deny(denial) => assert_eq!(denial.reason, DenialReason::Unresolvable),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_user_profile_reads_freely_but_does_not_write_silently() {
        let (_dir, root) = temp();
        let policy = Policy::for_user_profile(Some(&root));
        assert!(
            policy
                .check(
                    &Request::read(std::env::temp_dir().join("x"), "agent"),
                    now()
                )
                .is_allowed()
        );
        assert!(
            policy
                .check(
                    &Request::write(root.join("a"), root.join("b"), "agent"),
                    now()
                )
                .is_denied(),
            "writing needs its own grant"
        );
    }

    #[test]
    fn a_grant_only_covers_the_operations_it_names() {
        let (_dir, root) = temp();
        let mut policy = Policy::restrictive();
        policy.grant(Grant::new("ro", "read only", &[Operation::Read]).root(root.clone()));
        assert!(
            policy
                .check(&Request::read(root.join("a"), "agent"), now())
                .is_allowed()
        );
        assert!(
            policy
                .check(
                    &Request::write(root.join("a"), root.join("a"), "agent"),
                    now()
                )
                .is_denied()
        );
    }

    #[test]
    fn switching_the_panel_does_not_widen_the_scope() {
        // The policy has no idea which pane is focused: the same request is judged the same
        // way whatever the UI is showing.
        let (_dir, root) = temp();
        let policy = sandbox(&root);
        let outside = std::env::temp_dir().join("somewhere-else");
        assert!(
            policy
                .check(&Request::read(&outside, "agent"), now())
                .is_denied()
        );
        assert!(
            policy
                .check(&Request::read(root.join("inside"), "agent"), now())
                .is_allowed()
        );
    }

    #[test]
    fn a_pre_approval_runs_the_operation_without_a_question() {
        let (_dir, root) = temp();
        let mut policy = sandbox(&root);
        // A push has no path grant and always asks, so it is the honest test case.
        let push = Request::publish("git://example/repo.git", "agent");
        assert!(matches!(
            policy.check(&push, now()),
            Decision::NeedsApproval { .. }
        ));
        assert!(policy.preapprove(Operation::Publish, "I said so"));
        assert!(policy.check(&push, now()).is_allowed());
        assert!(policy.is_preapproved(Operation::Publish));
        // And it can be taken back.
        assert!(policy.unforgive(Operation::Publish));
        assert!(!policy.is_preapproved(Operation::Publish));
        assert!(matches!(
            policy.check(&push, now()),
            Decision::NeedsApproval { .. }
        ));
    }

    #[test]
    fn the_deny_list_still_wins_over_a_pre_approval() {
        let (_dir, root) = temp();
        let mut policy = sandbox(&root);
        let secret = root.join("secrets");
        policy.deny_paths.push(secret.clone());
        assert!(policy.preapprove(Operation::Delete, "delete whatever"));
        assert!(
            policy
                .check(&Request::delete(secret.join("key"), "agent"), now())
                .is_denied(),
            "a pre-approval cannot unlock the deny list"
        );
    }

    #[test]
    fn a_pre_approval_cannot_widen_the_profile_ceiling() {
        let (_dir, root) = temp();
        let mut policy = sandbox(&root);
        assert!(
            !policy.preapprove(Operation::Privileged, "root everything"),
            "the sandbox profile never offers elevated work"
        );
        assert!(!policy.is_preapproved(Operation::Privileged));
        // The profile that does offer it can store the decision.
        let mut privileged = Policy::from_settings(
            &crate::config::Agent {
                profile: "privileged".to_string(),
                ..Default::default()
            },
            &[],
        );
        assert!(privileged.preapprove(Operation::Privileged, "root"));
        assert!(privileged.is_preapproved(Operation::Privileged));
    }

    #[test]
    fn pre_approvals_come_from_the_settings_and_survive_a_round_trip() {
        let (_dir, root) = temp();
        let mut agent = crate::config::Agent {
            preapproved: vec![crate::config::Preapproval {
                operation: "publish".to_string(),
                label: "the user allowed pushes".to_string(),
                created: 1_700_000_000,
            }],
            ..Default::default()
        };
        let policy = Policy::from_settings(&agent, std::slice::from_ref(&root));
        assert!(policy.is_preapproved(Operation::Publish));
        assert_eq!(
            policy
                .preapproved
                .get(&Operation::Publish)
                .map(String::as_str),
            Some("the user allowed pushes")
        );
        // The settings keep the entry, so a restart remembers the decision.
        let text = toml::to_string_pretty(&agent).unwrap();
        assert!(text.contains("preapproved"), "{text}");
        let back: crate::config::Agent = toml::from_str(&text).unwrap();
        assert_eq!(back.preapproved, agent.preapproved);
        // An unknown operation name is ignored instead of failing the whole settings file.
        agent.preapproved.push(crate::config::Preapproval {
            operation: "teleport".to_string(),
            label: String::new(),
            created: 0,
        });
        let policy = Policy::from_settings(&agent, &[root]);
        assert_eq!(policy.preapproved.len(), 1, "only known operations count");
    }

    #[test]
    fn the_profiles_and_operations_are_serialisable_and_parse_safely() {
        assert_eq!(Profile::parse("privileged"), Profile::Privileged);
        assert_eq!(Profile::parse("USER"), Profile::User);
        assert_eq!(Profile::parse("nonsense"), Profile::Sandbox);
        let policy = sandbox(Path::new("/tmp"));
        let text = toml::to_string_pretty(&policy).unwrap();
        let back: Policy = toml::from_str(&text).unwrap();
        assert_eq!(back, policy);
        assert!(Operation::Delete.is_mutating());
        assert!(!Operation::Read.is_mutating());
    }
}

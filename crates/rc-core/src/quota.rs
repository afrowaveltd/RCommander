//! Token-quota ledger and admission evidence (Phase 16.20, second slice).
//!
//! The catalogue in [`crate::model_catalog`] records what the owner *configured*. This module is
//! the only place that knows what has actually been *used*, and it is deliberately the most
//! conservative component on the AI path:
//!
//! * A request reserves its estimated input plus its maximum output **before** dispatch, so two
//!   concurrent jobs cannot spend the same allowance and a crash cannot return an allowance that
//!   was already spent.
//! * Reported usage reconciles the reservation; **missing usage is charged in full**, because an
//!   unreported request still consumed tokens.
//! * A reservation left behind by a crash is reconciled against evidence after a grace period: a
//!   request that was dispatched is charged, a request that never left is released. It is never
//!   silently dropped and never replayed.
//! * A provider-defined limit whose reset was never observed reports *unknown*, and unknown is
//!   not unlimited.
//! * A provider's own rate-limit answer is stored as a wait keyed by scope (a quota or a model).
//!   The numeric `Retry-After` is used as a delay; anything else is untrusted text and is not
//!   interpreted here.
//!
//! Two deliberate omissions, stated so nobody mistakes them for coverage: provider-reported
//! counters (a provider that says "12 000 tokens left" is not modelled yet) and time zones other
//! than UTC for calendar windows. Neither is inferred from names, prices or parameter counts.
//!
//! There is no clock and no network here: every method takes `now` in Unix seconds, which is what
//! makes the tests deterministic. Nothing in this module can grant an OS, tool or credential
//! permission.

use crate::error::{CoreError, CoreResult};
use crate::model_catalog::{QuotaPeriod, QuotaWindow, TokenQuota};
use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Version of the on-disk state. A newer file is refused instead of guessed at.
pub const STATE_VERSION: u32 = 1;

/// An outstanding reservation older than this belongs to an interrupted attempt.
pub const ORPHAN_GRACE_SECONDS: i64 = 3_600;

/// How long a provider answer without a usable delay blocks its scope.
///
/// This is an editable default back-off, not an interpretation of untrusted text: a "429" with no
/// `Retry-After` says "later", and retrying immediately is the one thing that must not happen.
pub const UNKNOWN_WAIT_SECONDS: i64 = 60;

/// Charges older than this are pruned once their window has rolled, so the file stays small.
/// It is longer than any window (31 days for a calendar month, 30 days for a rolling month).
pub const RETENTION_SECONDS: i64 = 60 * 60 * 24 * 60;

/// File name of the ledger inside the per-user configuration directory.
///
/// It sits next to the settings because it is read back after a restart, and it is not part of the
/// settings file because a settings edit must never rewrite what has already been spent.
pub const LEDGER_FILE: &str = "quota-ledger.toml";

/// One charged amount against one quota.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Charge {
    /// Quota the tokens are booked against.
    pub quota_id: String,
    /// Unix seconds; the moment the request was reserved, not the moment it was settled, so the
    /// rolling window follows request order.
    pub at: i64,
    pub tokens: u64,
    /// Reservation that produced the charge; `None` for usage reported from outside RCommander.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reservation: Option<String>,
}

/// Allowance held for one request until its result is reconciled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    /// Caller-chosen stable id; a receipt and the audit log refer to it.
    pub id: String,
    /// Every quota this request consumes, in the order the caller listed them.
    pub quota_ids: Vec<String>,
    /// Estimated input plus maximum output.
    pub tokens: u64,
    /// Unix seconds when the allowance was taken.
    pub at: i64,
    /// True once the caller recorded that the request left. The caller must persist this **before**
    /// dispatch; an undispatched reservation is safe to release after a crash, a dispatched one is
    /// charged in full because it may have produced output.
    #[serde(default)]
    pub dispatched: bool,
}

/// Where a wait came from, so the UI can be honest about how much it knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitEvidence {
    /// A numeric `Retry-After`: a delay relative to when the answer arrived.
    RetryAfter,
    /// A rate-limit answer with no usable delay: only the conservative default back-off.
    Unknown,
    /// A reset time that was already parsed and validated by a trusted caller.
    Reset,
}

/// One wait against one scope (`a quota id` or `model:<catalogue id>`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wait {
    pub scope: String,
    /// Unix seconds. A wait is honoured while `until > now`.
    pub until: i64,
    pub evidence: WaitEvidence,
    /// Unix seconds when the wait was recorded, for the explanation shown to the owner.
    pub at: i64,
}

/// What is left of a configured limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remaining {
    Known {
        tokens: u64,
        /// When the known window rolls over, when the ledger can say.
        reset_at: Option<i64>,
    },
    /// Unknown is not unlimited: a caller must ask or stop, never assume.
    Unknown(UnknownRemaining),
}

/// Why a remaining allowance cannot be computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownRemaining {
    /// The provider owns the reset epoch and no verified observation was recorded.
    ProviderDefinedWithoutReset,
    /// The quota id is not in the configured catalogue.
    NotConfigured,
}

/// What one reconciliation actually booked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettleReport {
    pub reservation_id: String,
    pub quota_ids: Vec<String>,
    pub charged: u64,
    /// The provider did not report usage, so the whole reservation was charged.
    pub usage_missing: bool,
}

/// What reconciliation did with a reservation left behind by a crash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrphanReport {
    pub reservation_id: String,
    /// `Some(tokens)` when the interrupted request was charged in full, `None` when it was
    /// released because it never left the machine.
    pub charged: Option<u64>,
}

/// Why the ledger refused an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LedgerError {
    /// The reservation id is already in use; a second request must not reuse a receipt.
    DuplicateReservation(String),
    UnknownReservation(String),
    UnknownQuota(String),
    /// The same quota was listed twice for one request.
    DuplicateQuotaReference(String),
    /// A reservation must reserve something.
    EmptyReservation,
    /// The scope is rate limited and must not be asked again before `until`.
    Waiting {
        scope: String,
        until: i64,
    },
    /// The known allowance is smaller than the request.
    Exceeded {
        quota_id: String,
        remaining: u64,
        wanted: u64,
    },
    /// The allowance cannot be computed, so the request is refused rather than guessed at.
    UnknownRemaining(String),
    /// A dispatched request may already have spent tokens; charge it, do not release it.
    AlreadyDispatched(String),
    /// The provider owns this window and no reset was observed, so external usage cannot be booked.
    ProviderWindowWithoutReset(String),
    /// The catalogue itself is invalid.
    Catalogue(String),
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LedgerError::DuplicateReservation(id) => {
                write!(f, "the reservation “{id}” already exists")
            }
            LedgerError::UnknownReservation(id) => {
                write!(f, "there is no reservation “{id}”")
            }
            LedgerError::UnknownQuota(id) => write!(f, "the quota “{id}” is not configured"),
            LedgerError::DuplicateQuotaReference(id) => {
                write!(f, "the quota “{id}” is charged twice for one request")
            }
            LedgerError::EmptyReservation => {
                write!(f, "a reservation needs a positive token allowance")
            }
            LedgerError::Waiting { scope, until } => {
                write!(f, "“{scope}” is rate limited until {until}")
            }
            LedgerError::Exceeded {
                quota_id,
                remaining,
                wanted,
            } => write!(
                f,
                "the quota “{quota_id}” has {remaining} tokens left, {wanted} were requested"
            ),
            LedgerError::UnknownRemaining(id) => write!(
                f,
                "the remaining allowance of “{id}” is unknown, and unknown is not unlimited"
            ),
            LedgerError::AlreadyDispatched(id) => write!(
                f,
                "the reservation “{id}” was dispatched and may have spent tokens"
            ),
            LedgerError::ProviderWindowWithoutReset(id) => write!(
                f,
                "the provider owns the reset of “{id}” and no reset was observed"
            ),
            LedgerError::Catalogue(why) => write!(f, "the model catalogue is invalid: {why}"),
        }
    }
}

/// The durable ledger state, kept separate from the configured limits so a catalogue edit does not
/// rewrite history.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct StoredState {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    charges: Vec<Charge>,
    #[serde(default)]
    reservations: BTreeMap<String, Reservation>,
    #[serde(default)]
    waits: BTreeMap<String, Wait>,
    /// Quota id → Unix seconds of the last verified provider reset.
    #[serde(default)]
    resets: BTreeMap<String, i64>,
}

fn default_version() -> u32 {
    STATE_VERSION
}

/// Charges and reservations for the limits in [`TokenQuota`].
#[derive(Clone, Debug)]
pub struct QuotaLedger {
    quotas: BTreeMap<String, TokenQuota>,
    state: StoredState,
    path: Option<PathBuf>,
    dirty: bool,
}

impl QuotaLedger {
    /// A new, empty ledger for `quotas`; validates the limits it is given.
    pub fn new(quotas: &[TokenQuota]) -> CoreResult<Self> {
        let mut ledger = QuotaLedger {
            quotas: BTreeMap::new(),
            state: StoredState {
                version: STATE_VERSION,
                ..StoredState::default()
            },
            path: None,
            dirty: false,
        };
        ledger.set_quotas(quotas)?;
        Ok(ledger)
    }

    /// Replace the configured limits, keeping the charges already booked.
    ///
    /// History for a quota that is no longer configured is kept: removing a limit is not proof
    /// that the allowance it described came back.
    pub fn set_quotas(&mut self, quotas: &[TokenQuota]) -> CoreResult<()> {
        let mut next = BTreeMap::new();
        for quota in quotas {
            if quota.id.trim().is_empty() {
                return Err(CoreError::Invalid("a quota needs an id".into()));
            }
            if quota.tokens == 0 {
                return Err(CoreError::Invalid(format!(
                    "the quota “{}” allows no tokens at all",
                    quota.id
                )));
            }
            if next.insert(quota.id.clone(), quota.clone()).is_some() {
                return Err(CoreError::Invalid(format!(
                    "the quota “{}” is configured twice",
                    quota.id
                )));
            }
        }
        if self.quotas != next {
            self.quotas = next;
            self.dirty = true;
        }
        Ok(())
    }

    /// Read the ledger at `path`, or start an empty one when the file does not exist yet.
    pub fn load(path: impl Into<PathBuf>, quotas: &[TokenQuota]) -> CoreResult<Self> {
        let path = path.into();
        let mut ledger = QuotaLedger::new(quotas)?;
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let state: StoredState = toml::from_str(&text).map_err(|error| {
                    CoreError::Invalid(format!("{} is not a valid ledger: {error}", path.display()))
                })?;
                if state.version > STATE_VERSION {
                    return Err(CoreError::Invalid(format!(
                        "{} was written by a newer ledger (version {}), this build understands {STATE_VERSION}",
                        path.display(),
                        state.version
                    )));
                }
                for reservation in state.reservations.values() {
                    if reservation.tokens == 0 {
                        return Err(CoreError::Invalid(format!(
                            "{} holds an empty reservation “{}”",
                            path.display(),
                            reservation.id
                        )));
                    }
                }
                ledger.state = state;
                ledger.state.version = STATE_VERSION;
                ledger.dirty = false;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(CoreError::io(&path, error)),
        }
        ledger.path = Some(path);
        Ok(ledger)
    }

    /// Load the ledger for `catalog` from `dir`, starting empty when no file is there yet.
    ///
    /// This is the entry point a front-end uses: the configured limits come from the owner's
    /// catalogue, the durable history from the file beside the settings.
    pub fn load_in(
        dir: impl AsRef<Path>,
        catalog: &crate::model_catalog::ModelCatalog,
    ) -> CoreResult<Self> {
        QuotaLedger::load(dir.as_ref().join(LEDGER_FILE), &catalog.quotas)
    }

    /// Where this ledger is (or would be) written.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Point the ledger at a file without writing it yet.
    pub fn set_path(&mut self, path: impl Into<PathBuf>) {
        self.path = Some(path.into());
    }

    /// True when something changed since the last successful [`Self::save`].
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Atomically replace the ledger file.
    ///
    /// The temporary file is flushed and synced before it is renamed over the previous state, so a
    /// crash leaves either the old ledger or the new one and never a half-written mix.
    pub fn save(&mut self) -> CoreResult<()> {
        let Some(path) = self.path.clone() else {
            return Err(CoreError::Invalid(
                "the ledger has no path to be saved to".into(),
            ));
        };
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&parent).map_err(|error| CoreError::io(&parent, error))?;
        let text = toml::to_string_pretty(&self.state)
            .map_err(|error| CoreError::Invalid(format!("cannot encode the ledger: {error}")))?;
        let mut temp = tempfile::NamedTempFile::new_in(&parent)
            .map_err(|error| CoreError::io(&parent, error))?;
        temp.write_all(text.as_bytes())
            .map_err(|error| CoreError::io(&path, error))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| CoreError::io(&path, error))?;
        temp.persist(&path)
            .map_err(|error| CoreError::io(&path, error.error))?;
        self.dirty = false;
        Ok(())
    }

    /// The configured limit, when the catalogue still declares it.
    pub fn quota(&self, id: &str) -> Option<&TokenQuota> {
        self.quotas.get(id)
    }

    /// One outstanding reservation.
    pub fn reservation(&self, id: &str) -> Option<&Reservation> {
        self.state.reservations.get(id)
    }

    /// Every outstanding reservation, in id order so a report is stable.
    pub fn outstanding(&self) -> Vec<Reservation> {
        self.state.reservations.values().cloned().collect()
    }

    /// Charges booked for `quota_id`, oldest first (a receipt for the owner and the review).
    pub fn charges(&self, quota_id: &str) -> Vec<Charge> {
        let mut charges: Vec<Charge> = self
            .state
            .charges
            .iter()
            .filter(|charge| charge.quota_id == quota_id)
            .cloned()
            .collect();
        charges.sort_by_key(|charge| (charge.at, charge.tokens));
        charges
    }

    /// What is left of `quota_id` at `now`.
    ///
    /// Outstanding reservations count as spent: an allowance promised to a running job is not
    /// available to a second one.
    pub fn remaining(&self, quota_id: &str, now: i64) -> Remaining {
        let Some(quota) = self.quotas.get(quota_id) else {
            return Remaining::Unknown(UnknownRemaining::NotConfigured);
        };
        let Some(window) = self.window(quota, now) else {
            return Remaining::Unknown(UnknownRemaining::ProviderDefinedWithoutReset);
        };
        let used = self.used_since(quota_id, window.start);
        let reset_at = match quota.window {
            QuotaWindow::Rolling => self
                .oldest_since(quota_id, window.start)
                .map(|at| at + period_seconds(quota.period)),
            _ => window.reset_at,
        };
        Remaining::Known {
            tokens: quota.tokens.saturating_sub(used),
            reset_at,
        }
    }

    /// Take an allowance for one request, before it is dispatched.
    pub fn reserve(
        &mut self,
        id: &str,
        quota_ids: &[String],
        tokens: u64,
        now: i64,
    ) -> Result<Reservation, LedgerError> {
        if tokens == 0 {
            return Err(LedgerError::EmptyReservation);
        }
        if self.state.reservations.contains_key(id) {
            return Err(LedgerError::DuplicateReservation(id.to_string()));
        }
        let mut seen = BTreeSet::new();
        for quota_id in quota_ids {
            if !seen.insert(quota_id.as_str()) {
                return Err(LedgerError::DuplicateQuotaReference(quota_id.clone()));
            }
            if !self.quotas.contains_key(quota_id) {
                return Err(LedgerError::UnknownQuota(quota_id.clone()));
            }
        }
        // Waits first: a rate-limited provider must not even be reason-checked further.
        for quota_id in quota_ids {
            if let Some(until) = self.wait_until(quota_id, now) {
                return Err(LedgerError::Waiting {
                    scope: quota_id.clone(),
                    until,
                });
            }
        }
        for quota_id in quota_ids {
            match self.remaining(quota_id, now) {
                Remaining::Known { tokens: left, .. } => {
                    if left < tokens {
                        return Err(LedgerError::Exceeded {
                            quota_id: quota_id.clone(),
                            remaining: left,
                            wanted: tokens,
                        });
                    }
                }
                Remaining::Unknown(_) => {
                    return Err(LedgerError::UnknownRemaining(quota_id.clone()));
                }
            }
        }
        let reservation = Reservation {
            id: id.to_string(),
            quota_ids: quota_ids.to_vec(),
            tokens,
            at: now,
            dispatched: false,
        };
        self.state
            .reservations
            .insert(id.to_string(), reservation.clone());
        self.dirty = true;
        Ok(reservation)
    }

    /// Record that the request left the machine, **before** it does.
    pub fn mark_dispatched(&mut self, id: &str) -> Result<(), LedgerError> {
        let reservation = self
            .state
            .reservations
            .get_mut(id)
            .ok_or_else(|| LedgerError::UnknownReservation(id.to_string()))?;
        if !reservation.dispatched {
            reservation.dispatched = true;
            self.dirty = true;
        }
        Ok(())
    }

    /// Reconcile a finished request against what the provider reported.
    ///
    /// `reported = None` means the provider said nothing about usage, which is exactly the case
    /// that must not be treated as free: the reservation is charged in full.
    pub fn settle(&mut self, id: &str, reported: Option<u64>) -> Result<SettleReport, LedgerError> {
        let reservation = self
            .state
            .reservations
            .remove(id)
            .ok_or_else(|| LedgerError::UnknownReservation(id.to_string()))?;
        let usage_missing = reported.is_none();
        // A provider may report more than the reservation; charge what it says.
        let charged = reported.unwrap_or(reservation.tokens);
        for quota_id in &reservation.quota_ids {
            self.state.charges.push(Charge {
                quota_id: quota_id.clone(),
                at: reservation.at,
                tokens: charged,
                reservation: Some(reservation.id.clone()),
            });
        }
        self.dirty = true;
        Ok(SettleReport {
            reservation_id: reservation.id,
            quota_ids: reservation.quota_ids,
            charged,
            usage_missing,
        })
    }

    /// Give back an allowance for a request that was cancelled before it was sent.
    pub fn release(&mut self, id: &str) -> Result<(), LedgerError> {
        let reservation = self
            .state
            .reservations
            .get(id)
            .ok_or_else(|| LedgerError::UnknownReservation(id.to_string()))?;
        if reservation.dispatched {
            return Err(LedgerError::AlreadyDispatched(id.to_string()));
        }
        self.state.reservations.remove(id);
        self.dirty = true;
        Ok(())
    }

    /// Release a reservation for a request the caller has proved produced no output.
    ///
    /// Unlike [`Self::release`], this accepts a reservation that was already marked dispatched, so
    /// the caller must hold evidence: the provider refused the request (a rate-limit answer), or
    /// persisting the dispatch intent failed before anything was sent. Without such evidence the
    /// strict [`Self::release`] applies, because a dispatched request may have been processed.
    pub fn abandon(&mut self, id: &str) -> Result<(), LedgerError> {
        if self.state.reservations.remove(id).is_none() {
            return Err(LedgerError::UnknownReservation(id.to_string()));
        }
        self.dirty = true;
        Ok(())
    }

    /// Reconcile reservations left behind by a crash or a killed process.
    ///
    /// A dispatched request is charged in full (it may have produced output nobody reported), an
    /// undispatched one is released (nothing was sent). Both remove the reservation, so running
    /// this twice charges nothing the second time.
    pub fn reconcile_orphans(&mut self, now: i64, grace: i64) -> Vec<OrphanReport> {
        let stale: Vec<Reservation> = self
            .state
            .reservations
            .values()
            .filter(|reservation| now.saturating_sub(reservation.at) > grace)
            .cloned()
            .collect();
        let mut reports = Vec::new();
        for reservation in stale {
            if reservation.dispatched {
                let charged = reservation.tokens;
                if self.settle(&reservation.id, None).is_ok() {
                    reports.push(OrphanReport {
                        reservation_id: reservation.id,
                        charged: Some(charged),
                    });
                }
            } else if self.release(&reservation.id).is_ok() {
                reports.push(OrphanReport {
                    reservation_id: reservation.id,
                    charged: None,
                });
            }
        }
        reports
    }

    /// Record a provider rate-limit answer for `scope`.
    ///
    /// A numeric delay is used as given; without one the conservative default applies. Provider
    /// text is never parsed here — a trusted caller may pass an already validated reset instead.
    pub fn record_rate_limit(&mut self, scope: &str, retry_after_seconds: Option<u64>, now: i64) {
        let (until, evidence) = match retry_after_seconds {
            Some(seconds) => (
                now.saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX)),
                WaitEvidence::RetryAfter,
            ),
            None => (
                now.saturating_add(UNKNOWN_WAIT_SECONDS),
                WaitEvidence::Unknown,
            ),
        };
        self.record_wait(scope, until, evidence, now);
    }

    /// Record a wait for `scope` that is already `until` in Unix seconds.
    ///
    /// Used for a reset time a trusted caller parsed and validated, and for the numeric case above.
    /// A later wait replaces an earlier one; an earlier one never shortens a later one.
    pub fn record_wait(&mut self, scope: &str, until: i64, evidence: WaitEvidence, now: i64) {
        let keep = self
            .state
            .waits
            .get(scope)
            .is_some_and(|existing| existing.until >= until);
        if keep {
            return;
        }
        self.state.waits.insert(
            scope.to_string(),
            Wait {
                scope: scope.to_string(),
                until,
                evidence,
                at: now,
            },
        );
        self.dirty = true;
    }

    /// The wait in force for `scope` at `now`, if any.
    pub fn wait_until(&self, scope: &str, now: i64) -> Option<i64> {
        self.state
            .waits
            .get(scope)
            .map(|wait| wait.until)
            .filter(|until| *until > now)
    }

    /// The stored wait for `scope`, including one that has already expired (for the explanation).
    pub fn wait(&self, scope: &str) -> Option<&Wait> {
        self.state.waits.get(scope)
    }

    /// Forget the wait for `scope`, after a request to it succeeded.
    pub fn clear_wait(&mut self, scope: &str) {
        if self.state.waits.remove(scope).is_some() {
            self.dirty = true;
        }
    }

    /// Book usage reported from outside RCommander (an invoice, a provider dashboard).
    pub fn record_external_usage(
        &mut self,
        quota_id: &str,
        tokens: u64,
        now: i64,
    ) -> Result<(), LedgerError> {
        let quota = self
            .quotas
            .get(quota_id)
            .ok_or_else(|| LedgerError::UnknownQuota(quota_id.to_string()))?;
        // Usage outside the control of this ledger cannot be placed in a window whose start the
        // provider owns and whose reset was never observed: it would be charged to the wrong period.
        if quota.window == QuotaWindow::ProviderDefined && !self.state.resets.contains_key(quota_id)
        {
            return Err(LedgerError::ProviderWindowWithoutReset(
                quota_id.to_string(),
            ));
        }
        self.state.charges.push(Charge {
            quota_id: quota_id.to_string(),
            at: now,
            tokens,
            reservation: None,
        });
        self.dirty = true;
        Ok(())
    }

    /// Record that a provider confirmed its window restarted at `at`.
    ///
    /// This is the only thing that makes a provider-defined limit computable; until it arrives the
    /// ledger reports the allowance as unknown.
    pub fn record_provider_reset(&mut self, quota_id: &str, at: i64) -> Result<(), LedgerError> {
        if !self.quotas.contains_key(quota_id) {
            return Err(LedgerError::UnknownQuota(quota_id.to_string()));
        }
        let previous = self.state.resets.get(quota_id).copied();
        if previous.is_none_or(|previous| at > previous) {
            self.state.resets.insert(quota_id.to_string(), at);
            self.dirty = true;
        }
        Ok(())
    }

    /// Drop history that can no longer affect any window, and waits that have expired.
    ///
    /// Charges against a provider-defined window are kept until a reset is observed: pruning them
    /// would silently hand back an allowance that may have already been spent.
    pub fn prune(&mut self, now: i64) {
        let cutoff = now.saturating_sub(RETENTION_SECONDS);
        let provider_owned: BTreeSet<String> = self
            .quotas
            .values()
            .filter(|quota| quota.window == QuotaWindow::ProviderDefined)
            .map(|quota| quota.id.clone())
            .collect();
        let before = self.state.charges.len();
        self.state
            .charges
            .retain(|charge| charge.at >= cutoff || provider_owned.contains(&charge.quota_id));
        let expired: Vec<String> = self
            .state
            .waits
            .values()
            .filter(|wait| wait.until <= now)
            .map(|wait| wait.scope.clone())
            .collect();
        for scope in &expired {
            self.state.waits.remove(scope);
        }
        if self.state.charges.len() != before || !expired.is_empty() {
            self.dirty = true;
        }
    }

    /// The window in force for `quota` at `now`, or `None` when it cannot be known.
    fn window(&self, quota: &TokenQuota, now: i64) -> Option<Window> {
        match quota.window {
            QuotaWindow::Rolling => Some(Window {
                start: now.saturating_sub(period_seconds(quota.period)),
                reset_at: None,
            }),
            QuotaWindow::Calendar => Some(Window {
                start: calendar_start(now, quota.period),
                reset_at: Some(calendar_next(now, quota.period)),
            }),
            // The provider owns the reset epoch. Without a verified observation the ledger cannot
            // say what is left, and guessing would spend an allowance it does not understand.
            QuotaWindow::ProviderDefined => self.state.resets.get(&quota.id).map(|at| Window {
                start: *at,
                reset_at: None,
            }),
        }
    }

    /// Charged tokens plus outstanding reservations since `start`.
    fn used_since(&self, quota_id: &str, start: i64) -> u64 {
        let charged = self
            .state
            .charges
            .iter()
            .filter(|charge| charge.quota_id == quota_id && charge.at >= start)
            .fold(0u64, |total, charge| total.saturating_add(charge.tokens));
        let reserved = self
            .state
            .reservations
            .values()
            .filter(|reservation| {
                reservation.at >= start && reservation.quota_ids.iter().any(|id| id == quota_id)
            })
            .fold(0u64, |total, reservation| {
                total.saturating_add(reservation.tokens)
            });
        charged.saturating_add(reserved)
    }

    fn oldest_since(&self, quota_id: &str, start: i64) -> Option<i64> {
        self.state
            .charges
            .iter()
            .filter(|charge| charge.quota_id == quota_id && charge.at >= start)
            .map(|charge| charge.at)
            .min()
    }
}

/// The part of a window the ledger needs: where counting starts and when it rolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Window {
    start: i64,
    reset_at: Option<i64>,
}

/// Length of one rolling window in seconds.
///
/// A rolling month has no calendar answer; thirty days is the documented approximation and the
/// reason [`RETENTION_SECONDS`] is longer than a month.
fn period_seconds(period: QuotaPeriod) -> i64 {
    match period {
        QuotaPeriod::Hour => 3_600,
        QuotaPeriod::Day => 86_400,
        QuotaPeriod::Week => 604_800,
        QuotaPeriod::Month => 30 * 86_400,
    }
}

fn utc(now: i64) -> DateTime<Utc> {
    // Every value a system clock can produce is inside chrono's range; clamping here keeps a
    // corrupt file from panicking a ledger that only wants to count tokens.
    DateTime::from_timestamp(now, 0).unwrap_or(DateTime::UNIX_EPOCH)
}

/// Start of the calendar period containing `now`, in UTC.
///
/// Windows are deliberately UTC: a provider's own time zone is not modelled yet, so claiming a
/// local reset would be a guess. Weeks start on Monday.
fn calendar_start(now: i64, period: QuotaPeriod) -> i64 {
    let at = utc(now);
    let start = match period {
        QuotaPeriod::Hour => at
            .with_minute(0)
            .and_then(|at| at.with_second(0))
            .and_then(|at| at.with_nanosecond(0)),
        QuotaPeriod::Day => at
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|naive| Utc.from_utc_datetime(&naive)),
        QuotaPeriod::Week => (at.date_naive()
            - chrono::Duration::days(i64::from(at.weekday().num_days_from_monday())))
        .and_hms_opt(0, 0, 0)
        .map(|naive| Utc.from_utc_datetime(&naive)),
        QuotaPeriod::Month => chrono::NaiveDate::from_ymd_opt(at.year(), at.month(), 1)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|naive| Utc.from_utc_datetime(&naive)),
    };
    start.map(|value| value.timestamp()).unwrap_or(now)
}

/// Start of the calendar period after the one containing `now`.
fn calendar_next(now: i64, period: QuotaPeriod) -> i64 {
    match period {
        QuotaPeriod::Hour => calendar_start(now, period) + 3_600,
        QuotaPeriod::Day => calendar_start(now, period) + 86_400,
        QuotaPeriod::Week => calendar_start(now, period) + 604_800,
        QuotaPeriod::Month => {
            let at = utc(calendar_start(now, period));
            let (year, month) = if at.month() == 12 {
                (at.year() + 1, 1)
            } else {
                (at.year(), at.month() + 1)
            };
            chrono::NaiveDate::from_ymd_opt(year, month, 1)
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|naive| Utc.from_utc_datetime(&naive).timestamp())
                // A calendar month is at most 31 days; this keeps the answer monotonic if the
                // date arithmetic ever fails.
                .unwrap_or(calendar_start(now, period) + 31 * 86_400)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-27T12:00:00Z — a fixed instant so nothing here depends on the wall clock.
    const NOON: i64 = 1_795_780_800;

    fn hourly(tokens: u64) -> TokenQuota {
        TokenQuota {
            id: "hour".into(),
            tokens,
            period: QuotaPeriod::Hour,
            window: QuotaWindow::Rolling,
        }
    }

    fn monthly(tokens: u64) -> TokenQuota {
        TokenQuota {
            id: "month".into(),
            tokens,
            period: QuotaPeriod::Month,
            window: QuotaWindow::Calendar,
        }
    }

    fn ledger() -> QuotaLedger {
        QuotaLedger::new(&[hourly(1_000), monthly(10_000)]).unwrap()
    }

    fn ids(quotas: &[&str]) -> Vec<String> {
        quotas.iter().map(|id| (*id).to_string()).collect()
    }

    fn known(remaining: Remaining) -> (u64, Option<i64>) {
        match remaining {
            Remaining::Known { tokens, reset_at } => (tokens, reset_at),
            other => panic!("expected a known allowance, got {other:?}"),
        }
    }

    #[test]
    fn a_reservation_holds_allowance_so_two_jobs_cannot_spend_it_twice() {
        let mut ledger = ledger();
        ledger.reserve("job-1", &ids(&["hour"]), 600, NOON).unwrap();
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 400);
        let refused = ledger
            .reserve("job-2", &ids(&["hour"]), 600, NOON)
            .unwrap_err();
        assert_eq!(
            refused,
            LedgerError::Exceeded {
                quota_id: "hour".into(),
                remaining: 400,
                wanted: 600,
            },
            "the second job must see the first job's reservation"
        );
        // A smaller job still fits.
        ledger.reserve("job-3", &ids(&["hour"]), 400, NOON).unwrap();
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 0);
    }

    #[test]
    fn a_shared_quota_is_charged_by_every_model_that_uses_it() {
        let mut ledger = ledger();
        // Two different requests, one account pool: the second one sees the first charge.
        ledger.reserve("a", &ids(&["hour"]), 300, NOON).unwrap();
        ledger.settle("a", Some(250)).unwrap();
        ledger.reserve("b", &ids(&["hour"]), 300, NOON).unwrap();
        ledger.settle("b", Some(300)).unwrap();
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 1_000 - 550);
        assert_eq!(ledger.charges("hour").len(), 2);
    }

    #[test]
    fn a_rolling_window_returns_the_allowance_after_it_rolls() {
        let mut ledger = ledger();
        ledger.reserve("a", &ids(&["hour"]), 1_000, NOON).unwrap();
        ledger.settle("a", Some(1_000)).unwrap();
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 0);
        // Still inside the hour.
        assert!(matches!(
            ledger.reserve("b", &ids(&["hour"]), 10, NOON + 3_599),
            Err(LedgerError::Exceeded { .. })
        ));
        // One second past it, the allowance is back.
        assert_eq!(known(ledger.remaining("hour", NOON + 3_601)).0, 1_000);
        ledger
            .reserve("b", &ids(&["hour"]), 1_000, NOON + 3_601)
            .unwrap();
    }

    #[test]
    fn a_rolling_window_reports_when_its_oldest_charge_ages_out() {
        let mut ledger = ledger();
        ledger.reserve("a", &ids(&["hour"]), 100, NOON).unwrap();
        ledger.settle("a", Some(100)).unwrap();
        let (_, reset_at) = known(ledger.remaining("hour", NOON));
        assert_eq!(reset_at, Some(NOON + 3_600));
    }

    #[test]
    fn a_calendar_limit_resets_on_the_period_boundary() {
        let mut ledger = ledger();
        ledger.reserve("a", &ids(&["month"]), 9_500, NOON).unwrap();
        ledger.settle("a", Some(9_500)).unwrap();
        let (left, reset_at) = known(ledger.remaining("month", NOON));
        assert_eq!(left, 500);
        // 2026-12-01T00:00:00Z
        assert_eq!(reset_at, Some(1_796_083_200));
        // The last second of November still belongs to November.
        let last = 1_796_083_199;
        assert_eq!(known(ledger.remaining("month", last)).0, 500);
        // December starts a new window with the full limit.
        assert_eq!(known(ledger.remaining("month", 1_796_083_200)).0, 10_000);
    }

    #[test]
    fn a_calendar_hour_and_day_align_to_utc_boundaries() {
        // 2026-11-27T12:34:56Z
        let now = NOON + 34 * 60 + 56;
        assert_eq!(calendar_start(now, QuotaPeriod::Hour), NOON);
        assert_eq!(calendar_next(now, QuotaPeriod::Hour), NOON + 3_600);
        assert_eq!(calendar_start(now, QuotaPeriod::Day), NOON - 12 * 3_600);
        assert_eq!(
            calendar_start(now, QuotaPeriod::Week),
            // Monday 2026-11-23T00:00:00Z
            1_795_392_000
        );
        assert_eq!(
            calendar_start(now, QuotaPeriod::Month),
            NOON - 27 * 86_400 + 12 * 3_600
        );
    }

    #[test]
    fn a_provider_defined_limit_is_unknown_until_a_reset_is_observed() {
        let quotas = [TokenQuota {
            id: "pool".into(),
            tokens: 5_000,
            period: QuotaPeriod::Month,
            window: QuotaWindow::ProviderDefined,
        }];
        let mut ledger = QuotaLedger::new(&quotas).unwrap();
        assert_eq!(
            ledger.remaining("pool", NOON),
            Remaining::Unknown(UnknownRemaining::ProviderDefinedWithoutReset)
        );
        // Unknown is not unlimited: a request is refused instead of assumed free.
        assert_eq!(
            ledger.reserve("a", &ids(&["pool"]), 10, NOON).unwrap_err(),
            LedgerError::UnknownRemaining("pool".into())
        );
        ledger.record_provider_reset("pool", NOON).unwrap();
        assert_eq!(known(ledger.remaining("pool", NOON)).0, 5_000);
        ledger
            .reserve("a", &ids(&["pool"]), 4_000, NOON + 1)
            .unwrap();
        ledger.settle("a", Some(4_000)).unwrap();
        assert_eq!(known(ledger.remaining("pool", NOON + 2)).0, 1_000);
    }

    #[test]
    fn missing_usage_is_charged_in_full_and_reported_usage_is_trusted() {
        let mut ledger = ledger();
        ledger.reserve("a", &ids(&["hour"]), 500, NOON).unwrap();
        let settled = ledger.settle("a", None).unwrap();
        assert_eq!(settled.charged, 500);
        assert!(settled.usage_missing);
        ledger.reserve("b", &ids(&["hour"]), 500, NOON).unwrap();
        let settled = ledger.settle("b", Some(120)).unwrap();
        assert_eq!(settled.charged, 120);
        assert!(!settled.usage_missing);
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 1_000 - 620);
    }

    #[test]
    fn a_cancelled_request_returns_its_allowance_but_a_dispatched_one_does_not() {
        let mut ledger = ledger();
        ledger
            .reserve("cancelled", &ids(&["hour"]), 400, NOON)
            .unwrap();
        ledger.release("cancelled").unwrap();
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 1_000);
        assert!(ledger.reservation("cancelled").is_none());

        ledger.reserve("sent", &ids(&["hour"]), 400, NOON).unwrap();
        ledger.mark_dispatched("sent").unwrap();
        assert_eq!(
            ledger.release("sent").unwrap_err(),
            LedgerError::AlreadyDispatched("sent".into())
        );
        assert!(ledger.reservation("sent").is_some());
    }

    #[test]
    fn a_dispatched_reservation_is_only_abandoned_with_evidence() {
        let mut ledger = ledger();
        ledger
            .reserve("refused", &ids(&["hour"]), 300, NOON)
            .unwrap();
        ledger.mark_dispatched("refused").unwrap();
        // A plain release refuses a dispatched request: it may have been processed.
        assert!(ledger.release("refused").is_err());
        // A caller that holds evidence (the provider refused it) may abandon it.
        ledger.abandon("refused").unwrap();
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 1_000);
        assert!(ledger.reservation("refused").is_none());
        assert_eq!(
            ledger.abandon("refused").unwrap_err(),
            LedgerError::UnknownReservation("refused".into())
        );
        assert!(
            ledger.is_dirty(),
            "an abandoned allowance is a state change worth saving"
        );
    }

    #[test]
    fn an_interrupted_attempt_is_charged_once_after_the_grace_period() {
        // The monthly calendar window is used on purpose: the grace period is longer than an
        // hourly rolling window, so an hourly charge would already have rolled out of the sum.
        let mut ledger = ledger();
        ledger.reserve("sent", &ids(&["month"]), 700, NOON).unwrap();
        ledger.mark_dispatched("sent").unwrap();
        ledger
            .reserve("never-sent", &ids(&["month"]), 200, NOON)
            .unwrap();

        // Inside the grace period nothing is touched: the worker may still be alive.
        assert!(
            ledger
                .reconcile_orphans(NOON + 60, ORPHAN_GRACE_SECONDS)
                .is_empty()
        );
        assert_eq!(ledger.outstanding().len(), 2);

        let reports =
            ledger.reconcile_orphans(NOON + ORPHAN_GRACE_SECONDS + 1, ORPHAN_GRACE_SECONDS);
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].reservation_id, "never-sent");
        assert_eq!(
            reports[0].charged, None,
            "a request that never left is released"
        );
        assert_eq!(reports[1].reservation_id, "sent");
        assert_eq!(reports[1].charged, Some(700));
        assert_eq!(
            known(ledger.remaining("month", NOON + ORPHAN_GRACE_SECONDS + 2)).0,
            9_300
        );

        // Running it again charges nothing: the reservations are gone.
        assert!(
            ledger
                .reconcile_orphans(NOON + 100_000, ORPHAN_GRACE_SECONDS)
                .is_empty()
        );
        assert_eq!(
            known(ledger.remaining("month", NOON + ORPHAN_GRACE_SECONDS + 2)).0,
            9_300
        );
    }

    #[test]
    fn a_numeric_retry_after_blocks_the_quota_until_it_expires() {
        let mut ledger = ledger();
        ledger.record_rate_limit("hour", Some(90), NOON);
        let wait = ledger.wait("hour").cloned().unwrap();
        assert_eq!(wait.until, NOON + 90);
        assert_eq!(wait.evidence, WaitEvidence::RetryAfter);
        assert_eq!(
            ledger
                .reserve("a", &ids(&["hour"]), 10, NOON + 1)
                .unwrap_err(),
            LedgerError::Waiting {
                scope: "hour".into(),
                until: NOON + 90,
            }
        );
        assert_eq!(ledger.wait_until("hour", NOON + 90), None);
        ledger.reserve("a", &ids(&["hour"]), 10, NOON + 90).unwrap();
    }

    #[test]
    fn a_rate_limit_without_a_delay_uses_the_conservative_backoff_and_never_shortens_a_wait() {
        let mut ledger = ledger();
        ledger.record_rate_limit("hour", None, NOON);
        assert_eq!(
            ledger.wait("hour").unwrap().until,
            NOON + UNKNOWN_WAIT_SECONDS
        );
        assert_eq!(ledger.wait("hour").unwrap().evidence, WaitEvidence::Unknown);
        // A later, longer answer wins …
        ledger.record_rate_limit("hour", Some(600), NOON + 10);
        assert_eq!(ledger.wait("hour").unwrap().until, NOON + 610);
        // … and a shorter one does not cut it short.
        ledger.record_rate_limit("hour", Some(5), NOON + 20);
        assert_eq!(ledger.wait("hour").unwrap().until, NOON + 610);
        ledger.clear_wait("hour");
        assert!(ledger.wait("hour").is_none());
    }

    #[test]
    fn a_successful_request_can_clear_the_wait_and_a_model_scope_is_separate() {
        let mut ledger = ledger();
        ledger.record_rate_limit("model:local", Some(30), NOON);
        assert_eq!(ledger.wait_until("model:local", NOON), Some(NOON + 30));
        // The quota itself is not blocked by a model-scope answer.
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 1_000);
        ledger.reserve("a", &ids(&["hour"]), 10, NOON).unwrap();
    }

    #[test]
    fn external_usage_is_booked_as_a_charge_without_a_reservation() {
        let mut ledger = ledger();
        ledger.record_external_usage("month", 4_000, NOON).unwrap();
        let charges = ledger.charges("month");
        assert_eq!(charges.len(), 1);
        assert_eq!(charges[0].reservation, None);
        assert_eq!(known(ledger.remaining("month", NOON)).0, 6_000);
    }

    #[test]
    fn external_usage_for_a_provider_window_needs_an_observed_reset() {
        let quotas = [TokenQuota {
            id: "pool".into(),
            tokens: 1_000,
            period: QuotaPeriod::Month,
            window: QuotaWindow::ProviderDefined,
        }];
        let mut ledger = QuotaLedger::new(&quotas).unwrap();
        assert_eq!(
            ledger.record_external_usage("pool", 10, NOON).unwrap_err(),
            LedgerError::ProviderWindowWithoutReset("pool".into())
        );
        ledger.record_provider_reset("pool", NOON).unwrap();
        ledger.record_external_usage("pool", 10, NOON).unwrap();
        assert_eq!(known(ledger.remaining("pool", NOON)).0, 990);
    }

    #[test]
    fn a_reservation_needs_a_positive_allowance_and_known_quotas() {
        let mut ledger = ledger();
        assert_eq!(
            ledger
                .reserve("zero", &ids(&["hour"]), 0, NOON)
                .unwrap_err(),
            LedgerError::EmptyReservation
        );
        assert_eq!(
            ledger
                .reserve("unknown", &ids(&["nope"]), 10, NOON)
                .unwrap_err(),
            LedgerError::UnknownQuota("nope".into())
        );
        assert_eq!(
            ledger
                .reserve("twice", &ids(&["hour", "hour"]), 10, NOON)
                .unwrap_err(),
            LedgerError::DuplicateQuotaReference("hour".into())
        );
        ledger.reserve("a", &ids(&["hour"]), 10, NOON).unwrap();
        assert_eq!(
            ledger.reserve("a", &ids(&["hour"]), 10, NOON).unwrap_err(),
            LedgerError::DuplicateReservation("a".into())
        );
        // A model without any configured limit has nothing to charge against, which is allowed.
        ledger.reserve("free", &[], 10, NOON).unwrap();
    }

    #[test]
    fn the_ledger_survives_a_restart_and_keeps_history_of_removed_quotas() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quota.toml");
        let quotas = vec![hourly(1_000), monthly(10_000)];
        {
            let mut ledger = QuotaLedger::load(&path, &quotas).unwrap();
            ledger.reserve("a", &ids(&["hour"]), 400, NOON).unwrap();
            ledger.settle("a", Some(400)).unwrap();
            ledger
                .reserve("running", &ids(&["hour", "month"]), 100, NOON)
                .unwrap();
            ledger.mark_dispatched("running").unwrap();
            ledger.record_rate_limit("model:local", Some(120), NOON);
            ledger.save().unwrap();
            assert!(!ledger.is_dirty());
        }
        let mut reopened = QuotaLedger::load(&path, &quotas).unwrap();
        assert_eq!(known(reopened.remaining("hour", NOON)).0, 1_000 - 500);
        assert!(reopened.reservation("running").unwrap().dispatched);
        assert_eq!(reopened.wait_until("model:local", NOON), Some(NOON + 120));
        // Settling the reservation that was outstanding across the restart still works.
        assert_eq!(reopened.settle("running", Some(100)).unwrap().charged, 100);
        assert_eq!(known(reopened.remaining("hour", NOON)).0, 1_000 - 500);
        assert_eq!(known(reopened.remaining("month", NOON)).0, 10_000 - 100);
        reopened.save().unwrap();

        // A catalogue that lost the hourly limit keeps the charges, so re-adding it restores them.
        let narrower = vec![monthly(10_000)];
        let kept = QuotaLedger::load(&path, &narrower).unwrap();
        assert_eq!(
            kept.remaining("hour", NOON),
            Remaining::Unknown(UnknownRemaining::NotConfigured)
        );
        assert_eq!(kept.charges("hour").len(), 2);
        let restored = QuotaLedger::load(&path, &quotas).unwrap();
        assert_eq!(known(restored.remaining("hour", NOON)).0, 1_000 - 500);
    }

    #[test]
    fn a_ledger_is_loaded_from_the_configured_directory_of_its_catalogue() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = crate::model_catalog::ModelCatalog {
            quotas: vec![hourly(500)],
            ..crate::model_catalog::ModelCatalog::default()
        };
        let mut ledger = QuotaLedger::load_in(dir.path(), &catalog).unwrap();
        assert_eq!(
            ledger.path(),
            Some(dir.path().join(LEDGER_FILE).as_path()),
            "the ledger lives beside the settings of the same profile"
        );
        ledger.reserve("a", &ids(&["hour"]), 100, NOON).unwrap();
        ledger.settle("a", Some(100)).unwrap();
        ledger.save().unwrap();
        let reopened = QuotaLedger::load_in(dir.path(), &catalog).unwrap();
        assert_eq!(known(reopened.remaining("hour", NOON)).0, 400);
    }

    #[test]
    fn a_newer_ledger_file_is_refused_instead_of_guessed_at() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quota.toml");
        std::fs::write(&path, format!("version = {}\n", STATE_VERSION + 1)).unwrap();
        let error = QuotaLedger::load(&path, &[hourly(10)]).unwrap_err();
        assert!(
            error.to_string().contains("newer ledger"),
            "message: {error}"
        );
    }

    #[test]
    fn a_ledger_that_is_not_toml_is_refused_with_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quota.toml");
        std::fs::write(&path, "this is not a ledger").unwrap();
        let error = QuotaLedger::load(&path, &[hourly(10)]).unwrap_err();
        assert!(error.to_string().contains("quota.toml"), "message: {error}");
    }

    #[test]
    fn a_failed_save_leaves_the_previous_ledger_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quota.toml");
        let quotas = vec![hourly(1_000)];
        let mut ledger = QuotaLedger::load(&path, &quotas).unwrap();
        ledger.reserve("a", &ids(&["hour"]), 100, NOON).unwrap();
        ledger.settle("a", Some(100)).unwrap();
        ledger.save().unwrap();
        // A directory where the file should be makes the replacement fail after the temp write.
        let blocked = dir.path().join("blocked.toml");
        std::fs::create_dir(&blocked).unwrap();
        let mut second = QuotaLedger::load(dir.path().join("fresh.toml"), &quotas).unwrap();
        second.set_path(&blocked);
        second.reserve("b", &ids(&["hour"]), 100, NOON).unwrap();
        assert!(second.save().is_err());
        assert!(
            second.is_dirty(),
            "a failed save keeps the state marked dirty"
        );
        // The ledger that was written earlier still reads back unchanged.
        let reopened = QuotaLedger::load(&path, &quotas).unwrap();
        assert_eq!(known(reopened.remaining("hour", NOON)).0, 900);
    }

    #[test]
    fn pruning_keeps_what_a_window_still_counts_and_forgets_the_rest() {
        let quotas = [
            hourly(1_000),
            TokenQuota {
                id: "pool".into(),
                tokens: 1_000,
                period: QuotaPeriod::Month,
                window: QuotaWindow::ProviderDefined,
            },
        ];
        let mut ledger = QuotaLedger::new(&quotas).unwrap();
        let ancient = NOON - RETENTION_SECONDS - 10;
        // The provider reset before the ancient charge, so that charge is inside its window.
        ledger.record_provider_reset("pool", ancient - 100).unwrap();
        ledger.reserve("old", &ids(&["hour"]), 50, ancient).unwrap();
        ledger.settle("old", Some(50)).unwrap();
        ledger
            .reserve("pool-old", &ids(&["pool"]), 60, ancient)
            .unwrap();
        ledger.settle("pool-old", Some(60)).unwrap();
        ledger.record_rate_limit("hour", Some(10), NOON);
        ledger.prune(NOON + 20);
        assert!(
            ledger.charges("hour").is_empty(),
            "a rolled window is dropped"
        );
        assert_eq!(
            ledger.charges("pool").len(),
            1,
            "a provider window keeps its history until the provider says it reset"
        );
        assert!(ledger.wait("hour").is_none(), "an expired wait is dropped");
        assert_eq!(known(ledger.remaining("pool", NOON)).0, 940);
    }

    #[test]
    fn a_quota_set_can_change_without_losing_charges() {
        let mut ledger = ledger();
        ledger.reserve("a", &ids(&["hour"]), 100, NOON).unwrap();
        ledger.settle("a", Some(100)).unwrap();
        ledger
            .set_quotas(&[hourly(2_000), monthly(10_000)])
            .unwrap();
        assert!(ledger.is_dirty());
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 1_900);
        let error = ledger
            .set_quotas(&[TokenQuota {
                id: "hour".into(),
                tokens: 0,
                period: QuotaPeriod::Hour,
                window: QuotaWindow::Rolling,
            }])
            .unwrap_err();
        assert!(
            error.to_string().contains("allows no tokens"),
            "message: {error}"
        );
        // The refused set did not replace the working one.
        assert_eq!(known(ledger.remaining("hour", NOON)).0, 1_900);
    }
}

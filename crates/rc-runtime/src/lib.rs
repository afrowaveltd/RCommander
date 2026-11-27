//! `rc-runtime` — the headless execution boundary (Phase 25.5, first slice).
//!
//! Nothing here needs a window system, an event loop or a GUI thread: this is where a request
//! actually meets the owner's budget. The first slice is deliberately one vertical: **model
//! dispatch**, which until now was a contract with no consumer.
//!
//! Why a separate crate: `rc-core` is the platform-independent engine and stays free of network
//! clients, `rc-llm` knows how to talk to a model server and nothing about budgets, and the app
//! crate owns a window. The orchestration that has to happen *between* them — resolve, admit,
//! reserve, persist intent, call, reconcile — belongs in none of those, so it lives here and can
//! be tested with a fake backend and no socket.
//!
//! The rules the dispatcher enforces, in order:
//!
//! 1. a model that cannot be resolved is never dialed ([`rc_core::providers::resolve`]);
//! 2. a request that is not admitted never reaches a backend, and a refusal performs no
//!    reservation and no call ([`rc_core::admission::admit`]);
//! 3. the allowance is reserved **and its dispatch intent persisted** before the call, so a crash
//!    leaves evidence rather than a silently returned allowance;
//! 4. the answer reconciles against reported usage, and unreported usage is charged in full;
//! 5. a provider's rate-limit answer is not usage: the reservation is released and the wait it
//!    announced is recorded, so the next request waits instead of retrying into the wall;
//! 6. if the intent cannot be persisted, the request is not sent at all.

pub mod dispatch;

pub use dispatch::{Backend, DispatchOutcome, DispatchRequest, Dispatcher, LlmBackend, Receipt};

//! Deterministic authorization for AETHER's four independent enforcement points.
//!
//! This crate deliberately contains no network, database, or model dependencies.
//! Callers load current session/grant/caps state, invoke the policy, and persist
//! the returned audit record through their own audit transport.

mod action;
mod audit;
mod authn;
mod caps;
mod enforcement;
mod grant;
mod policy;
mod step_up;
mod tier;

pub use action::Action;
pub use audit::{AuditRecord, AuditSink, AuditedDecision, MemoryAuditSink};
pub use authn::{
    hash_session_token, verify_totp, AuthnError, LockoutState, PasswordPolicy, SecretSessionToken,
    SessionRecord, SessionTokenPair,
};
pub use caps::{CapsDiff, CapsError, CapsLimits, CapsSnapshot, CapsStore, CapsVersionState};
pub use enforcement::{enforce_at, EnforcementPoint};
pub use grant::{default_grant_lifetime_secs, Grant, GrantError, GrantStore};
pub use policy::{evaluate, Actor, ActorKind, Decision, EvaluationContext, Verdict};
pub use step_up::{StepUpError, StepUpStore, TotpVerifier};
pub use tier::{Tier, TierError};

/// Unix time in seconds. Services convert their clock type at the boundary.
pub type UnixSeconds = u64;

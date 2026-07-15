//! Order router — the single entry point for all order submission.
//!
//! Routes intents through authorization and risk checks to the paper ledger
//! or dispatches to venue adapters for live execution.

pub mod adapter;
pub mod breaker;
pub mod ceremony;
pub mod mismatch;
pub mod reconciler;
pub mod router;

pub use adapter::{
    AdapterError, AdapterRegistry, ReadOnlyAdapter, SandboxVenueAdapter, VenueAdapterClient,
    VenueBalances, VenueOrderObservation, VenueOrderResult,
};
pub use breaker::{BreakerConfig, BreakerState, RouterBreakers};
pub use mismatch::{MismatchConfig, MismatchEntry, MismatchError};
pub use reconciler::{ReconciledStatus, Reconciler, ReconcilerConfig, VenueObservation};
pub use router::{
    ExecutionAuditError, ExecutionAuditEvent, ExecutionAuditSink, MemoryExecutionAuditSink,
    OrderRouter, ReconciliationUpdate, RouterAuthContext, RouterConfig, RouterError, RouterResult,
};

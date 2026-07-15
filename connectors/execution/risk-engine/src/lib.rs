//! Deterministic risk engine for order intent evaluation.
//!
//! Evaluates every OrderIntent against seven checks before execution.
//! Pure function -- no clock reads, network, database, metrics, or audit side effects.

pub mod checks;
pub mod engine;

pub use engine::{PositionOutcome, RiskContext, RiskEngine, VenueHealthStatus};

//! Paper trading ledger service.
//!
//! Accepts paper OrderIntents, fills them against current order books
//! using the shared aether-fillmodel crate, and maintains paper-segregated
//! orders, fills, positions, and P&L.

pub mod ledger;
pub mod persistence;
pub mod pnl;
pub mod positions;
pub mod service;

pub use ledger::{PaperLedger, Submission};
pub use persistence::{AttributionClose, AttributionComponents, PostgresLedgerStore};
pub use pnl::PnLCalculator;
pub use positions::PositionTracker;
pub use service::PaperExecutionService;

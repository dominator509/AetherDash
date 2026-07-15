//! AETHER Terminal — Opportunity Scanner (EP-307 M1).
//!
//! The scanner consumes quote streams, applies the decomposition engine
//! (aether-decompose), and emits Opportunity messages for the order
//! router.  V1 is a structural scaffold; the scan loop will be added in
//! EP-307 M2.

pub mod cadence;
pub mod dedupe;
pub mod detect;
pub mod scan;
pub mod score;

pub use cadence::CadenceController;
pub use dedupe::Deduplicator;
pub use detect::{DetectedOpportunity, Detector, DetectorConfig};
pub use scan::{ScanConfig, ScanError, ScanOutcome, Scanner};
pub use score::{ScoredOpportunity, Scorer};

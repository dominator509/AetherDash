//! AETHER Risk Engine -- binary entry point.
//!
//! The risk engine is typically consumed as a library by the order router.
//! This binary exists for standalone smoke tests and diagnostics.

use aether_risk_engine::engine::{RiskConfig, RiskEngine};

fn main() {
    let engine = RiskEngine::new(RiskConfig::default());
    println!("AETHER Risk Engine v{} (pilot: deterministic)", env!("CARGO_PKG_VERSION"));
    println!("  max_drift:          {}", engine.config().max_drift);
    println!("  max_quote_staleness: {}ms", engine.config().max_quote_staleness_ms);
    println!("  hard_per_order_max: {}", engine.config().hard_per_order_max);
    println!("  max_future_skew_ms: {}", engine.config().max_future_skew_ms);
    println!("Engine ready.");
}

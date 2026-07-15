//! Scanner types and scan orchestration.
//!
//! V1: types + structural scaffold. The event-loop and quote-wiring
//! will be added in EP-307 M2.

use aether_decompose::decompose::DecompositionContext;
use rust_decimal::Decimal;

/// Configuration for the opportunity scanner.
pub struct ScanConfig {
    /// Maximum number of venue pairs to evaluate per cycle.
    pub max_pairs_per_cycle: usize,
    /// Minimum net edge (as a fraction) to emit an opportunity.
    pub min_net_edge: Decimal,
    /// Base decomposition context applied to all scans.
    pub base_context: DecompositionContext,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_pairs_per_cycle: 100,
            min_net_edge: Decimal::new(1, 3), // 0.001 = 10bps minimum
            base_context: DecompositionContext::default(),
        }
    }
}

impl std::fmt::Debug for ScanConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanConfig")
            .field("max_pairs_per_cycle", &self.max_pairs_per_cycle)
            .field("min_net_edge", &self.min_net_edge)
            .field("base_context", &format_args!("DecompositionContext(...)"))
            .finish()
    }
}

/// Outcome of a single scan cycle.
#[derive(Debug, Clone)]
pub struct ScanOutcome {
    /// Number of venue pairs evaluated.
    pub pairs_evaluated: usize,
    /// Number of opportunities found above the minimum edge.
    pub opportunities_found: usize,
}

/// Errors from the scanner.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("no quote data available for market pair")]
    NoQuoteData,
    #[error("decomposition error: {0}")]
    Decompose(String),
    #[error("internal scanner error: {0}")]
    Internal(String),
}

/// The opportunity scanner.
///
/// V1: structural scaffold only.  The event-loop driver is deferred
/// to EP-307 M2.
#[derive(Debug)]
pub struct Scanner {
    config: ScanConfig,
}

impl Scanner {
    /// Create a new scanner with the given configuration.
    pub fn new(config: ScanConfig) -> Self {
        Self { config }
    }

    /// Access the scanner configuration.
    pub fn config(&self) -> &ScanConfig {
        &self.config
    }

    /// Run one scan cycle (stub — V2 will wire the quote feed).
    pub fn scan_once(&self) -> ScanOutcome {
        // V1 stub: returns a no-op outcome.
        // EP-307 M2 will implement the event loop.
        ScanOutcome { pairs_evaluated: 0, opportunities_found: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_config_default() {
        let cfg = ScanConfig::default();
        assert_eq!(cfg.max_pairs_per_cycle, 100);
        assert_eq!(cfg.min_net_edge, Decimal::new(1, 3));
    }

    #[test]
    fn scanner_scan_once_returns_zeros() {
        let scanner = Scanner::new(ScanConfig::default());
        let outcome = scanner.scan_once();
        assert_eq!(outcome.pairs_evaluated, 0);
        assert_eq!(outcome.opportunities_found, 0);
    }
}

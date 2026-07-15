//! Mismatch configuration for settlement oracle discrepancies.
//!
//! A `mismatch.toml` file defines discount factors per venue pair to
//! account for oracle/settlement mismatches between venues.  V1 provides
//! the config model with conservative defaults; the TOML-file loader
//! is a placeholder for future wiring.

use rust_decimal::Decimal;
use std::collections::HashMap;

/// Configuration for settlement mismatch discounts per venue pair.
///
/// # Example TOML (future wire-up)
/// ```toml
/// [pairs]
/// "kalshi:polymarket" = 0.01
/// "kalshi:hyperliquid" = 0.02
///
/// [defaults]
/// discount = 0.005
/// ```
#[derive(Debug, Clone)]
pub struct MismatchConfig {
    /// Discount factor per venue pair keyed as `"venue_a:venue_b"` (sorted).
    pub pair_discounts: HashMap<String, Decimal>,
    /// Default discount when no specific pair is configured.
    pub default_discount: Decimal,
}

impl Default for MismatchConfig {
    fn default() -> Self {
        Self {
            pair_discounts: HashMap::new(),
            default_discount: Decimal::ZERO,
        }
    }
}

impl MismatchConfig {
    /// Create a new config with the given pair discounts and default.
    pub fn new(pair_discounts: HashMap<String, Decimal>, default_discount: Decimal) -> Self {
        Self {
            pair_discounts,
            default_discount,
        }
    }

    /// Look up the mismatch discount for a venue pair.
    /// Checks both `"a:b"` and `"b:a"` orderings.
    pub fn discount_for(&self, venue_a: &str, venue_b: &str) -> Decimal {
        let key_ab = format!("{}:{}", venue_a, venue_b);
        let key_ba = format!("{}:{}", venue_b, venue_a);
        self.pair_discounts
            .get(&key_ab)
            .or_else(|| self.pair_discounts.get(&key_ba))
            .copied()
            .unwrap_or(self.default_discount)
    }

}

/// Errors from loading mismatch configuration (reserved for V2 TOML loader).
#[derive(Debug, thiserror::Error)]
pub enum MismatchLoadError {
    /// TOML loading is not yet implemented in V1.
    #[error("TOML mismatch config loader not implemented in V1")]
    NotImplemented,
    #[error("parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatch_default_is_zero() {
        let cfg = MismatchConfig::default();
        assert_eq!(cfg.discount_for("kalshi", "polymarket"), Decimal::ZERO);
    }

    #[test]
    fn mismatch_discount_lookup() {
        let mut discounts = HashMap::new();
        discounts.insert("kalshi:polymarket".into(), Decimal::new(1, 2));
        let cfg = MismatchConfig::new(discounts, Decimal::ZERO);
        assert_eq!(cfg.discount_for("kalshi", "polymarket"), Decimal::new(1, 2));
    }

    #[test]
    fn mismatch_discount_reverse_lookup() {
        let mut discounts = HashMap::new();
        discounts.insert("polymarket:kalshi".into(), Decimal::new(2, 2));
        let cfg = MismatchConfig::new(discounts, Decimal::ZERO);
        // Should find "kalshi:polymarket" via "polymarket:kalshi"
        assert_eq!(cfg.discount_for("kalshi", "polymarket"), Decimal::new(2, 2));
    }

    #[test]
    fn mismatch_discount_falls_back_to_default() {
        let cfg = MismatchConfig::new(HashMap::new(), Decimal::new(5, 3));
        assert_eq!(cfg.discount_for("kalshi", "polymarket"), Decimal::new(5, 3));
    }
}

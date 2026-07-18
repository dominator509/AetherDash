//! Mismatch configuration for settlement oracle discrepancies.
//!
//! A `mismatch.toml` file defines discount factors per venue pair to
//! account for oracle/settlement mismatches between venues.

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;

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
        Self { pair_discounts: HashMap::new(), default_discount: Decimal::ZERO }
    }
}

impl MismatchConfig {
    /// Create a new config with the given pair discounts and default.
    pub fn new(pair_discounts: HashMap<String, Decimal>, default_discount: Decimal) -> Self {
        Self { pair_discounts, default_discount }
    }

    /// Look up the mismatch discount for a venue pair.
    /// Checks both `"a:b"` and `"b:a"` orderings.
    pub fn discount_for(&self, venue_a: &str, venue_b: &str) -> Decimal {
        if venue_a == venue_b {
            return Decimal::ZERO;
        }
        let key_ab = format!("{}:{}", venue_a, venue_b);
        let key_ba = format!("{}:{}", venue_b, venue_a);
        self.pair_discounts
            .get(&key_ab)
            .or_else(|| self.pair_discounts.get(&key_ba))
            .copied()
            .unwrap_or(self.default_discount)
    }

    /// Load and validate the repository-owned mismatch table used by the
    /// order router. Keeping a single embedded source prevents scanner/router
    /// settlement-risk drift.
    pub fn load_embedded() -> Result<Self, MismatchLoadError> {
        Self::from_toml(include_str!("../../../connectors/execution/order-router/mismatch.toml"))
    }

    pub fn from_toml(raw: &str) -> Result<Self, MismatchLoadError> {
        let parsed: toml::Value =
            toml::from_str(raw).map_err(|error| MismatchLoadError::Parse(error.to_string()))?;
        let table = parsed
            .as_table()
            .ok_or_else(|| MismatchLoadError::Parse("top level must be a table".into()))?;
        let mut discounts = HashMap::new();
        let mut default_discount = None;

        for (key, value) in table {
            let raw_discount = value
                .get("discount")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| MismatchLoadError::MissingDiscount(key.clone()))?;
            let discount = Decimal::from_str(raw_discount).map_err(|error| {
                MismatchLoadError::InvalidDiscount(key.clone(), error.to_string())
            })?;
            if !(Decimal::ZERO..=Decimal::ONE).contains(&discount) {
                return Err(MismatchLoadError::OutOfRange(key.clone()));
            }
            if key == "_default" {
                default_discount = Some(discount);
                continue;
            }
            let (left, right) =
                key.split_once('-').ok_or_else(|| MismatchLoadError::InvalidPair(key.clone()))?;
            discounts.insert(format!("{left}:{right}"), discount);
        }

        let default_discount = default_discount.ok_or(MismatchLoadError::MissingDefault)?;
        for (key, discount) in &discounts {
            let (left, right) =
                key.split_once(':').ok_or_else(|| MismatchLoadError::InvalidPair(key.clone()))?;
            if left == right {
                if *discount != Decimal::ZERO {
                    return Err(MismatchLoadError::SameSourceNonZero(key.clone()));
                }
                continue;
            }
            let reverse = format!("{right}:{left}");
            if discounts.get(&reverse) != Some(discount) {
                return Err(MismatchLoadError::AsymmetricPair(key.clone(), reverse));
            }
        }

        Ok(Self::new(discounts, default_discount))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MismatchLoadError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("mismatch entry '{0}' has no discount string")]
    MissingDiscount(String),
    #[error("mismatch entry '{0}' has invalid discount: {1}")]
    InvalidDiscount(String, String),
    #[error("mismatch entry '{0}' discount must be between 0 and 1")]
    OutOfRange(String),
    #[error("mismatch entry '{0}' is not a venue pair")]
    InvalidPair(String),
    #[error("mismatch table has no _default entry")]
    MissingDefault,
    #[error("same-source mismatch entry '{0}' must be zero")]
    SameSourceNonZero(String),
    #[error("mismatch entries '{0}' and '{1}' must be symmetric")]
    AsymmetricPair(String, String),
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

    #[test]
    fn embedded_mismatch_table_is_loaded() {
        let cfg = MismatchConfig::load_embedded().unwrap();
        assert_eq!(cfg.discount_for("kalshi", "polymarket"), Decimal::new(15, 2));
        assert_eq!(cfg.discount_for("unknown-a", "unknown-b"), Decimal::new(25, 2));
    }
}

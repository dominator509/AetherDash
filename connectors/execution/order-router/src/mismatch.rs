//! Settlement mismatch discount loader.
//!
//! Reads `mismatch.toml` and provides lookup by venue pair.
//! Same-source pairs have discount = 0.0 by definition.
//! Unlisted pairs use a configurable default (0.25).

use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;

/// A parsed mismatch entry with its discount factor and rationale.
#[derive(Debug, Clone, Deserialize)]
pub struct MismatchEntry {
    pub discount: String,
    pub rationale: String,
}

/// The parsed mismatch configuration.
#[derive(Debug, Clone, Default)]
pub struct MismatchConfig {
    entries: HashMap<String, MismatchEntry>,
    default_discount: Decimal,
}

#[derive(Debug, thiserror::Error)]
pub enum MismatchError {
    #[error("failed to parse mismatch.toml: {0}")]
    Parse(String),
    #[error("invalid discount value '{0}': {1}")]
    InvalidDiscount(String, String),
    #[error("missing required _default mismatch discount")]
    MissingDefault,
    #[error("discount for '{0}' must be between 0 and 1 inclusive")]
    DiscountOutOfRange(String),
    #[error("mismatch entry '{0}' is malformed: {1}")]
    MalformedEntry(String, String),
    #[error("mismatch entry '{0}' has no reverse entry '{1}'")]
    MissingReverse(String, String),
    #[error("mismatch entries '{0}' and '{1}' are not symmetric")]
    AsymmetricPair(String, String),
}

impl MismatchConfig {
    /// Load from the embedded mismatch.toml.
    pub fn load() -> Result<Self, MismatchError> {
        let raw: toml::Value = toml::from_str(include_str!("../mismatch.toml"))
            .map_err(|e| MismatchError::Parse(e.to_string()))?;

        let mut entries = HashMap::new();
        let mut default_discount = None;

        if let Some(table) = raw.as_table() {
            for (key, value) in table {
                if key == "_default" {
                    let d = value.get("discount").and_then(|v| v.as_str()).ok_or_else(|| {
                        MismatchError::MalformedEntry(key.clone(), "missing discount".into())
                    })?;
                    default_discount = Some(parse_discount(key, d)?);
                } else if let Ok(entry) = value.clone().try_into::<MismatchEntry>() {
                    parse_discount(key, &entry.discount)?;
                    entries.insert(key.clone(), entry);
                } else {
                    return Err(MismatchError::MalformedEntry(
                        key.clone(),
                        "expected discount and rationale strings".into(),
                    ));
                }
            }
        } else {
            return Err(MismatchError::Parse("top-level TOML value must be a table".into()));
        }

        for (key, entry) in &entries {
            let Some((left, right)) = key.split_once('-') else {
                return Err(MismatchError::MalformedEntry(
                    key.clone(),
                    "venue pair key must be venue-a-venue-b".into(),
                ));
            };
            if left == right {
                let discount = parse_discount(key, &entry.discount)?;
                if discount != Decimal::ZERO {
                    return Err(MismatchError::DiscountOutOfRange(key.clone()));
                }
                continue;
            }
            let reverse = format!("{}-{}", right, left);
            let Some(reverse_entry) = entries.get(&reverse) else {
                return Err(MismatchError::MissingReverse(key.clone(), reverse));
            };
            if parse_discount(key, &entry.discount)?
                != parse_discount(&reverse, &reverse_entry.discount)?
            {
                return Err(MismatchError::AsymmetricPair(key.clone(), reverse));
            }
        }

        Ok(Self {
            entries,
            default_discount: default_discount.ok_or(MismatchError::MissingDefault)?,
        })
    }

    /// Look up the discount for a venue pair.
    /// Returns 0.0 for same-venue pairs (by definition).
    /// Returns the configured default for unlisted pairs.
    pub fn discount_for(&self, venue_a: &str, venue_b: &str) -> Decimal {
        if venue_a == venue_b {
            return Decimal::ZERO;
        }
        let key = format!("{}-{}", venue_a, venue_b);
        if let Some(entry) = self.entries.get(&key) {
            return parse_discount(&key, &entry.discount).unwrap_or(self.default_discount);
        }
        self.default_discount
    }

    /// Compute the settlement-mismatch cost component for an edge.
    ///
    /// The configured discount is a fraction of positive gross spread. A
    /// non-positive spread has no extra settlement-mismatch cost; it is already
    /// not actionable as a positive edge.
    pub fn settlement_mismatch_cost(
        &self,
        venue_a: &str,
        venue_b: &str,
        gross_spread: Decimal,
    ) -> Decimal {
        if gross_spread <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        gross_spread * self.discount_for(venue_a, venue_b)
    }

    /// Get the rationale for a venue pair's discount.
    pub fn rationale(&self, venue_a: &str, venue_b: &str) -> String {
        if venue_a == venue_b {
            return "same source — no settlement mismatch risk".into();
        }
        let key = format!("{}-{}", venue_a, venue_b);
        self.entries
            .get(&key)
            .map(|e| e.rationale.clone())
            .unwrap_or_else(|| "unlisted pair — conservative default applied".into())
    }
}

fn parse_discount(key: &str, raw: &str) -> Result<Decimal, MismatchError> {
    let discount = raw
        .parse::<Decimal>()
        .map_err(|error| MismatchError::InvalidDiscount(raw.into(), error.to_string()))?;
    if !(Decimal::ZERO..=Decimal::ONE).contains(&discount) {
        return Err(MismatchError::DiscountOutOfRange(key.into()));
    }
    Ok(discount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_source_pairs_have_zero_discount() {
        let config = MismatchConfig::load().unwrap();
        assert_eq!(config.discount_for("kalshi", "kalshi"), Decimal::ZERO);
        assert_eq!(config.discount_for("polymarket", "polymarket"), Decimal::ZERO);
    }

    #[test]
    fn cross_oracle_pairs_have_configured_discount() {
        let config = MismatchConfig::load().unwrap();
        let discount = config.discount_for("kalshi", "polymarket");
        assert_eq!(discount, Decimal::new(15, 2)); // 0.15
        assert!(!config.rationale("kalshi", "polymarket").is_empty());
    }

    #[test]
    fn unlisted_pairs_use_default() {
        let config = MismatchConfig::load().unwrap();
        let discount = config.discount_for("binance", "coinbase");
        assert_eq!(discount, Decimal::new(25, 2)); // 0.25
    }

    #[test]
    fn symmetric_pairs_are_defined() {
        let config = MismatchConfig::load().unwrap();
        let a_to_b = config.discount_for("kalshi", "polymarket");
        let b_to_a = config.discount_for("polymarket", "kalshi");
        assert_eq!(a_to_b, b_to_a, "discount should be symmetric");
    }

    #[test]
    fn settlement_mismatch_cost_is_fraction_of_positive_gross_spread() {
        let config = MismatchConfig::load().unwrap();
        assert_eq!(
            config.settlement_mismatch_cost("kalshi", "polymarket", Decimal::new(100, 2)),
            Decimal::new(15, 2)
        );
        assert_eq!(
            config.settlement_mismatch_cost("kalshi", "polymarket", Decimal::new(-100, 2)),
            Decimal::ZERO
        );
    }
}

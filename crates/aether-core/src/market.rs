//! Market types: InstrumentKind, Market, PriceSemantics. SPEC-001 market data.
//! All comparisons across venues happen in probability or currency space after normalization.

use crate::ids::{MarketKey, VenueId};
use crate::time::UtcTime;
use serde::{Deserialize, Serialize};

/// Instrument kind classification. Closed set — unknown tag = validation error at boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    BinaryContract,
    CategoricalContract,
    ScalarContract,
    Equity,
    Option,
    Perp,
    Spot,
}

/// Market status. Closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketStatus {
    Open,
    Halted,
    Closed,
    Resolved,
}

/// Price semantics derived from InstrumentKind.
/// Binary/categorical: probability in [0,1] with tick size.
/// Scalar: unit + min/max.
/// Equities/options/crypto: currency prices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PriceSemantics {
    Probability {
        tick_size: String, // decimal string
    },
    Scalar {
        unit: String,
        min: String, // decimal string
        max: String, // decimal string
    },
    Currency,
}

/// A trading market. MarketKey is the universal join key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Market {
    pub key: MarketKey,
    pub venue: VenueId,
    pub kind: InstrumentKind,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_ref: Option<String>,
    pub status: MarketStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_ts: Option<UtcTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_ts: Option<UtcTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jurisdiction_flags: Vec<String>,
    pub venue_ref: serde_json::Value,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub meta: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_kind_serde_snake_case() {
        let kind = InstrumentKind::BinaryContract;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""binary_contract""#);
        let back: InstrumentKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, InstrumentKind::BinaryContract);
    }

    #[test]
    fn unknown_instrument_kind_is_error() {
        let result: Result<InstrumentKind, _> = serde_json::from_str(r#""future""#);
        assert!(result.is_err());
    }

    #[test]
    fn price_semantics_tagged_round_trip() {
        let ps = PriceSemantics::Probability { tick_size: "0.01".to_string() };
        let json = serde_json::to_string(&ps).unwrap();
        assert!(json.contains("probability"));
        let back: PriceSemantics = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ps);
    }

    #[test]
    fn market_status_serde() {
        for status in &[
            MarketStatus::Open,
            MarketStatus::Halted,
            MarketStatus::Closed,
            MarketStatus::Resolved,
        ] {
            let json = serde_json::to_string(status).unwrap();
            let back: MarketStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, back);
        }
    }
}

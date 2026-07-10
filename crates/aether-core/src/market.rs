//! Market types: InstrumentKind, Market, PriceSemantics. SPEC-001 market data.

use crate::ids::{MarketKey, VenueId};
use crate::time::UtcTime;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketStatus {
    Open,
    Halted,
    Closed,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PriceSemantics {
    Probability { tick_size: String },
    Scalar { unit: String, min: String, max: String },
    Currency,
}

/// A trading market. SPEC-001: description_ref is NOT optional per spec.
/// jurisdiction_flags is required (emit empty array, never omit).
/// venue_ref and meta are required JSON objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Market {
    pub key: MarketKey,
    pub venue: VenueId,
    pub kind: InstrumentKind,
    pub title: String,
    pub description_ref: String,
    pub status: MarketStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_ts: Option<UtcTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_ts: Option<UtcTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    pub jurisdiction_flags: Vec<String>,
    pub venue_ref: serde_json::Value,
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
    fn market_serde_includes_required_fields() {
        let m = Market {
            key: MarketKey::from_string("mkt:kalshi:BTC-75"),
            venue: VenueId::new("kalshi"),
            kind: InstrumentKind::BinaryContract,
            title: "BTC above $75k?".into(),
            description_ref: "BTC-75K-JUL10".into(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec!["US".into()],
            venue_ref: serde_json::json!({"ticker": "BTC-75K-JUL10"}),
            meta: serde_json::json!({"tick_size": "0.01"}),
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("description_ref"));
        assert!(json.contains("jurisdiction_flags"));
    }
}

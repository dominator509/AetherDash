//! Order, risk, and position types. SPEC-001 order & execution types.

use crate::decimal::{decimal_option_string, decimal_string};
use crate::ids::{MarketKey, Money, Ulid};
use crate::json::JsonObject;
use crate::quote::Quote;
use crate::time::UtcTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
    BuyNo,
    SellNo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    Limit,
    Market,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeUnit {
    Contracts,
    Shares,
    Base,
    Quote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeInForce {
    Ioc,
    Gtc,
    Day,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginKind {
    Human,
    Agent,
    Automation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskVerdictStatus {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskReasonCode {
    Liveness,
    PriceDrift,
    Balance,
    VenueHealth,
    CapExceeded,
    Jurisdiction,
    LiveDisabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskReason {
    pub code: RiskReasonCode,
    pub detail: String,
}

// ── Origin with validated tier ─────────────────────────────────────

/// Permission tier 1-5, validated on construction and deserialization.
#[derive(Debug, Clone, PartialEq)]
pub struct Origin {
    pub kind: OriginKind,
    tier: u8,
    pub actor_id: Ulid,
}

impl Origin {
    pub fn new(kind: OriginKind, tier: u8, actor_id: Ulid) -> Result<Self, OriginError> {
        if !(1..=5).contains(&tier) {
            return Err(OriginError::InvalidTier(tier));
        }
        Ok(Self { kind, tier, actor_id })
    }

    pub fn tier(&self) -> u8 {
        self.tier
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OriginError {
    #[error("tier must be 1..=5, got {0}")]
    InvalidTier(u8),
}

// Custom serde through validation
impl Serialize for Origin {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Origin", 3)?;
        st.serialize_field("kind", &self.kind)?;
        st.serialize_field("tier", &self.tier)?;
        st.serialize_field("actor_id", &self.actor_id)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for Origin {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            kind: OriginKind,
            tier: u8,
            actor_id: Ulid,
        }
        let w = Wire::deserialize(d)?;
        Origin::new(w.kind, w.tier, w.actor_id).map_err(serde::de::Error::custom)
    }
}

// ── Order Intent ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderIntent {
    pub id: Ulid,
    pub market: MarketKey,
    pub side: Side,
    pub order_type: OrderType,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "decimal_option_string")]
    pub limit_price: Option<Decimal>,
    #[serde(with = "decimal_string")]
    pub size: Decimal,
    pub size_unit: SizeUnit,
    pub tif: TimeInForce,
    pub paper: bool,
    pub origin: Origin,
    pub quote_snapshot: Quote,
    pub caps_version: Ulid,
    pub created_ts: UtcTime,
}

// ── Risk Verdict ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskVerdict {
    pub intent_id: Ulid,
    pub verdict: RiskVerdictStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<RiskReason>,
    pub ts: UtcTime,
}

impl RiskVerdict {
    pub fn allow(intent_id: Ulid) -> Self {
        Self { intent_id, verdict: RiskVerdictStatus::Allow, reasons: vec![], ts: UtcTime::now() }
    }

    pub fn deny(intent_id: Ulid, reasons: Vec<RiskReason>) -> Self {
        Self { intent_id, verdict: RiskVerdictStatus::Deny, reasons, ts: UtcTime::now() }
    }

    pub fn is_allowed(&self) -> bool {
        self.verdict == RiskVerdictStatus::Allow
    }
}

// ── Order ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub order_id: Ulid,
    pub market: MarketKey,
    pub side: Side,
    #[serde(with = "decimal_string")]
    pub price: Decimal,
    #[serde(with = "decimal_string")]
    pub size: Decimal,
    pub fee: Money,
    pub venue_ref: JsonObject,
    pub ts: UtcTime,
    pub paper: bool,
}

// ── Fill ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub order_id: Ulid,
    pub market: MarketKey,
    pub side: Side,
    #[serde(with = "decimal_string")]
    pub price: Decimal,
    #[serde(with = "decimal_string")]
    pub size: Decimal,
    pub fee: Money,
    pub venue_ref: JsonObject,
    pub ts: UtcTime,
    pub paper: bool,
}

// ── Position ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub market: MarketKey,
    #[serde(with = "decimal_string")]
    pub side_exposure: Decimal,
    #[serde(with = "decimal_string")]
    pub avg_price: Decimal,
    #[serde(with = "decimal_string")]
    pub size: Decimal,
    pub realized_pnl: Money,
    pub unrealized_pnl: Money,
    pub ts: UtcTime,
}

// ── Caps Snapshot ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapsSnapshot {
    pub version: Ulid,
    pub per_order_max: Money,
    pub daily_max: Money,
    #[serde(default, skip_serializing_if = "JsonObject::is_empty")]
    pub per_venue: JsonObject,
    #[serde(default, skip_serializing_if = "JsonObject::is_empty")]
    pub per_kind: JsonObject,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::VenueId;
    use crate::quote::QuoteSource;

    #[allow(dead_code)]
    fn test_market_key() -> MarketKey {
        MarketKey::new(&VenueId::new("kalshi").unwrap(), "INTC-50").unwrap()
    }

    #[allow(dead_code)]
    fn test_quote() -> Quote {
        Quote {
            market: test_market_key(),
            bid: Some(Decimal::new(65, 2)),
            ask: Some(Decimal::new(67, 2)),
            mid: Some(Decimal::new(66, 2)),
            last: None,
            bid_size: Some(Decimal::new(1000, 0)),
            ask_size: Some(Decimal::new(500, 0)),
            ts: UtcTime::from_unix_millis(1752152096789000).unwrap(),
            source: QuoteSource::Stream,
            seq: Some(1),
        }
    }

    #[test]
    fn origin_valid_tiers() {
        for t in 1..=5u8 {
            assert!(Origin::new(OriginKind::Human, t, Ulid::new()).is_ok());
        }
    }

    #[test]
    fn origin_rejects_tier_zero() {
        assert!(Origin::new(OriginKind::Human, 0, Ulid::new()).is_err());
    }

    #[test]
    fn origin_rejects_tier_six() {
        assert!(Origin::new(OriginKind::Human, 6, Ulid::new()).is_err());
    }

    #[test]
    fn origin_deserialize_rejects_invalid_tier() {
        let json = r#"{"kind":"human","tier":0,"actor_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#;
        let result: Result<Origin, _> = serde_json::from_str(json);
        assert!(result.is_err(), "tier 0 should be rejected");
    }

    #[test]
    fn order_intent_rejects_numeric_decimal() {
        let json = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV","market":"mkt:kalshi:INTC-50","side":"buy","order_type":"limit","size":10,"size_unit":"contracts","tif":"gtc","paper":true,"origin":{"kind":"human","tier":3,"actor_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"},"quote_snapshot":{},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z"}"#;
        let result: Result<OrderIntent, _> = serde_json::from_str(json);
        assert!(result.is_err(), "numeric size should be rejected");
    }

    #[test]
    fn risk_verdict_allow_is_empty_reasons() {
        let v = RiskVerdict::allow(Ulid::new());
        assert!(v.is_allowed());
        assert!(v.reasons.is_empty());
    }

    #[test]
    fn risk_verdict_deny_with_reasons() {
        let v = RiskVerdict::deny(
            Ulid::new(),
            vec![RiskReason {
                code: RiskReasonCode::CapExceeded,
                detail: "per_order_max $500 < $1,000".into(),
            }],
        );
        assert!(!v.is_allowed());
        assert_eq!(v.reasons[0].code, RiskReasonCode::CapExceeded);
    }
}

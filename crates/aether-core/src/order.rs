//! Order, risk, and position types. SPEC-001 order & execution types.
//! INV-1: these are deterministic code paths — never delegated to an LLM.

use crate::decimal::decimal_string;
use crate::ids::{MarketKey, Ulid};
use crate::quote::Quote;
use crate::time::UtcTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ── Enums ──────────────────────────────────────────────────────────

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
    User,
    AlertAction,
    Agent,
    Automation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskVerdictStatus {
    Allow,
    Deny,
}

// ── Risk Reason Codes (closed set) ─────────────────────────────────

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

// ── Origin (who placed the intent) ─────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Origin {
    pub kind: OriginKind,
    /// Permission tier 1-5
    pub tier: u8,
    pub actor_id: Ulid,
}

// ── Order Intent ───────────────────────────────────────────────────

/// An order intent — what the actor wants. The router's drift check
/// compares `quote_snapshot` against current prices (SPEC-012).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderIntent {
    pub id: Ulid,
    pub market: MarketKey,
    pub side: Side,
    pub order_type: OrderType,
    #[serde(skip_serializing_if = "Option::is_none")]
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

/// Risk engine output. Deny reasons from the closed set (SPEC-001).
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

// ── Money ──────────────────────────────────────────────────────────

/// Money value with currency. (Re-exported from ids.rs; canonical definition here.)
pub use crate::ids::Money;

// ── Order (accepted intent) ────────────────────────────────────────

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
    pub venue_ref: serde_json::Value,
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
    pub venue_ref: serde_json::Value,
    pub ts: UtcTime,
    pub paper: bool,
}

// ── Position ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub market: MarketKey,
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
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub per_venue: serde_json::Map<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub per_kind: serde_json::Map<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::VenueId;
    use crate::quote::QuoteSource;

    fn test_market_key() -> MarketKey {
        MarketKey::new(&VenueId::new("kalshi"), "INTC-50")
    }

    fn test_quote() -> Quote {
        Quote {
            market: test_market_key(),
            bid: Some(Decimal::new(65, 2)),
            ask: Some(Decimal::new(67, 2)),
            mid: Some(Decimal::new(66, 2)),
            last: None,
            bid_size: Some(Decimal::new(1000, 0)),
            ask_size: Some(Decimal::new(500, 0)),
            ts: UtcTime::from_unix_millis(1752152096789).unwrap(),
            source: QuoteSource::Stream,
            seq: Some(1),
        }
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

    #[test]
    fn order_intent_serde_round_trip() {
        let intent = OrderIntent {
            id: Ulid::new(),
            market: test_market_key(),
            side: Side::Buy,
            order_type: OrderType::Limit,
            limit_price: Some(Decimal::new(66, 2)),
            size: Decimal::new(10, 0),
            size_unit: SizeUnit::Contracts,
            tif: TimeInForce::Gtc,
            paper: true,
            origin: Origin { kind: OriginKind::User, tier: 3, actor_id: Ulid::new() },
            quote_snapshot: test_quote(),
            caps_version: Ulid::new(),
            created_ts: UtcTime::from_unix_millis(1752152096789).unwrap(),
        };
        let json = serde_json::to_string(&intent).unwrap();
        let back: OrderIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(intent.id, back.id);
        assert_eq!(intent.market, back.market);
        assert_eq!(intent.side, back.side);
        assert!(back.paper);
    }

    #[test]
    fn caps_snapshot_serde_round_trip() {
        let caps = CapsSnapshot {
            version: Ulid::new(),
            per_order_max: Money::new(Decimal::new(50000, 2), "USD"),
            daily_max: Money::new(Decimal::new(100000, 2), "USD"),
            per_venue: serde_json::Map::new(),
            per_kind: serde_json::Map::new(),
        };
        let json = serde_json::to_string(&caps).unwrap();
        let back: CapsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(caps.per_order_max.amount, back.per_order_max.amount);
    }
}

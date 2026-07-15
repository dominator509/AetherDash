#![allow(clippy::unwrap_used)]

use aether_core::ids::{MarketKey, Money, Ulid, VenueId};
use aether_core::json::JsonObject;
use aether_core::market::{InstrumentKind, Market, MarketStatus};
use aether_core::order::{
    CapsSnapshot, OrderIntent, OrderType, Origin, OriginKind, RiskReasonCode, Side, SizeUnit,
    TimeInForce,
};
use aether_core::quote::{Quote, QuoteSource};
use aether_core::time::UtcTime;
use aether_risk_engine::engine::{
    Balances, PositionOutcome, RiskContext, RiskEngine, VenueHealthStatus,
};
use rust_decimal::Decimal;
use std::collections::HashMap;

fn make_intent(paper: bool, limit_price: Option<Decimal>) -> OrderIntent {
    let venue = VenueId::new("test").unwrap();
    let market = MarketKey::new(&venue, "TEST").unwrap();
    let now = UtcTime::now();
    OrderIntent {
        id: Ulid::new(),
        market: market.clone(),
        side: Side::Buy,
        order_type: OrderType::Limit,
        limit_price,
        size: Decimal::new(10, 0),
        size_unit: SizeUnit::Contracts,
        tif: TimeInForce::Day,
        paper,
        origin: Origin::new(OriginKind::Agent, 3, Ulid::new()).unwrap(),
        quote_snapshot: Quote {
            market: market.clone(),
            bid: Some(Decimal::new(99, 2)),
            ask: Some(Decimal::new(101, 2)),
            mid: None,
            last: None,
            bid_size: None,
            ask_size: None,
            ts: now,
            source: QuoteSource::Stream,
            seq: None,
        },
        caps_version: caps_version(),
        created_ts: now,
    }
}

fn caps_version() -> Ulid {
    Ulid::from_string("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
}

fn make_context() -> RiskContext {
    let venue = VenueId::new("test").unwrap();
    let market_key = MarketKey::new(&venue, "TEST").unwrap();
    let evaluated_at = UtcTime::now();
    let mut balances = HashMap::new();
    balances.insert(
        venue.clone(),
        Balances {
            free: Decimal::new(100000, 2), // $1,000.00
            locked: Decimal::ZERO,
            currency: "USD".into(),
        },
    );
    let mut health = HashMap::new();
    health.insert(venue.clone(), VenueHealthStatus { status: "ok".into(), breaker_open: false });
    let active_caps = CapsSnapshot {
        version: Ulid::new(),
        per_order_max: Money::new(Decimal::new(500000, 2), "USD"),
        daily_max: Money::new(Decimal::new(5000000, 2), "USD"),
        per_venue: JsonObject::default(),
        per_kind: JsonObject::default(),
    };
    let mut caps_by_version = HashMap::new();
    caps_by_version.insert(caps_version(), active_caps.clone());
    let mut markets = HashMap::new();
    markets.insert(
        market_key.clone(),
        Market {
            key: market_key,
            venue,
            kind: InstrumentKind::BinaryContract,
            title: "test".into(),
            description_ref: "test".into(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: Vec::new(),
            venue_ref: JsonObject::default(),
            meta: JsonObject::default(),
        },
    );
    RiskContext {
        evaluated_at,
        markets,
        balances,
        positions: HashMap::new(),
        venue_health: health,
        active_caps,
        caps_by_version,
        daily_notional: Decimal::ZERO,
        jurisdiction_eligible: HashMap::new(),
        live_enabled: false,
    }
}

#[test]
fn allows_paper_order_when_live_disabled() {
    let engine = RiskEngine::with_defaults();
    let intent = make_intent(true, Some(Decimal::new(100, 2)));
    let ctx = make_context();
    let verdict = engine.evaluate(&intent, &ctx);
    assert!(verdict.is_allowed(), "paper orders allowed even when live_enabled=false");
}

#[test]
fn blocks_live_order_when_live_disabled() {
    let engine = RiskEngine::with_defaults();
    let intent = make_intent(false, Some(Decimal::new(100, 2)));
    let ctx = make_context();
    let verdict = engine.evaluate(&intent, &ctx);
    assert!(!verdict.is_allowed());
    assert!(verdict.reasons.iter().any(|r| r.code == RiskReasonCode::LiveDisabled));
}

#[test]
fn allows_live_order_when_live_enabled() {
    let engine = RiskEngine::with_defaults();
    let intent = make_intent(false, Some(Decimal::new(100, 2)));
    let mut ctx = make_context();
    ctx.live_enabled = true;
    ctx.jurisdiction_eligible.insert(VenueId::new("test").unwrap(), true);
    let verdict = engine.evaluate(&intent, &ctx);
    assert!(verdict.is_allowed());
}

#[test]
fn blocks_stale_quote() {
    let engine = RiskEngine::with_defaults();
    let mut intent = make_intent(true, Some(Decimal::new(100, 2)));
    // Artificially age the quote to epoch 0
    intent.quote_snapshot.ts = UtcTime::from_unix_millis(0).unwrap();
    let verdict = engine.evaluate(&intent, &make_context());
    assert!(!verdict.is_allowed());
    assert!(verdict.reasons.iter().any(|r| r.code == RiskReasonCode::Liveness));
}

#[test]
fn blocks_price_drift() {
    let engine = RiskEngine::with_defaults();
    // Limit 1.20 but ask is 1.01 -- adverse drift above the 2% maximum.
    let intent = make_intent(true, Some(Decimal::new(120, 2)));
    let verdict = engine.evaluate(&intent, &make_context());
    assert!(!verdict.is_allowed());
    assert!(verdict.reasons.iter().any(|r| r.code == RiskReasonCode::PriceDrift));
}

#[test]
fn allows_more_favorable_buy_limit() {
    let intent = make_intent(true, Some(Decimal::new(80, 2)));
    assert!(RiskEngine::with_defaults().evaluate(&intent, &make_context()).is_allowed());
}

#[test]
fn blocks_missing_intent_caps_snapshot() {
    let intent = make_intent(true, Some(Decimal::ONE));
    let mut ctx = make_context();
    ctx.caps_by_version.clear();
    let verdict = RiskEngine::with_defaults().evaluate(&intent, &ctx);
    assert!(verdict.reasons.iter().any(|reason| reason.code == RiskReasonCode::CapExceeded));
}

#[test]
fn verdict_timestamp_comes_from_context() {
    let intent = make_intent(true, Some(Decimal::ONE));
    let ctx = make_context();
    let first = RiskEngine::with_defaults().evaluate(&intent, &ctx);
    let second = RiskEngine::with_defaults().evaluate(&intent, &ctx);
    assert_eq!(first, second);
    assert_eq!(first.ts, ctx.evaluated_at);
}

#[test]
fn allows_small_drift() {
    let engine = RiskEngine::with_defaults();
    // Limit 1.02 vs ask 1.01 -- 1% drift, within default 2%
    let intent = make_intent(true, Some(Decimal::new(102, 2)));
    let verdict = engine.evaluate(&intent, &make_context());
    assert!(verdict.is_allowed());
}

#[test]
fn blocks_insufficient_balance() {
    let engine = RiskEngine::with_defaults();
    let intent = make_intent(true, Some(Decimal::new(100, 0))); // $100 per contract * 10 = $1,000
    let mut ctx = make_context();
    let venue = VenueId::new("test").unwrap();
    ctx.balances.insert(
        venue,
        Balances {
            free: Decimal::new(100, 2), // only $1 free
            locked: Decimal::ZERO,
            currency: "USD".into(),
        },
    );
    let verdict = engine.evaluate(&intent, &ctx);
    assert!(!verdict.is_allowed());
    assert!(verdict.reasons.iter().any(|r| r.code == RiskReasonCode::Balance));
}

#[test]
fn blocks_unhealthy_venue() {
    let engine = RiskEngine::with_defaults();
    let intent = make_intent(true, Some(Decimal::new(100, 2)));
    let mut ctx = make_context();
    let venue = VenueId::new("test").unwrap();
    ctx.venue_health
        .insert(venue, VenueHealthStatus { status: "down".into(), breaker_open: false });
    let verdict = engine.evaluate(&intent, &ctx);
    assert!(!verdict.is_allowed());
    assert!(verdict.reasons.iter().any(|r| r.code == RiskReasonCode::VenueHealth));
}

#[test]
fn blocks_breaker_open() {
    let engine = RiskEngine::with_defaults();
    let intent = make_intent(true, Some(Decimal::new(100, 2)));
    let mut ctx = make_context();
    let venue = VenueId::new("test").unwrap();
    ctx.venue_health.insert(venue, VenueHealthStatus { status: "ok".into(), breaker_open: true });
    let verdict = engine.evaluate(&intent, &ctx);
    assert!(!verdict.is_allowed());
    assert!(verdict.reasons.iter().any(|r| r.code == RiskReasonCode::VenueHealth));
}

#[test]
fn blocks_order_exceeding_per_order_cap() {
    let engine = RiskEngine::with_defaults();
    // $150 per contract * 100 contracts = $15,000 > default hard cap $10,000
    let mut intent = make_intent(true, Some(Decimal::new(150, 0)));
    intent.size = Decimal::new(100, 0);
    let verdict = engine.evaluate(&intent, &make_context());
    assert!(!verdict.is_allowed());
    assert!(verdict.reasons.iter().any(|r| r.code == RiskReasonCode::CapExceeded));
}

#[test]
fn allows_order_within_caps() {
    let engine = RiskEngine::with_defaults();
    let intent = make_intent(true, Some(Decimal::new(100, 2))); // $1 * 10 = $10
    let verdict = engine.evaluate(&intent, &make_context());
    assert!(verdict.is_allowed());
}

#[test]
fn purity_same_input_same_output() {
    let engine = RiskEngine::with_defaults();
    let intent = make_intent(true, Some(Decimal::new(100, 2)));
    let ctx = make_context();
    let v1 = engine.evaluate(&intent, &ctx);
    let v2 = engine.evaluate(&intent, &ctx);
    assert_eq!(v1, v2);
}

#[test]
fn closed_market_fails_liveness() {
    let intent = make_intent(true, Some(Decimal::ONE));
    let mut ctx = make_context();
    ctx.markets.get_mut(&intent.market).unwrap().status = MarketStatus::Closed;
    let verdict = RiskEngine::with_defaults().evaluate(&intent, &ctx);
    assert!(verdict.reasons.iter().any(|reason| reason.code == RiskReasonCode::Liveness));
}

#[test]
fn missing_balance_fails_closed() {
    let intent = make_intent(true, Some(Decimal::ONE));
    let mut ctx = make_context();
    ctx.balances.clear();
    let verdict = RiskEngine::with_defaults().evaluate(&intent, &ctx);
    assert!(verdict.reasons.iter().any(|reason| reason.code == RiskReasonCode::Balance));
}

#[test]
fn intent_stamped_cap_is_applied_when_lower_than_active_cap() {
    let mut intent = make_intent(true, Some(Decimal::ONE));
    intent.size = Decimal::new(10, 0);
    let mut ctx = make_context();
    ctx.caps_by_version.get_mut(&intent.caps_version).unwrap().per_order_max.amount =
        Decimal::new(5, 0);
    let verdict = RiskEngine::with_defaults().evaluate(&intent, &ctx);
    assert!(verdict.reasons.iter().any(|reason| reason.code == RiskReasonCode::CapExceeded));
}

#[test]
fn market_order_uses_quote_for_balance_and_caps() {
    let mut intent = make_intent(true, None);
    intent.order_type = OrderType::Market;
    intent.size = Decimal::new(20_000, 0);
    let verdict = RiskEngine::with_defaults().evaluate(&intent, &make_context());
    assert!(verdict.reasons.iter().any(|reason| reason.code == RiskReasonCode::Balance));
    assert!(verdict.reasons.iter().any(|reason| reason.code == RiskReasonCode::CapExceeded));
}

#[test]
fn sell_requires_position_inventory() {
    let mut intent = make_intent(true, Some(Decimal::new(99, 2)));
    intent.side = Side::Sell;
    let verdict = RiskEngine::with_defaults().evaluate(&intent, &make_context());
    assert!(verdict.reasons.iter().any(|reason| reason.code == RiskReasonCode::Balance));
}

#[test]
fn yes_inventory_does_not_authorize_no_sale() {
    let mut intent = make_intent(true, Some(Decimal::new(1, 2)));
    intent.side = Side::SellNo;
    intent.quote_snapshot.ask = Some(Decimal::new(99, 2));
    intent.quote_snapshot.bid = Some(Decimal::new(98, 2));
    let mut ctx = make_context();
    ctx.positions.insert((intent.market.clone(), PositionOutcome::Yes), intent.size);
    let verdict = RiskEngine::with_defaults().evaluate(&intent, &ctx);
    assert!(verdict.reasons.iter().any(|reason| reason.code == RiskReasonCode::Balance));
}

#[test]
fn live_order_without_jurisdiction_decision_fails_closed() {
    let intent = make_intent(false, Some(Decimal::ONE));
    let mut ctx = make_context();
    ctx.live_enabled = true;
    let verdict = RiskEngine::with_defaults().evaluate(&intent, &ctx);
    assert!(verdict.reasons.iter().any(|reason| reason.code == RiskReasonCode::Jurisdiction));
}

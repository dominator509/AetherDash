//! Golden vector generator with typed round-trips for ALL SPEC-001 types.
//! Run: cargo run -p aether-core --features golden-gen --bin gen-goldens

use aether_core::{
    audit, canonical, decimal, error, ids, json, market, opportunity, order, quote, time,
};
use rust_decimal::Decimal;
use serde::Serialize;
use std::{error::Error, fs};

#[derive(Serialize)]
struct GoldenEntry {
    name: String,
    #[serde(rename = "type")]
    typ: String,
    value: serde_json::Value,
    sha256: String,
}

fn g<T: Serialize>(name: &str, typ: &str, value: &T) -> Result<GoldenEntry, Box<dyn Error>> {
    Ok(GoldenEntry {
        name: name.into(),
        typ: typ.into(),
        value: serde_json::to_value(value)?,
        sha256: canonical::canonical_sha256(value)?,
    })
}
fn write(dir: &str, file: &str, e: &[GoldenEntry]) -> Result<(), Box<dyn Error>> {
    fs::write(format!("{dir}/{file}"), serde_json::to_string_pretty(e)?)?;
    Ok(())
}
fn ts() -> Result<time::UtcTime, Box<dyn Error>> {
    Ok(time::UtcTime::from_unix_millis(1752150896789)?)
}
fn ulid(s: &str) -> Result<ids::Ulid, ulid::DecodeError> {
    ids::Ulid::from_string(s)
}
fn v(s: &str) -> Result<ids::VenueId, ids::VenueIdError> {
    ids::VenueId::new(s)
}
fn mk(venue: &str, native: &str) -> Result<ids::MarketKey, ids::MarketKeyError> {
    ids::MarketKey::new(&v(venue)?, native)
}
fn mk_u(s: &str) -> ids::MarketKey {
    ids::MarketKey::from_string_unchecked(s)
}
fn jo(json: serde_json::Value) -> Result<json::JsonObject, json::JsonObjectError> {
    json::JsonObject::new(json)
}
fn mm(a: i64, s: u32, c: &str) -> ids::Money {
    ids::Money::new(Decimal::new(a, s), c)
}
fn dec(a: i64, s: u32) -> Decimal {
    Decimal::new(a, s)
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = "testdata/golden/core";
    fs::create_dir_all(out)?;
    let t = ts()?;

    write(
        out,
        "money.json",
        &[
            g("money_usd", "Money", &mm(12345, 2, "USD"))?,
            g("money_zero", "Money", &ids::Money::zero("USDC"))?,
            g("money_negative", "Money", &mm(-5000, 2, "USD"))?,
            g("money_18dec", "Money", &mm(123456789012345678i64, 18, "BTC"))?,
        ],
    )?;

    write(
        out,
        "market_key.json",
        &[
            g("market_key_kalshi", "MarketKey", &mk("kalshi", "BTC-75")?)?,
            g("market_key_polymarket", "MarketKey", &mk("polymarket", "WILL-BTC-TOUCH-100K")?)?,
        ],
    )?;

    write(
        out,
        "confidence.json",
        &[
            g("confidence_zero", "Confidence", &decimal::Confidence::new(Decimal::ZERO)?)?,
            g("confidence_half", "Confidence", &decimal::Confidence::new(dec(5, 1))?)?,
            g("confidence_one", "Confidence", &decimal::Confidence::new(Decimal::ONE)?)?,
        ],
    )?;

    let ec = |f, s, g, lq, c| opportunity::EdgeCosts {
        fees: f,
        slippage_est: s,
        funding_cost: Decimal::ZERO,
        gas_cost: g,
        bridge_cost: Decimal::ZERO,
        settlement_mismatch_discount: Decimal::ZERO,
        liquidity_haircut: lq,
        staleness_penalty: Decimal::ZERO,
        confidence_penalty: c,
    };
    write(
        out,
        "edge.json",
        &[
            g(
                "edge_positive_net",
                "EdgeDecomposition",
                &opportunity::EdgeDecomposition::compute(
                    dec(100, 2),
                    ec(dec(10, 2), dec(5, 2), dec(2, 2), dec(3, 2), Decimal::ZERO),
                ),
            )?,
            g(
                "edge_zero_net",
                "EdgeDecomposition",
                &opportunity::EdgeDecomposition::compute(
                    dec(20, 2),
                    ec(dec(10, 2), dec(5, 2), dec(2, 2), dec(3, 2), Decimal::ZERO),
                ),
            )?,
            g(
                "edge_explicit_zeros",
                "EdgeDecomposition",
                &opportunity::EdgeDecomposition::compute(
                    dec(10, 2),
                    ec(Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, dec(10, 2), Decimal::ZERO),
                ),
            )?,
        ],
    )?;

    let q1 = quote::Quote {
        market: mk("kalshi", "BTC-75")?,
        bid: Some(dec(65, 2)),
        ask: Some(dec(67, 2)),
        mid: Some(dec(66, 2)),
        last: None,
        bid_size: Some(dec(1000, 0)),
        ask_size: Some(dec(500, 0)),
        ts: t,
        source: quote::QuoteSource::Stream,
        seq: Some(1),
    };
    let q2 = quote::Quote {
        market: mk("polymarket", "TEST")?,
        bid: None,
        ask: None,
        mid: None,
        last: Some(dec(50, 2)),
        bid_size: None,
        ask_size: None,
        ts: t,
        source: quote::QuoteSource::Poll,
        seq: None,
    };
    write(out, "quote.json", &[g("quote_full", "Quote", &q1)?, g("quote_minimal", "Quote", &q2)?])?;

    let ob1 = quote::OrderBook::new(
        mk("kalshi", "BTC-75")?,
        vec![
            quote::BookLevel { price: dec(995, 2), size: dec(10, 0) },
            quote::BookLevel { price: dec(990, 2), size: dec(5, 0) },
        ],
        vec![
            quote::BookLevel { price: dec(1000, 2), size: dec(10, 0) },
            quote::BookLevel { price: dec(1005, 2), size: dec(5, 0) },
        ],
        2,
        t,
        Some(42),
    )?;
    let ob2 = quote::OrderBook::new(mk("polymarket", "EMPTY")?, vec![], vec![], 0, t, None)?;
    write(
        out,
        "order_book.json",
        &[
            g("order_book_simple", "OrderBook", &ob1)?,
            g("order_book_empty_sides", "OrderBook", &ob2)?,
        ],
    )?;

    let snap = quote::Quote {
        market: mk_u("mkt:kalshi:INTC-50"),
        bid: Some(dec(65, 2)),
        ask: Some(dec(67, 2)),
        mid: None,
        last: None,
        bid_size: None,
        ask_size: None,
        ts: t,
        source: quote::QuoteSource::Stream,
        seq: None,
    };
    #[allow(clippy::unwrap_used)]
    let u = || ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
    let oi1 = order::OrderIntent {
        id: u(),
        market: mk_u("mkt:kalshi:INTC-50"),
        side: order::Side::Buy,
        order_type: order::OrderType::Limit,
        limit_price: Some(dec(66, 2)),
        size: dec(10, 0),
        size_unit: order::SizeUnit::Contracts,
        tif: order::TimeInForce::Gtc,
        paper: true,
        origin: order::Origin::new(order::OriginKind::User, 3, u())?,
        quote_snapshot: snap.clone(),
        caps_version: u(),
        created_ts: t,
    };
    let oi2 = order::OrderIntent {
        id: u(),
        market: mk_u("mkt:polymarket:TEST"),
        side: order::Side::Sell,
        order_type: order::OrderType::Market,
        limit_price: None,
        size: dec(5, 0),
        size_unit: order::SizeUnit::Shares,
        tif: order::TimeInForce::Ioc,
        paper: false,
        origin: order::Origin::new(order::OriginKind::Automation, 1, u())?,
        quote_snapshot: snap,
        caps_version: u(),
        created_ts: t,
    };
    write(
        out,
        "order_intent.json",
        &[
            g("intent_limit_buy", "OrderIntent", &oi1)?,
            g("intent_market_sell", "OrderIntent", &oi2)?,
        ],
    )?;

    write(
        out,
        "risk_verdict.json",
        &[
            g(
                "verdict_allow",
                "RiskVerdict",
                &order::RiskVerdict {
                    intent_id: u(),
                    verdict: order::RiskVerdictStatus::Allow,
                    reasons: vec![],
                    ts: t,
                },
            )?,
            g(
                "verdict_deny",
                "RiskVerdict",
                &order::RiskVerdict {
                    intent_id: u(),
                    verdict: order::RiskVerdictStatus::Deny,
                    reasons: vec![order::RiskReason {
                        code: order::RiskReasonCode::CapExceeded,
                        detail: "cap $500 < $1000".into(),
                    }],
                    ts: t,
                },
            )?,
        ],
    )?;

    write(
        out,
        "order.json",
        &[
            g(
                "order_live",
                "Order",
                &order::Order {
                    order_id: u(),
                    market: mk_u("mkt:kalshi:INTC-50"),
                    side: order::Side::Buy,
                    price: dec(66, 2),
                    size: dec(10, 0),
                    fee: mm(50, 2, "USD"),
                    venue_ref: jo(serde_json::json!({"order_ref":"kalshi-123"}))?,
                    ts: t,
                    paper: false,
                },
            )?,
            g(
                "order_paper",
                "Order",
                &order::Order {
                    order_id: u(),
                    market: mk_u("mkt:polymarket:TEST"),
                    side: order::Side::Sell,
                    price: dec(5050, 2),
                    size: dec(5, 0),
                    fee: mm(125, 3, "USDC"),
                    venue_ref: jo(serde_json::json!({"order_ref":"pm-456"}))?,
                    ts: t,
                    paper: true,
                },
            )?,
        ],
    )?;

    write(
        out,
        "fill.json",
        &[
            g(
                "fill_partial",
                "Fill",
                &order::Fill {
                    order_id: u(),
                    market: mk_u("mkt:kalshi:INTC-50"),
                    side: order::Side::Buy,
                    price: dec(66, 2),
                    size: dec(6, 0),
                    fee: mm(30, 2, "USD"),
                    venue_ref: jo(serde_json::json!({"fill_ref":"kalshi-f-789"}))?,
                    ts: t,
                    paper: false,
                },
            )?,
            g(
                "fill_full",
                "Fill",
                &order::Fill {
                    order_id: u(),
                    market: mk_u("mkt:polymarket:TEST"),
                    side: order::Side::Sell,
                    price: dec(5050, 2),
                    size: dec(5, 0),
                    fee: mm(125, 3, "USDC"),
                    venue_ref: jo(serde_json::json!({"fill_ref":"pm-f-012"}))?,
                    ts: t,
                    paper: true,
                },
            )?,
        ],
    )?;

    write(
        out,
        "position.json",
        &[
            g(
                "position_long",
                "Position",
                &order::Position {
                    market: mk_u("mkt:kalshi:INTC-50"),
                    side_exposure: dec(100, 0),
                    avg_price: dec(65, 2),
                    size: dec(10, 0),
                    realized_pnl: mm(0, 0, "USD"),
                    unrealized_pnl: mm(1000, 2, "USD"),
                    ts: t,
                },
            )?,
            g(
                "position_flat",
                "Position",
                &order::Position {
                    market: mk_u("mkt:polymarket:TEST"),
                    side_exposure: Decimal::ZERO,
                    avg_price: Decimal::ZERO,
                    size: Decimal::ZERO,
                    realized_pnl: mm(-5000, 2, "USD"),
                    unrealized_pnl: ids::Money::zero("USD"),
                    ts: t,
                },
            )?,
        ],
    )?;

    write(
        out,
        "caps_snapshot.json",
        &[g(
            "caps_default",
            "CapsSnapshot",
            &order::CapsSnapshot {
                version: u(),
                per_order_max: mm(50000, 2, "USD"),
                daily_max: mm(100000, 2, "USD"),
                per_venue: serde_json::Map::new(),
                per_kind: serde_json::Map::new(),
            },
        )?],
    )?;

    write(
        out,
        "market.json",
        &[g(
            "market_kalshi_open",
            "Market",
            &market::Market {
                key: mk_u("mkt:kalshi:BTC-75"),
                venue: v("kalshi")?,
                kind: market::InstrumentKind::BinaryContract,
                title: "BTC above $75k?".into(),
                description_ref: "BTC-75K-JUL10".into(),
                status: market::MarketStatus::Open,
                close_ts: None,
                resolve_ts: None,
                outcome: None,
                jurisdiction_flags: vec!["US".into()],
                venue_ref: jo(serde_json::json!({"ticker":"BTC-75K-JUL10"}))?,
                meta: jo(serde_json::json!({"tick_size":"0.01"}))?,
            },
        )?],
    )?;

    use market::DecimalString;
    write(
        out,
        "price_semantics.json",
        &[
            g(
                "ps_probability",
                "PriceSemantics",
                &market::PriceSemantics::Probability { tick_size: DecimalString::new("0.01")? },
            )?,
            g(
                "ps_scalar",
                "PriceSemantics",
                &market::PriceSemantics::Scalar {
                    unit: "points".into(),
                    min: DecimalString::new("0")?,
                    max: DecimalString::new("100")?,
                },
            )?,
            g("ps_currency", "PriceSemantics", &market::PriceSemantics::Currency)?,
        ],
    )?;

    write(
        out,
        "opportunity.json",
        &[g(
            "opp_arbitrage",
            "Opportunity",
            &opportunity::Opportunity {
                id: u(),
                kind: opportunity::OpportunityKind::Arbitrage,
                legs: vec![
                    opportunity::OpportunityLeg {
                        market: mk_u("mkt:kalshi:BTC-75"),
                        side: order::Side::Buy,
                        target_price: Some(dec(65, 2)),
                        size_hint: Some(dec(10, 0)),
                    },
                    opportunity::OpportunityLeg {
                        market: mk_u("mkt:polymarket:WILL-BTC-TOUCH-100K"),
                        side: order::Side::SellNo,
                        target_price: Some(dec(68, 2)),
                        size_hint: Some(dec(10, 0)),
                    },
                ],
                gross_edge: dec(300, 2),
                edge: opportunity::EdgeDecomposition::compute(
                    dec(300, 2),
                    ec(dec(20, 2), dec(5, 2), dec(3, 2), dec(2, 2), dec(10, 2)),
                ),
                confidence: decimal::Confidence::new(dec(8, 1))?,
                detected_ts: t,
                expires_ts: None,
                explain_ref: opportunity::BrainRef {
                    object_id: u(),
                    provenance_hash: "abc123def456".into(),
                },
                trace_id: u(),
            },
        )?],
    )?;

    write(
        out,
        "audit_event.json",
        &[g(
            "audit_genesis",
            "AuditEvent",
            &audit::AuditEvent {
                seq: 1,
                prev_hash: String::new(),
                hash: "sha256:abc123def456".into(),
                ts: t,
                actor: "system".into(),
                action: "genesis".into(),
                subject: "audit_chain".into(),
                payload_hash: "sha256:000".into(),
            },
        )?],
    )?;

    write(
        out,
        "error_envelope.json",
        &[
            g(
                "error_invalid",
                "ErrorEnvelope",
                &error::ErrorEnvelope::new(
                    error::ErrorCode::InvalidArgument,
                    "market key not found",
                    u(),
                ),
            )?,
            g(
                "error_unavailable",
                "ErrorEnvelope",
                &error::ErrorEnvelope::new(
                    error::ErrorCode::Unavailable,
                    "venue not reachable",
                    u(),
                ),
            )?,
        ],
    )?;

    println!("Golden vectors generated in {out}");
    Ok(())
}

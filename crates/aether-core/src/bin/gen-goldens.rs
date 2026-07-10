//! Golden vector generator — produces test vectors with SHA-256 hashes for ALL SPEC-001 types.
//! Run: cargo run -p aether-core --features golden_gen --bin gen-goldens
//! This is a dev tool; panicking on invalid fixed test data is correct behavior.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use aether_core::audit::AuditEvent;
use aether_core::canonical::canonical_sha256;
use aether_core::decimal::Confidence;
use aether_core::error::{ErrorCode, ErrorEnvelope};
use aether_core::ids::{MarketKey, Money, Ulid, VenueId};
use aether_core::market::{InstrumentKind, Market, MarketStatus, PriceSemantics};
use aether_core::opportunity::{
    BrainRef, EdgeCosts, EdgeDecomposition, Opportunity, OpportunityKind, OpportunityLeg,
};
use aether_core::order::{
    CapsSnapshot, Fill, Order, OrderIntent, OrderType, Origin, OriginKind, Position, RiskReason,
    RiskReasonCode, RiskVerdict, RiskVerdictStatus, Side, SizeUnit, TimeInForce,
};
use aether_core::quote::{BookLevel, OrderBook, Quote, QuoteSource};
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use serde::Serialize;
use std::fs;

#[derive(Serialize)]
struct GoldenEntry {
    name: String,
    #[serde(rename = "type")]
    typ: String,
    value: serde_json::Value,
    sha256: String,
}

fn mk_ulid(s: &str) -> Ulid {
    Ulid::from_string(s).unwrap()
}
fn mk_key(s: &str) -> MarketKey {
    MarketKey::from_string_unchecked(s)
}
fn ts() -> UtcTime {
    UtcTime::from_unix_millis(1752150896789).unwrap()
}
fn mk_money(amt: i64, scale: u32, ccy: &str) -> Money {
    Money::new(Decimal::new(amt, scale), ccy)
}

fn make_golden<T: Serialize>(name: &str, typ: &str, value: &T) -> GoldenEntry {
    GoldenEntry {
        name: name.into(),
        typ: typ.into(),
        value: serde_json::to_value(value).expect("serialize"),
        sha256: canonical_sha256(value).expect("sha256"),
    }
}

fn write_goldens(dir: &str, file: &str, entries: &[GoldenEntry]) {
    let json = serde_json::to_string_pretty(entries).expect("serialize");
    fs::write(format!("{dir}/{file}"), json).expect("write");
}

fn main() {
    let out = "testdata/golden/core";
    fs::create_dir_all(out).expect("create dir");

    // ── Money (4 vectors) ──
    write_goldens(
        out,
        "money.json",
        &[
            make_golden("money_usd", "Money", &mk_money(12345, 2, "USD")),
            make_golden("money_zero", "Money", &Money::zero("USDC")),
            make_golden("money_negative", "Money", &mk_money(-5000, 2, "USD")),
            make_golden("money_18dec", "Money", &mk_money(123456789012345678i64, 18, "BTC")),
        ],
    );

    // ── MarketKey (2 vectors) ──
    write_goldens(
        out,
        "market_key.json",
        &[
            make_golden("market_key_kalshi", "MarketKey", &mk_key("mkt:kalshi:BTC-75")),
            make_golden(
                "market_key_polymarket",
                "MarketKey",
                &mk_key("mkt:polymarket:WILL-BTC-TOUCH-100K"),
            ),
        ],
    );

    // ── Confidence (3 vectors) ──
    write_goldens(
        out,
        "confidence.json",
        &[
            make_golden("confidence_zero", "Confidence", &Confidence::new(Decimal::ZERO).unwrap()),
            make_golden(
                "confidence_half",
                "Confidence",
                &Confidence::new(Decimal::new(5, 1)).unwrap(),
            ),
            make_golden("confidence_one", "Confidence", &Confidence::new(Decimal::ONE).unwrap()),
        ],
    );

    // ── EdgeDecomposition (3 vectors) ──
    write_goldens(
        out,
        "edge.json",
        &[
            make_golden(
                "edge_positive_net",
                "EdgeDecomposition",
                &EdgeDecomposition::compute(
                    Decimal::new(100, 2),
                    EdgeCosts {
                        fees: Decimal::new(10, 2),
                        slippage_est: Decimal::new(5, 2),
                        funding_cost: Decimal::ZERO,
                        gas_cost: Decimal::new(2, 2),
                        bridge_cost: Decimal::ZERO,
                        settlement_mismatch_discount: Decimal::ZERO,
                        liquidity_haircut: Decimal::new(3, 2),
                        staleness_penalty: Decimal::ZERO,
                        confidence_penalty: Decimal::ZERO,
                    },
                ),
            ),
            make_golden(
                "edge_zero_net",
                "EdgeDecomposition",
                &EdgeDecomposition::compute(
                    Decimal::new(20, 2),
                    EdgeCosts {
                        fees: Decimal::new(10, 2),
                        slippage_est: Decimal::new(5, 2),
                        funding_cost: Decimal::ZERO,
                        gas_cost: Decimal::new(2, 2),
                        bridge_cost: Decimal::ZERO,
                        settlement_mismatch_discount: Decimal::ZERO,
                        liquidity_haircut: Decimal::new(3, 2),
                        staleness_penalty: Decimal::ZERO,
                        confidence_penalty: Decimal::ZERO,
                    },
                ),
            ),
            make_golden(
                "edge_explicit_zeros",
                "EdgeDecomposition",
                &EdgeDecomposition::compute(
                    Decimal::new(10, 2),
                    EdgeCosts {
                        fees: Decimal::ZERO,
                        slippage_est: Decimal::ZERO,
                        funding_cost: Decimal::ZERO,
                        gas_cost: Decimal::ZERO,
                        bridge_cost: Decimal::ZERO,
                        settlement_mismatch_discount: Decimal::ZERO,
                        liquidity_haircut: Decimal::new(10, 2),
                        staleness_penalty: Decimal::ZERO,
                        confidence_penalty: Decimal::ZERO,
                    },
                ),
            ),
        ],
    );

    // ── Quote (2 vectors) ──
    write_goldens(
        out,
        "quote.json",
        &[
            make_golden(
                "quote_full",
                "Quote",
                &Quote {
                    market: mk_key("mkt:kalshi:BTC-75"),
                    bid: Some(Decimal::new(65, 2)),
                    ask: Some(Decimal::new(67, 2)),
                    mid: Some(Decimal::new(66, 2)),
                    last: None,
                    bid_size: Some(Decimal::new(1000, 0)),
                    ask_size: Some(Decimal::new(500, 0)),
                    ts: ts(),
                    source: QuoteSource::Stream,
                    seq: Some(1),
                },
            ),
            make_golden(
                "quote_minimal",
                "Quote",
                &Quote {
                    market: mk_key("mkt:polymarket:TEST"),
                    bid: None,
                    ask: None,
                    mid: None,
                    last: Some(Decimal::new(50, 2)),
                    bid_size: None,
                    ask_size: None,
                    ts: ts(),
                    source: QuoteSource::Poll,
                    seq: None,
                },
            ),
        ],
    );

    // ── OrderBook (2 vectors) ──
    write_goldens(
        out,
        "order_book.json",
        &[
            make_golden(
                "order_book_simple",
                "OrderBook",
                &OrderBook::new(
                    mk_key("mkt:kalshi:BTC-75"),
                    vec![
                        BookLevel { price: Decimal::new(995, 2), size: Decimal::new(10, 0) },
                        BookLevel { price: Decimal::new(990, 2), size: Decimal::new(5, 0) },
                    ],
                    vec![
                        BookLevel { price: Decimal::new(1000, 2), size: Decimal::new(10, 0) },
                        BookLevel { price: Decimal::new(1005, 2), size: Decimal::new(5, 0) },
                    ],
                    2,
                    ts(),
                    Some(42),
                )
                .unwrap(),
            ),
            make_golden(
                "order_book_empty_sides",
                "OrderBook",
                &OrderBook::new(mk_key("mkt:polymarket:EMPTY"), vec![], vec![], 0, ts(), None)
                    .unwrap(),
            ),
        ],
    );

    // ── OrderIntent (2 vectors) ──
    let quote_snap = Quote {
        market: mk_key("mkt:kalshi:INTC-50"),
        bid: Some(Decimal::new(65, 2)),
        ask: Some(Decimal::new(67, 2)),
        mid: None,
        last: None,
        bid_size: None,
        ask_size: None,
        ts: ts(),
        source: QuoteSource::Stream,
        seq: None,
    };
    write_goldens(
        out,
        "order_intent.json",
        &[
            make_golden(
                "intent_limit_buy",
                "OrderIntent",
                &OrderIntent {
                    id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    market: mk_key("mkt:kalshi:INTC-50"),
                    side: Side::Buy,
                    order_type: OrderType::Limit,
                    limit_price: Some(Decimal::new(66, 2)),
                    size: Decimal::new(10, 0),
                    size_unit: SizeUnit::Contracts,
                    tif: TimeInForce::Gtc,
                    paper: true,
                    origin: Origin::new(OriginKind::User, 3, mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"))
                        .unwrap(),
                    quote_snapshot: quote_snap.clone(),
                    caps_version: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    created_ts: ts(),
                },
            ),
            make_golden(
                "intent_market_sell",
                "OrderIntent",
                &OrderIntent {
                    id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    market: mk_key("mkt:polymarket:TEST"),
                    side: Side::Sell,
                    order_type: OrderType::Market,
                    limit_price: None,
                    size: Decimal::new(5, 0),
                    size_unit: SizeUnit::Shares,
                    tif: TimeInForce::Ioc,
                    paper: false,
                    origin: Origin::new(
                        OriginKind::Automation,
                        1,
                        mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    )
                    .unwrap(),
                    quote_snapshot: quote_snap,
                    caps_version: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    created_ts: ts(),
                },
            ),
        ],
    );

    // ── RiskVerdict (2 vectors) ──
    write_goldens(
        out,
        "risk_verdict.json",
        &[
            make_golden(
                "verdict_allow",
                "RiskVerdict",
                &RiskVerdict {
                    intent_id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    verdict: RiskVerdictStatus::Allow,
                    reasons: vec![],
                    ts: ts(),
                },
            ),
            make_golden(
                "verdict_deny",
                "RiskVerdict",
                &RiskVerdict {
                    intent_id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    verdict: RiskVerdictStatus::Deny,
                    reasons: vec![RiskReason {
                        code: RiskReasonCode::CapExceeded,
                        detail: "cap $500 < $1000".into(),
                    }],
                    ts: ts(),
                },
            ),
        ],
    );

    // ── Order + Fill (2 vectors each) ──
    write_goldens(
        out,
        "order.json",
        &[
            make_golden(
                "order_live",
                "Order",
                &Order {
                    order_id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    market: mk_key("mkt:kalshi:INTC-50"),
                    side: Side::Buy,
                    price: Decimal::new(66, 2),
                    size: Decimal::new(10, 0),
                    fee: mk_money(50, 2, "USD"),
                    venue_ref: serde_json::json!({"order_ref": "kalshi-123"}),
                    ts: ts(),
                    paper: false,
                },
            ),
            make_golden(
                "order_paper",
                "Order",
                &Order {
                    order_id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    market: mk_key("mkt:polymarket:TEST"),
                    side: Side::Sell,
                    price: Decimal::new(5050, 2),
                    size: Decimal::new(5, 0),
                    fee: mk_money(125, 3, "USDC"),
                    venue_ref: serde_json::json!({"order_ref": "pm-456"}),
                    ts: ts(),
                    paper: true,
                },
            ),
        ],
    );

    write_goldens(
        out,
        "fill.json",
        &[
            make_golden(
                "fill_partial",
                "Fill",
                &Fill {
                    order_id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    market: mk_key("mkt:kalshi:INTC-50"),
                    side: Side::Buy,
                    price: Decimal::new(66, 2),
                    size: Decimal::new(6, 0),
                    fee: mk_money(30, 2, "USD"),
                    venue_ref: serde_json::json!({"fill_ref": "kalshi-f-789"}),
                    ts: ts(),
                    paper: false,
                },
            ),
            make_golden(
                "fill_full",
                "Fill",
                &Fill {
                    order_id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    market: mk_key("mkt:polymarket:TEST"),
                    side: Side::Sell,
                    price: Decimal::new(5050, 2),
                    size: Decimal::new(5, 0),
                    fee: mk_money(125, 3, "USDC"),
                    venue_ref: serde_json::json!({"fill_ref": "pm-f-012"}),
                    ts: ts(),
                    paper: true,
                },
            ),
        ],
    );

    // ── Position (2 vectors) ──
    write_goldens(
        out,
        "position.json",
        &[
            make_golden(
                "position_long",
                "Position",
                &Position {
                    market: mk_key("mkt:kalshi:INTC-50"),
                    side_exposure: Decimal::new(100, 0),
                    avg_price: Decimal::new(65, 2),
                    size: Decimal::new(10, 0),
                    realized_pnl: mk_money(0, 0, "USD"),
                    unrealized_pnl: mk_money(1000, 2, "USD"),
                    ts: ts(),
                },
            ),
            make_golden(
                "position_flat",
                "Position",
                &Position {
                    market: mk_key("mkt:polymarket:TEST"),
                    side_exposure: Decimal::ZERO,
                    avg_price: Decimal::ZERO,
                    size: Decimal::ZERO,
                    realized_pnl: mk_money(-5000, 2, "USD"),
                    unrealized_pnl: Money::zero("USD"),
                    ts: ts(),
                },
            ),
        ],
    );

    // ── CapsSnapshot (1 vector) ──
    write_goldens(
        out,
        "caps_snapshot.json",
        &[make_golden(
            "caps_default",
            "CapsSnapshot",
            &CapsSnapshot {
                version: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                per_order_max: mk_money(50000, 2, "USD"),
                daily_max: mk_money(100000, 2, "USD"),
                per_venue: serde_json::Map::new(),
                per_kind: serde_json::Map::new(),
            },
        )],
    );

    // ── Market (1 vector) ──
    write_goldens(
        out,
        "market.json",
        &[make_golden(
            "market_kalshi_open",
            "Market",
            &Market {
                key: mk_key("mkt:kalshi:BTC-75"),
                venue: VenueId::new("kalshi"),
                kind: InstrumentKind::BinaryContract,
                title: "BTC above $75k?".into(),
                description_ref: "BTC-75K-JUL10".into(),
                status: MarketStatus::Open,
                close_ts: None,
                resolve_ts: None,
                outcome: None,
                jurisdiction_flags: vec!["US".into()],
                venue_ref: aether_core::market::JsonObject::new(
                    serde_json::json!({"ticker": "BTC-75K-JUL10"}),
                )
                .unwrap(),
                meta: aether_core::market::JsonObject::new(
                    serde_json::json!({"tick_size": "0.01"}),
                )
                .unwrap(),
            },
        )],
    );

    // ── PriceSemantics (3 vectors) ──
    write_goldens(
        out,
        "price_semantics.json",
        &[
            make_golden(
                "ps_probability",
                "PriceSemantics",
                &PriceSemantics::Probability {
                    tick_size: aether_core::market::DecimalString::new("0.01").unwrap(),
                },
            ),
            make_golden(
                "ps_scalar",
                "PriceSemantics",
                &PriceSemantics::Scalar {
                    unit: "points".into(),
                    min: aether_core::market::DecimalString::new("0").unwrap(),
                    max: aether_core::market::DecimalString::new("100").unwrap(),
                },
            ),
            make_golden("ps_currency", "PriceSemantics", &PriceSemantics::Currency),
        ],
    );

    // ── Opportunity (1 vector) ──
    write_goldens(
        out,
        "opportunity.json",
        &[make_golden(
            "opp_arbitrage",
            "Opportunity",
            &Opportunity {
                id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                kind: OpportunityKind::Arbitrage,
                legs: vec![
                    OpportunityLeg {
                        market: mk_key("mkt:kalshi:BTC-75"),
                        side: Side::Buy,
                        target_price: Some(Decimal::new(65, 2)),
                        size_hint: Some(Decimal::new(10, 0)),
                    },
                    OpportunityLeg {
                        market: mk_key("mkt:polymarket:WILL-BTC-TOUCH-100K"),
                        side: Side::SellNo,
                        target_price: Some(Decimal::new(68, 2)),
                        size_hint: Some(Decimal::new(10, 0)),
                    },
                ],
                gross_edge: Decimal::new(300, 2),
                edge: EdgeDecomposition::compute(
                    Decimal::new(300, 2),
                    EdgeCosts {
                        fees: Decimal::new(20, 2),
                        slippage_est: Decimal::new(5, 2),
                        funding_cost: Decimal::ZERO,
                        gas_cost: Decimal::new(3, 2),
                        bridge_cost: Decimal::ZERO,
                        settlement_mismatch_discount: Decimal::ZERO,
                        liquidity_haircut: Decimal::new(2, 2),
                        staleness_penalty: Decimal::ZERO,
                        confidence_penalty: Decimal::new(10, 2),
                    },
                ),
                confidence: Confidence::new(Decimal::new(8, 1)).unwrap(),
                detected_ts: ts(),
                expires_ts: None,
                explain_ref: BrainRef {
                    object_id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                    provenance_hash: "abc123def456".into(),
                },
                trace_id: mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
            },
        )],
    );

    // ── AuditEvent (1 vector) ──
    write_goldens(
        out,
        "audit_event.json",
        &[make_golden(
            "audit_genesis",
            "AuditEvent",
            &AuditEvent {
                seq: 1,
                prev_hash: String::new(),
                hash: "sha256:abc123def456".into(),
                ts: ts(),
                actor: "system".into(),
                action: "genesis".into(),
                subject: "audit_chain".into(),
                payload_hash: "sha256:000".into(),
            },
        )],
    );

    // ── ErrorEnvelope (2 vectors) ──
    write_goldens(
        out,
        "error_envelope.json",
        &[
            make_golden(
                "error_invalid",
                "ErrorEnvelope",
                &ErrorEnvelope::new(
                    ErrorCode::InvalidArgument,
                    "market key not found",
                    mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                ),
            ),
            make_golden(
                "error_unavailable",
                "ErrorEnvelope",
                &ErrorEnvelope::new(
                    ErrorCode::Unavailable,
                    "venue not reachable",
                    mk_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
                ),
            ),
        ],
    );

    println!("Golden vectors generated in {out}");
    println!("Files: money, market_key, confidence, edge, quote, order_book, order_intent, risk_verdict, order, fill, position, caps_snapshot, market, price_semantics, opportunity, audit_event, error_envelope");
}

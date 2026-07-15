//! Simulator integration tests including the EP-304 parity contract.
//!
//! SPEC-012: The simulator MUST produce identical fills to the paper ledger
//! (direct `walk_book` call) for the same book + intent.  This is the
//! **parity contract** between EP-304 (paper ledger) and EP-307 (simulator).

use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::order::{
    OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce,
};
use aether_core::quote::{BookLevel, OrderBook, Quote, QuoteSource};
use aether_core::time::UtcTime;
use aether_core::Fill;
use aether_fillmodel::config::FillConfig;
use aether_fillmodel::walk::walk_book;
use aether_simulator::{SimulationConfig, SimulationInput, Simulator};
use rust_decimal::Decimal;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn test_market() -> MarketKey {
    MarketKey::new(&VenueId::new("test").unwrap(), "TST").unwrap()
}

fn test_book() -> OrderBook {
    let ts = UtcTime::from_unix_millis(1752152096000).unwrap();
    OrderBook::new(
        test_market(),
        // Bids descending
        vec![
            BookLevel {
                price: Decimal::new(9980, 2),
                size: Decimal::new(1000, 0),
            },
            BookLevel {
                price: Decimal::new(9970, 2),
                size: Decimal::new(500, 0),
            },
        ],
        // Asks ascending
        vec![
            BookLevel {
                price: Decimal::new(10020, 2),
                size: Decimal::new(1000, 0),
            },
            BookLevel {
                price: Decimal::new(10050, 2),
                size: Decimal::new(500, 0),
            },
        ],
        2,
        ts,
        None,
    )
    .expect("valid test book")
}

/// Build an OrderIntent matching the simulator's internal `build_intent`.
fn paper_ledger_intent(
    book: &OrderBook,
    side: Side,
    size: Decimal,
) -> OrderIntent {
    let origin =
        Origin::new(OriginKind::Automation, 3, Ulid::new()).expect("valid origin with tier 3");
    let quote_snapshot = Quote {
        market: book.market.clone(),
        bid: book.bids().first().map(|l| l.price),
        ask: book.asks().first().map(|l| l.price),
        mid: None,
        last: None,
        bid_size: book.bids().first().map(|l| l.size),
        ask_size: book.asks().first().map(|l| l.size),
        ts: book.ts,
        source: QuoteSource::Snapshot,
        seq: book.seq,
    };
    OrderIntent {
        id: Ulid::new(),
        market: book.market.clone(),
        side,
        order_type: OrderType::Market,
        limit_price: None,
        size,
        size_unit: SizeUnit::Contracts,
        tif: TimeInForce::Ioc,
        paper: true,
        origin,
        quote_snapshot,
        caps_version: Ulid::new(),
        created_ts: UtcTime::now(),
    }
}

// ── Parity contract tests ───────────────────────────────────────────────────

/// The core SPEC-012 parity contract.
///
/// Given the same book + size, the simulator's fill path must produce
/// fills that are byte-for-byte equivalent (price, size, side, market)
/// to what the paper ledger produces via direct `walk_book`.
#[test]
fn parity_simulator_fills_match_paper_ledger() {
    let book = test_book();
    let notional = Decimal::new(100, 0); // small enough to fill at first level

    // ── Paper-ledger path: direct walk_book ──
    let paper_intent = paper_ledger_intent(&book, Side::Buy, notional);
    let fill_config = FillConfig::default();
    let paper_fills: Vec<Fill> =
        walk_book(&book, &paper_intent, &fill_config).expect("paper ledger walk should succeed");

    // ── Simulator path ──
    let input = SimulationInput {
        buy_price: Decimal::new(100, 0),
        sell_price: Decimal::new(101, 0),
        price_kind: "probability".into(),
        notional,
        buy_book: Some(book.clone()),
        sell_book: None,
        funding_rate: Decimal::ZERO,
        hold_hours: Decimal::ZERO,
        max_quote_age_ms: 0,
        tick_stale_ms: 5000,
        confidence: Decimal::ONE,
        is_cross_chain: false,
        mismatch_discount: Decimal::ZERO,
    };
    let simulator = Simulator::new(SimulationConfig::default());
    let sim_result =
        simulator.simulate(&input).expect("simulator should succeed");

    // ── Assertions ──
    assert_eq!(
        sim_result.buy_fills.len(),
        paper_fills.len(),
        "parity: fill count must match paper ledger"
    );

    for (i, (sim_fill, paper_fill)) in
        sim_result.buy_fills.iter().zip(paper_fills.iter()).enumerate()
    {
        assert_eq!(
            sim_fill.price, paper_fill.price,
            "parity: fill {i} price mismatch"
        );
        assert_eq!(
            sim_fill.size, paper_fill.size,
            "parity: fill {i} size mismatch"
        );
        assert_eq!(
            sim_fill.side, paper_fill.side,
            "parity: fill {i} side mismatch"
        );
        assert_eq!(
            sim_fill.market, paper_fill.market,
            "parity: fill {i} market mismatch"
        );
        assert_eq!(
            sim_fill.fee.amount, paper_fill.fee.amount,
            "parity: fill {i} fee amount mismatch"
        );
        assert_eq!(
            sim_fill.fee.currency, paper_fill.fee.currency,
            "parity: fill {i} fee currency mismatch"
        );
        assert_eq!(
            sim_fill.paper, paper_fill.paper,
            "parity: fill {i} paper flag mismatch"
        );
    }
}

/// Same test for Sell side — symmetric parity guarantee.
#[test]
fn parity_simulator_sell_fills_match_paper_ledger() {
    let book = test_book();
    let notional = Decimal::new(100, 0);

    // ── Paper-ledger path ──
    let paper_intent = paper_ledger_intent(&book, Side::Sell, notional);
    let fill_config = FillConfig::default();
    let paper_fills: Vec<Fill> =
        walk_book(&book, &paper_intent, &fill_config).expect("paper ledger sell walk");

    // ── Simulator path ──
    let input = SimulationInput {
        buy_price: Decimal::new(100, 0),
        sell_price: Decimal::new(101, 0),
        price_kind: "probability".into(),
        notional,
        buy_book: None,
        sell_book: Some(book),
        funding_rate: Decimal::ZERO,
        hold_hours: Decimal::ZERO,
        max_quote_age_ms: 0,
        tick_stale_ms: 5000,
        confidence: Decimal::ONE,
        is_cross_chain: false,
        mismatch_discount: Decimal::ZERO,
    };
    let simulator = Simulator::new(SimulationConfig::default());
    let sim_result =
        simulator.simulate(&input).expect("simulator should succeed");

    assert_eq!(
        sim_result.sell_fills.len(),
        paper_fills.len(),
        "parity: sell fill count must match paper ledger"
    );

    for (i, (sim_fill, paper_fill)) in
        sim_result.sell_fills.iter().zip(paper_fills.iter()).enumerate()
    {
        assert_eq!(sim_fill.price, paper_fill.price, "parity: sell fill {i} price mismatch");
        assert_eq!(sim_fill.size, paper_fill.size, "parity: sell fill {i} size mismatch");
        assert_eq!(sim_fill.side, paper_fill.side, "parity: sell fill {i} side mismatch");
    }
}

/// Fill amounts are deterministic — running the simulator twice on the
/// same input produces identical results.
#[test]
fn parity_deterministic_repeatability() {
    let book = test_book();
    let input = SimulationInput {
        buy_price: Decimal::new(100, 0),
        sell_price: Decimal::new(101, 0),
        price_kind: "probability".into(),
        notional: Decimal::new(100, 0),
        buy_book: Some(book),
        sell_book: None,
        funding_rate: Decimal::ZERO,
        hold_hours: Decimal::ZERO,
        max_quote_age_ms: 0,
        tick_stale_ms: 5000,
        confidence: Decimal::ONE,
        is_cross_chain: false,
        mismatch_discount: Decimal::ZERO,
    };
    let sim = Simulator::new(SimulationConfig::default());

    let r1 = sim.simulate(&input).expect("first run");
    let r2 = sim.simulate(&input).expect("second run");

    assert_eq!(r1.buy_fills.len(), r2.buy_fills.len());
    for (a, b) in r1.buy_fills.iter().zip(r2.buy_fills.iter()) {
        assert_eq!(a.price, b.price);
        assert_eq!(a.size, b.size);
    }
}

#![allow(clippy::unwrap_used)]

use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::order::{OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce};
use aether_core::quote::{BookLevel, OrderBook, Quote, QuoteSource};
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use std::str::FromStr;

use aether_fillmodel::config::FillConfig;
use aether_fillmodel::walk::{walk_book, FillError};

fn make_book(bids: Vec<(i64, i64)>, asks: Vec<(i64, i64)>) -> OrderBook {
    let venue = VenueId::new("test").unwrap();
    let market = MarketKey::new(&venue, "TEST").unwrap();
    let ts = UtcTime::from_unix_millis(1_750_000_000_000).unwrap();
    let bids: Vec<BookLevel> = bids
        .into_iter()
        .map(|(p, s)| BookLevel { price: Decimal::new(p, 2), size: Decimal::new(s, 0) })
        .collect();
    let asks: Vec<BookLevel> = asks
        .into_iter()
        .map(|(p, s)| BookLevel { price: Decimal::new(p, 2), size: Decimal::new(s, 0) })
        .collect();
    OrderBook::new(market, bids, asks, 2, ts, None).unwrap()
}

fn make_intent(side: Side, size: i64, limit: Option<i64>, paper: bool) -> OrderIntent {
    let venue = VenueId::new("test").unwrap();
    let market = MarketKey::new(&venue, "TEST").unwrap();
    OrderIntent {
        id: Ulid::new(),
        market: market.clone(),
        side,
        order_type: if limit.is_some() { OrderType::Limit } else { OrderType::Market },
        limit_price: limit.map(|l| Decimal::new(l, 2)),
        size: Decimal::new(size, 0),
        size_unit: SizeUnit::Contracts,
        tif: TimeInForce::Day,
        paper,
        origin: Origin::new(OriginKind::Agent, 3, Ulid::new()).unwrap(),
        quote_snapshot: Quote {
            market,
            bid: Some(Decimal::new(99, 2)),
            ask: Some(Decimal::new(101, 2)),
            mid: Some(Decimal::new(100, 2)),
            last: None,
            bid_size: Some(Decimal::new(1000, 0)),
            ask_size: Some(Decimal::new(500, 0)),
            ts: UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
            source: QuoteSource::Stream,
            seq: None,
        },
        caps_version: Ulid::new(),
        created_ts: UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
    }
}

#[test]
fn golden_buy_fills_against_asks() {
    let book = make_book(
        vec![(99, 100)],                        // bids
        vec![(101, 50), (102, 50), (103, 100)], // asks: 50@1.01, 50@1.02, 100@1.03
    );
    let intent = make_intent(Side::Buy, 80, None, true);
    let config = FillConfig::default();
    let fills = walk_book(&book, &intent, &config).unwrap();

    assert_eq!(fills.len(), 2);
    assert_eq!(fills[0].price, Decimal::new(101, 2)); // 50 @ 1.01
    assert_eq!(fills[0].size, Decimal::new(50, 0));
    assert_eq!(fills[1].price, Decimal::new(102, 2)); // 30 @ 1.02
    assert_eq!(fills[1].size, Decimal::new(30, 0));
}

#[test]
fn golden_buy_limit_violation() {
    let book = make_book(vec![(99, 100)], vec![(105, 100)]);
    let intent = make_intent(Side::Buy, 50, Some(102), true); // limit 1.02, best ask 1.05
    let config = FillConfig::default();
    let result = walk_book(&book, &intent, &config);
    assert!(matches!(result, Err(FillError::LimitViolation { .. })));
}

#[test]
fn golden_depth_exhaustion() {
    let book = make_book(vec![(99, 1000)], vec![(101, 10)]); // only 10 visible
    let intent = make_intent(Side::Buy, 50, None, true);
    let config = FillConfig::default();
    let fills = walk_book(&book, &intent, &config).unwrap();

    assert_eq!(fills.len(), 2);
    assert_eq!(fills[0].price, Decimal::new(101, 2)); // 10 visible
    assert_eq!(fills[0].size, Decimal::new(10, 0));
    // Remaining 40 at 1.01 × 1.05 = 1.0605
    assert_eq!(fills[1].price, Decimal::new(10605, 4)); // 1.0605
    assert_eq!(fills[1].size, Decimal::new(40, 0));
}

#[test]
fn golden_passive_at_touch_only_first_level() {
    let book = make_book(vec![(99, 1000)], vec![(101, 10), (102, 100)]);
    let intent = make_intent(Side::Buy, 50, None, true);
    let config = FillConfig::passive();
    let fills = walk_book(&book, &intent, &config).unwrap();

    // Only fills at first level (best ask)
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, Decimal::new(101, 2));
    assert_eq!(fills[0].size, Decimal::new(10, 0)); // only the 10 available at best
}

#[test]
fn golden_no_liquidity_on_empty_book() {
    let book = make_book(vec![], vec![]);
    let intent = make_intent(Side::Buy, 10, None, true);
    let config = FillConfig::default();
    let result = walk_book(&book, &intent, &config);
    assert!(matches!(result, Err(FillError::NoLiquidity { .. })));
}

#[test]
fn golden_determinism_same_inputs_same_outputs() {
    let book = make_book(vec![(99, 100)], vec![(101, 50), (102, 50)]);
    let intent = make_intent(Side::Buy, 60, None, true);
    let config = FillConfig::default();

    let first = walk_book(&book, &intent, &config).unwrap();
    let second = walk_book(&book, &intent, &config).unwrap();

    assert_eq!(first, second);
}

#[test]
fn golden_sell_fills_against_bids() {
    let book = make_book(
        vec![(99, 50), (98, 50), (97, 100)], // bids: 50@0.99, 50@0.98, 100@0.97
        vec![(101, 100)],                    // asks
    );
    let intent = make_intent(Side::Sell, 80, None, true);
    let config = FillConfig::default();
    let fills = walk_book(&book, &intent, &config).unwrap();

    assert_eq!(fills.len(), 2);
    assert_eq!(fills[0].price, Decimal::new(99, 2)); // 50 @ 0.99
    assert_eq!(fills[0].size, Decimal::new(50, 0));
    assert_eq!(fills[1].price, Decimal::new(98, 2)); // 30 @ 0.98
    assert_eq!(fills[1].size, Decimal::new(30, 0));
}

#[test]
fn golden_full_fill_exact_size() {
    let book = make_book(vec![(99, 100)], vec![(101, 50)]);
    let intent = make_intent(Side::Buy, 50, None, true);
    let config = FillConfig::default();
    let fills = walk_book(&book, &intent, &config).unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].size, Decimal::new(50, 0));
}

#[test]
fn golden_sell_limit_violation() {
    let book = make_book(vec![(95, 100)], vec![(101, 100)]);
    let intent = make_intent(Side::Sell, 50, Some(97), true); // limit 0.97, best bid 0.95
    let config = FillConfig::default();
    let result = walk_book(&book, &intent, &config);
    assert!(matches!(result, Err(FillError::LimitViolation { .. })));
}

#[test]
fn golden_sell_depth_exhaustion_worsens_price() {
    let book = make_book(vec![(99, 10)], vec![(101, 1000)]);
    let intent = make_intent(Side::Sell, 50, None, true);
    let config = FillConfig::default();
    let fills = walk_book(&book, &intent, &config).unwrap();

    assert_eq!(fills.len(), 2);
    assert_eq!(fills[1].price, Decimal::new(99, 2) / Decimal::new(105, 2));
    assert!(fills[1].price < fills[0].price);
}

#[test]
fn limit_order_keeps_executable_partial_fill() {
    let book = make_book(vec![(99, 100)], vec![(101, 10), (103, 100)]);
    let intent = make_intent(Side::Buy, 50, Some(102), true);
    let fills = walk_book(&book, &intent, &FillConfig::default()).unwrap();

    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, Decimal::new(101, 2));
    assert_eq!(fills[0].size, Decimal::new(10, 0));
}

#[test]
fn rejects_mismatched_book_market() {
    let book = make_book(vec![(99, 100)], vec![(101, 100)]);
    let mut intent = make_intent(Side::Buy, 10, None, true);
    intent.market = MarketKey::new(&VenueId::new("other").unwrap(), "OTHER").unwrap();

    assert!(matches!(
        walk_book(&book, &intent, &FillConfig::default()),
        Err(FillError::MarketMismatch { .. })
    ));
}

#[test]
fn rejects_non_positive_intent_size() {
    let book = make_book(vec![(99, 100)], vec![(101, 100)]);
    let intent = make_intent(Side::Buy, 0, None, true);

    assert!(matches!(
        walk_book(&book, &intent, &FillConfig::default()),
        Err(FillError::InvalidInput(_))
    ));
}

#[test]
fn arithmetic_overflow_is_an_error_not_a_panic() {
    let venue = VenueId::new("test").unwrap();
    let market = MarketKey::new(&venue, "TEST").unwrap();
    let book = OrderBook::new(
        market,
        vec![],
        vec![BookLevel { price: Decimal::MAX, size: Decimal::new(2, 0) }],
        1,
        UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
        None,
    )
    .unwrap();
    let intent = make_intent(Side::Buy, 2, None, true);

    assert!(matches!(
        walk_book(&book, &intent, &FillConfig::default()),
        Err(FillError::ArithmeticOverflow(_))
    ));
}

#[test]
fn published_parity_fixture_matches_public_walk_contract() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../testdata/golden/fillmodel/parity-basic.json"))
            .unwrap();
    let levels = |name: &str| {
        fixture[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|level| BookLevel {
                price: Decimal::from_str(level["price"].as_str().unwrap()).unwrap(),
                size: Decimal::from_str(level["size"].as_str().unwrap()).unwrap(),
            })
            .collect::<Vec<_>>()
    };
    let venue = VenueId::new("test").unwrap();
    let market = MarketKey::new(&venue, "TEST").unwrap();
    let book = OrderBook::new(
        market,
        levels("bids"),
        levels("asks"),
        2,
        UtcTime::from_unix_millis(1_750_000_000_000).unwrap(),
        None,
    )
    .unwrap();
    let intent = make_intent(Side::Buy, 60, None, true);
    let fills = aether_fillmodel::walk(&book, &intent, &FillConfig::default()).unwrap();
    let expected = fixture["expected"].as_array().unwrap();

    assert_eq!(fills.len(), expected.len());
    for (fill, expected) in fills.iter().zip(expected) {
        assert_eq!(fill.price, Decimal::from_str(expected["price"].as_str().unwrap()).unwrap());
        assert_eq!(fill.size, Decimal::from_str(expected["size"].as_str().unwrap()).unwrap());
    }
}

proptest::proptest! {
    #[test]
    fn deterministic_for_generated_books_and_sizes(
        best_ask_cents in 2i64..10_000,
        first_depth in 1i64..1_000,
        requested in 1i64..2_000,
    ) {
        let book = make_book(
            vec![(best_ask_cents - 1, 1_000)],
            vec![(best_ask_cents, first_depth), (best_ask_cents + 1, 1_000)],
        );
        let intent = make_intent(Side::Buy, requested, None, true);
        let first = walk_book(&book, &intent, &FillConfig::default()).unwrap();
        let second = walk_book(&book, &intent, &FillConfig::default()).unwrap();
        proptest::prop_assert_eq!(first, second);
    }
}

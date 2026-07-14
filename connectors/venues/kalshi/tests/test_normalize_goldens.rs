//! Golden tests for Kalshi normalization.
//!
//! These tests use hardcoded golden vectors to verify that normalization
//! produces exact, predictable output — not just "looks correct".
//!
//! Every field of the output is asserted against an expected value.

use aether_core::quote::QuoteSource;
use aether_venue_kalshi::normalize::{
    normalize_book, normalize_tick, KalshiBookLevel, KalshiBookSnapshot, KalshiTick,
};
use rust_decimal::Decimal;

// ---------------------------------------------------------------------------
// Tick golden
// ---------------------------------------------------------------------------

#[test]
fn normalize_tick_golden() {
    let raw = KalshiTick {
        ticker: "BTC-75".into(),
        price: Some(65),
        side: Some("yes".into()),
        ts: Some("2026-07-10T12:34:56.789Z".into()),
        volume: Some(1_500),
        bid: Some(64),
        ask: Some(66),
        last_price: Some(65),
        ..KalshiTick::default()
    };

    let quote = normalize_tick(&raw).unwrap();

    assert_eq!(quote.market.as_str(), "mkt:kalshi:btc-75");
    assert_eq!(quote.bid, Some(Decimal::new(64, 2))); // 0.64
    assert_eq!(quote.ask, Some(Decimal::new(66, 2))); // 0.66
    assert_eq!(quote.mid, None);
    assert_eq!(quote.last, Some(Decimal::new(65, 2))); // 0.65
    assert_eq!(quote.bid_size, None);
    assert_eq!(quote.ask_size, None);
    assert_eq!(quote.source, QuoteSource::Stream);
    assert_eq!(quote.seq, None);
    // Verify ts is the correct real-world timestamp
    // 2026-07-10T12:34:56.789Z = 1,752,150,896,789 ms since epoch
    assert_eq!(quote.ts.unix_millis(), 1_783_686_896_789);
}

#[test]
fn normalize_tick_golden_all_fields_present() {
    let raw = KalshiTick {
        ticker: "ETH-50".into(),
        price: Some(42),
        side: Some("no".into()),
        ts: Some("2026-07-11T08:15:30.500Z".into()),
        volume: Some(2_300),
        bid: Some(41),
        ask: Some(43),
        last_price: Some(42),
        ..KalshiTick::default()
    };

    let quote = normalize_tick(&raw).unwrap();

    assert_eq!(quote.market.as_str(), "mkt:kalshi:eth-50");
    assert_eq!(quote.bid, Some(Decimal::new(41, 2)));
    assert_eq!(quote.ask, Some(Decimal::new(43, 2)));
    assert_eq!(quote.last, Some(Decimal::new(42, 2)));
    assert_eq!(quote.ts.unix_millis(), 1_783_757_730_500);
}

#[test]
fn normalize_tick_golden_no_optional_fields() {
    let raw = KalshiTick {
        ticker: "TEST-1".into(),
        price: Some(50),
        side: Some("yes".into()),
        ts: Some("2026-07-10T12:00:00.000Z".into()),
        volume: None,
        bid: None,
        ask: None,
        last_price: None,
        ..KalshiTick::default()
    };

    let quote = normalize_tick(&raw).unwrap();

    assert_eq!(quote.market.as_str(), "mkt:kalshi:test-1");
    assert_eq!(quote.bid, None);
    assert_eq!(quote.ask, None);
    assert_eq!(quote.last, Some(Decimal::new(50, 2)));
    assert_eq!(quote.bid_size, None);
    assert_eq!(quote.ask_size, None);
}

// ---------------------------------------------------------------------------
// Malformed tick rejection
// ---------------------------------------------------------------------------

#[test]
fn normalize_tick_malformed_rejected() {
    // Missing ticker
    let raw = KalshiTick {
        ticker: "".into(),
        price: Some(50),
        side: Some("yes".into()),
        ts: Some("2026-07-10T12:00:00.000Z".into()),
        volume: None,
        bid: None,
        ask: None,
        last_price: None,
        ..KalshiTick::default()
    };
    assert!(normalize_tick(&raw).is_err());
}

#[test]
fn normalize_tick_bad_timestamp_rejected() {
    let raw = KalshiTick {
        ticker: "BTC-75".into(),
        price: Some(50),
        side: Some("yes".into()),
        ts: Some("definitely-not-a-timestamp".into()),
        volume: None,
        bid: None,
        ask: None,
        last_price: None,
        ..KalshiTick::default()
    };
    assert!(normalize_tick(&raw).is_err());
}

// ---------------------------------------------------------------------------
// Book golden
// ---------------------------------------------------------------------------

#[test]
fn normalize_book_golden() {
    let raw = KalshiBookSnapshot {
        ticker: "BTC-75".into(),
        ts: Some("2026-07-10T12:34:56.789Z".into()),
        bids: vec![
            KalshiBookLevel { price: 65, size: 500 },
            KalshiBookLevel { price: 64, size: 1_000 },
            KalshiBookLevel { price: 63, size: 2_000 },
        ],
        asks: vec![
            KalshiBookLevel { price: 66, size: 1_500 },
            KalshiBookLevel { price: 67, size: 1_200 },
            KalshiBookLevel { price: 68, size: 800 },
        ],
        ..KalshiBookSnapshot::default()
    };

    let ob = normalize_book(&raw).unwrap();

    assert_eq!(ob.market.as_str(), "mkt:kalshi:btc-75");
    assert_eq!(ob.ts.unix_millis(), 1_783_686_896_789);
    assert_eq!(ob.depth, 3);

    // Bids descending
    let bids = ob.bids();
    assert_eq!(bids.len(), 3);
    assert_eq!(bids[0].price, Decimal::new(65, 2)); // 0.65
    assert_eq!(bids[0].size, Decimal::new(500, 0));
    assert_eq!(bids[1].price, Decimal::new(64, 2)); // 0.64
    assert_eq!(bids[1].size, Decimal::new(1_000, 0));
    assert_eq!(bids[2].price, Decimal::new(63, 2)); // 0.63
    assert_eq!(bids[2].size, Decimal::new(2_000, 0));

    // Asks ascending
    let asks = ob.asks();
    assert_eq!(asks.len(), 3);
    assert_eq!(asks[0].price, Decimal::new(66, 2)); // 0.66
    assert_eq!(asks[0].size, Decimal::new(1_500, 0));
    assert_eq!(asks[1].price, Decimal::new(67, 2)); // 0.67
    assert_eq!(asks[1].size, Decimal::new(1_200, 0));
    assert_eq!(asks[2].price, Decimal::new(68, 2)); // 0.68
    assert_eq!(asks[2].size, Decimal::new(800, 0));
}

// ---------------------------------------------------------------------------
// Book ordering enforced
// ---------------------------------------------------------------------------

#[test]
fn normalize_book_bid_ordering_enforced() {
    // Bids are reverse-sorted (should get normalized to descending order)
    let raw = KalshiBookSnapshot {
        ticker: "BTC-75".into(),
        ts: Some("2026-07-10T12:34:56.789Z".into()),
        bids: vec![
            KalshiBookLevel { price: 63, size: 100 },
            KalshiBookLevel { price: 65, size: 300 },
            KalshiBookLevel { price: 64, size: 200 },
        ],
        asks: vec![],
        ..KalshiBookSnapshot::default()
    };

    let ob = normalize_book(&raw).unwrap();
    let bids = ob.bids();

    assert_eq!(bids.len(), 3);
    assert_eq!(bids[0].price, Decimal::new(65, 2));
    assert_eq!(bids[1].price, Decimal::new(64, 2));
    assert_eq!(bids[2].price, Decimal::new(63, 2));
}

#[test]
fn normalize_book_ask_ordering_enforced() {
    // Asks are reverse-sorted (should get normalized to ascending order)
    let raw = KalshiBookSnapshot {
        ticker: "BTC-75".into(),
        ts: Some("2026-07-10T12:34:56.789Z".into()),
        bids: vec![],
        asks: vec![
            KalshiBookLevel { price: 68, size: 100 },
            KalshiBookLevel { price: 66, size: 300 },
            KalshiBookLevel { price: 67, size: 200 },
        ],
        ..KalshiBookSnapshot::default()
    };

    let ob = normalize_book(&raw).unwrap();
    let asks = ob.asks();

    assert_eq!(asks.len(), 3);
    assert_eq!(asks[0].price, Decimal::new(66, 2));
    assert_eq!(asks[1].price, Decimal::new(67, 2));
    assert_eq!(asks[2].price, Decimal::new(68, 2));
}

#[test]
fn normalize_book_duplicate_prices_are_aggregated() {
    let raw = KalshiBookSnapshot {
        ticker: "BTC-75".into(),
        ts: Some("2026-07-10T12:34:56.789Z".into()),
        bids: vec![
            KalshiBookLevel { price: 65, size: 100 },
            KalshiBookLevel { price: 65, size: 200 },
        ],
        asks: vec![],
        ..KalshiBookSnapshot::default()
    };

    let book = normalize_book(&raw).unwrap();
    assert_eq!(book.bids().len(), 1);
    assert_eq!(book.bids()[0].size, Decimal::new(300, 0));
}

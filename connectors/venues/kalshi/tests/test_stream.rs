//! Integration tests for Kalshi WebSocket stream handling.
//!
//! Tests deserialization of recorded WS frames, subscribe message format,
//! gap detection, and full normalization pipelines from WS data types.
//!
//! No actual WebSocket connections are made — all tests use hardcoded JSON
//! frames and locally constructed types.

use aether_core::quote::{Quote, QuoteSource};
use aether_venue_kalshi::normalize::{
    normalize_book, normalize_tick, KalshiBookSnapshot, KalshiTick,
};
use aether_venue_kalshi::stream::{KalshiWsMessage, SeqTracker};
use rust_decimal::Decimal;

// ---------------------------------------------------------------------------
// Parse tick message
// ---------------------------------------------------------------------------

/// A recorded Kalshi tick WS frame.
const TICK_JSON: &str = r#"{
    "type": "ticker",
    "seq": 42,
    "data": {
        "ticker": "BTC-75",
        "price": 65,
        "side": "yes",
        "ts": "2026-07-10T12:34:56.789Z",
        "volume": 1500,
        "bid": 64,
        "ask": 66,
        "last_price": 65
    }
}"#;

#[test]
fn test_parse_tick_message() {
    let msg: KalshiWsMessage = serde_json::from_str(TICK_JSON).unwrap();

    assert_eq!(msg.msg_type, "ticker");
    assert_eq!(msg.seq, Some(42));

    let tick: KalshiTick = serde_json::from_value(msg.data).unwrap();
    assert_eq!(tick.ticker, "BTC-75");
    assert_eq!(tick.price, Some(65));
    assert_eq!(tick.side.as_deref(), Some("yes"));
    assert_eq!(tick.ts.as_deref(), Some("2026-07-10T12:34:56.789Z"));
    assert_eq!(tick.volume, Some(1500));
    assert_eq!(tick.bid, Some(64));
    assert_eq!(tick.ask, Some(66));
    assert_eq!(tick.last_price, Some(65));
}

// ---------------------------------------------------------------------------
// Parse book snapshot
// ---------------------------------------------------------------------------

const BOOK_JSON: &str = r#"{
    "type": "book_snapshot",
    "seq": 99,
    "data": {
        "ticker": "BTC-75",
        "ts": "2026-07-10T12:34:56.789Z",
        "bids": [
            {"price": 64, "size": 1000},
            {"price": 63, "size": 2000}
        ],
        "asks": [
            {"price": 66, "size": 1500},
            {"price": 67, "size": 1200}
        ]
    }
}"#;

#[test]
fn test_parse_book_snapshot() {
    let msg: KalshiWsMessage = serde_json::from_str(BOOK_JSON).unwrap();

    assert_eq!(msg.msg_type, "book_snapshot");
    assert_eq!(msg.seq, Some(99));

    let book: KalshiBookSnapshot = serde_json::from_value(msg.data).unwrap();
    assert_eq!(book.ticker, "BTC-75");
    assert_eq!(book.ts.as_deref(), Some("2026-07-10T12:34:56.789Z"));
    assert_eq!(book.bids.len(), 2);
    assert_eq!(book.asks.len(), 2);

    // First bid level
    assert_eq!(book.bids[0].price, 64);
    assert_eq!(book.bids[0].size, 1000);

    // First ask level
    assert_eq!(book.asks[0].price, 66);
    assert_eq!(book.asks[0].size, 1500);
}

// ---------------------------------------------------------------------------
// Subscribe message format
// ---------------------------------------------------------------------------

#[test]
fn test_subscribe_message_format() {
    use serde_json::json;

    let subscribe = json!({
        "type": "subscribe",
        "channels": ["ticker"],
        "tickers": ["BTC-75", "ETH-50"]
    });

    let json_str = serde_json::to_string(&subscribe).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["type"], "subscribe");
    assert_eq!(parsed["channels"][0], "ticker");
    assert_eq!(parsed["tickers"][0], "BTC-75");
    assert_eq!(parsed["tickers"][1], "ETH-50");
}

#[test]
fn test_subscribe_book_message_format() {
    use serde_json::json;

    let subscribe = json!({
        "type": "subscribe",
        "channels": ["book"],
        "tickers": ["BTC-75"]
    });

    let json_str = serde_json::to_string(&subscribe).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["type"], "subscribe");
    assert_eq!(parsed["channels"][0], "book");
    assert_eq!(parsed["tickers"][0], "BTC-75");
}

// ---------------------------------------------------------------------------
// Gap detection
// ---------------------------------------------------------------------------

#[test]
fn test_gap_detection() {
    let mut tracker = SeqTracker::new();

    // First seq — no gap
    assert!(tracker.observe(1).is_none());

    // Second seq contiguous — no gap
    assert!(tracker.observe(2).is_none());

    // Jump from 2 to 10 — should detect gap (expected=3, actual=10)
    let gap = tracker.observe(10);
    assert_eq!(gap, Some((3, 10)));
}

#[test]
fn test_gap_detection_no_gap_for_reset() {
    let mut tracker = SeqTracker::new();

    tracker.observe(100);
    tracker.reset();

    // After reset, receiving seq 1 is fine (no gap)
    assert!(tracker.observe(1).is_none());
}

// ---------------------------------------------------------------------------
// Normalize tick from WS
// ---------------------------------------------------------------------------

#[test]
fn test_normalize_tick_from_ws() {
    let msg: KalshiWsMessage = serde_json::from_str(TICK_JSON).unwrap();
    let tick: KalshiTick = serde_json::from_value(msg.data).unwrap();
    let quote: Quote = normalize_tick(&tick).unwrap();

    assert_eq!(quote.market.as_str(), "mkt:kalshi:btc-75");
    assert_eq!(quote.bid, Some(Decimal::new(64, 2)));
    assert_eq!(quote.ask, Some(Decimal::new(66, 2)));
    assert_eq!(quote.last, Some(Decimal::new(65, 2)));
    assert_eq!(quote.mid, None);
    assert_eq!(quote.bid_size, None);
    assert_eq!(quote.ask_size, None);
    assert_eq!(quote.source, QuoteSource::Stream);
    assert_eq!(quote.seq, None);
}

#[test]
fn test_normalize_tick_from_ws_minimal_fields() {
    let json = r#"{
        "type": "ticker",
        "data": {
            "ticker": "TEST-1",
            "price": 50,
            "side": "yes",
            "ts": "2026-07-10T12:00:00.000Z"
        }
    }"#;

    let msg: KalshiWsMessage = serde_json::from_str(json).unwrap();
    let tick: KalshiTick = serde_json::from_value(msg.data).unwrap();
    let quote = normalize_tick(&tick).unwrap();

    assert_eq!(quote.market.as_str(), "mkt:kalshi:test-1");
    assert_eq!(quote.bid, None);
    assert_eq!(quote.ask, None);
    assert_eq!(quote.last, Some(Decimal::new(50, 2)));
}

// ---------------------------------------------------------------------------
// Normalize book from WS
// ---------------------------------------------------------------------------

#[test]
fn test_normalize_book_from_ws() {
    let msg: KalshiWsMessage = serde_json::from_str(BOOK_JSON).unwrap();
    let book: KalshiBookSnapshot = serde_json::from_value(msg.data).unwrap();
    let ob = normalize_book(&book).unwrap();

    assert_eq!(ob.market.as_str(), "mkt:kalshi:btc-75");
    assert_eq!(ob.bids().len(), 2);
    assert_eq!(ob.asks().len(), 2);

    // Bids descending
    assert_eq!(ob.bids()[0].price, Decimal::new(64, 2));
    assert_eq!(ob.bids()[0].size, Decimal::new(1000, 0));
    assert_eq!(ob.bids()[1].price, Decimal::new(63, 2));

    // Asks ascending
    assert_eq!(ob.asks()[0].price, Decimal::new(66, 2));
    assert_eq!(ob.asks()[1].price, Decimal::new(67, 2));
}

#[test]
fn test_normalize_book_from_ws_empty_levels() {
    let json = r#"{
        "type": "book_snapshot",
        "data": {
            "ticker": "EMPTY-1",
            "ts": "2026-07-10T12:00:00.000Z",
            "bids": [],
            "asks": []
        }
    }"#;

    let msg: KalshiWsMessage = serde_json::from_str(json).unwrap();
    let book: KalshiBookSnapshot = serde_json::from_value(msg.data).unwrap();
    let ob = normalize_book(&book).unwrap();

    assert_eq!(ob.market.as_str(), "mkt:kalshi:empty-1");
    assert!(ob.bids().is_empty());
    assert!(ob.asks().is_empty());
    assert_eq!(ob.depth, 0);
}

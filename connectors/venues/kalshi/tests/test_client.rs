//! Integration tests for the Kalshi REST client using recorded JSON fixtures.
//!
//! These tests verify that deserialization and normalization work correctly
//! without requiring live API access.

use aether_venue_kalshi::client::{KalshiMarket, MarketsResponse};
use aether_venue_kalshi::normalize_market;

/// Load a fixture file from the `tests/fixtures/` directory.
fn load_fixture(name: &str) -> String {
    let path = format!("tests/fixtures/{}", name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read fixture {name}: {e}"))
}

// -----------------------------------------------------------------------
// Client deserialization tests
// -----------------------------------------------------------------------

#[test]
fn parse_markets_response_from_fixture() {
    let json = load_fixture("kalshi_markets.json");
    let resp: MarketsResponse =
        serde_json::from_str(&json).expect("should deserialize markets response from fixture");

    assert_eq!(resp.markets.len(), 3, "expected 3 markets in fixture");
    assert!(resp.cursor.is_some(), "expected cursor to be present");
}

#[test]
fn parse_markets_response_first_market() {
    let json = load_fixture("kalshi_markets.json");
    let resp: MarketsResponse = serde_json::from_str(&json).unwrap();

    let first = &resp.markets[0];
    assert_eq!(first.ticker, "BTC-75");
    assert_eq!(first.status, "open");
    assert_eq!(first.yes_ask, Some(48));
    assert_eq!(first.yes_bid, Some(47));
    assert_eq!(first.close_ts, Some(1_720_000_000_000));
    assert_eq!(first.volume, Some(154_230));
    assert_eq!(first.tick_size, Some(vec![1, 99]));
}

#[test]
fn parse_markets_response_settled_market() {
    let json = load_fixture("kalshi_markets.json");
    let resp: MarketsResponse = serde_json::from_str(&json).unwrap();

    let settled = &resp.markets[2];
    assert_eq!(settled.ticker, "UBER-25");
    assert_eq!(settled.status, "closed");
    assert_eq!(settled.result, Some("yes".into()));
    assert_eq!(settled.settlement_ts, Some(serde_json::json!(1_718_000_000_000_i64)));
}

#[test]
fn parse_single_market_response_from_fixture() {
    let json = load_fixture("kalshi_market_BTC-75.json");

    // Kalshi wraps single-market responses in { "market": { ... } }
    #[derive(serde::Deserialize)]
    struct MarketWrapper {
        market: KalshiMarket,
    }

    let wrapped: MarketWrapper = serde_json::from_str(&json)
        .expect("should deserialize single market response from fixture");

    assert_eq!(wrapped.market.ticker, "BTC-75");
    assert_eq!(wrapped.market.status, "open");
    assert_eq!(wrapped.market.yes_ask, Some(48));
}

// -----------------------------------------------------------------------
// Normalization tests
// -----------------------------------------------------------------------

#[test]
fn normalize_market_from_fixture() {
    let json = load_fixture("kalshi_market_BTC-75.json");

    #[derive(serde::Deserialize)]
    struct Wrapper {
        market: KalshiMarket,
    }

    let wrapped: Wrapper = serde_json::from_str(&json).unwrap();
    let domain =
        normalize_market(wrapped.market).expect("should normalize valid market from fixture");

    assert_eq!(domain.key.as_str(), "mkt:kalshi:btc-75");
    assert_eq!(domain.venue.as_str(), "kalshi");
    assert_eq!(domain.title, "Will Bitcoin be above $75,000 at 4 PM ET?");
    assert_eq!(domain.description_ref, "BTC > $75k?");
    assert!(domain.close_ts.is_some());
    assert!(domain.resolve_ts.is_none());
    assert!(domain.outcome.is_none());
}

#[test]
fn normalize_multiple_markets_from_fixture() {
    let json = load_fixture("kalshi_markets.json");
    let resp: MarketsResponse = serde_json::from_str(&json).unwrap();

    for raw in resp.markets {
        let domain = normalize_market(raw).expect("all fixtures should normalize without error");

        // All test markets should have valid keys
        assert!(
            domain.key.as_str().starts_with("mkt:kalshi:"),
            "key should start with mkt:kalshi:, got {}",
            domain.key
        );
    }
}

#[test]
fn normalize_market_ticker_lowercased() {
    let json = load_fixture("kalshi_market_BTC-75.json");

    #[derive(serde::Deserialize)]
    struct Wrapper {
        market: KalshiMarket,
    }

    let wrapped: Wrapper = serde_json::from_str(&json).unwrap();

    // Even though fixture has uppercase ticker, the key should be lowercased
    assert_eq!(wrapped.market.ticker, "BTC-75");

    let domain = normalize_market(wrapped.market).unwrap();
    assert_eq!(domain.key.as_str(), "mkt:kalshi:btc-75");
}

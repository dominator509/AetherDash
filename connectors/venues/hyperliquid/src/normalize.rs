//! Market normalization: Hyperliquid raw types -> canonical `aether_core::Market`, `Quote`, and `OrderBook`.
//!
//! | HL field | Canonical field | Notes |
//! |---|---|---|
//! | `asset.name` | `MarketKey` | `mkt:hyperliquid:{name}` lowercased |
//! | `asset.name` | `title` | Pass-through |
//! | — | `InstrumentKind` | Always `Perp` |
//! | — | `MarketStatus` | Always `Open` (unless delisted) |
//! | `asset.is_delisted` | `status` | `Closed` if delisted |
//! | raw JSON | `venue_ref` | Preserved for provenance |
//! | `asset.sz_decimals`, `max_leverage` | `meta` | Preserved |
//!
//! # Mid price -> Quote normalization
//!
//! Hyperliquid's `allMids` returns only a mid price per coin. Fields that the
//! venue did not publish remain absent; the boundary never invents a spread.
//!
//! # L2 book normalization
//!
//! Hyperliquid's `l2Book` returns levels with `levels[0]` = bids (descending)
//! and `levels[1]` = asks (ascending). We map directly to `BookLevel`.

use crate::client::{HlAsset, HlAssetCtx, HlBookSnapshot, HlSpotPair, HlSpotToken};
use aether_core::ids::{MarketKey, VenueId};
use aether_core::json::JsonObject;
use aether_core::market::{InstrumentKind, Market, MarketStatus};
use aether_core::quote::{BookLevel, OrderBook, OrderBookError, Quote, QuoteSource};
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Raw types (for internal use, including replay)
// ---------------------------------------------------------------------------

/// A tick from the allMids polling.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlMidTick {
    pub coin: String,
    pub mid: String,
    pub ts: i64,
}

/// Normalized ClobLevel for L2 book levels.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClobLevel {
    pub px: String,
    pub sz: String,
}

/// Normalized ClobBookSnapshot from l2Book.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClobBookSnapshot {
    pub coin: String,
    pub time: i64,
    pub bids: Vec<ClobLevel>,
    pub asks: Vec<ClobLevel>,
}

/// Normalized ClobTick (mid price tick with time).
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClobTick {
    pub coin: String,
    pub mid: String,
    pub ts: i64,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during market normalization.
#[derive(Error, Debug)]
pub enum NormalizeError {
    /// The market name is empty or missing.
    #[error("market has no name")]
    MissingName,

    /// Failed to construct a `MarketKey` from the name.
    #[error("invalid MarketKey: {0}")]
    MarketKey(#[from] aether_core::ids::MarketKeyError),

    /// Failed to construct the `VenueRef` JSON object.
    #[error("venue_ref construction failed: {0}")]
    JsonObject(#[from] aether_core::json::JsonObjectError),

    /// OrderBook validation failed (ordering invariant violated).
    #[error("order book error: {0}")]
    OrderBook(#[from] OrderBookError),

    /// Invalid decimal string.
    #[error("invalid decimal '{raw}': {detail}")]
    DecimalParse { raw: String, detail: String },

    /// Invalid timestamp.
    #[error("timestamp parse error for '{raw}': {detail}")]
    TimestampParse { raw: String, detail: String },

    /// No levels in the book snapshot.
    #[error("no levels in book snapshot for {coin}")]
    MissingLevels { coin: String },

    /// Level array doesn't have 2 elements (bids + asks).
    #[error("expected 2 level arrays (bids+asks), got {count} for {coin}")]
    WrongLevelCount { coin: String, count: usize },
}

// ---------------------------------------------------------------------------
// REST market normalization
// ---------------------------------------------------------------------------

/// Normalize a raw Hyperliquid asset into the canonical `aether_core::Market`.
pub fn normalize_market(raw: HlAsset, ctx: Option<&HlAssetCtx>) -> Result<Market, NormalizeError> {
    let name = raw.name.trim();
    if name.is_empty() {
        return Err(NormalizeError::MissingName);
    }

    let venue = VenueId::new("hyperliquid")
        .map_err(|e| NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw }))?;

    let key = MarketKey::new(&venue, &name.to_lowercase())?;

    let kind = InstrumentKind::Perp;

    let status =
        if raw.is_delisted == Some(true) { MarketStatus::Closed } else { MarketStatus::Open };

    let venue_ref = JsonObject::new(serde_json::to_value(&raw).unwrap_or_else(|_| json!({})))?;

    let meta_parts = json!({
        "sz_decimals": raw.sz_decimals,
        "max_leverage": raw.max_leverage,
        "margin_table_id": raw.margin_table_id,
        "only_isolated": raw.only_isolated,
        "is_delisted": raw.is_delisted,
        "funding": ctx.and_then(|c| c.funding.as_deref()),
        "open_interest": ctx.and_then(|c| c.open_interest.as_deref()),
        "prev_day_px": ctx.and_then(|c| c.prev_day_px.as_deref()),
        "day_ntl_vlm": ctx.and_then(|c| c.day_ntl_vlm.as_deref()),
        "oracle_px": ctx.and_then(|c| c.oracle_px.as_deref()),
        "mark_px": ctx.and_then(|c| c.mark_px.as_deref()),
        "mid_px": ctx.and_then(|c| c.mid_px.as_deref()),
    });

    let meta = JsonObject::new(meta_parts)?;

    Ok(Market {
        key,
        venue,
        kind,
        title: name.to_string(),
        description_ref: String::new(),
        status,
        close_ts: None,
        resolve_ts: None,
        outcome: None,
        jurisdiction_flags: vec!["US".to_string()],
        venue_ref,
        meta,
    })
}

/// Normalize one Hyperliquid spot pair using its stable API index (`@N`).
pub fn normalize_spot_market(
    raw: HlSpotPair,
    tokens: &[HlSpotToken],
    ctx: Option<&HlAssetCtx>,
) -> Result<Market, NormalizeError> {
    if raw.name.trim().is_empty() || raw.tokens.len() != 2 {
        return Err(NormalizeError::MissingName);
    }
    let venue = VenueId::new("hyperliquid")
        .map_err(|e| NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw }))?;
    let api_coin = format!("@{}", raw.index);
    let key = MarketKey::new(&venue, &api_coin)?;
    let token_name = |index: u32| {
        tokens.iter().find(|token| token.index == index).map(|token| token.name.as_str())
    };
    let venue_ref = JsonObject::new(serde_json::to_value(&raw).unwrap_or_else(|_| json!({})))?;
    let meta = JsonObject::new(json!({
        "api_coin": api_coin,
        "base_token": token_name(raw.tokens[0]),
        "quote_token": token_name(raw.tokens[1]),
        "mark_px": ctx.and_then(|value| value.mark_px.as_deref()),
        "mid_px": ctx.and_then(|value| value.mid_px.as_deref()),
        "day_ntl_vlm": ctx.and_then(|value| value.day_ntl_vlm.as_deref()),
    }))?;
    Ok(Market {
        key,
        venue,
        kind: InstrumentKind::Spot,
        title: raw.name,
        description_ref: String::new(),
        status: MarketStatus::Open,
        close_ts: None,
        resolve_ts: None,
        outcome: None,
        jurisdiction_flags: vec!["US".to_string()],
        venue_ref,
        meta,
    })
}

// ---------------------------------------------------------------------------
// Mid price -> Quote normalization
// ---------------------------------------------------------------------------

/// Convert a Hyperliquid mid price string to a canonical `Quote`.
///
/// Since `allMids` publishes no bid, ask, last, or sizes, those fields remain
/// absent. Real bid/ask data comes from `l2Book` or the BBO subscription.
pub fn normalize_mid_to_quote(
    coin: &str,
    mid_str: &str,
    ts_ms: i64,
) -> Result<Quote, NormalizeError> {
    if coin.is_empty() {
        return Err(NormalizeError::MissingName);
    }

    let venue = VenueId::new("hyperliquid")
        .map_err(|e| NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw }))?;
    let market = MarketKey::new(&venue, &coin.to_lowercase())?;
    let mid = parse_decimal(mid_str)?;
    if mid <= Decimal::ZERO {
        return Err(NormalizeError::DecimalParse {
            raw: mid_str.to_string(),
            detail: "mid price must be positive".to_string(),
        });
    }

    let ts = UtcTime::from_unix_millis(ts_ms).map_err(|e| NormalizeError::TimestampParse {
        raw: ts_ms.to_string(),
        detail: e.to_string(),
    })?;

    Ok(Quote {
        market,
        bid: None,
        ask: None,
        mid: Some(mid),
        last: None,
        bid_size: None,
        ask_size: None,
        ts,
        source: QuoteSource::Poll,
        seq: None,
    })
}

// ---------------------------------------------------------------------------
// L2 book normalization
// ---------------------------------------------------------------------------

/// Normalize a raw `HlBookSnapshot` into a canonical `OrderBook`.
pub fn normalize_book(raw: &HlBookSnapshot) -> Result<OrderBook, NormalizeError> {
    let coin = raw.coin.trim();
    if coin.is_empty() {
        return Err(NormalizeError::MissingName);
    }
    if raw.levels.len() != 2 {
        return Err(NormalizeError::WrongLevelCount {
            coin: coin.to_string(),
            count: raw.levels.len(),
        });
    }

    let venue = VenueId::new("hyperliquid")
        .map_err(|e| NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw }))?;
    let market = MarketKey::new(&venue, &coin.to_lowercase())?;

    let bids: Vec<BookLevel> = raw.levels[0]
        .iter()
        .map(|level| {
            let price = parse_decimal(&level.px)?;
            let size = parse_decimal(&level.sz)?;
            ensure_positive_level(&level.px, price, "price")?;
            ensure_positive_level(&level.sz, size, "size")?;
            Ok(BookLevel { price, size })
        })
        .collect::<Result<Vec<_>, NormalizeError>>()?;

    let asks: Vec<BookLevel> = raw.levels[1]
        .iter()
        .map(|level| {
            let price = parse_decimal(&level.px)?;
            let size = parse_decimal(&level.sz)?;
            ensure_positive_level(&level.px, price, "price")?;
            ensure_positive_level(&level.sz, size, "size")?;
            Ok(BookLevel { price, size })
        })
        .collect::<Result<Vec<_>, NormalizeError>>()?;

    let ts = UtcTime::from_unix_millis(raw.time).map_err(|e| NormalizeError::TimestampParse {
        raw: raw.time.to_string(),
        detail: e.to_string(),
    })?;

    let depth = bids.len().max(asks.len());

    Ok(OrderBook::new(market, bids, asks, depth, ts, None)?)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_decimal(value: &str) -> Result<Decimal, NormalizeError> {
    value.parse::<Decimal>().map_err(|error| NormalizeError::DecimalParse {
        raw: value.to_string(),
        detail: error.to_string(),
    })
}

fn ensure_positive_level(raw: &str, value: Decimal, field: &str) -> Result<(), NormalizeError> {
    if value > Decimal::ZERO {
        Ok(())
    } else {
        Err(NormalizeError::DecimalParse {
            raw: raw.to_string(),
            detail: format!("book {field} must be positive"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HlAsset, HlAssetCtx};

    fn make_asset() -> HlAsset {
        HlAsset {
            name: "BTC".into(),
            sz_decimals: Some(5),
            max_leverage: Some(40),
            margin_table_id: Some(56),
            only_isolated: Some(false),
            is_delisted: Some(false),
        }
    }

    fn make_ctx() -> HlAssetCtx {
        HlAssetCtx {
            funding: Some("0.00001234".into()),
            open_interest: Some("1234.5".into()),
            prev_day_px: Some("50000.0".into()),
            day_ntl_vlm: Some("1000000.0".into()),
            premium: Some("0.0001".into()),
            oracle_px: Some("50100.0".into()),
            mark_px: Some("50150.0".into()),
            mid_px: Some("50145.0".into()),
            impact_pxs: Some(vec!["50140.0".into(), "50150.0".into()]),
        }
    }

    #[test]
    fn normalize_open_market() {
        let raw = make_asset();
        let ctx = make_ctx();
        let m = normalize_market(raw, Some(&ctx)).unwrap();

        assert_eq!(m.key.as_str(), "mkt:hyperliquid:btc");
        assert_eq!(m.venue.as_str(), "hyperliquid");
        assert_eq!(m.kind, InstrumentKind::Perp);
        assert_eq!(m.status, MarketStatus::Open);
        assert_eq!(m.title, "BTC");
        assert!(m.outcome.is_none());
        assert!(!m.venue_ref.is_empty());
        assert!(!m.meta.is_empty());
        assert_eq!(m.close_ts, None);
    }

    #[test]
    fn normalize_delisted_market() {
        let mut raw = make_asset();
        raw.is_delisted = Some(true);
        let m = normalize_market(raw, None).unwrap();
        assert_eq!(m.status, MarketStatus::Closed);
    }

    #[test]
    fn normalize_market_without_ctx() {
        let raw = make_asset();
        let m = normalize_market(raw, None).unwrap();
        assert_eq!(m.key.as_str(), "mkt:hyperliquid:btc");
        assert_eq!(m.status, MarketStatus::Open);
        // meta should have nulls for ctx fields
        let meta = m.meta.as_value();
        assert!(meta.get("funding").is_some());
        assert!(meta.get("funding").unwrap().is_null());
    }

    #[test]
    fn normalize_spot_market_uses_stable_api_index() {
        let pair = HlSpotPair {
            name: "PURR/USDC".into(),
            tokens: vec![1, 0],
            index: 0,
            is_canonical: Some(true),
        };
        let tokens = vec![
            HlSpotToken {
                name: "USDC".into(),
                index: 0,
                sz_decimals: 8,
                wei_decimals: 6,
                token_id: None,
                is_canonical: Some(true),
            },
            HlSpotToken {
                name: "PURR".into(),
                index: 1,
                sz_decimals: 0,
                wei_decimals: 5,
                token_id: None,
                is_canonical: Some(true),
            },
        ];
        let market = normalize_spot_market(pair, &tokens, None).unwrap();
        assert_eq!(market.key.as_str(), "mkt:hyperliquid:@0");
        assert_eq!(market.kind, InstrumentKind::Spot);
        assert_eq!(market.title, "PURR/USDC");
        assert_eq!(market.meta.as_value()["base_token"], "PURR");
    }

    #[test]
    fn normalize_empty_name_errors() {
        let raw = HlAsset { name: "".into(), ..HlAsset::default() };
        let result = normalize_market(raw, None);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::MissingName)));
    }

    #[test]
    fn normalize_name_lowercased_in_key() {
        let raw = HlAsset { name: "BTC".into(), ..HlAsset::default() };
        let m = normalize_market(raw, None).unwrap();
        assert_eq!(m.key.as_str(), "mkt:hyperliquid:btc");
    }

    // ── Mid -> Quote tests ──

    #[test]
    fn normalize_mid_to_quote_with_valid_price() {
        let quote = normalize_mid_to_quote("BTC", "67234.5", 1700000000000).unwrap();
        assert_eq!(quote.market.as_str(), "mkt:hyperliquid:btc");
        assert_eq!(quote.mid, Some(Decimal::new(672345, 1))); // 67234.5
        assert_eq!(quote.bid, None);
        assert_eq!(quote.ask, None);
        assert_eq!(quote.last, None);
        assert_eq!(quote.source, QuoteSource::Poll);
    }

    #[test]
    fn normalize_mid_empty_coin_errors() {
        let result = normalize_mid_to_quote("", "100.0", 1700000000000);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::MissingName)));
    }

    #[test]
    fn normalize_mid_invalid_decimal_errors() {
        let result = normalize_mid_to_quote("BTC", "not-a-number", 1700000000000);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_mid_non_positive_errors() {
        assert!(normalize_mid_to_quote("BTC", "0", 1700000000000).is_err());
        assert!(normalize_mid_to_quote("BTC", "-1", 1700000000000).is_err());
    }

    // ── Book normalization tests ──

    #[test]
    fn normalize_book_creates_orderbook() {
        let raw = HlBookSnapshot {
            coin: "BTC".into(),
            time: 1754450974231,
            levels: vec![
                vec![
                    crate::client::HlBookLevel {
                        px: "113378.0".into(),
                        sz: "7.6699".into(),
                        n: 17,
                    },
                    crate::client::HlBookLevel {
                        px: "113377.0".into(),
                        sz: "4.13714".into(),
                        n: 8,
                    },
                ],
                vec![crate::client::HlBookLevel {
                    px: "113398.0".into(),
                    sz: "0.11543".into(),
                    n: 3,
                }],
            ],
        };

        let ob = normalize_book(&raw).unwrap();
        assert_eq!(ob.market.as_str(), "mkt:hyperliquid:btc");
        assert_eq!(ob.bids().len(), 2);
        assert_eq!(ob.asks().len(), 1);

        // Bids should be sorted descending
        assert!(ob.bids()[0].price > ob.bids()[1].price);
        // Asks should be sorted ascending
        assert!(ob.asks().len() == 1);
    }

    #[test]
    fn normalize_book_empty_coin_errors() {
        let raw = HlBookSnapshot { coin: "".into(), time: 1000, levels: vec![vec![], vec![]] };
        let result = normalize_book(&raw);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::MissingName)));
    }

    #[test]
    fn normalize_book_wrong_number_of_levels_errors() {
        let raw = HlBookSnapshot {
            coin: "BTC".into(),
            time: 1000,
            levels: vec![vec![]], // only 1 level array
        };
        let result = normalize_book(&raw);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::WrongLevelCount { .. })));
    }
}

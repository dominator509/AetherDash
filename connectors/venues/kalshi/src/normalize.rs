//! Market normalization: Kalshi raw types -> canonical `aether_core::Market`, `Quote`, and `OrderBook`.
//!
//! # M3 conversions
//!
//! | Kalshi field | Canonical field | Notes |
//! |---|---|---|
//! | `ticker` | `MarketKey` | `mkt:kalshi:{ticker}` lowercased |
//! | `title` | `title` | Pass-through |
//! | `status` | `MarketStatus` | open/halted/closed/resolved mapped |
//! | `close_ts` | `close_ts` | Unix millis -> UtcTime |
//! | `settlement_ts` | `resolve_ts` | Unix millis -> UtcTime |
//! | `result` | `outcome` | Pass-through |
//! | yes/no ask/bid | `meta` | Raw cents stored; Probability via PriceSemantics |
//! | `tick_size` | `meta` | Preserved as-is |
//! | entire raw json | `venue_ref` | Preserved for provenance |
//!
//! # WS tick normalization
//!
//! | Kalshi WS field | Canonical field | Notes |
//! |---|---|---|
//! | `ticker` | `market` | `mkt:kalshi:{ticker}` lowercased |
//! | `bid` / `ask` (cents) | `bid` / `ask` (Decimal) | cents/100 |
//! | `price` (cents) | `last` (Decimal) | cents/100 |
//! | `ts` (ISO 8601) | `ts` (UtcTime) | RFC3339 parsed |
//! | `volume` | — | Not mapped to Quote |
//!
//! # WS book normalization
//!
//! | Kalshi WS field | Canonical field | Notes |
//! |---|---|---|
//! | `ticker` | `market` | `mkt:kalshi:{ticker}` lowercased |
//! | `bids[*].price` (cents) | `bids[*].price` (Decimal) | cents/100, desc sorted |
//! | `asks[*].price` (cents) | `asks[*].price` (Decimal) | cents/100, asc sorted |
//! | `bids[*].size` | `bids[*].size` (Decimal) | raw contracts |
//! | `ts` (ISO 8601) | `ts` (UtcTime) | RFC3339 parsed |

use crate::client::KalshiMarket;
use aether_core::ids::{MarketKey, VenueId};
use aether_core::json::JsonObject;
use aether_core::market::{InstrumentKind, Market, MarketStatus};
use aether_core::quote::{BookLevel, OrderBook, OrderBookError, Quote, QuoteSource};
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Raw WebSocket types
// ---------------------------------------------------------------------------

/// A single tick from the Kalshi WebSocket `ticker` channel.
///
/// Used in integration tests. The `dead_code` allowance is because the
/// integration test crate is a separate compilation unit that dead-code
/// analysis doesn't account for.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KalshiTick {
    /// Market ticker symbol, e.g. `"BTC-75"`.
    #[serde(rename = "market_ticker", alias = "ticker")]
    pub ticker: String,
    /// Last trade price in cents (1-99).
    #[serde(default)]
    pub price: Option<i64>,
    /// Current fixed-point last price.
    #[serde(default)]
    pub price_dollars: Option<String>,
    /// Side of the last trade: `"yes"` or `"no"`.
    #[serde(default)]
    pub side: Option<String>,
    /// ISO 8601 timestamp of the tick.
    #[serde(default, alias = "time")]
    pub ts: Option<String>,
    #[serde(default)]
    pub ts_ms: Option<i64>,
    /// Trade volume in contracts.
    #[serde(default)]
    pub volume: Option<i64>,
    /// Best bid for Yes in cents.
    #[serde(default)]
    pub bid: Option<i64>,
    /// Best ask for Yes in cents.
    #[serde(default)]
    pub ask: Option<i64>,
    /// Last trade price (duplicate of `price` in some Kalshi versions).
    #[serde(default)]
    pub last_price: Option<i64>,
    #[serde(default)]
    pub yes_bid_dollars: Option<String>,
    #[serde(default)]
    pub yes_ask_dollars: Option<String>,
    #[serde(default)]
    pub yes_bid_size_fp: Option<String>,
    #[serde(default)]
    pub yes_ask_size_fp: Option<String>,
}

/// A single level in the Kalshi order book.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KalshiBookLevel {
    /// Price in cents.
    pub price: i64,
    /// Size in contracts.
    pub size: i64,
}

/// A full order-book snapshot from the Kalshi WebSocket `book` channel.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KalshiBookSnapshot {
    /// Market ticker symbol, e.g. `"BTC-75"`.
    #[serde(rename = "market_ticker", alias = "ticker")]
    pub ticker: String,
    /// ISO 8601 timestamp of the snapshot.
    #[serde(default)]
    pub ts: Option<String>,
    /// Bid levels (may be unsorted).
    #[serde(default)]
    pub bids: Vec<KalshiBookLevel>,
    /// Ask levels (may be unsorted).
    #[serde(default)]
    pub asks: Vec<KalshiBookLevel>,
    /// Current fixed-point Yes and No bid levels.
    #[serde(default)]
    pub yes_dollars_fp: Vec<(String, String)>,
    #[serde(default)]
    pub no_dollars_fp: Vec<(String, String)>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KalshiBookDelta {
    pub market_ticker: String,
    pub price_dollars: String,
    pub delta_fp: String,
    pub side: String,
    #[serde(default)]
    pub ts: Option<String>,
    #[serde(default)]
    pub ts_ms: Option<i64>,
}

/// Stateful order-book reconstruction from one snapshot plus deltas.
#[derive(Debug)]
pub struct KalshiBookState {
    ticker: String,
    yes: BTreeMap<Decimal, Decimal>,
    no: BTreeMap<Decimal, Decimal>,
    ts: UtcTime,
    seq: Option<u64>,
}

impl KalshiBookState {
    pub fn from_snapshot(
        raw: &KalshiBookSnapshot,
        seq: Option<u64>,
    ) -> Result<Self, NormalizeError> {
        let mut yes = BTreeMap::new();
        let mut no = BTreeMap::new();
        for level in &raw.bids {
            *yes.entry(cents_to_decimal(level.price)).or_default() += Decimal::new(level.size, 0);
        }
        for level in &raw.asks {
            *no.entry(Decimal::ONE - cents_to_decimal(level.price)).or_default() +=
                Decimal::new(level.size, 0);
        }
        for (price, size) in &raw.yes_dollars_fp {
            *yes.entry(parse_decimal(price)?).or_default() += parse_decimal(size)?;
        }
        for (price, size) in &raw.no_dollars_fp {
            *no.entry(parse_decimal(price)?).or_default() += parse_decimal(size)?;
        }
        let ts = raw.ts.as_deref().map(parse_iso8601).transpose()?.unwrap_or_else(UtcTime::now);
        Ok(Self { ticker: raw.ticker.clone(), yes, no, ts, seq })
    }

    pub fn apply_delta(
        &mut self,
        delta: &KalshiBookDelta,
        seq: Option<u64>,
    ) -> Result<(), NormalizeError> {
        if delta.market_ticker != self.ticker {
            return Err(NormalizeError::MissingTicker);
        }
        let price = parse_decimal(&delta.price_dollars)?;
        let change = parse_decimal(&delta.delta_fp)?;
        let levels = match delta.side.as_str() {
            "yes" => &mut self.yes,
            "no" => &mut self.no,
            side => {
                return Err(NormalizeError::InvalidSide {
                    ticker: self.ticker.clone(),
                    side: side.to_string(),
                })
            }
        };
        let new_size = levels.get(&price).copied().unwrap_or(Decimal::ZERO) + change;
        if new_size <= Decimal::ZERO {
            levels.remove(&price);
        } else {
            levels.insert(price, new_size);
        }
        self.ts = match (&delta.ts, delta.ts_ms) {
            (Some(value), _) => parse_iso8601(value)?,
            (None, Some(value)) => UtcTime::from_unix_millis(value).map_err(|error| {
                NormalizeError::TimestampParse { raw: value.to_string(), detail: error.to_string() }
            })?,
            (None, None) => self.ts,
        };
        self.seq = seq;
        Ok(())
    }

    pub fn to_order_book(&self) -> Result<OrderBook, NormalizeError> {
        let venue = VenueId::new("kalshi").map_err(|e| {
            NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw })
        })?;
        let market = MarketKey::new(&venue, &self.ticker.to_lowercase())?;
        let bids = self
            .yes
            .iter()
            .rev()
            .map(|(price, size)| BookLevel { price: *price, size: *size })
            .collect::<Vec<_>>();
        let asks = self
            .no
            .iter()
            .rev()
            .map(|(price, size)| BookLevel { price: Decimal::ONE - *price, size: *size })
            .collect::<Vec<_>>();
        let depth = bids.len().max(asks.len());
        Ok(OrderBook::new(market, bids, asks, depth, self.ts, self.seq)?)
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during market normalization.
///
/// Following the M3 spec: malformed markets are quarantined (return `Err`),
/// not panicked.
#[derive(Error, Debug)]
pub enum NormalizeError {
    /// The ticker field is empty or missing.
    #[error("market has no ticker")]
    MissingTicker,

    /// Failed to parse the Kalshi market status string.
    #[error("unknown Kalshi status '{status}'")]
    UnknownStatus {
        /// The raw status string from the API.
        status: String,
    },

    /// Failed to construct a `MarketKey` from the ticker.
    #[error("invalid MarketKey: {0}")]
    MarketKey(#[from] aether_core::ids::MarketKeyError),

    /// Failed to construct the `VenueRef` JSON object.
    #[error("venue_ref construction failed: {0}")]
    JsonObject(#[from] aether_core::json::JsonObjectError),

    /// The market has no `tick_size` array, needed for price semantics.
    #[error("missing tick_size for market {ticker}")]
    #[allow(dead_code)]
    MissingTickSize {
        /// The ticker of the malformed market.
        ticker: String,
    },

    /// Failed to parse an ISO 8601 / RFC3339 timestamp.
    #[allow(dead_code)]
    #[error("timestamp parse error for '{raw}': {detail}")]
    TimestampParse {
        /// The raw timestamp string.
        raw: String,
        /// Details about the parse failure.
        detail: String,
    },

    /// OrderBook validation failed (ordering invariant violated).
    #[error("order book error: {0}")]
    OrderBook(#[from] OrderBookError),

    /// The side field in a tick is neither "yes" nor "no".
    #[allow(dead_code)]
    #[error("invalid tick side '{side}' for ticker {ticker}")]
    InvalidSide {
        /// The ticker symbol.
        ticker: String,
        /// The invalid side value.
        side: String,
    },

    #[error("missing price for ticker {ticker}")]
    MissingPrice { ticker: String },

    #[error("invalid decimal '{raw}': {detail}")]
    DecimalParse { raw: String, detail: String },
}

// ---------------------------------------------------------------------------
// REST market normalization (M2/M3)
// ---------------------------------------------------------------------------

/// Normalize a raw Kalshi market into the canonical `aether_core::Market`.
///
/// # Errors
///
/// Returns `NormalizeError` (not panic) on any malformed input, per the M3
/// quarantine contract.
pub fn normalize_market(raw: KalshiMarket) -> Result<Market, NormalizeError> {
    let ticker = raw.ticker.trim();
    if ticker.is_empty() {
        return Err(NormalizeError::MissingTicker);
    }

    let venue = VenueId::new("kalshi")
        .map_err(|e| NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw }))?;

    let key = MarketKey::new(&venue, &ticker.to_lowercase())?;

    let kind = InstrumentKind::BinaryContract;

    let status = normalize_status(&raw.status)?;

    let close_ts = raw
        .close_time
        .as_deref()
        .map(parse_iso8601)
        .transpose()?
        .or_else(|| raw.close_ts.and_then(|ms| UtcTime::from_unix_millis(ms).ok()));

    let resolve_ts = raw.settlement_ts.as_ref().map(parse_timestamp_value).transpose()?;

    let outcome = raw.result.clone();

    let jurisdiction_flags = vec!["US".to_string()];

    // Build venue_ref from the entire raw market JSON
    let venue_ref = JsonObject::new(serde_json::to_value(&raw).unwrap_or_else(|_| json!({})))?;

    // Build meta with tick_size and current prices
    let tick_size_cents = raw.tick_size.as_deref().unwrap_or(&[]);
    let tick_size_min = tick_size_cents.first().copied().unwrap_or(1);
    let tick_size_max = tick_size_cents.last().copied().unwrap_or(99);

    let meta = JsonObject::new(json!({
        "tick_size_min": tick_size_min,
        "tick_size_max": tick_size_max,
        "yes_ask": raw.yes_ask,
        "yes_bid": raw.yes_bid,
        "no_ask": raw.no_ask,
        "no_bid": raw.no_bid,
        "yes_ask_dollars": raw.yes_ask_dollars,
        "yes_bid_dollars": raw.yes_bid_dollars,
        "no_ask_dollars": raw.no_ask_dollars,
        "no_bid_dollars": raw.no_bid_dollars,
        "last_price_dollars": raw.last_price_dollars,
        "volume": raw.volume,
        "open_interest": raw.open_interest,
    }))?;

    Ok(Market {
        key,
        venue,
        kind,
        title: raw.title,
        description_ref: raw.semi_title.unwrap_or_default(),
        status,
        close_ts,
        resolve_ts,
        outcome,
        jurisdiction_flags,
        venue_ref,
        meta,
    })
}

/// Map a Kalshi status string to the canonical `MarketStatus`.
fn normalize_status(status: &str) -> Result<MarketStatus, NormalizeError> {
    match status {
        "open" => Ok(MarketStatus::Open),
        "closed" => Ok(MarketStatus::Closed),
        "settled" => Ok(MarketStatus::Resolved),
        _ => Err(NormalizeError::UnknownStatus { status: status.to_string() }),
    }
}

/// Convert cents (Kalshi price units, 1-99) to a probability `Decimal` string
/// (0.01-0.99).
///
/// # Panics
///
/// Panics if `cents` is outside 1..=99 (defensive: should be validated upstream).
#[allow(dead_code)]
pub fn cents_to_probability(cents: i64) -> String {
    assert!((1..=99).contains(&cents), "cents must be 1..=99, got {cents}");
    format!("{:.2}", cents as f64 / 100.0)
}

// ---------------------------------------------------------------------------
// WS tick normalization
// ---------------------------------------------------------------------------

/// Convert Kalshi cents to a `Decimal` (e.g. 65 -> 0.65).
#[allow(dead_code)]
fn cents_to_decimal(cents: i64) -> Decimal {
    Decimal::new(cents, 2)
}

/// Parse an RFC 3339 / ISO 8601 timestamp string into `UtcTime`.
///
/// Reuses `UtcTime`'s own `Deserialize` implementation which validates UTC
/// offset and normalizes to millisecond precision.
#[allow(dead_code)]
fn parse_iso8601(ts: &str) -> Result<UtcTime, NormalizeError> {
    let json = format!("\"{}\"", ts);
    serde_json::from_str(&json)
        .map_err(|e| NormalizeError::TimestampParse { raw: ts.to_string(), detail: e.to_string() })
}

fn parse_timestamp_value(value: &serde_json::Value) -> Result<UtcTime, NormalizeError> {
    match value {
        serde_json::Value::String(value) => parse_iso8601(value),
        serde_json::Value::Number(value) => value
            .as_i64()
            .and_then(|millis| UtcTime::from_unix_millis(millis).ok())
            .ok_or_else(|| NormalizeError::TimestampParse {
                raw: value.to_string(),
                detail: "expected unix milliseconds".into(),
            }),
        _ => Err(NormalizeError::TimestampParse {
            raw: value.to_string(),
            detail: "expected RFC3339 string or unix milliseconds".into(),
        }),
    }
}

fn parse_decimal(value: &str) -> Result<Decimal, NormalizeError> {
    value.parse::<Decimal>().map_err(|error| NormalizeError::DecimalParse {
        raw: value.to_string(),
        detail: error.to_string(),
    })
}

/// Normalize a raw `KalshiTick` (from WebSocket) into a canonical `Quote`.
///
/// # Normalization rules
///
/// - `market` key: `mkt:kalshi:{ticker}` (lowercased)
/// - `bid` / `ask`: cents -> `Decimal` (cents / 100)
/// - `last`: the tick's `price` field as `Decimal`
/// - `ts`: ISO 8601 string -> `UtcTime`
/// - `source`: always `QuoteSource::Stream`
///
/// # Errors
///
/// Returns `NormalizeError` if the ticker is empty, the timestamp is
/// malformed, or the MarketKey construction fails.
#[allow(dead_code)]
pub fn normalize_tick(raw: &KalshiTick) -> Result<Quote, NormalizeError> {
    let ticker = raw.ticker.trim();
    if ticker.is_empty() {
        return Err(NormalizeError::MissingTicker);
    }

    let venue = VenueId::new("kalshi")
        .map_err(|e| NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw }))?;
    let market = MarketKey::new(&venue, &ticker.to_lowercase())?;
    let ts = match (&raw.ts, raw.ts_ms) {
        (Some(value), _) => parse_iso8601(value)?,
        (None, Some(millis)) => UtcTime::from_unix_millis(millis).map_err(|error| {
            NormalizeError::TimestampParse { raw: millis.to_string(), detail: error.to_string() }
        })?,
        (None, None) => {
            return Err(NormalizeError::TimestampParse {
                raw: String::new(),
                detail: "ticker message has no time or ts_ms".into(),
            })
        }
    };

    let price = match (&raw.price_dollars, raw.price.or(raw.last_price)) {
        (Some(value), _) => parse_decimal(value)?,
        (None, Some(cents)) => cents_to_decimal(cents),
        (None, None) => return Err(NormalizeError::MissingPrice { ticker: ticker.to_string() }),
    };
    let bid = raw
        .yes_bid_dollars
        .as_deref()
        .map(parse_decimal)
        .transpose()?
        .or_else(|| raw.bid.map(cents_to_decimal));
    let ask = raw
        .yes_ask_dollars
        .as_deref()
        .map(parse_decimal)
        .transpose()?
        .or_else(|| raw.ask.map(cents_to_decimal));

    Ok(Quote {
        market,
        bid,
        ask,
        mid: None,
        last: Some(price),
        bid_size: raw.yes_bid_size_fp.as_deref().map(parse_decimal).transpose()?,
        ask_size: raw.yes_ask_size_fp.as_deref().map(parse_decimal).transpose()?,
        ts,
        source: QuoteSource::Stream,
        seq: None,
    })
}

// ---------------------------------------------------------------------------
// WS book normalization
// ---------------------------------------------------------------------------

/// Normalize a raw `KalshiBookSnapshot` (from WebSocket) into a canonical
/// `OrderBook`.
///
/// # Normalization rules
///
/// - `market` key: `mkt:kalshi:{ticker}` (lowercased)
/// - Bids sorted descending by price, asks sorted ascending by price
/// - Each level's price: cents -> `Decimal` (cents / 100)
/// - Each level's size: raw contracts -> `Decimal`
/// - `ts`: ISO 8601 string -> `UtcTime`
///
/// # Errors
///
/// Returns `NormalizeError` if the ticker is empty, the timestamp is
/// malformed, or the MarketKey construction fails. OrderBook validation
/// (`OrderBook::new`) is guaranteed to pass because we sort before
/// constructing.
#[allow(dead_code)]
pub fn normalize_book(raw: &KalshiBookSnapshot) -> Result<OrderBook, NormalizeError> {
    let ticker = raw.ticker.trim();
    if ticker.is_empty() {
        return Err(NormalizeError::MissingTicker);
    }

    KalshiBookState::from_snapshot(raw, None)?.to_order_book()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::KalshiMarket;

    fn make_open_market() -> KalshiMarket {
        KalshiMarket {
            ticker: "BTC-75".into(),
            title: "Will Bitcoin be above $75k?".into(),
            semi_title: Some("BTC > $75k?".into()),
            status: "open".into(),
            yes_ask: Some(48),
            yes_bid: Some(47),
            no_ask: Some(53),
            no_bid: Some(52),
            result: None,
            settlement_ts: None,
            close_ts: Some(1_720_000_000_000),
            volume: Some(154_230),
            open_interest: Some(45_200),
            volume_24h: Some(12_300),
            open_interest_24h: Some(3_200),
            created_time: Some(json!(1_710_000_000_000_i64)),
            tick_size: Some(vec![1, 99]),
            yes_sub_title: Some("Yes".into()),
            no_sub_title: Some("No".into()),
            ..KalshiMarket::default()
        }
    }

    #[test]
    fn normalize_open_market() {
        let raw = make_open_market();
        let m = normalize_market(raw).unwrap();

        assert_eq!(m.key.as_str(), "mkt:kalshi:btc-75");
        assert_eq!(m.venue.as_str(), "kalshi");
        assert_eq!(m.kind, InstrumentKind::BinaryContract);
        assert_eq!(m.status, MarketStatus::Open);
        assert_eq!(m.title, "Will Bitcoin be above $75k?");
        assert_eq!(m.description_ref, "BTC > $75k?");
        assert!(m.outcome.is_none());
        assert!(!m.jurisdiction_flags.is_empty());
        assert_eq!(m.jurisdiction_flags[0], "US");
        assert!(!m.venue_ref.is_empty());
        assert!(!m.meta.is_empty());
    }

    #[test]
    fn normalize_closed_market() {
        let mut raw = make_open_market();
        raw.status = "closed".into();
        let m = normalize_market(raw).unwrap();
        assert_eq!(m.status, MarketStatus::Closed);
    }

    #[test]
    fn normalize_settled_market() {
        let mut raw = make_open_market();
        raw.status = "settled".into();
        raw.result = Some("yes".into());
        raw.settlement_ts = Some(json!(1_730_000_000_000_i64));
        let m = normalize_market(raw).unwrap();
        assert_eq!(m.status, MarketStatus::Resolved);
        assert_eq!(m.outcome, Some("yes".into()));
        assert!(m.resolve_ts.is_some());
    }

    #[test]
    fn normalize_unknown_status_errors() {
        let mut raw = make_open_market();
        raw.status = "unknown".into();
        let result = normalize_market(raw);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::UnknownStatus { .. })));
    }

    #[test]
    fn normalize_empty_ticker_errors() {
        let mut raw = make_open_market();
        raw.ticker = "".into();
        let result = normalize_market(raw);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_ticker_lowercased_in_key() {
        let mut raw = make_open_market();
        raw.ticker = "BTC-75".into();
        let m = normalize_market(raw).unwrap();
        assert_eq!(m.key.as_str(), "mkt:kalshi:btc-75");
    }

    #[test]
    fn normalize_preserves_venue_ref() {
        let raw = make_open_market();
        let m = normalize_market(raw).unwrap();
        let vr = m.venue_ref.as_value();
        assert!(vr.get("ticker").is_some());
        assert!(vr.get("title").is_some());
        assert!(vr.get("status").is_some());
    }

    #[test]
    fn cents_to_probability_works() {
        assert_eq!(cents_to_probability(50), "0.50");
        assert_eq!(cents_to_probability(1), "0.01");
        assert_eq!(cents_to_probability(99), "0.99");
    }

    #[test]
    #[should_panic(expected = "cents must be 1..=99")]
    fn cents_to_probability_panics_outside_range() {
        cents_to_probability(0);
    }

    // ── WS tick normalization tests ──

    fn make_tick() -> KalshiTick {
        KalshiTick {
            ticker: "BTC-75".into(),
            price: Some(65),
            side: Some("yes".into()),
            ts: Some("2026-07-10T12:34:56.789Z".into()),
            volume: Some(1_500),
            bid: Some(64),
            ask: Some(66),
            last_price: Some(65),
            ..KalshiTick::default()
        }
    }

    #[test]
    fn normalize_tick_creates_quote() {
        let tick = make_tick();
        let quote = normalize_tick(&tick).unwrap();

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
    fn normalize_current_ticker_payload() {
        let raw: KalshiTick = serde_json::from_str(
            r#"{
                "market_ticker":"FED-23DEC-T3.00",
                "price_dollars":"0.480",
                "yes_bid_dollars":"0.450",
                "yes_ask_dollars":"0.530",
                "yes_bid_size_fp":"300.00",
                "yes_ask_size_fp":"150.00",
                "ts_ms":1669149841000
            }"#,
        )
        .unwrap();
        let quote = normalize_tick(&raw).unwrap();
        assert_eq!(quote.market.as_str(), "mkt:kalshi:fed-23dec-t3.00");
        assert_eq!(quote.bid, Some(Decimal::new(450, 3)));
        assert_eq!(quote.ask, Some(Decimal::new(530, 3)));
        assert_eq!(quote.last, Some(Decimal::new(480, 3)));
        assert_eq!(quote.bid_size, Some(Decimal::new(30000, 2)));
        assert_eq!(quote.ts.unix_millis(), 1_669_149_841_000);
    }

    #[test]
    fn normalize_tick_empty_ticker_errors() {
        let tick = KalshiTick {
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
        let result = normalize_tick(&tick);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_tick_bad_timestamp_errors() {
        let tick = KalshiTick {
            ticker: "BTC-75".into(),
            price: Some(50),
            side: Some("yes".into()),
            ts: Some("not-a-timestamp".into()),
            volume: None,
            bid: None,
            ask: None,
            last_price: None,
            ..KalshiTick::default()
        };
        let result = normalize_tick(&tick);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::TimestampParse { .. })));
    }

    #[test]
    fn normalize_tick_missing_bid_ask() {
        let tick = KalshiTick {
            ticker: "BTC-75".into(),
            price: Some(50),
            side: Some("yes".into()),
            ts: Some("2026-07-10T12:00:00.000Z".into()),
            volume: None,
            bid: None,
            ask: None,
            last_price: None,
            ..KalshiTick::default()
        };
        let quote = normalize_tick(&tick).unwrap();
        assert_eq!(quote.bid, None);
        assert_eq!(quote.ask, None);
        assert_eq!(quote.last, Some(Decimal::new(50, 2)));
    }

    // ── WS book normalization tests ──

    fn make_book_snapshot() -> KalshiBookSnapshot {
        KalshiBookSnapshot {
            ticker: "BTC-75".into(),
            ts: Some("2026-07-10T12:34:56.789Z".into()),
            bids: vec![
                KalshiBookLevel { price: 64, size: 1_000 },
                KalshiBookLevel { price: 63, size: 2_000 },
                KalshiBookLevel { price: 65, size: 500 },
            ],
            asks: vec![
                KalshiBookLevel { price: 66, size: 1_500 },
                KalshiBookLevel { price: 68, size: 800 },
                KalshiBookLevel { price: 67, size: 1_200 },
            ],
            ..KalshiBookSnapshot::default()
        }
    }

    #[test]
    fn normalize_book_creates_orderbook() {
        let snap = make_book_snapshot();
        let ob = normalize_book(&snap).unwrap();

        assert_eq!(ob.market.as_str(), "mkt:kalshi:btc-75");
    }

    #[test]
    fn normalize_current_orderbook_snapshot() {
        let raw: KalshiBookSnapshot = serde_json::from_str(
            r#"{
                "market_ticker":"FED-23DEC-T3.00",
                "ts":"2022-11-22T20:44:01Z",
                "yes_dollars_fp":[["0.0800","300.00"],["0.2200","333.00"]],
                "no_dollars_fp":[["0.5400","20.00"],["0.5600","146.00"]]
            }"#,
        )
        .unwrap();
        let book = normalize_book(&raw).unwrap();
        assert_eq!(book.bids()[0].price, Decimal::new(2200, 4));
        assert_eq!(book.asks()[0].price, Decimal::new(4400, 4));
        assert_eq!(book.asks()[1].price, Decimal::new(4600, 4));
    }

    #[test]
    fn current_orderbook_delta_updates_state() {
        let snapshot: KalshiBookSnapshot = serde_json::from_str(
            r#"{
                "market_ticker":"FED-23DEC-T3.00",
                "yes_dollars_fp":[["0.2200","333.00"]],
                "no_dollars_fp":[["0.5600","146.00"]]
            }"#,
        )
        .unwrap();
        let mut state = KalshiBookState::from_snapshot(&snapshot, Some(2)).unwrap();
        let delta: KalshiBookDelta = serde_json::from_str(
            r#"{
                "market_ticker":"FED-23DEC-T3.00",
                "price_dollars":"0.2200",
                "delta_fp":"-33.00",
                "side":"yes",
                "ts_ms":1669149841000
            }"#,
        )
        .unwrap();
        state.apply_delta(&delta, Some(3)).unwrap();
        let book = state.to_order_book().unwrap();
        assert_eq!(book.bids()[0].size, Decimal::new(30000, 2));
        assert_eq!(book.seq, Some(3));
        assert_eq!(book.ts.unix_millis(), 1_669_149_841_000);
    }

    #[test]
    fn normalize_book_bids_descending() {
        let snap = make_book_snapshot();
        let ob = normalize_book(&snap).unwrap();

        let bids = ob.bids();
        assert_eq!(bids.len(), 3);
        // Bids should be sorted descending: 0.65, 0.64, 0.63
        assert_eq!(bids[0].price, Decimal::new(65, 2));
        assert_eq!(bids[1].price, Decimal::new(64, 2));
        assert_eq!(bids[2].price, Decimal::new(63, 2));
    }

    #[test]
    fn normalize_book_asks_ascending() {
        let snap = make_book_snapshot();
        let ob = normalize_book(&snap).unwrap();

        let asks = ob.asks();
        assert_eq!(asks.len(), 3);
        // Asks should be sorted ascending: 0.66, 0.67, 0.68
        assert_eq!(asks[0].price, Decimal::new(66, 2));
        assert_eq!(asks[1].price, Decimal::new(67, 2));
        assert_eq!(asks[2].price, Decimal::new(68, 2));
    }

    #[test]
    fn normalize_book_empty_ticker_errors() {
        let snap = KalshiBookSnapshot {
            ticker: "".into(),
            ts: Some("2026-07-10T12:00:00.000Z".into()),
            bids: vec![],
            asks: vec![],
            ..KalshiBookSnapshot::default()
        };
        let result = normalize_book(&snap);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_book_bad_timestamp_errors() {
        let snap = KalshiBookSnapshot {
            ticker: "BTC-75".into(),
            ts: Some("bad-ts".into()),
            bids: vec![],
            asks: vec![],
            ..KalshiBookSnapshot::default()
        };
        let result = normalize_book(&snap);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::TimestampParse { .. })));
    }

    #[test]
    fn normalize_book_empty_books() {
        let snap = KalshiBookSnapshot {
            ticker: "BTC-75".into(),
            ts: Some("2026-07-10T12:00:00.000Z".into()),
            bids: vec![],
            asks: vec![],
            ..KalshiBookSnapshot::default()
        };
        let ob = normalize_book(&snap).unwrap();
        assert!(ob.bids().is_empty());
        assert!(ob.asks().is_empty());
        assert_eq!(ob.depth, 0);
    }

    #[test]
    fn cents_to_decimal_works() {
        assert_eq!(cents_to_decimal(0), Decimal::new(0, 2));
        assert_eq!(cents_to_decimal(1), Decimal::new(1, 2));
        assert_eq!(cents_to_decimal(50), Decimal::new(50, 2));
        assert_eq!(cents_to_decimal(99), Decimal::new(99, 2));
        assert_eq!(cents_to_decimal(100), Decimal::new(100, 2));
    }

    #[test]
    fn parse_iso8601_valid() {
        let ts = parse_iso8601("2026-07-10T12:34:56.789Z").unwrap();
        assert_eq!(ts.unix_millis(), 1_783_686_896_789);
    }

    #[test]
    fn parse_iso8601_invalid() {
        let result = parse_iso8601("not-a-date");
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::TimestampParse { .. })));
    }
}

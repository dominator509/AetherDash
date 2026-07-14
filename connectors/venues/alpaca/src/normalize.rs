//! Market normalization: Alpaca raw types -> canonical `aether_core::Market`, `Quote`, and `OrderBook`.
//!
//! # M3 conversions
//!
//! | Alpaca field | Canonical field | Notes |
//! |---|---|---|
//! | `symbol` | `MarketKey` | `mkt:alpaca:{symbol}` lowercased |
//! | `name` | `title` | Pass-through |
//! | `status` | `MarketStatus` | active/inactive mapped |
//! | `class` | `kind` | Always `Equity` for us_equity |
//! | entire raw json | `venue_ref` | Preserved for provenance |
//!
//! # Snapshot normalization
//!
//! | Alpaca field | Canonical field | Notes |
//! |---|---|---|
//! | `symbol` | `market` | `mkt:alpaca:{symbol}` lowercased |
//! | `latestQuote.bp` / `latestQuote.ap` | `bid` / `ask` | Decimal (USD dollars) |
//! | `latestTrade.p` | `last` | Decimal (USD dollars) |
//! | `latestQuote.bs` / `latestQuote.as` | `bid_size` / `ask_size` | Decimal |
//! | timestamp | `ts` | RFC3339 parsed |

use crate::client::{AlpacaAsset, AlpacaSnapshot};
use aether_core::ids::{MarketKey, VenueId};
use aether_core::json::JsonObject;
use aether_core::market::{InstrumentKind, Market, MarketStatus};
use aether_core::quote::{Quote, QuoteSource};
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use serde_json::json;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during market normalization.
#[derive(Error, Debug)]
pub enum NormalizeError {
    /// The symbol field is empty or missing.
    #[error("asset has no symbol")]
    MissingSymbol,

    /// Failed to parse the Alpaca asset status string.
    #[error("unknown Alpaca status '{status}'")]
    UnknownStatus {
        /// The raw status string from the API.
        status: String,
    },

    /// Failed to construct a `MarketKey` from the symbol.
    #[error("invalid MarketKey: {0}")]
    MarketKey(#[from] aether_core::ids::MarketKeyError),

    /// Failed to construct the `VenueRef` JSON object.
    #[error("venue_ref construction failed: {0}")]
    JsonObject(#[from] aether_core::json::JsonObjectError),

    /// Failed to parse an ISO 8601 / RFC3339 timestamp.
    #[error("timestamp parse error for '{raw}': {detail}")]
    TimestampParse {
        /// The raw timestamp string.
        raw: String,
        /// Details about the parse failure.
        detail: String,
    },

    #[error("invalid decimal '{raw}': {detail}")]
    DecimalParse { raw: String, detail: String },

    /// No trade or quote data to build a Quote from.
    #[error("snapshot has no trade or quote data for {symbol}")]
    NoData { symbol: String },
}

// ---------------------------------------------------------------------------
// REST asset normalization (M2/M3)
// ---------------------------------------------------------------------------

/// Normalize a raw Alpaca asset into the canonical `aether_core::Market`.
///
/// # Errors
///
/// Returns `NormalizeError` on any malformed input.
pub fn normalize_asset(raw: AlpacaAsset) -> Result<Market, NormalizeError> {
    let symbol = raw.symbol.trim();
    if symbol.is_empty() {
        return Err(NormalizeError::MissingSymbol);
    }

    let venue = VenueId::new("alpaca")
        .map_err(|e| NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw }))?;

    let key = MarketKey::new(&venue, &symbol.to_lowercase())?;

    let kind = InstrumentKind::Equity;

    let status = normalize_status(&raw.status)?;

    let jurisdiction_flags = vec!["US".to_string()];

    // Build venue_ref from the entire raw asset JSON
    let venue_ref = JsonObject::new(serde_json::to_value(&raw).unwrap_or_else(|_| json!({})))?;

    // Build meta with asset attributes
    let meta = JsonObject::new(json!({
        "exchange": raw.exchange,
        "asset_class": raw.asset_class,
        "tradable": raw.tradable,
        "marginable": raw.marginable,
        "shortable": raw.shortable,
        "easy_to_borrow": raw.easy_to_borrow,
        "fractionable": raw.fractionable,
        "maintenance_margin_requirement": raw.maintenance_margin_requirement,
    }))?;

    Ok(Market {
        key,
        venue,
        kind,
        title: raw.name,
        description_ref: raw.exchange.clone(),
        status,
        close_ts: None,
        resolve_ts: None,
        outcome: None,
        jurisdiction_flags,
        venue_ref,
        meta,
    })
}

/// Map an Alpaca asset status string to the canonical `MarketStatus`.
fn normalize_status(status: &str) -> Result<MarketStatus, NormalizeError> {
    match status {
        "active" => Ok(MarketStatus::Open),
        "inactive" => Ok(MarketStatus::Closed),
        _ => Err(NormalizeError::UnknownStatus { status: status.to_string() }),
    }
}

// ---------------------------------------------------------------------------
// Snapshot normalization
// ---------------------------------------------------------------------------

/// Normalize a raw `AlpacaSnapshot` into a canonical `Quote`.
///
/// Extracts bid/ask from `latest_quote` and last trade price from `latest_trade`.
///
/// # Normalization rules
///
/// - `market` key: `mkt:alpaca:{symbol}` (lowercased)
/// - `bid` / `ask`: from quote fields, stored as Decimal (USD dollars)
/// - `last`: trade price as Decimal
/// - `ts`: best available timestamp (quote > trade)
/// - `source`: always `QuoteSource::Poll` (from snapshot REST endpoint)
///
/// # Errors
///
/// Returns `NormalizeError` if the symbol is empty, timestamps are malformed,
/// or MarketKey construction fails.
#[allow(dead_code)] // REST snapshot normalizer is retained for recorder/offline tooling.
pub fn normalize_snapshot(raw: &AlpacaSnapshot) -> Result<Quote, NormalizeError> {
    normalize_snapshot_with_source(raw, QuoteSource::Poll)
}

/// Normalize a WebSocket-derived snapshot without mislabeling it as REST data.
pub fn normalize_stream_snapshot(raw: &AlpacaSnapshot) -> Result<Quote, NormalizeError> {
    normalize_snapshot_with_source(raw, QuoteSource::Stream)
}

fn normalize_snapshot_with_source(
    raw: &AlpacaSnapshot,
    source: QuoteSource,
) -> Result<Quote, NormalizeError> {
    let symbol = raw.symbol.as_deref().unwrap_or("").trim();
    if symbol.is_empty() {
        return Err(NormalizeError::MissingSymbol);
    }

    let venue = VenueId::new("alpaca")
        .map_err(|e| NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw }))?;
    let market = MarketKey::new(&venue, &symbol.to_lowercase())?;

    let mut bid = None;
    let mut ask = None;
    let mut bid_size = None;
    let mut ask_size = None;
    let mut last = None;
    let mut ts = None;

    if let Some(ref quote) = raw.latest_quote {
        bid = parse_optional_decimal(quote.bp.as_deref(), "bid price", true)?;
        ask = parse_optional_decimal(quote.ap.as_deref(), "ask price", true)?;
        bid_size = parse_optional_decimal(quote.bs.as_deref(), "bid size", false)?;
        ask_size = parse_optional_decimal(quote.ask_size.as_deref(), "ask size", false)?;
        if let Some(ref t) = quote.t {
            ts = Some(parse_iso8601(t)?);
        }
    }

    if let Some(ref trade) = raw.latest_trade {
        last = parse_optional_decimal(trade.p.as_deref(), "last price", true)?;
        if ts.is_none() {
            if let Some(ref t) = trade.t {
                ts = Some(parse_iso8601(t)?);
            }
        }
    }

    if ts.is_none() {
        ts = Some(UtcTime::now());
    }

    if bid.is_none() && ask.is_none() && last.is_none() {
        return Err(NormalizeError::NoData { symbol: symbol.to_string() });
    }

    Ok(Quote {
        market,
        bid,
        ask,
        mid: match (bid, ask) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::TWO),
            _ => None,
        },
        last,
        bid_size,
        ask_size,
        ts: ts.unwrap_or_else(UtcTime::now),
        source,
        seq: None,
    })
}

fn parse_optional_decimal(
    raw: Option<&str>,
    field: &str,
    require_positive: bool,
) -> Result<Option<Decimal>, NormalizeError> {
    let Some(raw) = raw else { return Ok(None) };
    let value = raw.parse::<Decimal>().map_err(|error| NormalizeError::DecimalParse {
        raw: raw.to_string(),
        detail: error.to_string(),
    })?;
    let valid = if require_positive { value > Decimal::ZERO } else { value >= Decimal::ZERO };
    if !valid {
        return Err(NormalizeError::DecimalParse {
            raw: raw.to_string(),
            detail: format!(
                "{field} must be {}",
                if require_positive { "positive" } else { "non-negative" }
            ),
        });
    }
    Ok(Some(value))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse an RFC 3339 / ISO 8601 timestamp string into `UtcTime`.
fn parse_iso8601(ts: &str) -> Result<UtcTime, NormalizeError> {
    let json = format!("\"{}\"", ts);
    serde_json::from_str(&json)
        .map_err(|e| NormalizeError::TimestampParse { raw: ts.to_string(), detail: e.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_asset() -> AlpacaAsset {
        AlpacaAsset {
            id: "904837e3-3b76-47ec-b432-046db621571b".into(),
            asset_class: "us_equity".into(),
            exchange: "NASDAQ".into(),
            symbol: "AAPL".into(),
            name: "Apple Inc. Common Stock".into(),
            status: "active".into(),
            tradable: true,
            marginable: true,
            shortable: true,
            easy_to_borrow: true,
            fractionable: true,
            maintenance_margin_requirement: Some(30),
        }
    }

    #[test]
    fn normalize_active_asset() {
        let raw = make_asset();
        let m = normalize_asset(raw).unwrap();

        assert_eq!(m.key.as_str(), "mkt:alpaca:aapl");
        assert_eq!(m.venue.as_str(), "alpaca");
        assert_eq!(m.kind, InstrumentKind::Equity);
        assert_eq!(m.status, MarketStatus::Open);
        assert_eq!(m.title, "Apple Inc. Common Stock");
        assert_eq!(m.description_ref, "NASDAQ");
        assert!(!m.venue_ref.is_empty());
        assert!(!m.meta.is_empty());
    }

    #[test]
    fn normalize_inactive_asset() {
        let mut raw = make_asset();
        raw.status = "inactive".into();
        let m = normalize_asset(raw).unwrap();
        assert_eq!(m.status, MarketStatus::Closed);
    }

    #[test]
    fn normalize_unknown_status_errors() {
        let mut raw = make_asset();
        raw.status = "unknown".into();
        let result = normalize_asset(raw);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::UnknownStatus { .. })));
    }

    #[test]
    fn normalize_empty_symbol_errors() {
        let mut raw = make_asset();
        raw.symbol = "".into();
        let result = normalize_asset(raw);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_symbol_lowercased_in_key() {
        let raw = make_asset();
        let m = normalize_asset(raw).unwrap();
        assert_eq!(m.key.as_str(), "mkt:alpaca:aapl");
    }

    #[test]
    fn normalize_preserves_venue_ref() {
        let raw = make_asset();
        let m = normalize_asset(raw).unwrap();
        let vr = m.venue_ref.as_value();
        assert!(vr.get("symbol").is_some());
        assert!(vr.get("name").is_some());
        assert!(vr.get("status").is_some());
    }

    // ── Snapshot normalization tests ──

    fn make_snapshot() -> AlpacaSnapshot {
        AlpacaSnapshot {
            symbol: Some("AAPL".into()),
            latest_trade: Some(crate::client::AlpacaTrade {
                t: Some("2021-05-11T20:00:00.435997104Z".into()),
                x: Some("Q".into()),
                p: Some("125.91".into()),
                s: Some("5589631".into()),
                i: Some(179430),
                z: Some("C".into()),
                ..Default::default()
            }),
            latest_quote: Some(crate::client::AlpacaQuote {
                t: Some("2021-05-11T22:05:02.307304704Z".into()),
                ax: Some("P".into()),
                ap: Some("125.68".into()),
                ask_size: Some("12".into()),
                bx: Some("P".into()),
                bp: Some("125.6".into()),
                bs: Some("4".into()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn normalize_snapshot_creates_quote() {
        let snap = make_snapshot();
        let quote = normalize_snapshot(&snap).unwrap();

        assert_eq!(quote.market.as_str(), "mkt:alpaca:aapl");
        assert!(quote.bid.is_some());
        assert!(quote.ask.is_some());
        assert!(quote.last.is_some());
        assert_eq!(quote.source, QuoteSource::Poll);
    }

    #[test]
    fn normalize_snapshot_bid_ask_values() {
        let snap = make_snapshot();
        let quote = normalize_snapshot(&snap).unwrap();

        assert_eq!(quote.bid.unwrap().to_string(), "125.6");
        assert_eq!(quote.ask.unwrap().to_string(), "125.68");
        assert_eq!(quote.last.unwrap().to_string(), "125.91");
        assert_eq!(quote.mid.unwrap().to_string(), "125.64");
    }

    #[test]
    fn normalize_snapshot_empty_symbol_errors() {
        let snap = AlpacaSnapshot { symbol: Some("".into()), ..Default::default() };
        let result = normalize_snapshot(&snap);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_snapshot_no_data_errors() {
        let snap = AlpacaSnapshot { symbol: Some("AAPL".into()), ..Default::default() };
        let result = normalize_snapshot(&snap);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::NoData { .. })));
    }

    #[test]
    fn normalize_snapshot_bad_timestamp_errors() {
        let mut snap = make_snapshot();
        snap.latest_quote = Some(crate::client::AlpacaQuote {
            t: Some("not-a-timestamp".into()),
            bp: Some("100".into()),
            ..Default::default()
        });
        snap.latest_trade = None;
        let result = normalize_snapshot(&snap);
        assert!(result.is_err());
        assert!(matches!(result, Err(NormalizeError::TimestampParse { .. })));
    }

    #[test]
    fn parse_iso8601_valid() {
        let ts = parse_iso8601("2021-05-11T20:00:00.435997104Z").unwrap();
        assert!(ts.unix_millis() > 0);
    }

    #[test]
    fn parse_iso8601_invalid() {
        let result = parse_iso8601("not-a-date");
        assert!(result.is_err());
    }
}

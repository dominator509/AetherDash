//! Market normalization: Polymarket raw types -> canonical `aether_core::Market`,
//! `Quote`, and `OrderBook`.
//!
//! # Normalization rules
//!
//! | Polymarket field | Canonical field | Notes |
//! |---|---|---|
//! | `condition_id` | `MarketKey` | `mkt:polymarket:{token_id}` per outcome token |
//! | `question` | `title` | Combined with outcome label: `"{question} — {outcome}"` |
//! | `outcome_prices[i]` | `meta.outcome_price` | Already in `[0,1]` probability space |
//! | `active` / `closed` | `MarketStatus` | See `normalize_status` |
//! | `clob_token_ids[i]` | `key.native_id` | Each token is a separate `Market` |
//! | entire raw JSON | `venue_ref` | Preserved for provenance |
//!
//! # Polymarket specifics
//!
//! - Binary outcomes (2 outcomes) produce `BinaryContract` markets.
//! - Multi-outcome (>2) produce `CategoricalContract` markets.
//! - Prices are already probability-shaped decimal strings in `[0,1]` USDC terms.
//! - US is blocked: `jurisdiction_flags` always includes `["US"]`.
//!
//! # Source-of-truth priority
//!
//! Per AGENTS.md 4.1: when this module disagrees with aether-core, aether-core wins.

use crate::client::GammaMarket;
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
// Raw CLOB types
// ---------------------------------------------------------------------------

/// A single level in a Polymarket CLOB order book.
///
/// Both `price` and `size` are decimal strings (e.g. `"0.65"`, `"100.5"`).
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClobLevel {
    /// Price as a decimal string (already a probability in `[0,1]`).
    pub price: String,
    /// Size as a decimal string (USDC units).
    pub size: String,
}

/// A raw CLOB order-book snapshot from the Polymarket REST API.
///
/// `market` is the **condition ID** that groups outcome tokens under one event;
/// `asset_id` is the individual **token ID** for a single outcome.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClobBookSnapshot {
    /// Condition ID (e.g. `"0xabc..."`).
    pub market: String,
    /// Token / asset ID for a single outcome.
    pub asset_id: String,
    /// Venue timestamp (epoch milliseconds on the current API; RFC 3339 is also accepted).
    pub timestamp: String,
    /// Bid levels (may be unsorted in raw form).
    pub bids: Vec<ClobLevel>,
    /// Ask levels (may be unsorted in raw form).
    pub asks: Vec<ClobLevel>,
    /// Optional hash for change-detection / deduplication.
    #[serde(default)]
    pub hash: Option<String>,
}

/// A price change or trade event from the Polymarket WebSocket feed.
///
/// The `event_type` distinguishes `"last_trade_price"` (trades) from
/// `"price_change"` (book-level updates).
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClobTick {
    /// Event type discriminator, e.g. `"last_trade_price"`, `"price_change"`.
    pub event_type: String,
    /// Token / asset ID for a single outcome.
    pub asset_id: String,
    /// Condition ID for the parent market.
    pub market: String,
    /// Price as a decimal string (already a probability in `[0,1]`).
    pub price: String,
    /// Size as a decimal string.
    pub size: String,
    /// Trade side: `"BUY"` or `"SELL"` (may be absent on book-level ticks).
    #[serde(default)]
    pub side: Option<String>,
    /// Current best bid after a `price_change` / `best_bid_ask` event.
    #[serde(default)]
    pub best_bid: Option<String>,
    /// Current best ask after a `price_change` / `best_bid_ask` event.
    #[serde(default)]
    pub best_ask: Option<String>,
    /// Venue timestamp (epoch milliseconds on the current API; RFC 3339 is also accepted).
    #[serde(default)]
    pub timestamp: Option<String>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during Polymarket normalization.
///
/// Per the M3 quarantine contract: malformed data always returns `Err`, never
/// panics.
#[derive(Error, Debug)]
pub enum NormalizeError {
    /// A required field is missing or empty.
    #[error("missing or empty field: {0}")]
    MissingField(String),

    /// Failed to construct a `MarketKey` (invalid venue or native ID).
    #[error("invalid MarketKey: {0}")]
    MarketKey(#[from] aether_core::ids::MarketKeyError),

    /// Failed to construct the `venue_ref` or `meta` JSON object.
    #[error("JSON object construction failed: {0}")]
    JsonObject(#[from] aether_core::json::JsonObjectError),

    /// Failed to parse an ISO 8601 / RFC3339 timestamp.
    #[allow(dead_code)]
    #[error("timestamp parse error for '{raw}': {detail}")]
    TimestampParse {
        /// The raw timestamp string that failed to parse.
        raw: String,
        /// Details about the parse failure.
        detail: String,
    },

    /// OrderBook validation failed (ordering invariant violated).
    #[error("order book error: {0}")]
    OrderBook(#[from] OrderBookError),

    /// Failed to parse a decimal string.
    #[error("invalid decimal '{raw}': {detail}")]
    DecimalParse {
        /// The raw string that failed decimal parsing.
        raw: String,
        /// Details about the parse failure.
        detail: String,
    },

    /// A syntactically valid decimal violated probability or size bounds.
    #[error("decimal out of range for {field}: '{raw}'")]
    DecimalOutOfRange { field: &'static str, raw: String },

    /// A top-of-book update crossed the market.
    #[error("invalid quote: bid {bid} exceeds ask {ask}")]
    InvalidQuote { bid: Decimal, ask: Decimal },
}

// ---------------------------------------------------------------------------
// Venue helper
// ---------------------------------------------------------------------------

/// Return the canonical `VenueId` for Polymarket.
fn venue() -> Result<VenueId, NormalizeError> {
    VenueId::new("polymarket")
        .map_err(|e| NormalizeError::MarketKey(aether_core::ids::MarketKeyError { raw: e.raw }))
}

// ---------------------------------------------------------------------------
// Status normalisation (Gamma boolean pair -> canonical enum)
// ---------------------------------------------------------------------------

/// Map the Gamma API `active` / `closed` boolean pair to `MarketStatus`.
///
/// | active | closed | Status     |
/// |--------|--------|------------|
/// | true   | false  | `Open`     |
/// | false  | true   | `Closed`   |
/// | true   | true   | `Closed`   |
/// | false  | false  | `Halted`   |
fn normalize_status(active: bool, closed: bool) -> MarketStatus {
    match (active, closed) {
        (true, false) => MarketStatus::Open,
        (false, true) | (true, true) => MarketStatus::Closed,
        (false, false) => MarketStatus::Halted,
    }
}

// ---------------------------------------------------------------------------
// Market normalisation (M2 / M3)
// ---------------------------------------------------------------------------

/// Normalise the **first** outcome of a raw `GammaMarket` into a single
/// canonical `Market`.
///
/// This is a convenience wrapper around [`normalize_markets`] for callers that
/// expect a one-to-one market response (e.g. single-market lookup).
///
/// # Errors
///
/// Returns `NormalizeError::MissingField("outcomes")` when the market has no
/// outcomes, and all errors that [`normalize_markets`] can produce.
#[allow(dead_code)]
pub fn normalize_market(raw: GammaMarket) -> Result<Market, NormalizeError> {
    normalize_markets(raw)?
        .into_iter()
        .next()
        .ok_or_else(|| NormalizeError::MissingField("outcomes".into()))
}

/// Normalise a raw `GammaMarket` into **one canonical `Market` per outcome
/// token**.
///
/// # Normalisation rules
///
/// - **MarketKey**: `mkt:polymarket:{token_id}` where `token_id` comes from
///   `clob_token_ids[i]`.
/// - **Kind**: `BinaryContract` when there are exactly 2 outcomes (always
///   "Yes"/"No" in Polymarket), `CategoricalContract` for >2 outcomes.
/// - **Status**: mapped from `active` / `closed` via [`normalize_status`].
/// - **Title**: `"{question} — {outcome_label}"`.
/// - **Jurisdiction**: `["US"]` — Polymarket blocks US persons.
/// - **`venue_ref`**: copy of the entire raw `GammaMarket` JSON for provenance.
/// - **`meta`**: condition_id, outcome_index, outcome_label, sibling outcomes
///   and token IDs, outcome_prices, volume, liquidity.
/// - **Prices**: parsed as `Decimal` from strings already in `[0,1]`
///   probability space — no scaling needed.
///
/// # Errors
///
/// Returns `MissingField` if:
/// - The `question` is empty / whitespace-only.
/// - `outcomes`, `outcome_prices`, or `clob_token_ids` are empty or
///   length-mismatched.
/// - Any token ID is empty after trimming.
/// - An outcome price cannot be parsed as `Decimal`.
#[allow(dead_code)]
pub fn normalize_markets(raw: GammaMarket) -> Result<Vec<Market>, NormalizeError> {
    let question = raw.question.trim();
    if question.is_empty() {
        return Err(NormalizeError::MissingField("question".into()));
    }

    let v = venue()?;

    let status = normalize_status(raw.active, raw.closed);

    // Gamma API returns these as JSON-encoded arrays of strings.
    // Parse them once so we can index and iterate.
    let outcomes: Vec<String> = serde_json::from_str(&raw.outcomes)
        .map_err(|e| NormalizeError::MissingField(format!("failed to parse outcomes JSON: {e}")))?;
    let outcome_prices: Vec<String> = serde_json::from_str(&raw.outcome_prices).map_err(|e| {
        NormalizeError::MissingField(format!("failed to parse outcome_prices JSON: {e}"))
    })?;
    let clob_token_ids: Vec<String> = serde_json::from_str(&raw.clob_token_ids).map_err(|e| {
        NormalizeError::MissingField(format!("failed to parse clob_token_ids JSON: {e}"))
    })?;

    let kind = if outcomes.len() == 2 {
        InstrumentKind::BinaryContract
    } else {
        InstrumentKind::CategoricalContract
    };

    if outcomes.is_empty() {
        return Err(NormalizeError::MissingField("outcomes".into()));
    }
    if clob_token_ids.is_empty() {
        return Err(NormalizeError::MissingField("clob_token_ids".into()));
    }
    if outcomes.len() != clob_token_ids.len() {
        return Err(NormalizeError::MissingField("outcome/token_id count mismatch".into()));
    }
    if outcomes.len() != outcome_prices.len() {
        return Err(NormalizeError::MissingField("outcome/price count mismatch".into()));
    }

    // Build the shared venue_ref from the full raw market.
    let venue_ref = JsonObject::new(serde_json::to_value(&raw).unwrap_or_else(|_| json!({})))?;

    let mut markets = Vec::with_capacity(outcomes.len());

    for i in 0..outcomes.len() {
        let token_id = clob_token_ids[i].trim();
        if token_id.is_empty() {
            return Err(NormalizeError::MissingField(format!("clob_token_ids[{}]", i)));
        }

        let key = MarketKey::new(&v, token_id)?;

        let title = format!("{} — {}", question, outcomes[i].trim());

        // Parse the outcome price — already in [0,1] probability space.
        let price_val: Option<Decimal> = {
            let trimmed = outcome_prices[i].trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(parse_probability(trimmed)?)
            }
        };

        // Sibling data for cross-outcome reference.
        let sibling_outcomes: Vec<&str> = outcomes
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, label)| label.as_str())
            .collect();

        let sibling_token_ids: Vec<&str> = clob_token_ids
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, id)| id.as_str())
            .collect();

        let meta = JsonObject::new(json!({
            "condition_id": raw.condition_id,
            "outcome_index": i,
            "outcome_label": outcomes[i],
            "outcomes": outcomes,
            "sibling_outcomes": sibling_outcomes,
            "sibling_token_ids": sibling_token_ids,
            "outcome_price": price_val,
            "outcome_prices": outcome_prices,
            "token_ids": clob_token_ids,
            "volume": raw.volume_num,
            "liquidity": raw.liquidity_num,
        }))?;

        markets.push(Market {
            key,
            venue: v.clone(),
            kind,
            title,
            description_ref: question.to_string(),
            status,
            close_ts: raw.end_date.as_deref().map(parse_venue_timestamp).transpose()?,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec!["US".to_string()],
            venue_ref: venue_ref.clone(),
            meta,
        });
    }

    Ok(markets)
}

// ---------------------------------------------------------------------------
// Timestamp and decimal helpers
// ---------------------------------------------------------------------------

/// Parse an RFC 3339 / ISO 8601 timestamp string into `UtcTime`.
///
/// Reuses `UtcTime`'s own `Deserialize` implementation (handles `Z` suffix,
/// `+00:00` offset, normalises sub-millisecond precision).
#[allow(dead_code)]
fn parse_venue_timestamp(ts: &str) -> Result<UtcTime, NormalizeError> {
    if !ts.is_empty() && ts.bytes().all(|byte| byte.is_ascii_digit()) {
        let millis = ts.parse::<i64>().map_err(|error| NormalizeError::TimestampParse {
            raw: ts.to_string(),
            detail: error.to_string(),
        })?;
        return UtcTime::from_unix_millis(millis).map_err(|error| NormalizeError::TimestampParse {
            raw: ts.to_string(),
            detail: error.to_string(),
        });
    }
    let json = format!("\"{}\"", ts);
    serde_json::from_str(&json)
        .map_err(|e| NormalizeError::TimestampParse { raw: ts.to_string(), detail: e.to_string() })
}

/// Parse a decimal string into `Decimal`.
fn parse_decimal(s: &str) -> Result<Decimal, NormalizeError> {
    s.parse::<Decimal>()
        .map_err(|e| NormalizeError::DecimalParse { raw: s.to_string(), detail: e.to_string() })
}

fn parse_probability(s: &str) -> Result<Decimal, NormalizeError> {
    let value = parse_decimal(s)?;
    if !(Decimal::ZERO..=Decimal::ONE).contains(&value) {
        return Err(NormalizeError::DecimalOutOfRange { field: "probability", raw: s.to_string() });
    }
    Ok(value)
}

fn parse_size(s: &str) -> Result<Decimal, NormalizeError> {
    let value = parse_decimal(s)?;
    if value < Decimal::ZERO {
        return Err(NormalizeError::DecimalOutOfRange { field: "size", raw: s.to_string() });
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Order-book normalisation (M3)
// ---------------------------------------------------------------------------

/// Normalise a raw `ClobBookSnapshot` into a canonical `OrderBook`.
///
/// # Normalisation rules
///
/// - **MarketKey**: `mkt:polymarket:{asset_id}` where `asset_id` is the
///   outcome token ID.
/// - **Bids**: sorted descending by price.
/// - **Asks**: sorted ascending by price.
/// - **Prices**: probability decimal strings already in `[0,1]` — no scaling.
/// - **Timestamp**: epoch milliseconds or RFC 3339 / ISO 8601 -> `UtcTime`.
///
/// # Errors
///
/// Returns error if `asset_id` is empty, the timestamp is malformed, or any
/// price / size field cannot be parsed as a `Decimal`.
#[allow(dead_code)]
pub fn normalize_book(raw: ClobBookSnapshot) -> Result<OrderBook, NormalizeError> {
    let asset_id = raw.asset_id.trim();
    if asset_id.is_empty() {
        return Err(NormalizeError::MissingField("asset_id".into()));
    }

    let v = venue()?;
    let market = MarketKey::new(&v, asset_id)?;

    let ts = parse_venue_timestamp(&raw.timestamp)?;

    let mut bids: Vec<BookLevel> = raw
        .bids
        .iter()
        .map(|level| {
            let price = parse_probability(&level.price)?;
            let size = parse_size(&level.size)?;
            Ok(BookLevel { price, size })
        })
        .collect::<Result<Vec<_>, NormalizeError>>()?;

    let mut asks: Vec<BookLevel> = raw
        .asks
        .iter()
        .map(|level| {
            let price = parse_probability(&level.price)?;
            let size = parse_size(&level.size)?;
            Ok(BookLevel { price, size })
        })
        .collect::<Result<Vec<_>, NormalizeError>>()?;

    // Canonical ordering: bids descending, asks ascending.
    bids.sort_by_key(|b| std::cmp::Reverse(b.price));
    asks.sort_by_key(|a| a.price);

    let depth = bids.len().max(asks.len());

    Ok(OrderBook::new(market, bids, asks, depth, ts, None)?)
}

// ---------------------------------------------------------------------------
// Tick / quote normalisation (M3)
// ---------------------------------------------------------------------------

/// Normalise a raw `ClobTick` (from WebSocket) into a canonical `Quote`.
///
/// # Normalisation rules
///
/// - **MarketKey**: `mkt:polymarket:{asset_id}`.
/// - **`last`**: populated only for `last_trade_price` events.
/// - **`bid` / `ask`**: populated from documented best prices on book updates.
/// - **`ts`**: epoch milliseconds or ISO 8601 -> `UtcTime`; falls back to `UtcTime::now()`
///   when the tick has no timestamp.
/// - **`source`**: always `QuoteSource::Stream`.
/// - **`bid` / `ask` / `mid`**: populated from the documented best prices on
///   `price_change` and `best_bid_ask`; `last_trade_price` populates only `last`.
///
/// # Errors
///
/// Returns error if `asset_id` is empty, the timestamp is malformed, or the
/// price field cannot be parsed as `Decimal`.
#[allow(dead_code)]
pub fn normalize_tick(raw: ClobTick) -> Result<Quote, NormalizeError> {
    let asset_id = raw.asset_id.trim();
    if asset_id.is_empty() {
        return Err(NormalizeError::MissingField("asset_id".into()));
    }

    let v = venue()?;
    let market = MarketKey::new(&v, asset_id)?;

    let ts = match &raw.timestamp {
        Some(ts) => parse_venue_timestamp(ts)?,
        None => UtcTime::now(),
    };

    if !raw.size.is_empty() {
        parse_size(&raw.size)?;
    }

    let (bid, ask, last) = match raw.event_type.as_str() {
        "last_trade_price" => (None, None, Some(parse_probability(&raw.price)?)),
        "price_change" | "best_bid_ask" => {
            let bid = raw.best_bid.as_deref().map(parse_probability).transpose()?;
            let ask = raw.best_ask.as_deref().map(parse_probability).transpose()?;
            if bid.is_none() && ask.is_none() {
                return Err(NormalizeError::MissingField("best_bid/best_ask".into()));
            }
            if let (Some(bid), Some(ask)) = (bid, ask) {
                if bid > ask {
                    return Err(NormalizeError::InvalidQuote { bid, ask });
                }
            }
            (bid, ask, None)
        }
        _ => return Err(NormalizeError::MissingField("supported event_type".into())),
    };
    let mid = match (bid, ask) {
        (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::TWO),
        _ => None,
    };

    Ok(Quote {
        market,
        bid,
        ask,
        mid,
        last,
        bid_size: None,
        ask_size: None,
        ts,
        source: QuoteSource::Stream,
        seq: None,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::GammaMarket;

    // ── Helpers ─────────────────────────────────────────────────────────

    fn make_binary_market() -> GammaMarket {
        serde_json::from_str(
            r#"{
                "id": "1",
                "question": "Will BTC be above $75k?",
                "slug": "btc-75k",
                "conditionId": "0xabc123",
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.65\", \"0.35\"]",
                "clobTokenIds": "[\"123456789\", \"987654321\"]",
                "active": true,
                "closed": false,
                "volumeNum": 1000000.0,
                "liquidityNum": 500000.0
            }"#,
        )
        .unwrap()
    }

    fn make_categorical_market() -> GammaMarket {
        serde_json::from_str(
            r#"{
                "id": "2",
                "question": "Which party wins the 2028 election?",
                "slug": "election-2028",
                "conditionId": "0xdef456",
                "outcomes": "[\"Democrat\", \"Republican\", \"Libertarian\", \"Green\"]",
                "outcomePrices": "[\"0.45\", \"0.40\", \"0.10\", \"0.05\"]",
                "clobTokenIds": "[\"111\", \"222\", \"333\", \"444\"]",
                "active": true,
                "closed": false,
                "volumeNum": 2000000.0,
                "liquidityNum": 1000000.0
            }"#,
        )
        .unwrap()
    }

    // ── Market normalisation tests ─────────────────────────────────────

    #[test]
    fn normalize_binary_two_outcomes() {
        let raw = make_binary_market();
        let markets = normalize_markets(raw).unwrap();

        assert_eq!(markets.len(), 2, "should produce one Market per outcome");

        // ---- Yes token (index 0) ----
        assert_eq!(markets[0].key.as_str(), "mkt:polymarket:123456789", "key uses clob_token_id");
        assert_eq!(markets[0].kind, InstrumentKind::BinaryContract, "2 outcomes -> BinaryContract");
        assert_eq!(markets[0].status, MarketStatus::Open, "active && !closed");
        assert!(
            markets[0].title.contains("Will BTC be above $75k?"),
            "title includes question: {}",
            markets[0].title
        );
        assert!(
            markets[0].title.contains("Yes"),
            "title includes outcome label: {}",
            markets[0].title
        );
        assert_eq!(markets[0].jurisdiction_flags, vec!["US"], "US is blocked");
        assert!(!markets[0].venue_ref.is_empty(), "venue_ref populated");
        assert!(!markets[0].meta.is_empty(), "meta populated");

        // ---- No token (index 1) ----
        assert_eq!(
            markets[1].key.as_str(),
            "mkt:polymarket:987654321",
            "second token gets its own key"
        );
        assert!(markets[1].title.contains("No"), "title includes 'No': {}", markets[1].title);

        // Keys must be unique.
        assert_ne!(markets[0].key, markets[1].key, "outcome tokens must have distinct keys");
    }

    #[test]
    fn normalize_categorical_multi_outcome() {
        let raw = make_categorical_market();
        let markets = normalize_markets(raw).unwrap();

        assert_eq!(markets.len(), 4, "4 outcomes -> 4 Markets");

        for m in &markets {
            assert_eq!(
                m.kind,
                InstrumentKind::CategoricalContract,
                ">2 outcomes -> CategoricalContract"
            );
            assert_eq!(m.status, MarketStatus::Open);
        }

        let keys: Vec<&str> = markets.iter().map(|m| m.key.as_str()).collect();
        assert!(keys.contains(&"mkt:polymarket:111"), "Democrat token present");
        assert!(keys.contains(&"mkt:polymarket:222"), "Republican token present");
        assert!(keys.contains(&"mkt:polymarket:333"), "Libertarian token present");
        assert!(keys.contains(&"mkt:polymarket:444"), "Green token present");
    }

    #[test]
    fn normalize_single_market_convenience() {
        // `normalize_market` should return only the first outcome.
        let raw = make_binary_market();
        let m = normalize_market(raw).unwrap();

        assert_eq!(m.key.as_str(), "mkt:polymarket:123456789");
        assert_eq!(m.kind, InstrumentKind::BinaryContract);
        assert!(m.title.contains("Yes"), "first outcome is Yes");
    }

    #[test]
    fn normalize_closed_market_stays_closed_until_on_chain_resolution() {
        let raw: GammaMarket = serde_json::from_str(
            r#"{
                "id": "3",
                "question": "Will it rain tomorrow?",
                "slug": "rain-tomorrow",
                "conditionId": "0xresolved",
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"1.0\", \"0.0\"]",
                "clobTokenIds": "[\"rain_yes\", \"rain_no\"]",
                "active": false,
                "closed": true,
                "volumeNum": 100000.0,
                "liquidityNum": 50000.0
            }"#,
        )
        .unwrap();

        let markets = normalize_markets(raw).unwrap();
        assert_eq!(markets.len(), 2);
        for m in &markets {
            assert_eq!(m.status, MarketStatus::Closed, "Gamma closed is not on-chain resolved");
        }
    }

    #[test]
    fn inactive_unclosed_market_is_halted_not_malformed() {
        assert_eq!(normalize_status(false, false), MarketStatus::Halted);
    }

    #[test]
    fn normalize_empty_question_errors() {
        let raw: GammaMarket = serde_json::from_str(
            r#"{
                "id": "4",
                "question": "",
                "slug": "empty-q",
                "conditionId": "0xerr",
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.5\", \"0.5\"]",
                "clobTokenIds": "[\"e1\", \"e2\"]",
                "active": true,
                "closed": false
            }"#,
        )
        .unwrap();

        let result = normalize_markets(raw);
        assert!(result.is_err(), "empty question should fail");
        assert!(
            matches!(&result, Err(NormalizeError::MissingField(f)) if f == "question"),
            "expected MissingField('question'), got {:?}",
            result
        );
    }

    #[test]
    fn normalize_empty_token_id_errors() {
        let raw: GammaMarket = serde_json::from_str(
            r#"{
                "id": "5",
                "question": "Test question?",
                "slug": "test-q",
                "conditionId": "0xerr",
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.5\", \"0.5\"]",
                "clobTokenIds": "[\"valid_id\", \"\"]",
                "active": true,
                "closed": false
            }"#,
        )
        .unwrap();

        let result = normalize_markets(raw);
        assert!(result.is_err(), "empty token ID should fail");
        assert!(
            matches!(&result, Err(NormalizeError::MissingField(f)) if f == "clob_token_ids[1]"),
            "expected MissingField('clob_token_ids[1]'), got {:?}",
            result
        );
    }

    #[test]
    fn normalize_outcome_price_mismatch_errors() {
        let raw: GammaMarket = serde_json::from_str(
            r#"{
                "id": "6",
                "question": "Test?",
                "slug": "test-mismatch",
                "conditionId": "0xerr",
                "outcomes": "[\"A\", \"B\", \"C\"]",
                "outcomePrices": "[\"0.3\", \"0.3\"]",
                "clobTokenIds": "[\"a1\", \"b2\", \"c3\"]",
                "active": true,
                "closed": false
            }"#,
        )
        .unwrap();

        let result = normalize_markets(raw);
        assert!(result.is_err(), "mismatched prices should fail");
        assert!(
            matches!(&result, Err(NormalizeError::MissingField(f)) if f == "outcome/price count mismatch"),
            "expected count-mismatch error, got {:?}",
            result
        );
    }

    // ── Order-book normalisation tests ─────────────────────────────────

    #[test]
    fn normalize_book_creates_orderbook() {
        let raw = ClobBookSnapshot {
            market: "0xabc123".into(),
            asset_id: "123456789".into(),
            timestamp: "2026-07-10T12:34:56.789Z".into(),
            bids: vec![
                ClobLevel { price: "0.65".into(), size: "100.5".into() },
                ClobLevel { price: "0.64".into(), size: "200.0".into() },
                ClobLevel { price: "0.63".into(), size: "50.2".into() },
            ],
            asks: vec![
                ClobLevel { price: "0.66".into(), size: "150.0".into() },
                ClobLevel { price: "0.67".into(), size: "80.3".into() },
                ClobLevel { price: "0.68".into(), size: "120.1".into() },
            ],
            hash: Some("0xdeadbeef".into()),
        };

        let ob = normalize_book(raw).unwrap();

        assert_eq!(ob.market.as_str(), "mkt:polymarket:123456789", "key uses asset_id");
        assert_eq!(ob.bids().len(), 3);
        assert_eq!(ob.asks().len(), 3);

        // Bids descending: 0.65 > 0.64 > 0.63
        assert_eq!(ob.bids()[0].price, Decimal::new(65, 2));
        assert_eq!(ob.bids()[1].price, Decimal::new(64, 2));
        assert_eq!(ob.bids()[2].price, Decimal::new(63, 2));

        // Asks ascending: 0.66 < 0.67 < 0.68
        assert_eq!(ob.asks()[0].price, Decimal::new(66, 2));
        assert_eq!(ob.asks()[1].price, Decimal::new(67, 2));
        assert_eq!(ob.asks()[2].price, Decimal::new(68, 2));

        // Size parsing: 100.5 -> 1005 / 10^1
        assert_eq!(ob.bids()[0].size, Decimal::new(1005, 1));
        assert_eq!(ob.asks()[0].size, Decimal::new(1500, 1));
    }

    #[test]
    fn normalize_book_sorts_levels() {
        // Feed unsorted levels and verify canonical ordering.
        let raw = ClobBookSnapshot {
            market: "0xmkt".into(),
            asset_id: "tok_1".into(),
            timestamp: "2026-07-10T12:00:00.000Z".into(),
            bids: vec![
                ClobLevel { price: "0.30".into(), size: "10".into() },
                ClobLevel { price: "0.50".into(), size: "20".into() },
                ClobLevel { price: "0.40".into(), size: "15".into() },
            ],
            asks: vec![
                ClobLevel { price: "0.70".into(), size: "10".into() },
                ClobLevel { price: "0.60".into(), size: "15".into() },
                ClobLevel { price: "0.55".into(), size: "25".into() },
            ],
            hash: None,
        };

        let ob = normalize_book(raw).unwrap();

        // Bids: 0.50, 0.40, 0.30
        assert_eq!(ob.bids()[0].price, Decimal::new(50, 2));
        assert_eq!(ob.bids()[1].price, Decimal::new(40, 2));
        assert_eq!(ob.bids()[2].price, Decimal::new(30, 2));

        // Asks: 0.55, 0.60, 0.70
        assert_eq!(ob.asks()[0].price, Decimal::new(55, 2));
        assert_eq!(ob.asks()[1].price, Decimal::new(60, 2));
        assert_eq!(ob.asks()[2].price, Decimal::new(70, 2));
    }

    #[test]
    fn normalize_book_empty_asset_id_errors() {
        let raw = ClobBookSnapshot {
            market: "0xmkt".into(),
            asset_id: "".into(),
            timestamp: "2026-07-10T12:00:00.000Z".into(),
            bids: vec![],
            asks: vec![],
            hash: None,
        };

        let result = normalize_book(raw);
        assert!(result.is_err(), "empty asset_id should fail");
        assert!(
            matches!(&result, Err(NormalizeError::MissingField(f)) if f == "asset_id"),
            "expected MissingField('asset_id'), got {:?}",
            result
        );
    }

    #[test]
    fn normalize_book_bad_timestamp_errors() {
        let raw = ClobBookSnapshot {
            market: "0xmkt".into(),
            asset_id: "tok_1".into(),
            timestamp: "not-a-timestamp".into(),
            bids: vec![],
            asks: vec![],
            hash: None,
        };

        let result = normalize_book(raw);
        assert!(result.is_err());
        assert!(
            matches!(&result, Err(NormalizeError::TimestampParse { .. })),
            "expected TimestampParse error, got {:?}",
            result
        );
    }

    // ── Tick normalisation tests ───────────────────────────────────────

    #[test]
    fn normalize_tick_creates_quote() {
        let raw = ClobTick {
            event_type: "last_trade_price".into(),
            asset_id: "123456789".into(),
            market: "0xabc123".into(),
            price: "0.65".into(),
            size: "10.0".into(),
            side: Some("BUY".into()),
            best_bid: None,
            best_ask: None,
            timestamp: Some("2026-07-10T12:34:56.789Z".into()),
        };

        let quote = normalize_tick(raw).unwrap();

        assert_eq!(quote.market.as_str(), "mkt:polymarket:123456789", "key uses asset_id");
        assert_eq!(quote.last, Some(Decimal::new(65, 2)), "price -> 0.65");
        assert_eq!(quote.bid, None, "no bid in price-change tick");
        assert_eq!(quote.ask, None, "no ask in price-change tick");
        assert_eq!(quote.bid_size, None);
        assert_eq!(quote.ask_size, None);
        assert_eq!(quote.source, QuoteSource::Stream);
        assert_eq!(quote.seq, None);
    }

    #[test]
    fn normalize_tick_empty_asset_id_errors() {
        let raw = ClobTick {
            event_type: "last_trade_price".into(),
            asset_id: "".into(),
            market: "0xabc".into(),
            price: "0.50".into(),
            size: "5.0".into(),
            side: None,
            best_bid: None,
            best_ask: None,
            timestamp: Some("2026-07-10T12:00:00.000Z".into()),
        };

        let result = normalize_tick(raw);
        assert!(result.is_err(), "empty asset_id should fail");
        assert!(
            matches!(&result, Err(NormalizeError::MissingField(f)) if f == "asset_id"),
            "expected MissingField('asset_id'), got {:?}",
            result
        );
    }

    #[test]
    fn normalize_tick_bad_timestamp_errors() {
        let raw = ClobTick {
            event_type: "last_trade_price".into(),
            asset_id: "tok_1".into(),
            market: "0xmkt".into(),
            price: "0.50".into(),
            size: "5.0".into(),
            side: None,
            best_bid: None,
            best_ask: None,
            timestamp: Some("bad-ts".into()),
        };

        let result = normalize_tick(raw);
        assert!(result.is_err());
        assert!(
            matches!(&result, Err(NormalizeError::TimestampParse { .. })),
            "expected TimestampParse error, got {:?}",
            result
        );
    }

    #[test]
    fn normalize_tick_missing_timestamp_uses_now() {
        let raw = ClobTick {
            event_type: "last_trade_price".into(),
            asset_id: "tok_1".into(),
            market: "0xmkt".into(),
            price: "0.50".into(),
            size: "5.0".into(),
            side: None,
            best_bid: None,
            best_ask: None,
            timestamp: None,
        };

        let quote = normalize_tick(raw).unwrap();
        // When timestamp is absent, UtcTime::now() is used.
        let now = UtcTime::now();
        let diff = now.unix_millis() - quote.ts.unix_millis();
        assert!(diff.abs() < 5000, "timestamp should be close to now (diff={}ms)", diff);
    }

    #[test]
    fn normalize_tick_bad_price_errors() {
        let raw = ClobTick {
            event_type: "last_trade_price".into(),
            asset_id: "tok_1".into(),
            market: "0xmkt".into(),
            price: "not-a-number".into(),
            size: "5.0".into(),
            side: None,
            best_bid: None,
            best_ask: None,
            timestamp: Some("2026-07-10T12:00:00.000Z".into()),
        };

        let result = normalize_tick(raw);
        assert!(result.is_err());
        assert!(
            matches!(&result, Err(NormalizeError::DecimalParse { .. })),
            "expected DecimalParse error, got {:?}",
            result
        );
    }

    // ── Meta content tests ─────────────────────────────────────────────

    #[test]
    fn market_meta_includes_condition_and_siblings() {
        let raw = make_binary_market();
        let markets = normalize_markets(raw).unwrap();

        let meta0 = markets[0].meta.as_value();
        assert_eq!(meta0["condition_id"], "0xabc123");
        assert_eq!(meta0["outcome_index"], 0);
        assert_eq!(meta0["outcome_label"], "Yes");

        // Sibling outcomes for index 0 is ["No"].
        let siblings = meta0["sibling_outcomes"].as_array().unwrap();
        assert_eq!(siblings.len(), 1);
        assert_eq!(siblings[0], "No");

        let meta1 = markets[1].meta.as_value();
        assert_eq!(meta1["outcome_index"], 1);
        assert_eq!(meta1["outcome_label"], "No");
        let siblings1 = meta1["sibling_outcomes"].as_array().unwrap();
        assert_eq!(siblings1[0], "Yes");
    }

    #[test]
    fn market_meta_includes_prices_and_volume() {
        let raw = make_binary_market();
        let markets = normalize_markets(raw).unwrap();

        let meta = markets[0].meta.as_value();
        assert_eq!(meta["volume"], "1000000.0");
        assert_eq!(meta["liquidity"], "500000.0");

        let prices = meta["outcome_prices"].as_array().unwrap();
        assert_eq!(prices.len(), 2);

        let token_ids = meta["token_ids"].as_array().unwrap();
        assert_eq!(token_ids.len(), 2);
    }
}

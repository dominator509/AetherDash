//! Polymarket CLOB WebSocket stream client.
//!
//! Provides WebSocket connectivity to the Polymarket CLOB API for
//! real-time order-book and price data.  The market channel requires
//! no authentication.
//!
//! # Reconnection
//!
//! Uses SPEC-006 full-jitter exponential backoff on disconnect.
//! Circuit breaker opens after 5 consecutive WebSocket errors.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__POLYMARKET_WS_URL` | Override WS URL (default: `wss://ws-subscriptions-clob.polymarket.com/ws/market`) |

use crate::normalize::{
    normalize_book, normalize_tick, ClobBookSnapshot, ClobLevel, ClobTick, NormalizeError,
};
use aether_bus::envelope::Envelope;
use aether_bus::producer::MessageProducer;
use aether_bus::producer::ProducerError;
use aether_bus::quarantine::{ObjectStore, Quarantine, QuarantineError};
use aether_bus::retry::{CircuitBreaker, RetryPolicy};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::tungstenite::Error as WsError;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default Polymarket CLOB WebSocket URL (market channel, no auth).
const DEFAULT_WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

/// Base delay for exponential backoff (milliseconds).
const BACKOFF_BASE_MS: u64 = 200;

/// Maximum delay cap for exponential backoff (milliseconds).
const BACKOFF_CAP_MS: u64 = 30_000;

/// Interval for sending WebSocket keep-alive PING messages (seconds).
const PING_INTERVAL_SECS: u64 = 30;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the Polymarket CLOB WebSocket client.
#[derive(Error, Debug)]
pub enum StreamError {
    /// WebSocket connection failed.
    #[error("WebSocket connection failed: {0}")]
    Connection(String),

    /// WebSocket protocol error.
    #[error("WebSocket protocol error: {0}")]
    Protocol(#[from] WsError),

    /// Failed to serialize/deserialize a message.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Normalization pipeline error.
    #[error("normalization error: {0}")]
    Normalize(#[from] NormalizeError),

    /// Message producer error.
    #[error("producer error: {0}")]
    Producer(#[from] ProducerError),

    /// Malformed payload could not be preserved and published to quarantine.
    #[error("quarantine error: {0}")]
    Quarantine(#[from] QuarantineError),
}

// ---------------------------------------------------------------------------
// WS message types
// ---------------------------------------------------------------------------

/// A raw message from the Polymarket CLOB WebSocket API.
///
/// All fields are optional — the presence of each depends on the
/// `event_type`.  Messages are flat (not wrapped in a nested `data`
/// envelope).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketWsMessage {
    /// Event type discriminator: `"book"`, `"price_change"`,
    /// `"last_trade_price"`, `"tick_size_change"`.
    #[serde(rename = "event_type", default)]
    pub event_type: String,

    /// Asset / token identifier (hex string) for the affected market.
    #[serde(default)]
    pub asset_id: Option<String>,

    /// Market contract address (hex).
    #[serde(default)]
    pub market: Option<String>,

    /// Price as a decimal string (present on `price_change`,
    /// `last_trade_price`).
    #[serde(default)]
    pub price: Option<String>,

    /// Size as a decimal string (present on book levels).
    #[serde(default)]
    pub size: Option<String>,

    /// Side: `"BUY"` or `"SELL"`.
    #[serde(default)]
    pub side: Option<String>,

    /// Current best bid on `best_bid_ask` events.
    #[serde(default)]
    pub best_bid: Option<String>,

    /// Current best ask on `best_bid_ask` events.
    #[serde(default)]
    pub best_ask: Option<String>,

    /// ISO 8601 timestamp of the event.
    #[serde(default)]
    pub timestamp: Option<String>,

    /// Full bid levels (present on `book` snapshots/diffs).
    #[serde(default)]
    pub bids: Option<Vec<ClobLevel>>,

    /// Full ask levels (present on `book` snapshots/diffs).
    #[serde(default)]
    pub asks: Option<Vec<ClobLevel>>,

    /// Snapshot hash for integrity verification.
    #[serde(default)]
    pub hash: Option<String>,

    /// Price-level changes on the current documented `price_change` shape.
    #[serde(default)]
    pub price_changes: Vec<PriceChange>,
}

/// One asset-specific entry nested inside a `price_change` event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceChange {
    pub asset_id: String,
    pub price: String,
    pub size: String,
    pub side: String,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub best_bid: Option<String>,
    #[serde(default)]
    pub best_ask: Option<String>,
}

/// Subscription request sent to the Polymarket CLOB WebSocket.
///
/// The market channel requires no authentication — just the asset IDs
/// and the channel type.
#[derive(Debug, Serialize)]
struct SubscribeRequest {
    /// List of asset / token identifiers to subscribe to.
    #[serde(rename = "assets_ids")]
    pub asset_ids: Vec<String>,
    /// Channel type — must be lowercase `"market"`.
    #[serde(rename = "type")]
    pub channel_type: String,
    /// Enables best-price and market lifecycle events.
    pub custom_feature_enabled: bool,
}

// ---------------------------------------------------------------------------
// SPEC-006 full-jitter backoff
// ---------------------------------------------------------------------------

/// Compute a sleep duration using SPEC-006 full-jitter exponential backoff.
///
/// Formula: `sleep = random(0, min(cap, base * 2^attempt))`
fn full_jitter_backoff(attempt: u32, base_ms: u64, cap_ms: u64) -> Duration {
    RetryPolicy {
        max_attempts: 5,
        base_delay: Duration::from_millis(base_ms),
        max_delay: Duration::from_millis(cap_ms),
    }
    .delay_for_attempt(attempt)
}

// ---------------------------------------------------------------------------
// PolymarketStream
// ---------------------------------------------------------------------------

/// An unauthenticated WebSocket client for Polymarket CLOB real-time
/// market data.
#[derive(Debug)]
pub struct PolymarketStream {
    /// WebSocket URL.
    ws_url: String,
}

impl PolymarketStream {
    /// Create a new stream client with the given WS URL.
    pub fn new(ws_url: impl Into<String>) -> Self {
        Self { ws_url: ws_url.into() }
    }

    /// Create a new stream client, loading WS URL from
    /// `AETHER_VENUE__POLYMARKET_WS_URL` (falls back to the default).
    pub fn from_env() -> Self {
        let ws_url = std::env::var("AETHER_VENUE__POLYMARKET_WS_URL")
            .unwrap_or_else(|_| DEFAULT_WS_URL.to_string());
        Self::new(ws_url)
    }

    // -----------------------------------------------------------------------
    // Connection
    // -----------------------------------------------------------------------

    /// Connect to the Polymarket CLOB WebSocket.
    ///
    /// The market channel is public; no authentication headers are sent.
    pub async fn connect(
        &self,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        StreamError,
    > {
        let request = self
            .ws_url
            .clone()
            .into_client_request()
            .map_err(|e| StreamError::Connection(e.to_string()))?;

        debug!(url = %self.ws_url, "connecting to Polymarket CLOB WebSocket");

        let (ws_stream, _) =
            connect_async(request).await.map_err(|e| StreamError::Connection(e.to_string()))?;

        info!("Polymarket CLOB WebSocket connected");
        Ok(ws_stream)
    }

    // -----------------------------------------------------------------------
    // Subscriptions
    // -----------------------------------------------------------------------

    /// Subscribe to the lowercase `market` channel for the given asset IDs.
    ///
    /// Sends a JSON subscribe message over the WebSocket.
    pub async fn subscribe(
        &self,
        ws_sink: &mut (impl futures::Sink<Message, Error = WsError> + Unpin),
        asset_ids: &[String],
    ) -> Result<(), StreamError> {
        let req = SubscribeRequest {
            asset_ids: asset_ids.to_vec(),
            channel_type: "market".into(),
            custom_feature_enabled: true,
        };
        let json =
            serde_json::to_string(&req).map_err(|e| StreamError::Serialization(e.to_string()))?;

        debug!(json = %json, "subscribing to market channel");
        ws_sink.send(Message::Text(json)).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Continuous streaming — ticks
    // -----------------------------------------------------------------------

    /// Stream normalized `Quote`s to the `md.ticks.polymarket` bus topic.
    ///
    /// This method runs an infinite loop with reconnection:
    /// 1. Connect to the Polymarket CLOB WebSocket
    /// 2. Subscribe to the `MARKET` channel for the given asset IDs
    /// 3. Read messages; on `price_change` / `last_trade_price` events,
    ///    normalize to `Quote` and send to the bus
    /// 4. On disconnect, back off and reconnect
    /// 5. On >5 consecutive errors, open the circuit breaker and return
    pub async fn stream_ticks_to_bus<Q: MessageProducer, S: ObjectStore>(
        &self,
        tickers: &[String],
        producer: &impl MessageProducer,
        quarantine_producer: &Q,
        quarantine_storage: &S,
    ) -> Result<(), StreamError> {
        let topic = "md.ticks.polymarket";
        let mut attempt: u32 = 0;
        let mut breaker = CircuitBreaker::new();

        loop {
            if !breaker.allow_request() {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            if attempt > 0 {
                let delay = full_jitter_backoff(attempt, BACKOFF_BASE_MS, BACKOFF_CAP_MS);
                info!(attempt, delay_ms = delay.as_millis(), "reconnecting to Polymarket WS");
                tokio::time::sleep(delay).await;
            }

            match self.connect().await {
                Ok(ws) => {
                    attempt = 0;

                    let (mut ws_sink, mut ws_stream) = ws.split();

                    if let Err(e) = self.subscribe(&mut ws_sink, tickers).await {
                        warn!(error = %e, "subscribe failed, will reconnect");
                        breaker.record_failure();
                        continue;
                    }

                    // Keep-alive ping sender
                    let mut ping_interval =
                        tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));

                    loop {
                        tokio::select! {
                            _ = ping_interval.tick() => {
                                if let Err(e) = ws_sink.send(Message::Text("PING".into())).await {
                                    warn!(error = %e, "keep-alive PING send failed");
                                    break;
                                }
                            }
                            msg = ws_stream.next() => {
                                match msg {
                                    Some(Ok(frame)) => {
                                        match frame {
                                            Message::Text(text) => {
                                                if text.trim().eq_ignore_ascii_case("PONG") {
                                                    continue;
                                                }
                                                let ws_messages = match parse_ws_messages(&text) {
                                                    Ok(messages) => messages,
                                                    Err(e) => {
                                                        quarantine_payload(
                                                            quarantine_producer,
                                                            quarantine_storage,
                                                            "polymarket",
                                                            &format!("invalid WebSocket JSON: {e}"),
                                                            text.as_bytes(),
                                                        ).await?;
                                                        warn!(error = %e, "invalid Polymarket WS message");
                                                        continue;
                                                    }
                                                };
                                                breaker.record_success();

                                                for ws_msg in ws_messages {
                                                    match ws_msg.event_type.as_str() {
                                                        "price_change" | "last_trade_price" | "best_bid_ask" => {
                                                            let ticks = match ticks_from_ws(&ws_msg) {
                                                                Ok(ticks) => ticks,
                                                                Err(e) => {
                                                                    quarantine_payload(
                                                                        quarantine_producer,
                                                                        quarantine_storage,
                                                                        "polymarket",
                                                                        &e.to_string(),
                                                                        text.as_bytes(),
                                                                    ).await?;
                                                                    continue;
                                                                }
                                                            };
                                                            for tick in ticks {
                                                                match normalize_tick(tick) {
                                                            Ok(quote) => {
                                                                let envelope = Envelope::new("quote", &quote);
                                                                if let Err(e) = producer.send(
                                                                    topic,
                                                                    envelope,
                                                                    Some(quote.market.as_str()),
                                                                ).await {
                                                                    return Err(StreamError::Producer(e));
                                                                }
                                                            }
                                                            Err(e) => {
                                                                quarantine_payload(
                                                                    quarantine_producer,
                                                                    quarantine_storage,
                                                                    "polymarket",
                                                                    &e.to_string(),
                                                                    text.as_bytes(),
                                                                ).await?;
                                                                warn!(error = %e, "tick normalization failed");
                                                            }
                                                                }
                                                            }
                                                        }
                                                        _ => {
                                                        // Ignore book, tick_size_change, etc.
                                                        }
                                                    }
                                                }
                                            }
                                            Message::Ping(payload) => {
                                                // Respond to server ping with pong
                                                if let Err(e) = ws_sink.send(Message::Pong(payload)).await {
                                                    warn!(error = %e, "failed to send Pong");
                                                    break;
                                                }
                                            }
                                            Message::Close(_) => {
                                                info!("WS connection closed by server");
                                                break;
                                            }
                                            _ => continue,
                                        }
                                    }
                                    Some(Err(e)) => {
                                        warn!(error = %e, "WS stream error");
                                        break;
                                    }
                                    None => {
                                        info!("WS stream ended");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "connection attempt failed");
                    attempt += 1;
                    breaker.record_failure();
                }
            }

            attempt += 1;
        }
    }

    // -----------------------------------------------------------------------
    // Continuous streaming — books
    // -----------------------------------------------------------------------

    /// Stream normalized `OrderBook`s to the `md.books.polymarket` bus topic.
    ///
    /// Same reconnection and breaker logic as [`stream_ticks_to_bus`].
    ///
    /// Maintains an in-memory `HashMap<String, ClobBookSnapshot>` of the
    /// latest book state per asset ID, updated from incoming `book` messages.
    pub async fn stream_books_to_bus<Q: MessageProducer, S: ObjectStore>(
        &self,
        tickers: &[String],
        producer: &impl MessageProducer,
        quarantine_producer: &Q,
        quarantine_storage: &S,
    ) -> Result<(), StreamError> {
        let topic = "md.books.polymarket";
        let mut attempt: u32 = 0;
        let mut breaker = CircuitBreaker::new();
        let mut books: HashMap<String, ClobBookSnapshot> = HashMap::new();

        loop {
            if !breaker.allow_request() {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            if attempt > 0 {
                let delay = full_jitter_backoff(attempt, BACKOFF_BASE_MS, BACKOFF_CAP_MS);
                info!(attempt, delay_ms = delay.as_millis(), "reconnecting to Polymarket WS");
                tokio::time::sleep(delay).await;
            }

            match self.connect().await {
                Ok(ws) => {
                    attempt = 0;
                    books.clear();

                    let (mut ws_sink, mut ws_stream) = ws.split();

                    if let Err(e) = self.subscribe(&mut ws_sink, tickers).await {
                        warn!(error = %e, "subscribe failed, will reconnect");
                        breaker.record_failure();
                        continue;
                    }

                    let mut ping_interval =
                        tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));

                    loop {
                        tokio::select! {
                            _ = ping_interval.tick() => {
                                if let Err(e) = ws_sink.send(Message::Text("PING".into())).await {
                                    warn!(error = %e, "keep-alive PING send failed");
                                    break;
                                }
                            }
                            msg = ws_stream.next() => {
                                match msg {
                                    Some(Ok(frame)) => {
                                        match frame {
                                            Message::Text(text) => {
                                                if text.trim().eq_ignore_ascii_case("PONG") {
                                                    continue;
                                                }
                                                let ws_messages = match parse_ws_messages(&text) {
                                                    Ok(messages) => messages,
                                                    Err(e) => {
                                                        quarantine_payload(
                                                            quarantine_producer,
                                                            quarantine_storage,
                                                            "polymarket",
                                                            &format!("invalid WebSocket JSON: {e}"),
                                                            text.as_bytes(),
                                                        ).await?;
                                                        warn!(error = %e, "invalid Polymarket WS message");
                                                        continue;
                                                    }
                                                };
                                                breaker.record_success();

                                                for ws_msg in ws_messages {
                                                    match ws_msg.event_type.as_str() {
                                                    "book" => {
                                                        let asset_id = match ws_msg.asset_id.clone() {
                                                            Some(id) => id,
                                                            None => {
                                                                quarantine_payload(
                                                                    quarantine_producer,
                                                                    quarantine_storage,
                                                                    "polymarket",
                                                                    "book message missing asset_id",
                                                                    text.as_bytes(),
                                                                ).await?;
                                                                warn!("book message missing asset_id; payload quarantined");
                                                                continue;
                                                            }
                                                        };

                                                        let asset_id_clone = asset_id.clone();

                                                        // Build or update the local book snapshot
                                                        let snapshot = ClobBookSnapshot {
                                                            market: ws_msg.market.clone().unwrap_or_default(),
                                                            asset_id,
                                                            timestamp: ws_msg.timestamp.clone().unwrap_or_default(),
                                                            bids: ws_msg.bids.unwrap_or_default(),
                                                            asks: ws_msg.asks.unwrap_or_default(),
                                                            hash: ws_msg.hash.clone(),
                                                        };

                                                        books.insert(asset_id_clone.clone(), snapshot);

                                                        // Retrieve the stored snapshot and normalize
                                                        if let Some(stored) = books.get(&asset_id_clone) {
                                                            match normalize_book(stored.clone()) {
                                                                Ok(book) => {
                                                                    let envelope = Envelope::new("order_book", &book);
                                                                    if let Err(e) = producer.send(
                                                                        topic,
                                                                        envelope,
                                                                        Some(book.market.as_str()),
                                                                    ).await {
                                                                        return Err(StreamError::Producer(e));
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    quarantine_payload(
                                                                        quarantine_producer,
                                                                        quarantine_storage,
                                                                        "polymarket",
                                                                        &format!("book normalization: {e}"),
                                                                        text.as_bytes(),
                                                                    ).await?;
                                                                    warn!(error = %e, "book normalization failed");
                                                                }
                                                            }
                                                        }
                                                    }
                                                    "price_change" => {
                                                        for change in &ws_msg.price_changes {
                                                            let Some(snapshot) = books.get_mut(&change.asset_id) else {
                                                                warn!(asset_id = %change.asset_id, "price change arrived before initial book snapshot");
                                                                continue;
                                                            };
                                                            if let Err(e) = apply_price_change(
                                                                snapshot,
                                                                change,
                                                                ws_msg.timestamp.as_deref(),
                                                                ws_msg.market.as_deref(),
                                                            ) {
                                                                quarantine_payload(
                                                                    quarantine_producer,
                                                                    quarantine_storage,
                                                                    "polymarket",
                                                                    &format!("book delta: {e}"),
                                                                    text.as_bytes(),
                                                                ).await?;
                                                                continue;
                                                            }
                                                            match normalize_book(snapshot.clone()) {
                                                                Ok(book) => {
                                                                    producer.send(
                                                                        topic,
                                                                        Envelope::new("order_book", &book),
                                                                        Some(book.market.as_str()),
                                                                    ).await?;
                                                                }
                                                                Err(e) => {
                                                                    quarantine_payload(
                                                                        quarantine_producer,
                                                                        quarantine_storage,
                                                                        "polymarket",
                                                                        &format!("book delta normalization: {e}"),
                                                                        text.as_bytes(),
                                                                    ).await?;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    _ => {
                                                        // Ignore price_change, tick_size_change, etc.
                                                        continue;
                                                    }
                                                }
                                                }
                                            }
                                            Message::Ping(payload) => {
                                                if let Err(e) = ws_sink.send(Message::Pong(payload)).await {
                                                    warn!(error = %e, "failed to send Pong");
                                                    break;
                                                }
                                            }
                                            Message::Close(_) => {
                                                info!("WS connection closed by server");
                                                // Clear book state on disconnect so we get fresh snapshots
                                                books.clear();
                                                break;
                                            }
                                            _ => continue,
                                        }
                                    }
                                    Some(Err(e)) => {
                                        warn!(error = %e, "WS stream error");
                                        break;
                                    }
                                    None => {
                                        info!("WS stream ended");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "connection attempt failed");
                    attempt += 1;
                    breaker.record_failure();
                }
            }

            attempt += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers for the documented object-or-array WebSocket frame shapes
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(untagged)]
enum WsFrame {
    One(Box<PolymarketWsMessage>),
    Many(Vec<PolymarketWsMessage>),
}

pub(crate) fn parse_ws_messages(text: &str) -> Result<Vec<PolymarketWsMessage>, serde_json::Error> {
    match serde_json::from_str::<WsFrame>(text)? {
        WsFrame::One(message) => Ok(vec![*message]),
        WsFrame::Many(messages) => Ok(messages),
    }
}

pub(crate) fn ticks_from_ws(msg: &PolymarketWsMessage) -> Result<Vec<ClobTick>, NormalizeError> {
    if msg.event_type == "price_change" {
        if msg.price_changes.is_empty() {
            return Err(NormalizeError::MissingField("price_changes".into()));
        }
        return Ok(msg
            .price_changes
            .iter()
            .map(|change| ClobTick {
                event_type: msg.event_type.clone(),
                asset_id: change.asset_id.clone(),
                price: change.price.clone(),
                market: msg.market.clone().unwrap_or_default(),
                size: change.size.clone(),
                side: Some(change.side.clone()),
                best_bid: change.best_bid.clone(),
                best_ask: change.best_ask.clone(),
                timestamp: msg.timestamp.clone(),
            })
            .collect());
    }

    Ok(vec![ClobTick {
        event_type: msg.event_type.clone(),
        asset_id: msg.asset_id.clone().unwrap_or_default(),
        price: msg.price.clone().unwrap_or_default(),
        market: msg.market.clone().unwrap_or_default(),
        size: msg.size.clone().unwrap_or_default(),
        side: msg.side.clone(),
        best_bid: msg.best_bid.clone(),
        best_ask: msg.best_ask.clone(),
        timestamp: msg.timestamp.clone(),
    }])
}

fn apply_price_change(
    snapshot: &mut ClobBookSnapshot,
    change: &PriceChange,
    timestamp: Option<&str>,
    market: Option<&str>,
) -> Result<(), NormalizeError> {
    let price = change.price.parse::<rust_decimal::Decimal>().map_err(|error| {
        NormalizeError::DecimalParse { raw: change.price.clone(), detail: error.to_string() }
    })?;
    let size = change.size.parse::<rust_decimal::Decimal>().map_err(|error| {
        NormalizeError::DecimalParse { raw: change.size.clone(), detail: error.to_string() }
    })?;
    if size < rust_decimal::Decimal::ZERO {
        return Err(NormalizeError::DecimalOutOfRange { field: "size", raw: change.size.clone() });
    }

    let levels = match change.side.as_str() {
        "BUY" => &mut snapshot.bids,
        "SELL" => &mut snapshot.asks,
        _ => return Err(NormalizeError::MissingField("price_change.side".into())),
    };
    let existing = levels
        .iter()
        .position(|level| level.price.parse::<rust_decimal::Decimal>().ok() == Some(price));
    if size == rust_decimal::Decimal::ZERO {
        if let Some(index) = existing {
            levels.remove(index);
        }
    } else if let Some(index) = existing {
        levels[index].size.clone_from(&change.size);
    } else {
        levels.push(ClobLevel { price: change.price.clone(), size: change.size.clone() });
    }

    if let Some(timestamp) = timestamp {
        snapshot.timestamp = timestamp.to_string();
    }
    if let Some(market) = market {
        snapshot.market = market.to_string();
    }
    snapshot.hash.clone_from(&change.hash);
    Ok(())
}

// ---------------------------------------------------------------------------
// Quarantine helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn message_bytes(message: &Message) -> &[u8] {
    match message {
        Message::Text(text) => text.as_bytes(),
        Message::Binary(bytes) => bytes,
        _ => &[],
    }
}

async fn quarantine_payload<Q: MessageProducer, S: ObjectStore>(
    producer: &Q,
    storage: &S,
    venue: &str,
    reason: &str,
    raw_payload: &[u8],
) -> Result<String, QuarantineError> {
    Quarantine::publish(producer, storage, venue, reason, raw_payload).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aether_bus::producer::StubProducer;
    use aether_bus::quarantine::StubObjectStore;

    // ── Subscribe message format ──

    #[test]
    fn subscribe_message_format() {
        let req = SubscribeRequest {
            asset_ids: vec!["0xtoken1".into(), "0xtoken2".into()],
            channel_type: "market".into(),
            custom_feature_enabled: true,
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["assets_ids"][0], "0xtoken1");
        assert_eq!(parsed["assets_ids"][1], "0xtoken2");
        assert_eq!(parsed["type"], "market");
        assert_eq!(parsed["custom_feature_enabled"], true);
    }

    // ── WS message deserialization ──

    #[test]
    fn parse_book_message() {
        let json = r#"{
            "event_type": "book",
            "asset_id": "0x12345",
            "market": "0xabc",
            "timestamp": "2026-07-10T12:34:56.789Z",
            "bids": [
                {"price": "0.65", "size": "100.5"},
                {"price": "0.64", "size": "200.0"}
            ],
            "asks": [
                {"price": "0.67", "size": "50.2"}
            ],
            "hash": "0xdead"
        }"#;

        let msg: PolymarketWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.event_type, "book");
        assert_eq!(msg.asset_id.as_deref(), Some("0x12345"));
        assert_eq!(msg.market.as_deref(), Some("0xabc"));
        assert!(msg.price.is_none());
        assert!(msg.side.is_none());

        let bids = msg.bids.unwrap();
        assert_eq!(bids.len(), 2);
        assert_eq!(bids[0].price, "0.65");
        assert_eq!(bids[0].size, "100.5");
        assert_eq!(bids[1].price, "0.64");

        let asks = msg.asks.unwrap();
        assert_eq!(asks.len(), 1);
        assert_eq!(asks[0].price, "0.67");

        assert_eq!(msg.hash.as_deref(), Some("0xdead"));
    }

    #[test]
    fn parse_price_change_message() {
        let json = r#"{
            "event_type": "price_change",
            "market": "0xabc",
            "price_changes": [{
                "asset_id": "12345",
                "price": "0.65",
                "side": "BUY",
                "size": "10.0",
                "hash": "0xdef",
                "best_bid": "0.65",
                "best_ask": "0.67"
            }],
            "timestamp": "1752150896789"
        }"#;

        let msg: PolymarketWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.event_type, "price_change");
        assert_eq!(msg.price_changes.len(), 1);
        assert_eq!(msg.price_changes[0].asset_id, "12345");
        assert_eq!(msg.price_changes[0].best_bid.as_deref(), Some("0.65"));
        let ticks = ticks_from_ws(&msg).unwrap();
        let quote = normalize_tick(ticks.into_iter().next().unwrap()).unwrap();
        assert_eq!(quote.bid, Some("0.65".parse().unwrap()));
        assert_eq!(quote.ask, Some("0.67".parse().unwrap()));
        assert!(quote.last.is_none());
        assert!(msg.bids.is_none());
        assert!(msg.asks.is_none());
    }

    #[test]
    fn parse_initial_book_array() {
        let frames = parse_ws_messages(
            r#"[{"event_type":"book","asset_id":"123","market":"0xabc","timestamp":"1752150896789","bids":[],"asks":[]}]"#,
        )
        .unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].asset_id.as_deref(), Some("123"));
    }

    #[test]
    fn parse_last_trade_price_message() {
        let json = r#"{
            "event_type": "last_trade_price",
            "asset_id": "0x12345",
            "market": "0xabc",
            "price": "0.63",
            "timestamp": "2026-07-10T12:35:00.000Z"
        }"#;

        let msg: PolymarketWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.event_type, "last_trade_price");
        assert_eq!(msg.price.as_deref(), Some("0.63"));
        assert!(msg.side.is_none());
    }

    #[test]
    fn parse_tick_size_change_message() {
        let json = r#"{
            "event_type": "tick_size_change",
            "asset_id": "0x12345",
            "price": "0.01"
        }"#;

        let msg: PolymarketWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.event_type, "tick_size_change");
        assert_eq!(msg.asset_id.as_deref(), Some("0x12345"));
    }

    #[test]
    fn parse_message_with_minimal_fields() {
        let json = r#"{"event_type": "book", "asset_id": "0x1"}"#;
        let msg: PolymarketWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.event_type, "book");
        assert_eq!(msg.asset_id.as_deref(), Some("0x1"));
        assert!(msg.market.is_none());
        assert!(msg.price.is_none());
        assert!(msg.bids.is_none());
        assert!(msg.asks.is_none());
    }

    #[test]
    fn parse_message_unknown_event_type() {
        let json = r#"{"event_type": "unknown_event"}"#;
        let msg: PolymarketWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.event_type, "unknown_event");
    }

    #[test]
    fn parse_message_empty_json() {
        let json = r#"{}"#;
        let msg: PolymarketWsMessage = serde_json::from_str(json).unwrap();
        assert!(msg.event_type.is_empty());
        assert!(msg.asset_id.is_none());
    }

    // ── Backoff ──

    #[test]
    fn full_jitter_backoff_bounds() {
        let d = full_jitter_backoff(0, 200, 30_000);
        assert!(d.as_millis() <= 200);

        let d = full_jitter_backoff(20, 200, 30_000);
        assert!(d.as_millis() <= 30_000);
    }

    // ── Quarantine ──

    #[tokio::test]
    async fn malformed_frame_is_preserved_and_published_to_quarantine() {
        let producer = StubProducer::new();
        let storage = StubObjectStore::new();
        let raw = br#"{"event_type":"book","bids":"INVALID"}"#;

        let hash =
            quarantine_payload(&producer, &storage, "polymarket", "malformed JSON for book", raw)
                .await
                .unwrap();

        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "quarantine.polymarket");
        drop(sent);
        let stored = storage.objects.lock().unwrap();
        assert_eq!(stored.get(&format!("quarantine/polymarket/{hash}")), Some(&raw.to_vec()));
    }
}

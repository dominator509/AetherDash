//! Kalshi WebSocket stream client.
//!
//! Provides authenticated WebSocket connectivity to the Kalshi trade API for
//! real-time market data (ticks and order-book snapshots).
//!
//! # Reconnection
//!
//! Uses SPEC-006 full-jitter exponential backoff on disconnect. A simple
//! circuit breaker opens after 5 consecutive WebSocket errors and prevents
//! further connection attempts until reset.
//!
//! # Gap detection
//!
//! The Kalshi WS protocol includes an optional `seq` field on messages.
//! A gap (seq > expected + 1) triggers a resubscribe and a warning log.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__KALSHI_WS_URL` | Override WS URL (default: demo) |
//! | `AETHER_VENUE__KALSHI_KEY_ID` | API key ID (see [`auth`](crate::auth)) |
//! | `AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH` | PEM key path (see [`auth`](crate::auth)) |

use crate::auth::KalshiAuth;
#[cfg(test)]
use crate::normalize::normalize_book;
use crate::normalize::{
    normalize_tick, KalshiBookDelta, KalshiBookSnapshot, KalshiBookState, KalshiTick,
};
use aether_bus::envelope::Envelope;
use aether_bus::producer::MessageProducer;
use aether_bus::producer::ProducerError;
use aether_bus::quarantine::{ObjectStore, Quarantine, QuarantineError};
use futures::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::tungstenite::Error as WsError;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum consecutive WebSocket errors before the circuit breaker opens.
const BREAKER_THRESHOLD: u32 = 5;

/// Base delay for exponential backoff (milliseconds).
const BACKOFF_BASE_MS: u64 = 200;

/// Maximum delay cap for exponential backoff (milliseconds).
const BACKOFF_CAP_MS: u64 = 30_000;

/// Default Kalshi demo WebSocket URL. Order-capable connectors fail safe to sandbox.
const DEFAULT_WS_URL: &str = "wss://external-api.demo.kalshi.co/trade-api/ws/v2";

/// Kalshi WebSocket channel name for ticker data.
const CHANNEL_TICKER: &str = "ticker";

/// Kalshi WebSocket channel name for order-book data.
const CHANNEL_BOOK: &str = "orderbook_delta";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the Kalshi WebSocket client.
#[derive(Error, Debug)]
pub enum StreamError {
    /// Authentication setup failed.
    #[error("auth error: {0}")]
    Auth(String),

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
    Normalize(#[from] crate::normalize::NormalizeError),

    /// Message producer error.
    #[error("producer error: {0}")]
    Producer(#[from] ProducerError),

    /// Malformed payload could not be preserved and published to quarantine.
    #[error("quarantine error: {0}")]
    Quarantine(#[from] QuarantineError),

    /// Circuit breaker tripped — too many consecutive failures.
    #[error("circuit breaker open after {threshold} consecutive failures")]
    BreakerOpen {
        /// The breaker threshold that was exceeded.
        threshold: u32,
    },

    /// Gap detected in WebSocket sequence numbers.
    #[error("gap detected: expected seq {expected}, got {actual}")]
    GapDetected {
        /// The expected sequence number.
        expected: u64,
        /// The actual sequence number received.
        actual: u64,
    },
}

// ---------------------------------------------------------------------------
// WS message types
// ---------------------------------------------------------------------------

/// A raw message from the Kalshi WebSocket API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KalshiWsMessage {
    /// Message type: `"ticker"`, `"orderbook_snapshot"`, etc.
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Optional sequence number for gap detection.
    #[serde(default)]
    pub seq: Option<u64>,
    /// Subscription ID assigned by Kalshi.
    #[serde(default)]
    pub sid: Option<u64>,
    /// The message payload (type-specific).
    #[serde(rename = "msg", alias = "data")]
    pub data: serde_json::Value,
}

/// Subscription request sent to the Kalshi WebSocket.
#[derive(Debug, Serialize)]
struct SubscribeRequest {
    id: u64,
    cmd: String,
    params: SubscribeParams,
}

#[derive(Debug, Serialize)]
struct SubscribeParams {
    channels: Vec<String>,
    market_tickers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_yes_price: Option<bool>,
}

// ---------------------------------------------------------------------------
// Circuit breaker
// ---------------------------------------------------------------------------

/// A simple failure-count circuit breaker for WebSocket connections.
///
/// Opens after `threshold` consecutive failures. Once open, all operations
/// return `BreakerOpen` until the breaker is reset.
#[derive(Debug)]
pub struct StreamBreaker {
    threshold: u32,
    failures: u32,
    open: bool,
}

impl StreamBreaker {
    /// Create a new breaker with the given failure threshold.
    pub fn new(threshold: u32) -> Self {
        Self { threshold, failures: 0, open: false }
    }

    /// Record a failure. Returns `BreakerOpen` if the threshold is reached.
    #[allow(clippy::result_large_err)]
    pub fn record_failure(&mut self) -> Result<(), StreamError> {
        self.failures += 1;
        if self.failures >= self.threshold {
            self.open = true;
            return Err(StreamError::BreakerOpen { threshold: self.threshold });
        }
        Ok(())
    }

    /// Record a success — resets the failure count.
    pub fn record_success(&mut self) {
        self.failures = 0;
    }

    /// Returns true if the breaker is open (too many failures).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Reset the breaker to closed state.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.failures = 0;
        self.open = false;
    }
}

// ---------------------------------------------------------------------------
// SPEC-006 full-jitter backoff
// ---------------------------------------------------------------------------

/// Compute a sleep duration using SPEC-006 full-jitter exponential backoff.
///
/// Formula: `sleep = random(0, min(cap, base * 2^attempt))`
fn full_jitter_backoff(attempt: u32, base_ms: u64, cap_ms: u64) -> Duration {
    let exp = base_ms.saturating_mul(2u64.saturating_pow(attempt.min(30)));
    let max = cap_ms.min(exp);
    // Deterministic pseudo-random jitter using DefaultHasher
    let mut hasher = DefaultHasher::new();
    attempt.hash(&mut hasher);
    let hash = hasher.finish();
    let sleep = hash % (max + 1);
    Duration::from_millis(sleep)
}

// ---------------------------------------------------------------------------
// Sequence number tracker for gap detection
// ---------------------------------------------------------------------------

/// Tracks the last-seen WebSocket sequence number and detects gaps.
#[derive(Debug, Default)]
pub struct SeqTracker {
    last: Option<u64>,
}

impl SeqTracker {
    /// Create a new tracker with no previous sequence number.
    pub fn new() -> Self {
        Self { last: None }
    }

    /// Observe a sequence number. Returns `None` if no gap (first observation
    /// or contiguous). Returns `Some((expected, actual))` if a gap is detected.
    pub fn observe(&mut self, seq: u64) -> Option<(u64, u64)> {
        let gap = match self.last {
            Some(last) if seq > last + 1 => Some((last + 1, seq)),
            _ => None,
        };
        self.last = Some(seq);
        gap
    }

    /// Reset the tracker (e.g., after resubscribe).
    pub fn reset(&mut self) {
        self.last = None;
    }
}

// ---------------------------------------------------------------------------
// KalshiStream
// ---------------------------------------------------------------------------

/// An authenticated WebSocket client for Kalshi real-time market data.
#[derive(Debug)]
pub struct KalshiStream {
    /// WebSocket URL (prod or demo).
    ws_url: String,
    /// Authentication handle.
    auth: KalshiAuth,
    next_command_id: AtomicU64,
}

impl KalshiStream {
    /// Create a new stream client with the given WS URL and auth.
    pub fn new(ws_url: impl Into<String>, auth: KalshiAuth) -> Self {
        Self { ws_url: ws_url.into(), auth, next_command_id: AtomicU64::new(1) }
    }

    /// Create a new stream client, loading WS URL from
    /// `AETHER_VENUE__KALSHI_WS_URL` (falls back to the demo environment).
    pub fn from_env(auth: KalshiAuth) -> Self {
        let ws_url = std::env::var("AETHER_VENUE__KALSHI_WS_URL")
            .unwrap_or_else(|_| DEFAULT_WS_URL.to_string());
        Self::new(ws_url, auth)
    }

    // -----------------------------------------------------------------------
    // Connection
    // -----------------------------------------------------------------------

    /// Connect to the Kalshi WebSocket API with authentication.
    ///
    /// Authentication is supplied in Kalshi's three access headers.
    pub async fn connect(
        &self,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        StreamError,
    > {
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let signature = self
            .auth
            .sign_request("GET", "/trade-api/ws/v2", &timestamp)
            .map_err(|e| StreamError::Auth(e.to_string()))?;
        let mut request = self
            .ws_url
            .clone()
            .into_client_request()
            .map_err(|e| StreamError::Connection(e.to_string()))?;
        request.headers_mut().insert(
            "KALSHI-ACCESS-KEY",
            HeaderValue::from_str(self.auth.key_id())
                .map_err(|e| StreamError::Auth(e.to_string()))?,
        );
        request.headers_mut().insert(
            "KALSHI-ACCESS-SIGNATURE",
            HeaderValue::from_str(&signature).map_err(|e| StreamError::Auth(e.to_string()))?,
        );
        request.headers_mut().insert(
            "KALSHI-ACCESS-TIMESTAMP",
            HeaderValue::from_str(&timestamp).map_err(|e| StreamError::Auth(e.to_string()))?,
        );
        debug!(url = %self.ws_url, "connecting to Kalshi WebSocket");

        let (ws_stream, _) =
            connect_async(request).await.map_err(|e| StreamError::Connection(e.to_string()))?;

        info!("Kalshi WebSocket connected");
        Ok(ws_stream)
    }

    // -----------------------------------------------------------------------
    // Subscriptions
    // -----------------------------------------------------------------------

    /// Subscribe to the `ticker` channel for the given tickers.
    ///
    /// Sends a JSON subscribe message over the WebSocket.
    pub async fn subscribe_ticks(
        &self,
        ws_sink: &mut (impl futures::Sink<Message, Error = WsError> + Unpin),
        tickers: &[String],
    ) -> Result<(), StreamError> {
        let req = SubscribeRequest {
            id: self.next_command_id.fetch_add(1, Ordering::Relaxed),
            cmd: "subscribe".into(),
            params: SubscribeParams {
                channels: vec![CHANNEL_TICKER.to_string()],
                market_tickers: tickers.to_vec(),
                use_yes_price: None,
            },
        };
        let json =
            serde_json::to_string(&req).map_err(|e| StreamError::Serialization(e.to_string()))?;

        debug!(json = %json, "subscribing to ticker channel");
        ws_sink.send(Message::Text(json)).await?;
        Ok(())
    }

    /// Subscribe to the `book` channel for the given tickers.
    ///
    /// Sends a JSON subscribe message over the WebSocket.
    pub async fn subscribe_books(
        &self,
        ws_sink: &mut (impl futures::Sink<Message, Error = WsError> + Unpin),
        tickers: &[String],
    ) -> Result<(), StreamError> {
        let req = SubscribeRequest {
            id: self.next_command_id.fetch_add(1, Ordering::Relaxed),
            cmd: "subscribe".into(),
            params: SubscribeParams {
                channels: vec![CHANNEL_BOOK.to_string()],
                market_tickers: tickers.to_vec(),
                use_yes_price: Some(true),
            },
        };
        let json =
            serde_json::to_string(&req).map_err(|e| StreamError::Serialization(e.to_string()))?;

        debug!(json = %json, "subscribing to book channel");
        ws_sink.send(Message::Text(json)).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Continuous streaming
    // -----------------------------------------------------------------------

    /// Stream normalized `Quote`s to the `md.ticks.kalshi` bus topic.
    ///
    /// This method runs an infinite loop with reconnection:
    /// 1. Connect to the Kalshi WebSocket
    /// 2. Subscribe to the `ticker` channel for the given tickers
    /// 3. Read messages, normalize to `Quote`, send to bus
    /// 4. On disconnect, back off and reconnect
    /// 5. On >5 consecutive errors, open the circuit breaker and return
    pub async fn stream_ticks_to_bus<Q: MessageProducer, S: ObjectStore>(
        &self,
        tickers: &[String],
        producer: &impl MessageProducer,
        quarantine_producer: &Q,
        quarantine_storage: &S,
    ) -> Result<(), StreamError> {
        let topic = "md.ticks.kalshi";
        let mut attempt: u32 = 0;
        let mut breaker = StreamBreaker::new(BREAKER_THRESHOLD);
        let mut seq_tracker = SeqTracker::new();

        loop {
            // Check breaker
            if breaker.is_open() {
                return Err(StreamError::BreakerOpen { threshold: BREAKER_THRESHOLD });
            }

            // Backoff if reconnecting
            if attempt > 0 {
                let delay = full_jitter_backoff(attempt, BACKOFF_BASE_MS, BACKOFF_CAP_MS);
                info!(attempt, delay_ms = delay.as_millis(), "reconnecting to Kalshi WS");
                tokio::time::sleep(delay).await;
            }

            match self.connect().await {
                Ok(ws) => {
                    attempt = 0; // Reset backoff on successful connect
                    breaker.record_success();
                    seq_tracker.reset();

                    let (mut ws_sink, mut ws_stream) = ws.split();

                    // Subscribe
                    if let Err(e) = self.subscribe_ticks(&mut ws_sink, tickers).await {
                        warn!(error = %e, "subscribe failed, will reconnect");
                        let _ = breaker.record_failure();
                        continue;
                    }

                    // Message loop
                    loop {
                        match ws_stream.next().await {
                            Some(Ok(msg)) => {
                                match handle_ws_message::<KalshiTick>(
                                    &msg,
                                    &mut seq_tracker,
                                    "ticker",
                                ) {
                                    Ok(Some(tick)) => match normalize_tick(&tick) {
                                        Ok(quote) => {
                                            let envelope = Envelope::new("quote", &quote);
                                            if let Err(e) = producer
                                                .send(topic, envelope, Some(quote.market.as_str()))
                                                .await
                                            {
                                                return Err(StreamError::Producer(e));
                                            }
                                        }
                                        Err(e) => {
                                            let raw = message_bytes(&msg);
                                            quarantine_payload(
                                                quarantine_producer,
                                                quarantine_storage,
                                                "kalshi",
                                                &e.to_string(),
                                                raw,
                                            )
                                            .await?;
                                            warn!(error = %e, "tick normalization failed");
                                        }
                                    },
                                    Ok(None) => {
                                        // Message was not a tick update (e.g., subscription confirmation)
                                        continue;
                                    }
                                    Err(StreamError::GapDetected { expected, actual }) => {
                                        warn!(
                                            expected,
                                            actual,
                                            "seq gap detected in ticker stream, resubscribing"
                                        );
                                        // Resubscribe to recover from the gap
                                        if let Err(e) =
                                            self.subscribe_ticks(&mut ws_sink, tickers).await
                                        {
                                            warn!(error = %e, "resubscribe after gap failed");
                                            break; // Break inner loop to reconnect
                                        }
                                        seq_tracker.reset();
                                    }
                                    Err(e) => {
                                        if matches!(e, StreamError::Serialization(_)) {
                                            quarantine_payload(
                                                quarantine_producer,
                                                quarantine_storage,
                                                "kalshi",
                                                &e.to_string(),
                                                message_bytes(&msg),
                                            )
                                            .await?;
                                        }
                                        warn!(error = %e, "message handling error");
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                warn!(error = %e, "WS stream error");
                                break; // Break inner loop to reconnect
                            }
                            None => {
                                info!("WS stream ended");
                                break; // Break inner loop to reconnect
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "connection attempt failed");
                    attempt += 1;
                    if let Err(be) = breaker.record_failure() {
                        error!("circuit breaker opened: {be}");
                        return Err(be);
                    }
                }
            }

            attempt += 1;
        }
    }

    /// Stream normalized `OrderBook`s to the `md.books.kalshi` bus topic.
    ///
    /// Same reconnection and breaker logic as [`stream_ticks_to_bus`].
    pub async fn stream_books_to_bus<Q: MessageProducer, S: ObjectStore>(
        &self,
        tickers: &[String],
        producer: &impl MessageProducer,
        quarantine_producer: &Q,
        quarantine_storage: &S,
    ) -> Result<(), StreamError> {
        let topic = "md.books.kalshi";
        let mut attempt: u32 = 0;
        let mut breaker = StreamBreaker::new(BREAKER_THRESHOLD);
        let mut seq_tracker = SeqTracker::new();
        let mut books: HashMap<String, KalshiBookState> = HashMap::new();

        loop {
            if breaker.is_open() {
                return Err(StreamError::BreakerOpen { threshold: BREAKER_THRESHOLD });
            }

            if attempt > 0 {
                let delay = full_jitter_backoff(attempt, BACKOFF_BASE_MS, BACKOFF_CAP_MS);
                info!(attempt, delay_ms = delay.as_millis(), "reconnecting to Kalshi WS");
                tokio::time::sleep(delay).await;
            }

            match self.connect().await {
                Ok(ws) => {
                    attempt = 0;
                    breaker.record_success();
                    seq_tracker.reset();

                    let (mut ws_sink, mut ws_stream) = ws.split();

                    if let Err(e) = self.subscribe_books(&mut ws_sink, tickers).await {
                        warn!(error = %e, "subscribe failed, will reconnect");
                        let _ = breaker.record_failure();
                        continue;
                    }

                    loop {
                        match ws_stream.next().await {
                            Some(Ok(Message::Text(text))) => {
                                let ws_msg: KalshiWsMessage = match serde_json::from_str(&text) {
                                    Ok(value) => value,
                                    Err(error) => {
                                        quarantine_payload(
                                            quarantine_producer,
                                            quarantine_storage,
                                            "kalshi",
                                            &format!("invalid WebSocket JSON: {error}"),
                                            text.as_bytes(),
                                        )
                                        .await?;
                                        warn!(%error, "invalid Kalshi book message");
                                        continue;
                                    }
                                };
                                if let Some(seq) = ws_msg.seq {
                                    if let Some((expected, actual)) = seq_tracker.observe(seq) {
                                        warn!(expected, actual, "seq gap detected in book stream");
                                        books.clear();
                                        if self
                                            .subscribe_books(&mut ws_sink, tickers)
                                            .await
                                            .is_err()
                                        {
                                            break;
                                        }
                                        seq_tracker.reset();
                                        continue;
                                    }
                                }

                                let book = match ws_msg.msg_type.as_str() {
                                    "orderbook_snapshot" => {
                                        match serde_json::from_value::<KalshiBookSnapshot>(
                                            ws_msg.data,
                                        )
                                        .and_then(
                                            |snapshot| {
                                                let ticker = snapshot.ticker.clone();
                                                KalshiBookState::from_snapshot(
                                                    &snapshot, ws_msg.seq,
                                                )
                                                .map(|state| (ticker, state))
                                                .map_err(serde::de::Error::custom)
                                            },
                                        ) {
                                            Ok((ticker, state)) => {
                                                books.insert(ticker.clone(), state);
                                                books
                                                    .get(&ticker)
                                                    .map(KalshiBookState::to_order_book)
                                            }
                                            Err(error) => {
                                                quarantine_payload(
                                                    quarantine_producer,
                                                    quarantine_storage,
                                                    "kalshi",
                                                    &format!(
                                                        "book snapshot normalization: {error}"
                                                    ),
                                                    text.as_bytes(),
                                                )
                                                .await?;
                                                warn!(%error, "book snapshot normalization failed");
                                                None
                                            }
                                        }
                                    }
                                    "orderbook_delta" => {
                                        match serde_json::from_value::<KalshiBookDelta>(ws_msg.data)
                                        {
                                            Ok(delta) => {
                                                match books.get_mut(&delta.market_ticker) {
                                                    Some(state) => {
                                                        if let Err(error) =
                                                            state.apply_delta(&delta, ws_msg.seq)
                                                        {
                                                            quarantine_payload(
                                                            quarantine_producer,
                                                            quarantine_storage,
                                                            "kalshi",
                                                            &format!("book delta normalization: {error}"),
                                                            text.as_bytes(),
                                                        )
                                                        .await?;
                                                            warn!(%error, "book delta normalization failed");
                                                            None
                                                        } else {
                                                            Some(state.to_order_book())
                                                        }
                                                    }
                                                    None => {
                                                        warn!(ticker = %delta.market_ticker, "delta arrived before snapshot");
                                                        None
                                                    }
                                                }
                                            }
                                            Err(error) => {
                                                quarantine_payload(
                                                    quarantine_producer,
                                                    quarantine_storage,
                                                    "kalshi",
                                                    &format!("invalid book delta: {error}"),
                                                    text.as_bytes(),
                                                )
                                                .await?;
                                                warn!(%error, "invalid book delta");
                                                None
                                            }
                                        }
                                    }
                                    _ => None,
                                };

                                if let Some(result) = book {
                                    match result {
                                        Ok(book) => {
                                            let envelope = Envelope::new("order_book", &book);
                                            if let Err(error) = producer
                                                .send(topic, envelope, Some(book.market.as_str()))
                                                .await
                                            {
                                                return Err(StreamError::Producer(error));
                                            }
                                        }
                                        Err(error) => warn!(%error, "book normalization failed"),
                                    }
                                }
                            }
                            Some(Ok(_)) => continue,
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
                Err(e) => {
                    warn!(error = %e, "connection attempt failed");
                    attempt += 1;
                    if let Err(be) = breaker.record_failure() {
                        error!("circuit breaker opened: {be}");
                        return Err(be);
                    }
                }
            }

            attempt += 1;
        }
    }
}

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
// Message handling
// ---------------------------------------------------------------------------

/// Handle an incoming WebSocket message.
///
/// Parses the message as a `KalshiWsMessage`. If the message type matches
/// the expected type for `T`, extracts and returns the data payload.
/// Returns `Ok(None)` for non-matching message types (e.g., subscription
/// confirmations).
///
/// Updates the `SeqTracker` for gap detection.
#[allow(clippy::result_large_err)]
fn handle_ws_message<T: DeserializeOwned>(
    msg: &Message,
    seq_tracker: &mut SeqTracker,
    expected_type: &str,
) -> Result<Option<T>, StreamError> {
    let text = match msg {
        Message::Text(t) => t,
        Message::Binary(b) => {
            // Binary frames are not expected; log and skip
            debug!(len = b.len(), "received unexpected binary WS frame");
            return Ok(None);
        }
        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => return Ok(None),
        Message::Close(_) => return Ok(None),
    };

    let ws_msg: KalshiWsMessage =
        serde_json::from_str(text).map_err(|e| StreamError::Serialization(e.to_string()))?;

    // Check for gap
    if let Some(seq) = ws_msg.seq {
        if let Some((expected, actual)) = seq_tracker.observe(seq) {
            return Err(StreamError::GapDetected { expected, actual });
        }
    }

    // Check if this is the expected message type
    if ws_msg.msg_type != expected_type {
        // Not the message type we're looking for — skip silently
        return Ok(None);
    }

    // Deserialize the data field as T
    let payload: T = serde_json::from_value(ws_msg.data.clone())
        .map_err(|e| StreamError::Serialization(e.to_string()))?;

    Ok(Some(payload))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aether_bus::producer::StubProducer;
    use aether_bus::quarantine::StubObjectStore;
    use aether_core::quote::QuoteSource;
    use rust_decimal::Decimal;

    #[tokio::test]
    async fn malformed_frame_is_preserved_and_published_to_venue_quarantine() {
        let producer = StubProducer::new();
        let storage = StubObjectStore::new();
        let raw = br#"{"type":"ticker","msg":{"market_ticker":"BROKEN"}}"#;

        let hash =
            quarantine_payload(&producer, &storage, "kalshi", "missing price and timestamp", raw)
                .await
                .unwrap();

        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "quarantine.kalshi");
        drop(sent);
        let stored = storage.objects.lock().unwrap();
        assert_eq!(stored.get(&format!("quarantine/kalshi/{hash}")), Some(&raw.to_vec()));
    }

    // ── WS message deserialization ──

    #[test]
    fn parse_tick_message() {
        let json = r#"{
            "type": "ticker",
            "seq": 42,
            "data": {
                "ticker": "BTC-75",
                "price": 65,
                "side": "yes",
                "ts": "2026-07-10T12:34:56.789Z",
                "volume": 1500,
                "bid": 64,
                "ask": 66
            }
        }"#;

        let msg: KalshiWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "ticker");
        assert_eq!(msg.seq, Some(42));

        let tick: KalshiTick = serde_json::from_value(msg.data).unwrap();
        assert_eq!(tick.ticker, "BTC-75");
        assert_eq!(tick.price, Some(65));
        assert_eq!(tick.side.as_deref(), Some("yes"));
        assert_eq!(tick.bid, Some(64));
        assert_eq!(tick.ask, Some(66));
        assert_eq!(tick.volume, Some(1500));
    }

    #[test]
    fn parse_book_snapshot() {
        let json = r#"{
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
                    {"price": 66, "size": 1500}
                ]
            }
        }"#;

        let msg: KalshiWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "book_snapshot");
        assert_eq!(msg.seq, Some(99));

        let book: KalshiBookSnapshot = serde_json::from_value(msg.data).unwrap();
        assert_eq!(book.ticker, "BTC-75");
        assert_eq!(book.bids.len(), 2);
        assert_eq!(book.asks.len(), 1);
        assert_eq!(book.bids[0].price, 64);
        assert_eq!(book.bids[1].size, 2000);
    }

    #[test]
    fn parse_ws_message_without_seq() {
        let json = r#"{
            "type": "ticker",
            "data": {
                "ticker": "BTC-75",
                "price": 50,
                "side": "yes",
                "ts": "2026-07-10T12:00:00.000Z"
            }
        }"#;

        let msg: KalshiWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.seq, None);
    }

    #[test]
    fn parse_ws_message_with_extra_fields() {
        let json = r#"{
            "type": "subscription",
            "data": {"status": "ok", "channels": ["ticker"]}
        }"#;

        let msg: KalshiWsMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "subscription");
        assert!(msg.seq.is_none());
    }

    // ── Subscribe message format ──

    #[test]
    fn subscribe_message_format() {
        let req = SubscribeRequest {
            id: 1,
            cmd: "subscribe".into(),
            params: SubscribeParams {
                channels: vec!["ticker".into()],
                market_tickers: vec!["BTC-75".into(), "ETH-50".into()],
                use_yes_price: None,
            },
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["cmd"], "subscribe");
        assert_eq!(parsed["params"]["channels"][0], "ticker");
        assert_eq!(parsed["params"]["market_tickers"][0], "BTC-75");
        assert_eq!(parsed["params"]["market_tickers"][1], "ETH-50");
    }

    #[test]
    fn subscribe_book_message_format() {
        let req = SubscribeRequest {
            id: 2,
            cmd: "subscribe".into(),
            params: SubscribeParams {
                channels: vec!["orderbook_delta".into()],
                market_tickers: vec!["BTC-75".into()],
                use_yes_price: Some(true),
            },
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["id"], 2);
        assert_eq!(parsed["cmd"], "subscribe");
        assert_eq!(parsed["params"]["channels"][0], "orderbook_delta");
        assert_eq!(parsed["params"]["market_tickers"][0], "BTC-75");
        assert_eq!(parsed["params"]["use_yes_price"], true);
    }

    // ── Gap detection ──

    #[test]
    fn gap_detection_no_gap() {
        let mut tracker = SeqTracker::new();
        assert_eq!(tracker.observe(1), None);
        assert_eq!(tracker.observe(2), None);
        assert_eq!(tracker.observe(3), None);
    }

    #[test]
    fn gap_detection_detects_skip() {
        let mut tracker = SeqTracker::new();
        assert_eq!(tracker.observe(1), None);
        let gap = tracker.observe(5);
        assert_eq!(gap, Some((2, 5)));
    }

    #[test]
    fn gap_detection_after_reset() {
        let mut tracker = SeqTracker::new();
        assert_eq!(tracker.observe(10), None);
        tracker.reset();
        assert_eq!(tracker.observe(1), None); // Reset clears last, so 1 is fine
    }

    #[test]
    fn gap_detection_multiple_gaps() {
        let mut tracker = SeqTracker::new();
        assert_eq!(tracker.observe(1), None);
        assert_eq!(tracker.observe(2), None);
        // Jump from 2 -> 10 (skip 3-9)
        let gap = tracker.observe(10);
        assert_eq!(gap, Some((3, 10)));
        // Subsequent contiguous
        assert_eq!(tracker.observe(11), None);
    }

    // ── Normalization from WS frames ──

    #[test]
    fn normalize_tick_from_ws() {
        let json = r#"{
            "type": "ticker",
            "seq": 42,
            "data": {
                "ticker": "BTC-75",
                "price": 65,
                "side": "yes",
                "ts": "2026-07-10T12:34:56.789Z",
                "volume": 1500,
                "bid": 64,
                "ask": 66
            }
        }"#;

        let msg: KalshiWsMessage = serde_json::from_str(json).unwrap();
        let tick: KalshiTick = serde_json::from_value(msg.data).unwrap();
        let quote = normalize_tick(&tick).unwrap();

        assert_eq!(quote.market.as_str(), "mkt:kalshi:btc-75");
        assert_eq!(quote.bid, Some(Decimal::new(64, 2)));
        assert_eq!(quote.ask, Some(Decimal::new(66, 2)));
        assert_eq!(quote.last, Some(Decimal::new(65, 2)));
        assert_eq!(quote.source, QuoteSource::Stream);
    }

    #[test]
    fn normalize_book_from_ws() {
        let json = r#"{
            "type": "book_snapshot",
            "seq": 99,
            "data": {
                "ticker": "BTC-75",
                "ts": "2026-07-10T12:34:56.789Z",
                "bids": [
                    {"price": 65, "size": 500},
                    {"price": 64, "size": 1000},
                    {"price": 63, "size": 2000}
                ],
                "asks": [
                    {"price": 66, "size": 1500},
                    {"price": 67, "size": 1200}
                ]
            }
        }"#;

        let msg: KalshiWsMessage = serde_json::from_str(json).unwrap();
        let book: KalshiBookSnapshot = serde_json::from_value(msg.data).unwrap();
        let ob = normalize_book(&book).unwrap();

        assert_eq!(ob.market.as_str(), "mkt:kalshi:btc-75");
        assert_eq!(ob.bids().len(), 3);
        assert_eq!(ob.asks().len(), 2);

        // Bids are already sorted descending
        assert_eq!(ob.bids()[0].price, Decimal::new(65, 2));
        assert_eq!(ob.bids()[1].price, Decimal::new(64, 2));
        assert_eq!(ob.bids()[2].price, Decimal::new(63, 2));

        // Asks ascending
        assert_eq!(ob.asks()[0].price, Decimal::new(66, 2));
        assert_eq!(ob.asks()[1].price, Decimal::new(67, 2));
    }

    // ── Backoff ──

    #[test]
    fn full_jitter_backoff_bounds() {
        // Base case: attempt 0 should sleep between 0 and base_ms
        let d = full_jitter_backoff(0, 200, 30_000);
        assert!(d.as_millis() <= 200);

        // High attempt should cap at cap_ms
        let d = full_jitter_backoff(20, 200, 30_000);
        assert!(d.as_millis() <= 30_000);
    }

    // ── Breaker ──

    #[test]
    fn breaker_opens_after_threshold() {
        let mut breaker = StreamBreaker::new(3);
        assert!(!breaker.is_open());

        assert!(breaker.record_failure().is_ok());
        assert!(!breaker.is_open());

        assert!(breaker.record_failure().is_ok());
        assert!(!breaker.is_open());

        // Third failure should open breaker
        let result = breaker.record_failure();
        assert!(result.is_err());
        assert!(matches!(result, Err(StreamError::BreakerOpen { threshold: 3 })));
        assert!(breaker.is_open());
    }

    #[test]
    fn breaker_resets_on_success() {
        let mut breaker = StreamBreaker::new(3);
        breaker.record_failure().unwrap();
        breaker.record_failure().unwrap();
        breaker.record_success(); // Reset counter
        let result = breaker.record_failure();
        assert!(result.is_ok()); // Back to 1 failure
    }

    #[test]
    fn breaker_reset_clears_state() {
        let mut breaker = StreamBreaker::new(2);
        breaker.record_failure().unwrap();
        breaker.record_failure().unwrap_err(); // Opens
        assert!(breaker.is_open());

        breaker.reset();
        assert!(!breaker.is_open());
        assert!(breaker.record_failure().is_ok());
    }

    // ── handle_ws_message ──

    #[test]
    fn handle_ws_message_skips_binary() {
        let msg = Message::Binary(vec![0, 1, 2]);
        let mut tracker = SeqTracker::new();
        let result: Result<Option<KalshiTick>, _> = handle_ws_message(&msg, &mut tracker, "ticker");
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn handle_ws_message_skips_ping() {
        let msg = Message::Ping(vec![]);
        let mut tracker = SeqTracker::new();
        let result: Result<Option<KalshiTick>, _> = handle_ws_message(&msg, &mut tracker, "ticker");
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn handle_ws_message_reports_gap() {
        let json = r#"{
            "type": "ticker",
            "seq": 5,
            "data": {
                "ticker": "T",
                "price": 50,
                "side": "yes",
                "ts": "2026-07-10T12:00:00.000Z"
            }
        }"#;
        let msg = Message::Text(json.into());
        let mut tracker = SeqTracker::new();

        // First message at seq 1
        tracker.observe(1);

        // Second message at seq 5 — gap!
        let result: Result<Option<KalshiTick>, _> = handle_ws_message(&msg, &mut tracker, "ticker");
        assert!(matches!(result, Err(StreamError::GapDetected { expected: 2, actual: 5 })));
    }
}

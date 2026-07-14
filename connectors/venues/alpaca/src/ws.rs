//! Alpaca WebSocket stream client.
//!
//! Provides authenticated WebSocket connectivity to the Alpaca Data API v2
//! streaming endpoint for real-time trades and quotes.
//!
//! # Connection flow
//!
//! 1. Connect to `wss://stream.data.alpaca.markets/v2/iex`
//! 2. Authenticate: `{"action":"auth","key":"...","secret":"..."}`
//! 3. Subscribe: `{"action":"subscribe","trades":["AAPL"],"quotes":["AAPL"]}`
//! 4. Receive real-time trade/quote messages
//!
//! # Reconnection
//!
//! Uses SPEC-006 full-jitter exponential backoff on disconnect. A simple
//! circuit breaker opens after 5 consecutive WebSocket errors.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__ALPACA_WS_URL` | Override WS URL (default: IEX feed) |

use crate::auth::AlpacaAuth;
use crate::client::AlpacaSnapshot;
use crate::normalize::{normalize_stream_snapshot, NormalizeError};
use aether_bus::envelope::Envelope;
use aether_bus::producer::{MessageProducer, ProducerError};
use aether_bus::quarantine::{ObjectStore, Quarantine, QuarantineError};
use aether_bus::retry::{CircuitBreaker, RetryPolicy};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
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

/// Default Alpaca IEX WebSocket URL.
const DEFAULT_WS_URL: &str = "wss://stream.data.alpaca.markets/v2/iex";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the Alpaca WebSocket client.
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
    Normalize(#[from] NormalizeError),

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

    /// Authentication failed.
    #[error("authentication failed: {0}")]
    AuthFailed(String),
}

// ---------------------------------------------------------------------------
// WS message types
// ---------------------------------------------------------------------------

/// A raw message from the Alpaca WebSocket API.
///
/// Messages are always wrapped in JSON arrays, even single messages.
/// We deserialize as an array of these.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AlpacaWsMessage {
    /// A standard data or control message.
    Message {
        /// Message type: `"t"` (trade), `"q"` (quote), `"success"`, `"subscription"`, `"error"`.
        #[serde(rename = "T")]
        msg_type: String,
        /// Additional fields are captured as raw JSON.
        #[serde(flatten)]
        data: serde_json::Map<String, serde_json::Value>,
    },
}

/// Auth message sent to the Alpaca WebSocket.
#[derive(Debug, Serialize)]
struct AuthMessage {
    action: String,
    key: String,
    secret: String,
}

/// Subscribe message sent to the Alpaca WebSocket.
#[derive(Debug, Serialize)]
struct SubscribeMessage {
    action: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    trades: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    quotes: Vec<String>,
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
    inner: CircuitBreaker,
}

impl StreamBreaker {
    /// Create a new breaker with the given failure threshold.
    pub fn new(threshold: u32) -> Self {
        debug_assert_eq!(threshold, CircuitBreaker::SPEC_DEFAULT_THRESHOLD);
        Self { threshold, inner: CircuitBreaker::new() }
    }

    /// Record a failure. Returns `BreakerOpen` if the threshold is reached.
    #[allow(clippy::result_large_err)]
    pub fn record_failure(&mut self) -> Result<(), StreamError> {
        self.inner.record_failure();
        if !self.inner.allow_request() {
            return Err(StreamError::BreakerOpen { threshold: self.threshold });
        }
        Ok(())
    }

    /// Record a success — resets the failure count.
    pub fn record_success(&mut self) {
        self.inner.record_success();
    }

    /// Returns true if the breaker is open (too many failures).
    pub fn is_open(&mut self) -> bool {
        !self.inner.allow_request()
    }

    /// Reset the breaker to closed state.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.inner = CircuitBreaker::new();
    }
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
// AlpacaStream
// ---------------------------------------------------------------------------

/// An authenticated WebSocket client for Alpaca real-time market data.
#[derive(Debug)]
pub struct AlpacaStream {
    /// WebSocket URL.
    ws_url: String,
    /// Authentication handle.
    auth: AlpacaAuth,
}

impl AlpacaStream {
    /// Create a new stream client with the given WS URL and auth.
    pub fn new(ws_url: impl Into<String>, auth: AlpacaAuth) -> Self {
        Self { ws_url: ws_url.into(), auth }
    }

    /// Create a new stream client, loading WS URL from
    /// `AETHER_VENUE__ALPACA_WS_URL` (falls back to IEX feed).
    pub fn from_env(auth: AlpacaAuth) -> Self {
        let ws_url = std::env::var("AETHER_VENUE__ALPACA_WS_URL")
            .unwrap_or_else(|_| DEFAULT_WS_URL.to_string());
        Self::new(ws_url, auth)
    }

    // -----------------------------------------------------------------------
    // Connection
    // -----------------------------------------------------------------------

    /// Connect to the Alpaca WebSocket API and authenticate.
    pub async fn connect(
        &self,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        StreamError,
    > {
        debug!(url = %self.ws_url, "connecting to Alpaca WebSocket");

        let request = self
            .ws_url
            .clone()
            .into_client_request()
            .map_err(|e| StreamError::Connection(e.to_string()))?;

        let (mut ws_stream, _) =
            connect_async(request).await.map_err(|e| StreamError::Connection(e.to_string()))?;

        // Authenticate
        let auth_msg = AuthMessage {
            action: "auth".into(),
            key: self.auth.key_id().to_string(),
            secret: self.auth.secret_key().to_string(),
        };
        let auth_json = serde_json::to_string(&auth_msg)
            .map_err(|e| StreamError::Serialization(e.to_string()))?;

        debug!("sending auth message");
        ws_stream
            .send(Message::Text(auth_json))
            .await
            .map_err(|e| StreamError::Connection(e.to_string()))?;

        // Wait for auth response
        let timeout_dur = Duration::from_secs(10);
        let deadline = tokio::time::Instant::now() + timeout_dur;

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(StreamError::AuthFailed("auth timeout after 10s".into()));
            }

            match tokio::time::timeout(Duration::from_secs(1), ws_stream.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Some(response) = check_auth_response(&text) {
                        if response == "authenticated" {
                            info!("Alpaca WebSocket authenticated");
                            break;
                        } else {
                            return Err(StreamError::AuthFailed(response.to_string()));
                        }
                    }
                    // Otherwise it might be the "connected" message, keep waiting
                }
                Ok(Some(Ok(_))) => {
                    // Non-text messages during auth are ignored
                    continue;
                }
                Ok(Some(Err(e))) => {
                    return Err(StreamError::Connection(e.to_string()));
                }
                Ok(None) => {
                    return Err(StreamError::Connection("WebSocket closed during auth".into()));
                }
                Err(_) => {
                    // timeout waiting for auth response, try again
                    continue;
                }
            }
        }

        Ok(ws_stream)
    }

    // -----------------------------------------------------------------------
    // Subscriptions
    // -----------------------------------------------------------------------

    /// Subscribe to trades for the given symbols.
    pub async fn subscribe_trades(
        &self,
        ws_sink: &mut (impl futures::Sink<Message, Error = WsError> + Unpin),
        symbols: &[String],
    ) -> Result<(), StreamError> {
        let msg = SubscribeMessage {
            action: "subscribe".into(),
            trades: symbols.to_vec(),
            quotes: vec![],
        };
        let json =
            serde_json::to_string(&msg).map_err(|e| StreamError::Serialization(e.to_string()))?;

        debug!(json = %json, "subscribing to trades");
        ws_sink.send(Message::Text(json)).await?;
        Ok(())
    }

    /// Subscribe to quotes for the given symbols.
    pub async fn subscribe_quotes(
        &self,
        ws_sink: &mut (impl futures::Sink<Message, Error = WsError> + Unpin),
        symbols: &[String],
    ) -> Result<(), StreamError> {
        let msg = SubscribeMessage {
            action: "subscribe".into(),
            trades: vec![],
            quotes: symbols.to_vec(),
        };
        let json =
            serde_json::to_string(&msg).map_err(|e| StreamError::Serialization(e.to_string()))?;

        debug!(json = %json, "subscribing to quotes");
        ws_sink.send(Message::Text(json)).await?;
        Ok(())
    }

    /// Subscribe to both trades and quotes for the given symbols.
    pub async fn subscribe_all(
        &self,
        ws_sink: &mut (impl futures::Sink<Message, Error = WsError> + Unpin),
        symbols: &[String],
    ) -> Result<(), StreamError> {
        let msg = SubscribeMessage {
            action: "subscribe".into(),
            trades: symbols.to_vec(),
            quotes: symbols.to_vec(),
        };
        let json =
            serde_json::to_string(&msg).map_err(|e| StreamError::Serialization(e.to_string()))?;

        debug!(json = %json, "subscribing to trades and quotes");
        ws_sink.send(Message::Text(json)).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Continuous streaming
    // -----------------------------------------------------------------------

    /// Stream normalized `Quote`s to the `md.ticks.alpaca` bus topic.
    ///
    /// This method runs an infinite loop with reconnection:
    /// 1. Connect to the Alpaca WebSocket
    /// 2. Authenticate
    /// 3. Subscribe to trades and quotes for the given symbols
    /// 4. Read messages, normalize to `Quote`, send to bus
    /// 5. On disconnect, back off and reconnect
    /// 6. On >5 consecutive errors, open the circuit breaker and return
    pub async fn stream_ticks_to_bus<Q: MessageProducer, S: ObjectStore>(
        &self,
        symbols: &[String],
        producer: &impl MessageProducer,
        quarantine_producer: &Q,
        quarantine_storage: &S,
    ) -> Result<(), StreamError> {
        let topic = "md.ticks.alpaca";
        let mut attempt: u32 = 0;
        let mut breaker = StreamBreaker::new(BREAKER_THRESHOLD);

        loop {
            // Check breaker
            if breaker.is_open() {
                return Err(StreamError::BreakerOpen { threshold: BREAKER_THRESHOLD });
            }

            // Backoff if reconnecting
            if attempt > 0 {
                let delay = full_jitter_backoff(attempt, BACKOFF_BASE_MS, BACKOFF_CAP_MS);
                info!(attempt, delay_ms = delay.as_millis(), "reconnecting to Alpaca WS");
                tokio::time::sleep(delay).await;
            }

            match self.connect().await {
                Ok(ws) => {
                    attempt = 0; // Reset backoff on successful connect
                    breaker.record_success();

                    let (mut ws_sink, mut ws_stream) = ws.split();

                    // Subscribe to both trades and quotes
                    if let Err(e) = self.subscribe_all(&mut ws_sink, symbols).await {
                        warn!(error = %e, "subscribe failed, will reconnect");
                        let _ = breaker.record_failure();
                        continue;
                    }

                    // Message loop
                    loop {
                        match ws_stream.next().await {
                            Some(Ok(msg)) => match handle_ws_message(&msg) {
                                Ok(snapshots) => {
                                    for snapshot in snapshots {
                                        match normalize_stream_snapshot(&snapshot) {
                                            Ok(quote) => {
                                                let envelope = Envelope::new("quote", &quote);
                                                if let Err(e) = producer
                                                    .send(
                                                        topic,
                                                        envelope,
                                                        Some(quote.market.as_str()),
                                                    )
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
                                                    "alpaca",
                                                    &e.to_string(),
                                                    raw,
                                                )
                                                .await?;
                                                warn!(error = %e, "snapshot normalization failed");
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    if matches!(&e, StreamError::Serialization(_)) {
                                        quarantine_payload(
                                            quarantine_producer,
                                            quarantine_storage,
                                            "alpaca",
                                            &e.to_string(),
                                            message_bytes(&msg),
                                        )
                                        .await?;
                                    }
                                    warn!(error = %e, "message handling error");
                                }
                            },
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
}

// ---------------------------------------------------------------------------
// Message handling helpers
// ---------------------------------------------------------------------------

/// Check if a WebSocket text message is an auth response.
/// Returns `Some("authenticated")` on success, `Some("error message")` on failure,
/// or `None` if it's not an auth-related message.
fn check_auth_response(text: &str) -> Option<String> {
    if let Ok(messages) = serde_json::from_str::<Vec<serde_json::Value>>(text) {
        for msg in messages {
            let msg_type = msg.get("T").and_then(|v| v.as_str()).unwrap_or("");
            match msg_type {
                "success" => {
                    if let Some(content) = msg.get("msg").and_then(|v| v.as_str()) {
                        if content == "authenticated" || content == "connected" {
                            return Some(content.to_string());
                        }
                    }
                }
                "error" => {
                    if let Some(content) = msg.get("msg").and_then(|v| v.as_str()) {
                        return Some(format!("error: {content}"));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Handle an incoming WebSocket message.
///
/// Parses the message as a JSON array of Alpaca WS messages. If any messages
/// contain trade (`"t"`) or quote (`"q"`) data, each data item becomes its
/// own snapshot. Alpaca may batch unrelated symbols in one frame; keeping
/// events separate prevents cross-symbol price contamination.
#[allow(clippy::result_large_err)]
fn handle_ws_message(msg: &Message) -> Result<Vec<AlpacaSnapshot>, StreamError> {
    let text = match msg {
        Message::Text(t) => t,
        Message::Binary(b) => {
            debug!(len = b.len(), "received unexpected binary WS frame");
            return Ok(Vec::new());
        }
        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => return Ok(Vec::new()),
        Message::Close(_) => return Ok(Vec::new()),
    };

    // Messages from Alpaca are always JSON arrays
    let messages: Vec<serde_json::Value> =
        serde_json::from_str(text).map_err(|e| StreamError::Serialization(e.to_string()))?;

    let mut snapshots = Vec::new();

    for msg_value in messages {
        let msg_type = msg_value.get("T").and_then(|v| v.as_str()).unwrap_or("");
        let snapshot = match msg_type {
            "t" => {
                let symbol = required_symbol(&msg_value).map_err(StreamError::Serialization)?;
                Some(AlpacaSnapshot {
                    symbol: Some(symbol),
                    latest_trade: Some(crate::client::AlpacaTrade {
                        t: string_field(&msg_value, "t"),
                        x: string_field(&msg_value, "x"),
                        p: decimal_field(&msg_value, "p").map_err(StreamError::Serialization)?,
                        s: decimal_field(&msg_value, "s").map_err(StreamError::Serialization)?,
                        ..Default::default()
                    }),
                    ..Default::default()
                })
            }
            "q" => {
                let symbol = required_symbol(&msg_value).map_err(StreamError::Serialization)?;
                Some(AlpacaSnapshot {
                    symbol: Some(symbol),
                    latest_quote: Some(crate::client::AlpacaQuote {
                        t: string_field(&msg_value, "t"),
                        ap: decimal_field(&msg_value, "ap").map_err(StreamError::Serialization)?,
                        bp: decimal_field(&msg_value, "bp").map_err(StreamError::Serialization)?,
                        ask_size: decimal_field(&msg_value, "as")
                            .map_err(StreamError::Serialization)?,
                        bs: decimal_field(&msg_value, "bs").map_err(StreamError::Serialization)?,
                        ..Default::default()
                    }),
                    ..Default::default()
                })
            }
            "b" | "subscription" | "success" => None,
            _ => {
                debug!(type = %msg_type, "unrecognized WS message type");
                None
            }
        };
        if let Some(snapshot) = snapshot {
            snapshots.push(snapshot);
        }
    }
    Ok(snapshots)
}

fn required_symbol(value: &serde_json::Value) -> Result<String, String> {
    value
        .get("S")
        .and_then(serde_json::Value::as_str)
        .filter(|symbol| !symbol.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "trade/quote message has no symbol field".to_string())
}

fn string_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value.get(field).and_then(serde_json::Value::as_str).map(str::to_owned)
}

fn decimal_field(value: &serde_json::Value, field: &str) -> Result<Option<String>, String> {
    match value.get(field) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(number)) => Ok(Some(number.to_string())),
        Some(serde_json::Value::String(number)) => Ok(Some(number.clone())),
        Some(other) => {
            Err(format!("field '{field}' must be a decimal number or string, got {other}"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use aether_bus::producer::StubProducer;
    use aether_bus::quarantine::StubObjectStore;

    #[tokio::test]
    async fn malformed_frame_is_preserved_and_published_to_venue_quarantine() {
        let producer = StubProducer::new();
        let storage = StubObjectStore::new();
        let raw = br#"[{"T":"t","S":"AAPL"}]"#;

        let hash =
            quarantine_payload(&producer, &storage, "alpaca", "missing price and timestamp", raw)
                .await
                .unwrap();

        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "quarantine.alpaca");
        drop(sent);
        let stored = storage.objects.lock().unwrap();
        assert_eq!(stored.get(&format!("quarantine/alpaca/{hash}")), Some(&raw.to_vec()));
    }

    // ── WS message parsing ──

    #[test]
    fn parse_trade_message() {
        let json = r#"[{
            "T": "t",
            "S": "AAPL",
            "p": 125.91,
            "s": 100,
            "t": "2021-05-11T20:00:00.435997104Z",
            "x": "Q",
            "i": 179430,
            "z": "C"
        }]"#;

        let msg = Message::Text(json.into());
        let result = handle_ws_message(&msg).unwrap();
        assert_eq!(result.len(), 1);
        let snap = &result[0];
        assert_eq!(snap.symbol.as_deref(), Some("AAPL"));
        assert!(snap.latest_trade.is_some());
        assert_eq!(snap.latest_trade.as_ref().unwrap().p.as_deref(), Some("125.91"));
    }

    #[test]
    fn parse_quote_message() {
        let json = r#"[{
            "T": "q",
            "S": "AAPL",
            "bp": 125.6,
            "ap": 125.68,
            "bs": 4,
            "as": 12,
            "t": "2021-05-11T22:05:02.307304704Z",
            "x": "P"
        }]"#;

        let msg = Message::Text(json.into());
        let result = handle_ws_message(&msg).unwrap();
        assert_eq!(result.len(), 1);
        let snap = &result[0];
        assert_eq!(snap.symbol.as_deref(), Some("AAPL"));
        assert!(snap.latest_quote.is_some());
        let quote = snap.latest_quote.as_ref().unwrap();
        assert_eq!(quote.bp.as_deref(), Some("125.6"));
        assert_eq!(quote.ap.as_deref(), Some("125.68"));
    }

    #[test]
    fn parse_combined_trade_quote() {
        let json = r#"[{
            "T": "t",
            "S": "AAPL",
            "p": 125.91,
            "s": 100,
            "t": "2021-05-11T20:00:00.435997104Z",
            "x": "Q"
        }, {
            "T": "q",
            "S": "AAPL",
            "bp": 125.6,
            "ap": 125.68,
            "bs": 4,
            "as": 12,
            "t": "2021-05-11T22:05:02.307304704Z"
        }]"#;

        let msg = Message::Text(json.into());
        let result = handle_ws_message(&msg).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].latest_trade.is_some());
        assert!(result[1].latest_quote.is_some());

        assert_eq!(result[0].symbol.as_deref(), Some("AAPL"));
        assert_eq!(result[1].symbol.as_deref(), Some("AAPL"));
    }

    #[test]
    fn batched_symbols_are_never_coalesced() {
        let msg = Message::Text(
            r#"[{"T":"t","S":"AAPL","p":125.91,"s":10,"t":"2021-05-11T20:00:00Z"},
                {"T":"q","S":"MSFT","bp":245.30,"ap":245.32,"bs":4,"as":6,"t":"2021-05-11T20:00:01Z"}]"#
                .into(),
        );
        let snapshots = handle_ws_message(&msg).unwrap();
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].symbol.as_deref(), Some("AAPL"));
        assert_eq!(snapshots[1].symbol.as_deref(), Some("MSFT"));
        assert!(snapshots[0].latest_quote.is_none());
        assert!(snapshots[1].latest_trade.is_none());
        assert_eq!(snapshots[1].latest_quote.as_ref().unwrap().bp.as_deref(), Some("245.30"));
    }

    #[test]
    fn parse_subscription_confirmation() {
        let json = r#"[{"T":"success","msg":"connected"}]"#;
        let msg = Message::Text(json.into());
        let result = handle_ws_message(&msg).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_empty_array() {
        let json = r#"[]"#;
        let msg = Message::Text(json.into());
        let result = handle_ws_message(&msg).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn check_auth_response_authenticated() {
        let result = check_auth_response(r#"[{"T":"success","msg":"authenticated"}]"#);
        assert_eq!(result, Some("authenticated".to_string()));
    }

    #[test]
    fn check_auth_response_connected() {
        let result = check_auth_response(r#"[{"T":"success","msg":"connected"}]"#);
        assert_eq!(result, Some("connected".to_string()));
    }

    #[test]
    fn check_auth_response_error() {
        let result = check_auth_response(r#"[{"T":"error","msg":"auth failed"}]"#);
        assert_eq!(result, Some("error: auth failed".to_string()));
    }

    // ── Backoff ──

    #[test]
    fn full_jitter_backoff_bounds() {
        let d = full_jitter_backoff(0, 200, 30_000);
        assert!(d.as_millis() <= 200);

        let d = full_jitter_backoff(20, 200, 30_000);
        assert!(d.as_millis() <= 30_000);
    }

    // ── Breaker ──

    #[test]
    fn breaker_opens_after_threshold() {
        let mut breaker = StreamBreaker::new(BREAKER_THRESHOLD);
        assert!(!breaker.is_open());
        for _ in 0..(BREAKER_THRESHOLD - 1) {
            assert!(breaker.record_failure().is_ok());
        }
        let result = breaker.record_failure();
        assert!(result.is_err());
        assert!(breaker.is_open());
    }

    #[test]
    fn breaker_resets_on_success() {
        let mut breaker = StreamBreaker::new(BREAKER_THRESHOLD);
        breaker.record_failure().unwrap();
        breaker.record_failure().unwrap();
        breaker.record_success();
        let result = breaker.record_failure();
        assert!(result.is_ok());
    }

    #[test]
    fn handle_ws_message_skips_binary() {
        let msg = Message::Binary(vec![0, 1, 2]);
        let result = handle_ws_message(&msg).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn handle_ws_message_skips_ping() {
        let msg = Message::Ping(vec![]);
        let result = handle_ws_message(&msg).unwrap();
        assert!(result.is_empty());
    }
}

//! Polymarket CLOB REST client.
//!
//! Provides an HTTP client for the Polymarket CLOB API order-book
//! endpoints.  Public market-data reads require no authentication.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__POLYMARKET_CLOB_URL` | Override CLOB API origin (default: `https://clob.polymarket.com`) |

#![allow(dead_code)]

use aether_bus::retry::{CircuitBreaker, RetryPolicy};
use serde::Deserialize;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::Mutex;

const MAX_429_RETRIES: u32 = 3;

// ---------------------------------------------------------------------------
// Embedded manifest for rate-limit configuration
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct EmbeddedManifest {
    rate_limits: EmbeddedRateLimits,
}

#[derive(Deserialize)]
struct EmbeddedRateLimits {
    rest_per_min: u32,
}

fn manifest_rest_budget() -> u32 {
    let manifest: EmbeddedManifest = match toml::from_str(include_str!("../venue.toml")) {
        Ok(m) => m,
        Err(error) => panic!("embedded Polymarket venue manifest is invalid: {error}"),
    };
    assert!(manifest.rate_limits.rest_per_min > 0, "rest_per_min must be positive");
    manifest.rate_limits.rest_per_min
}

// ---------------------------------------------------------------------------
// Token-bucket rate limiter
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    capacity: u32,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: u32) -> Self {
        Self { tokens: capacity as f64, capacity, last_refill: Instant::now() }
    }

    fn refill(&mut self) {
        let elapsed = self.last_refill.elapsed().as_secs_f64();
        self.tokens =
            (self.tokens + elapsed * self.capacity as f64 / 60.0).min(self.capacity as f64);
        self.last_refill = Instant::now();
    }

    fn try_take(&mut self) -> Option<Duration> {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            None
        } else {
            Some(Duration::from_secs_f64((1.0 - self.tokens) * 60.0 / self.capacity as f64))
        }
    }
}

// Re-use canonical raw types from the normalization module.
pub use crate::normalize::ClobBookSnapshot;

/// Private REST deserialization helper for the CLOB API's snake_case schema.
#[derive(Debug, Clone, Deserialize)]
struct RawClobBook {
    market: String,
    asset_id: String,
    timestamp: String,
    #[serde(default)]
    bids: Vec<crate::normalize::ClobLevel>,
    #[serde(default)]
    asks: Vec<crate::normalize::ClobLevel>,
    #[serde(default)]
    hash: Option<String>,
}

impl From<RawClobBook> for ClobBookSnapshot {
    fn from(raw: RawClobBook) -> Self {
        ClobBookSnapshot {
            market: raw.market,
            asset_id: raw.asset_id,
            timestamp: raw.timestamp,
            bids: raw.bids,
            asks: raw.asks,
            hash: raw.hash,
        }
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the Polymarket CLOB REST client.
#[derive(Error, Debug)]
pub enum ClobError {
    /// HTTP request failed (network, timeout, etc.).
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned a non-success status code.
    #[error("Polymarket CLOB API returned HTTP {status}")]
    Api {
        /// HTTP status code.
        status: reqwest::StatusCode,
    },

    /// Failed to parse the API response JSON.
    #[error("response parse error: {detail}")]
    Parse {
        /// Human-readable parse error detail.
        detail: String,
        /// The originating serde error.
        #[source]
        source: serde_json::Error,
        /// Exact response bytes retained for quarantine; never rendered by Display.
        raw: Vec<u8>,
    },

    /// Repeated 429 responses opened the local rate-limit circuit breaker.
    #[error("Polymarket CLOB rate-limit circuit breaker is open")]
    RateLimitBreaker,
}

impl ClobError {
    /// Exact venue response bytes when this error represents a parse failure.
    pub fn raw_payload(&self) -> Option<&[u8]> {
        match self {
            Self::Parse { raw, .. } => Some(raw),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// An unauthenticated HTTP client for the Polymarket CLOB API.
///
/// All market-data endpoints are public; no API key or signature is required.
#[derive(Debug)]
pub struct ClobClient {
    /// Base URL for all CLOB API requests.
    base_url: String,
    /// Shared HTTP client.
    http: reqwest::Client,
    /// Token-bucket rate limiter.
    limiter: Mutex<TokenBucket>,
    /// Shared SPEC-006 breaker for repeated HTTP 429 responses.
    rate_breaker: Mutex<CircuitBreaker>,
}

impl ClobClient {
    /// Create a new CLOB client with the given base URL.
    ///
    /// `base_url` is the origin only, for example `https://clob.polymarket.com`.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            limiter: Mutex::new(TokenBucket::new(manifest_rest_budget())),
            rate_breaker: Mutex::new(CircuitBreaker::new()),
        }
    }

    /// Create a new CLOB client, loading the base URL from
    /// `AETHER_VENUE__POLYMARKET_CLOB_URL` (falls back to the default
    /// `https://clob.polymarket.com`).
    pub fn from_env() -> Self {
        let base_url = std::env::var("AETHER_VENUE__POLYMARKET_CLOB_URL")
            .unwrap_or_else(|_| "https://clob.polymarket.com".to_string());
        Self::new(base_url)
    }

    /// Remaining requests in the pack-enforced REST budget.
    pub async fn rate_remaining(&self) -> u32 {
        let mut bucket = self.limiter.lock().await;
        bucket.refill();
        bucket.tokens.floor() as u32
    }

    /// Fetch an order-book snapshot for the given token / asset ID.
    ///
    /// The `token_id` is the Polymarket conditional token identifier (hex
    /// string) for the outcome.
    pub async fn get_order_book(&self, token_id: &str) -> Result<ClobBookSnapshot, ClobError> {
        let path = format!("/book?token_id={}", url_encode(token_id));
        let body = self.get_text(&path).await?;
        let raw: RawClobBook = serde_json::from_str(&body).map_err(|source| ClobError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })?;
        Ok(ClobBookSnapshot::from(raw))
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Acquire a rate-limit token, blocking until one is available.
    async fn acquire_rate_token(&self) {
        loop {
            let wait = {
                let mut bucket = self.limiter.lock().await;
                match bucket.try_take() {
                    None => return,
                    Some(w) => w,
                }
            };
            tokio::time::sleep(wait).await;
        }
    }

    /// Perform a GET request without authentication headers.
    ///
    /// The Polymarket CLOB API is public; no signing is needed.
    pub(crate) async fn get_text(&self, path: &str) -> Result<String, ClobError> {
        let url = format!("{}{}", self.base_url, path);
        for attempt in 0..=MAX_429_RETRIES {
            if !self.rate_breaker.lock().await.allow_request() {
                return Err(ClobError::RateLimitBreaker);
            }
            self.acquire_rate_token().await;
            let resp = self
                .http
                .get(&url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "aether-venue-polymarket/0.1.0")
                .header("Accept", "application/json")
                .send()
                .await?;
            let status = resp.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                self.rate_breaker.lock().await.record_failure();
                if attempt < MAX_429_RETRIES {
                    tokio::time::sleep(RetryPolicy::default().delay_for_attempt(attempt)).await;
                    continue;
                }
            }
            if !status.is_success() {
                return Err(ClobError::Api { status });
            }
            self.rate_breaker.lock().await.record_success();
            return Ok(resp.text().await?);
        }
        Err(ClobError::RateLimitBreaker)
    }
}

/// Minimal percent-encoding for path segments.
fn url_encode(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::ClobLevel;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn order_book_uses_documented_singular_endpoint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 4096];
            let count = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..count]).into_owned();
            let body = r#"{"market":"0xabc","asset_id":"abc/def","timestamp":"1752150896000","hash":"0xhash","bids":[],"asks":[],"min_order_size":"5","tick_size":"0.01","neg_risk":false}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            request
        });

        let client = ClobClient::new(format!("http://{address}"));
        let book = client.get_order_book("abc/def").await.unwrap();
        assert_eq!(book.asset_id, "abc/def");
        let request = server.await.unwrap();
        assert!(request.starts_with("GET /book?token_id=abc%2Fdef HTTP/1.1"));
    }

    #[test]
    fn deserialize_snapshot_with_all_fields() {
        // Official CLOB REST responses use snake_case.
        let json = r#"{
            "market": "0xabc123",
            "asset_id": "12345678901234567890",
            "timestamp": "1752150896789",
            "bids": [
                {"price": "0.65", "size": "100.5"},
                {"price": "0.64", "size": "200.0"}
            ],
            "asks": [
                {"price": "0.67", "size": "50.2"},
                {"price": "0.68", "size": "75.0"}
            ],
            "hash": "0xdeadbeef"
        }"#;

        let raw: RawClobBook = serde_json::from_str(json).unwrap();
        let snap = ClobBookSnapshot::from(raw);
        assert_eq!(snap.market, "0xabc123");
        assert_eq!(snap.asset_id, "12345678901234567890");
        assert_eq!(snap.timestamp, "1752150896789");
        assert_eq!(snap.bids.len(), 2);
        assert_eq!(snap.asks.len(), 2);
        assert_eq!(snap.bids[0].price, "0.65");
        assert_eq!(snap.bids[0].size, "100.5");
        assert_eq!(snap.asks[1].price, "0.68");
        assert_eq!(snap.hash.as_deref(), Some("0xdeadbeef"));
    }

    #[test]
    fn deserialize_snapshot_missing_identity_is_rejected() {
        let json = r#"{
            "bids": [],
            "asks": []
        }"#;

        assert!(serde_json::from_str::<RawClobBook>(json).is_err());
    }

    #[test]
    fn deserialize_empty_snapshot_is_rejected() {
        let json = r#"{}"#;
        assert!(serde_json::from_str::<RawClobBook>(json).is_err());
    }

    #[test]
    fn deserialize_level() {
        let json = r#"{"price": "0.50", "size": "99.9"}"#;
        let level: ClobLevel = serde_json::from_str(json).unwrap();
        assert_eq!(level.price, "0.50");
        assert_eq!(level.size, "99.9");
    }

    #[test]
    fn token_bucket_enforces_limit() {
        let budget = manifest_rest_budget();
        let mut bucket = TokenBucket::new(budget);
        for _ in 0..budget {
            assert!(bucket.try_take().is_none());
        }
        let wait = bucket.try_take().expect("request beyond budget must wait");
        assert!(wait > Duration::ZERO);
        assert!(wait <= Duration::from_secs(1));
    }

    #[test]
    fn clob_error_raw_payload() {
        let err = ClobError::Parse {
            detail: "bad json".into(),
            source: serde_json::from_str::<()>("").unwrap_err(),
            raw: b"garbage".to_vec(),
        };
        assert_eq!(err.raw_payload(), Some(&b"garbage"[..]));
    }

    #[test]
    fn clob_error_raw_payload_non_parse() {
        let err = ClobError::Api { status: reqwest::StatusCode::INTERNAL_SERVER_ERROR };
        assert!(err.raw_payload().is_none());
    }
}

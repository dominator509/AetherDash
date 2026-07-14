//! Kalshi REST API client.
//!
//! Provides an authenticated HTTP client for the Kalshi trade API v2.
//! Supports listing markets and fetching individual market details.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__KALSHI_BASE_URL` | Override API origin (default: demo) |
//! | `AETHER_VENUE__KALSHI_KEY_ID` | API key ID (see [`auth`]) |
//! | `AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH` | PEM key path (see [`auth`]) |

use crate::auth::{AuthError, KalshiAuth};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::Mutex;

const MAX_429_RETRIES: u32 = 3;
const RATE_BREAKER_THRESHOLD: u32 = 5;

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
        Ok(manifest) => manifest,
        Err(error) => panic!("embedded Kalshi venue manifest is invalid: {error}"),
    };
    assert!(manifest.rate_limits.rest_per_min > 0, "rest_per_min must be positive");
    manifest.rate_limits.rest_per_min
}

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

// ---------------------------------------------------------------------------
// Raw API response types
// ---------------------------------------------------------------------------

/// A single market as returned by the Kalshi REST API (v2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KalshiMarket {
    /// Unique ticker symbol, e.g. `"BTC-75"`.
    pub ticker: String,
    /// Human-readable title.
    pub title: String,
    /// Short display title.
    #[serde(default)]
    pub semi_title: Option<String>,
    /// Market status: `"open"`, `"closed"`, `"settled"`.
    pub status: String,
    /// Best ask price for Yes in cents (1-99).
    #[serde(default)]
    pub yes_ask: Option<i64>,
    /// Best bid price for Yes in cents (1-99).
    #[serde(default)]
    pub yes_bid: Option<i64>,
    /// Best ask price for No in cents (1-99).
    #[serde(default)]
    pub no_ask: Option<i64>,
    /// Best bid price for No in cents (1-99).
    #[serde(default)]
    pub no_bid: Option<i64>,
    /// Settlement outcome, e.g. `"yes"` or `"no"` when resolved.
    #[serde(default)]
    pub result: Option<String>,
    /// Unix millis when the market settled.
    #[serde(default)]
    pub settlement_ts: Option<serde_json::Value>,
    /// Unix millis when the market closed for trading.
    #[serde(default)]
    pub close_ts: Option<i64>,
    /// Current API close timestamp (RFC3339).
    #[serde(default)]
    pub close_time: Option<String>,
    /// Cumulative volume in contracts.
    #[serde(default)]
    pub volume: Option<i64>,
    /// Open interest in contracts.
    #[serde(default)]
    pub open_interest: Option<i64>,
    /// 24h volume.
    #[serde(default)]
    pub volume_24h: Option<i64>,
    /// 24h open interest.
    #[serde(default)]
    pub open_interest_24h: Option<i64>,
    /// Unix millis when the market was created.
    #[serde(default)]
    pub created_time: Option<serde_json::Value>,
    /// Tick size as `[min, max]` cents, e.g. `[1, 99]`.
    #[serde(default)]
    pub tick_size: Option<Vec<i64>>,
    /// Label for the Yes outcome.
    #[serde(default)]
    pub yes_sub_title: Option<String>,
    /// Label for the No outcome.
    #[serde(default)]
    pub no_sub_title: Option<String>,
    /// Current fixed-point dollar price fields.
    #[serde(default)]
    pub yes_ask_dollars: Option<String>,
    #[serde(default)]
    pub yes_bid_dollars: Option<String>,
    #[serde(default)]
    pub no_ask_dollars: Option<String>,
    #[serde(default)]
    pub no_bid_dollars: Option<String>,
    #[serde(default)]
    pub last_price_dollars: Option<String>,
}

/// Response wrapper for paginated market listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarketsResponse {
    /// Cursor for pagination.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Array of markets on this page.
    #[serde(default)]
    pub markets: Vec<KalshiMarket>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the Kalshi REST client.
#[derive(Error, Debug)]
pub enum ClientError {
    /// Authentication setup failed.
    #[error("auth error: {0}")]
    Auth(#[from] AuthError),

    /// HTTP request failed (network, timeout, etc.).
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned a non-success status code.
    #[error("Kalshi API returned HTTP {status}")]
    Api {
        /// HTTP status code.
        status: reqwest::StatusCode,
    },

    /// Failed to parse the API response JSON.
    #[error("response parse error: {detail}")]
    Parse {
        detail: String,
        #[source]
        source: serde_json::Error,
        /// Exact response bytes retained for quarantine; never rendered by Display.
        raw: Vec<u8>,
    },

    /// Repeated 429 responses opened the local rate-limit circuit breaker.
    #[error("Kalshi rate-limit circuit breaker is open")]
    RateLimitBreaker,
}

impl ClientError {
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

/// An authenticated HTTP client for the Kalshi trade API v2.
#[derive(Debug)]
pub struct KalshiClient {
    /// Base URL for all requests (prod or demo).
    base_url: String,
    /// Authentication handle.
    auth: KalshiAuth,
    /// Shared HTTP client.
    http: reqwest::Client,
    limiter: Mutex<TokenBucket>,
    consecutive_429s: AtomicU32,
}

impl KalshiClient {
    /// Create a new client from env-configured auth and the given base URL.
    ///
    /// `base_url` is the origin only (no `/trade-api/v2` suffix), for example
    /// `https://external-api.demo.kalshi.co`.
    pub fn new(base_url: impl Into<String>, auth: KalshiAuth) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            auth,
            http: reqwest::Client::new(),
            limiter: Mutex::new(TokenBucket::new(manifest_rest_budget())),
            consecutive_429s: AtomicU32::new(0),
        }
    }

    /// Create a new client, loading base URL from
    /// `AETHER_VENUE__KALSHI_BASE_URL` (falls back to the demo environment).
    pub fn from_env(auth: KalshiAuth) -> Self {
        let base_url = std::env::var("AETHER_VENUE__KALSHI_BASE_URL")
            .unwrap_or_else(|_| "https://external-api.demo.kalshi.co".to_string());
        Self::new(base_url, auth)
    }

    /// Whether order operations are safe for this configured endpoint.
    /// Production hosts are intentionally rejected inside the pack; the router
    /// remains the sole owner of any future live-order enablement.
    pub(crate) fn is_sandbox_endpoint(&self) -> bool {
        reqwest::Url::parse(&self.base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .is_some_and(|host| {
                host == "external-api.demo.kalshi.co" || host == "localhost" || host == "127.0.0.1"
            })
    }

    async fn acquire_rate_token(&self) {
        loop {
            let wait = {
                let mut bucket = self.limiter.lock().await;
                match bucket.try_take() {
                    None => return,
                    Some(wait) => wait,
                }
            };
            tokio::time::sleep(wait).await;
        }
    }

    /// Remaining requests in the pack-enforced REST budget.
    pub async fn rate_remaining(&self) -> u32 {
        let mut bucket = self.limiter.lock().await;
        bucket.refill();
        bucket.tokens.floor() as u32
    }

    /// Fetch a paginated list of markets.
    pub async fn get_markets(
        &self,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<MarketsResponse, ClientError> {
        let mut path = format!("/trade-api/v2/markets?limit={}", limit);
        if let Some(c) = cursor {
            path.push_str(&format!("&cursor={}", url_encode(c)));
        }

        let body = self.get_text(&path).await?;
        serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })
    }

    /// Fetch a single market by ticker symbol.
    pub async fn get_market(&self, ticker: &str) -> Result<KalshiMarket, ClientError> {
        let path = format!("/trade-api/v2/markets/{}", url_encode(ticker));
        let body = self.get_text(&path).await?;

        // Kalshi wraps single-market responses in an object with a `market` key
        #[derive(Deserialize)]
        struct MarketWrapper {
            market: KalshiMarket,
        }

        let wrapper: MarketWrapper = serde_json::from_str(&body).map_err(|source| {
            ClientError::Parse { detail: source.to_string(), source, raw: body.into_bytes() }
        })?;
        Ok(wrapper.market)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Perform a GET request with Kalshi auth headers and return the response
    /// body as text.
    pub(crate) async fn get_text(&self, path: &str) -> Result<String, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        for attempt in 0..=MAX_429_RETRIES {
            self.acquire_rate_token().await;
            let timestamp = chrono::Utc::now().timestamp_millis().to_string();
            let signature = self.auth.sign_request("GET", path, &timestamp)?;
            let resp = self
                .http
                .get(&url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "aether-venue-kalshi/0.1.0")
                .header("Accept", "application/json")
                .header("KALSHI-ACCESS-KEY", self.auth.key_id())
                .header("KALSHI-ACCESS-SIGNATURE", signature)
                .header("KALSHI-ACCESS-TIMESTAMP", &timestamp)
                .send()
                .await?;
            let status = resp.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let failures = self.consecutive_429s.fetch_add(1, Ordering::Relaxed) + 1;
                if failures >= RATE_BREAKER_THRESHOLD {
                    return Err(ClientError::RateLimitBreaker);
                }
                if attempt < MAX_429_RETRIES {
                    tokio::time::sleep(Duration::from_millis(200 * 2_u64.pow(attempt))).await;
                    continue;
                }
            }
            if !status.is_success() {
                return Err(ClientError::Api { status });
            }
            self.consecutive_429s.store(0, Ordering::Relaxed);
            return Ok(resp.text().await?);
        }
        Err(ClientError::RateLimitBreaker)
    }

    /// Perform a POST request with Kalshi auth headers and return the response
    /// body as text.
    ///
    /// The `body` parameter is the raw JSON string to send as the request body.
    pub(crate) async fn post_text(&self, path: &str, body: &str) -> Result<String, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        for attempt in 0..=MAX_429_RETRIES {
            self.acquire_rate_token().await;
            let timestamp = chrono::Utc::now().timestamp_millis().to_string();
            let signature = self.auth.sign_request("POST", path, &timestamp)?;
            let resp = self
                .http
                .post(&url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "aether-venue-kalshi/0.1.0")
                .header("Accept", "application/json")
                .header("KALSHI-ACCESS-KEY", self.auth.key_id())
                .header("KALSHI-ACCESS-SIGNATURE", signature)
                .header("KALSHI-ACCESS-TIMESTAMP", &timestamp)
                .body(body.to_string())
                .send()
                .await?;
            let status = resp.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let failures = self.consecutive_429s.fetch_add(1, Ordering::Relaxed) + 1;
                if failures >= RATE_BREAKER_THRESHOLD {
                    return Err(ClientError::RateLimitBreaker);
                }
                if attempt < MAX_429_RETRIES {
                    tokio::time::sleep(Duration::from_millis(200 * 2_u64.pow(attempt))).await;
                    continue;
                }
            }
            if !status.is_success() {
                return Err(ClientError::Api { status });
            }
            self.consecutive_429s.store(0, Ordering::Relaxed);
            return Ok(resp.text().await?);
        }
        Err(ClientError::RateLimitBreaker)
    }

    /// Perform an authenticated DELETE request.
    pub(crate) async fn delete_text(&self, path: &str) -> Result<String, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        for attempt in 0..=MAX_429_RETRIES {
            self.acquire_rate_token().await;
            let timestamp = chrono::Utc::now().timestamp_millis().to_string();
            let signature = self.auth.sign_request("DELETE", path, &timestamp)?;
            let response = self
                .http
                .delete(&url)
                .header("Accept", "application/json")
                .header("KALSHI-ACCESS-KEY", self.auth.key_id())
                .header("KALSHI-ACCESS-SIGNATURE", signature)
                .header("KALSHI-ACCESS-TIMESTAMP", timestamp)
                .send()
                .await?;
            let status = response.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let failures = self.consecutive_429s.fetch_add(1, Ordering::Relaxed) + 1;
                if failures >= RATE_BREAKER_THRESHOLD {
                    return Err(ClientError::RateLimitBreaker);
                }
                if attempt < MAX_429_RETRIES {
                    tokio::time::sleep(Duration::from_millis(200 * 2_u64.pow(attempt))).await;
                    continue;
                }
            }
            if !status.is_success() {
                return Err(ClientError::Api { status });
            }
            self.consecutive_429s.store(0, Ordering::Relaxed);
            return Ok(response.text().await?);
        }
        Err(ClientError::RateLimitBreaker)
    }
}

/// Minimal percent-encoding for path segments (covers ticker symbols with
/// special characters).
fn url_encode(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_market_with_all_fields() {
        let json = r#"{
            "ticker": "BTC-75",
            "title": "Will Bitcoin be above $75,000 at 4 PM ET?",
            "semi_title": "BTC > $75k?",
            "status": "open",
            "yes_ask": 48,
            "yes_bid": 47,
            "no_ask": 53,
            "no_bid": 52,
            "result": null,
            "settlement_ts": null,
            "close_ts": 1720000000000,
            "volume": 154230,
            "open_interest": 45200,
            "volume_24h": 12300,
            "open_interest_24h": 3200,
            "created_time": 1710000000000,
            "tick_size": [1, 99],
            "yes_sub_title": "Yes",
            "no_sub_title": "No"
        }"#;

        let m: KalshiMarket = serde_json::from_str(json).unwrap();
        assert_eq!(m.ticker, "BTC-75");
        assert_eq!(m.status, "open");
        assert_eq!(m.yes_ask, Some(48));
        assert_eq!(m.yes_bid, Some(47));
        assert_eq!(m.close_ts, Some(1_720_000_000_000));
        assert_eq!(m.tick_size, Some(vec![1, 99]));
        assert!(m.result.is_none());
    }

    #[test]
    fn deserialize_market_with_minimal_fields() {
        let json = r#"{
            "ticker": "TEST-1",
            "title": "Test market",
            "status": "closed"
        }"#;

        let m: KalshiMarket = serde_json::from_str(json).unwrap();
        assert_eq!(m.ticker, "TEST-1");
        assert_eq!(m.status, "closed");
        assert!(m.yes_ask.is_none());
        assert!(m.volume.is_none());
    }

    #[test]
    fn deserialize_markets_response() {
        let json = r#"{
            "cursor": "next_page_cursor",
            "markets": [
                {
                    "ticker": "BTC-75",
                    "title": "Bitcoin test",
                    "status": "open"
                },
                {
                    "ticker": "ETH-50",
                    "title": "Ethereum test",
                    "status": "open"
                }
            ]
        }"#;

        let resp: MarketsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.cursor, Some("next_page_cursor".into()));
        assert_eq!(resp.markets.len(), 2);
        assert_eq!(resp.markets[0].ticker, "BTC-75");
    }

    #[test]
    fn deserialize_markets_response_with_empty_list() {
        let json = r#"{"markets": []}"#;
        let resp: MarketsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.markets.is_empty());
        assert!(resp.cursor.is_none());
    }

    #[test]
    fn deserialize_current_fixed_point_market_fields() {
        let json = r#"{
            "ticker":"FED-23DEC-T3.00",
            "title":"Federal funds target",
            "status":"open",
            "close_time":"2026-07-13T20:00:00Z",
            "settlement_ts":"2026-07-13T21:00:00Z",
            "yes_bid_dollars":"0.4500",
            "yes_ask_dollars":"0.5300",
            "last_price_dollars":"0.4800"
        }"#;
        let market: KalshiMarket = serde_json::from_str(json).unwrap();
        assert_eq!(market.close_time.as_deref(), Some("2026-07-13T20:00:00Z"));
        assert_eq!(market.yes_bid_dollars.as_deref(), Some("0.4500"));
        assert_eq!(market.settlement_ts, Some(serde_json::json!("2026-07-13T21:00:00Z")));
    }

    #[test]
    fn manifest_rest_budget_is_enforced_before_refill() {
        let budget = manifest_rest_budget();
        let mut bucket = TokenBucket::new(budget);
        for _ in 0..budget {
            assert!(bucket.try_take().is_none());
        }
        let wait = bucket.try_take().expect("request 101 must wait for a refill token");
        assert!(wait > Duration::ZERO);
        assert!(wait <= Duration::from_secs(1));
    }
}

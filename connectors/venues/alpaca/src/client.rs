//! Alpaca REST API client.
//!
//! Provides an authenticated HTTP client for the Alpaca Trading API v2 and
//! Data API v2. Supports listing assets, fetching account info, submitting
//! orders, and retrieving market-data snapshots.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__ALPACA_BASE_URL` | Override REST API origin (default: paper) |
//! | `AETHER_VENUE__ALPACA_DATA_URL` | Override Data API origin (default: data) |
//! | `AETHER_VENUE__ALPACA_KEY_ID` | API key ID (see [`auth`]) |
//! | `AETHER_VENUE__ALPACA_SECRET` | API secret key (see [`auth`]) |

use crate::auth::{AlpacaAuth, AuthError};
use aether_bus::retry::{CircuitBreaker, RetryPolicy};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::Mutex;

const MAX_429_RETRIES: u32 = 3;

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
        Err(error) => panic!("embedded Alpaca venue manifest is invalid: {error}"),
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

/// A single asset as returned by the Alpaca REST API (v2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AlpacaAsset {
    /// Asset UUID.
    pub id: String,
    /// Asset class: `"us_equity"` or `"crypto"`.
    #[serde(rename = "class")]
    pub asset_class: String,
    /// Exchange (e.g. `"NASDAQ"`, `"NYSE"`, `"OTC"`).
    #[serde(default)]
    pub exchange: String,
    /// Ticker symbol (e.g. `"AAPL"`).
    pub symbol: String,
    /// Official full name of the asset.
    #[serde(default)]
    pub name: String,
    /// Status: `"active"` or `"inactive"`.
    #[serde(default)]
    pub status: String,
    /// Whether the asset is tradable on Alpaca.
    #[serde(default)]
    pub tradable: bool,
    /// Whether the asset is marginable.
    #[serde(default)]
    pub marginable: bool,
    /// Whether the asset is shortable.
    #[serde(default)]
    pub shortable: bool,
    /// Whether the asset is easy to borrow.
    #[serde(default)]
    pub easy_to_borrow: bool,
    /// Whether the asset supports fractional shares.
    #[serde(default)]
    pub fractionable: bool,
    /// Maintenance margin requirement % (equities only).
    #[serde(default)]
    pub maintenance_margin_requirement: Option<i64>,
}

/// Account information from GET /v2/account.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AlpacaAccount {
    /// Account UUID.
    pub id: String,
    /// Account status: `"ACTIVE"`, `"ONBOARDING"`, etc.
    #[serde(default)]
    pub status: String,
    /// Currency (e.g. `"USD"`).
    #[serde(default)]
    pub currency: String,
    /// Buying power (depends on multiplier).
    #[serde(default)]
    pub buying_power: String,
    /// Regulation T buying power.
    #[serde(default)]
    pub regt_buying_power: Option<String>,
    /// Day-trading buying power.
    #[serde(default)]
    pub daytrading_buying_power: Option<String>,
    /// Cash balance.
    #[serde(default)]
    pub cash: String,
    /// Total portfolio value.
    #[serde(default)]
    pub portfolio_value: String,
    /// Current equity.
    #[serde(default)]
    pub equity: String,
    /// Equity as of last market close.
    #[serde(default)]
    pub last_equity: String,
    /// Long position market value.
    #[serde(default)]
    pub long_market_value: String,
    /// Short position market value.
    #[serde(default)]
    pub short_market_value: String,
    /// Initial margin requirement.
    #[serde(default)]
    pub initial_margin: String,
    /// Maintenance margin requirement.
    #[serde(default)]
    pub maintenance_margin: String,
    /// Pattern day trader flag.
    #[serde(default)]
    pub pattern_day_trader: bool,
    /// Whether shorting is enabled.
    #[serde(default)]
    pub shorting_enabled: bool,
    /// Whether trading is suspended by user.
    #[serde(default)]
    pub trade_suspended_by_user: bool,
    /// Whether trading is blocked.
    #[serde(default)]
    pub trading_blocked: bool,
    /// Whether transfers are blocked.
    #[serde(default)]
    pub transfers_blocked: bool,
    /// Whether the account is blocked.
    #[serde(default)]
    pub account_blocked: bool,
    /// Free-form account number.
    #[serde(default)]
    pub account_number: String,
    /// Day trade count in the last 5 days.
    #[serde(default)]
    pub daytrade_count: i64,
    /// Margin multiplier.
    #[serde(default)]
    pub multiplier: String,
}

// ---------------------------------------------------------------------------
// Snapshot types (Data API v2)
// ---------------------------------------------------------------------------

/// A single trade from the Alpaca Data API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlpacaTrade {
    /// Timestamp (RFC 3339).
    #[serde(default)]
    pub t: Option<String>,
    /// Exchange code.
    #[serde(default)]
    pub x: Option<String>,
    /// Price, preserved as decimal text.
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub p: Option<String>,
    /// Size (shares), preserved as decimal text.
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub s: Option<String>,
    /// Trade conditions.
    #[serde(default)]
    pub c: Option<Vec<String>>,
    /// Trade ID.
    #[serde(default)]
    pub i: Option<i64>,
    /// Tape (C, N, B, or D).
    #[serde(default)]
    pub z: Option<String>,
}

/// A single quote from the Alpaca Data API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlpacaQuote {
    /// Timestamp (RFC 3339).
    #[serde(default)]
    pub t: Option<String>,
    /// Ask exchange.
    #[serde(default)]
    pub ax: Option<String>,
    /// Ask price, preserved as decimal text.
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub ap: Option<String>,
    /// Ask size, preserved as decimal text.
    #[serde(default, rename = "as", deserialize_with = "deserialize_optional_decimal_string")]
    pub ask_size: Option<String>,
    /// Bid exchange.
    #[serde(default)]
    pub bx: Option<String>,
    /// Bid price, preserved as decimal text.
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub bp: Option<String>,
    /// Bid size, preserved as decimal text.
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub bs: Option<String>,
    /// Quote conditions.
    #[serde(default)]
    pub c: Option<Vec<String>>,
}

/// A bar from the Alpaca Data API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlpacaBar {
    /// Timestamp (RFC 3339).
    #[serde(default)]
    pub t: Option<String>,
    /// Open price.
    #[serde(default)]
    pub o: Option<f64>,
    /// High price.
    #[serde(default)]
    pub h: Option<f64>,
    /// Low price.
    #[serde(default)]
    pub l: Option<f64>,
    /// Close price.
    #[serde(default)]
    pub c: Option<f64>,
    /// Volume.
    #[serde(default)]
    pub v: Option<f64>,
    /// Number of trades.
    #[serde(default)]
    pub n: Option<i64>,
    /// Volume-weighted average price.
    #[serde(default)]
    pub vw: Option<f64>,
}

fn deserialize_optional_decimal_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(serde_json::Value::Number(value)) => Ok(Some(value.to_string())),
        Some(other) => {
            Err(serde::de::Error::custom(format!("expected decimal string or number, got {other}")))
        }
    }
}

/// A snapshot from the Alpaca Data API (GET /v2/stocks/{symbol}/snapshot).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlpacaSnapshot {
    /// Ticker symbol.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Latest trade.
    #[serde(default)]
    pub latest_trade: Option<AlpacaTrade>,
    /// Latest quote.
    #[serde(default)]
    pub latest_quote: Option<AlpacaQuote>,
    /// Current minute bar.
    #[serde(default)]
    pub minute_bar: Option<AlpacaBar>,
    /// Current daily bar.
    #[serde(default)]
    pub daily_bar: Option<AlpacaBar>,
    /// Previous daily bar.
    #[serde(default)]
    pub prev_daily_bar: Option<AlpacaBar>,
}

// ---------------------------------------------------------------------------
// Order types
// ---------------------------------------------------------------------------

/// Request body for POST /v2/orders.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AlpacaOrderRequest {
    pub symbol: String,
    pub side: String,
    pub qty: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub time_in_force: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended_hours: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
}

/// Response from POST /v2/orders.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AlpacaOrderResponse {
    /// Order ID assigned by Alpaca.
    pub id: String,
    /// Client-supplied order ID.
    #[serde(default)]
    pub client_order_id: Option<String>,
    /// Order status: `"accepted"`, `"filled"`, `"canceled"`, etc.
    #[serde(default)]
    pub status: Option<String>,
    /// Filled quantity (string).
    #[serde(default)]
    pub filled_qty: Option<String>,
    /// Filled average price (string).
    #[serde(default)]
    pub filled_avg_price: Option<String>,
    /// Order symbol.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Order side.
    #[serde(default)]
    pub side: Option<String>,
    /// Order type.
    #[serde(rename = "type", default)]
    pub order_type: Option<String>,
    /// Limit price.
    #[serde(default)]
    pub limit_price: Option<String>,
    /// Stop price.
    #[serde(default)]
    pub stop_price: Option<String>,
    /// Time in force.
    #[serde(default)]
    pub time_in_force: Option<String>,
    /// Quantity ordered.
    #[serde(default)]
    pub qty: Option<String>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the Alpaca REST client.
#[derive(Error, Debug)]
pub enum ClientError {
    /// Authentication setup failed.
    #[error("auth error: {0}")]
    Auth(#[from] AuthError),

    /// HTTP request failed (network, timeout, etc.).
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned a non-success status code.
    #[error("Alpaca API returned HTTP {status}: {body}")]
    Api {
        /// HTTP status code.
        status: reqwest::StatusCode,
        /// Response body.
        body: String,
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
    #[error("Alpaca rate-limit circuit breaker is open")]
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

/// An authenticated HTTP client for the Alpaca Trading and Data APIs.
#[derive(Debug)]
pub struct AlpacaClient {
    /// Base URL for trading REST API (paper by default).
    base_url: String,
    /// Base URL for Data API v2.
    data_url: String,
    /// Authentication handle.
    auth: AlpacaAuth,
    /// Shared HTTP client.
    http: reqwest::Client,
    limiter: Mutex<TokenBucket>,
    rate_breaker: Mutex<CircuitBreaker>,
}

impl AlpacaClient {
    /// Create a new client with the given URLs and auth.
    pub fn new(base_url: impl Into<String>, data_url: impl Into<String>, auth: AlpacaAuth) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            data_url: data_url.into().trim_end_matches('/').to_string(),
            auth,
            http: reqwest::Client::new(),
            limiter: Mutex::new(TokenBucket::new(manifest_rest_budget())),
            rate_breaker: Mutex::new(CircuitBreaker::new()),
        }
    }

    /// Create a new client, loading base URL from
    /// `AETHER_VENUE__ALPACA_BASE_URL` (falls back to paper) and data URL from
    /// `AETHER_VENUE__ALPACA_DATA_URL` (falls back to data.alpaca.markets).
    pub fn from_env(auth: AlpacaAuth) -> Self {
        let base_url = std::env::var("AETHER_VENUE__ALPACA_BASE_URL")
            .unwrap_or_else(|_| "https://paper-api.alpaca.markets".to_string());
        let data_url = std::env::var("AETHER_VENUE__ALPACA_DATA_URL")
            .unwrap_or_else(|_| "https://data.alpaca.markets".to_string());
        Self::new(base_url, data_url, auth)
    }

    /// Whether order operations are safe for this configured endpoint.
    /// Paper trading pack rejects production hosts.
    pub(crate) fn is_sandbox_endpoint(&self) -> bool {
        reqwest::Url::parse(&self.base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .is_some_and(|host| {
                host == "paper-api.alpaca.markets" || host == "localhost" || host == "127.0.0.1"
            })
    }

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("apca-api-key-id"),
            #[allow(clippy::expect_used)]
            HeaderValue::from_str(self.auth.key_id()).expect("valid API key ID header"),
        );
        headers.insert(
            HeaderName::from_static("apca-api-secret-key"),
            #[allow(clippy::expect_used)]
            HeaderValue::from_str(self.auth.secret_key()).expect("valid API secret header"),
        );
        headers
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

    // -----------------------------------------------------------------------
    // Asset endpoints
    // -----------------------------------------------------------------------

    /// List all tradeable assets (GET /v2/assets).
    ///
    /// Filters for `status=active` and `asset_class=us_equity` by default.
    pub async fn list_assets(
        &self,
        status: Option<&str>,
        asset_class: Option<&str>,
    ) -> Result<Vec<AlpacaAsset>, ClientError> {
        let mut path = "/v2/assets".to_string();
        let mut params = Vec::new();
        if let Some(s) = status {
            params.push(format!("status={}", url_encode(s)));
        }
        if let Some(c) = asset_class {
            params.push(format!("asset_class={}", url_encode(c)));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }

        let body = self.get_text(&path).await?;
        serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })
    }

    /// Get a single asset by symbol (GET /v2/assets/{symbol}).
    pub async fn get_asset(&self, symbol: &str) -> Result<AlpacaAsset, ClientError> {
        let path = format!("/v2/assets/{}", url_encode(symbol));
        let body = self.get_text(&path).await?;
        serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })
    }

    // -----------------------------------------------------------------------
    // Snapshot / market data
    // -----------------------------------------------------------------------

    /// Get a snapshot for a stock symbol (GET /v2/stocks/{symbol}/snapshot).
    pub async fn get_snapshot(&self, symbol: &str) -> Result<AlpacaSnapshot, ClientError> {
        let path = format!("/v2/stocks/{}/snapshot", url_encode(symbol));
        let body = self.data_get_text(&path).await?;
        serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })
    }

    // -----------------------------------------------------------------------
    // Account endpoints
    // -----------------------------------------------------------------------

    /// Get account information (GET /v2/account).
    pub async fn get_account(&self) -> Result<AlpacaAccount, ClientError> {
        let body = self.get_text("/v2/account").await?;
        serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })
    }

    // -----------------------------------------------------------------------
    // Order endpoints
    // -----------------------------------------------------------------------

    /// Submit an order (POST /v2/orders).
    pub async fn submit_order(
        &self,
        order: &AlpacaOrderRequest,
    ) -> Result<AlpacaOrderResponse, ClientError> {
        let json_body = serde_json::to_string(order).map_err(|e| ClientError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            body: e.to_string(),
        })?;
        let body = self.post_text("/v2/orders", &json_body).await?;
        serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })
    }

    /// Cancel an order by its venue-side ID (DELETE /v2/orders/{id}).
    pub async fn cancel_order(&self, order_id: &str) -> Result<(), ClientError> {
        let path = format!("/v2/orders/{}", url_encode(order_id));
        self.delete_text(&path).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Perform a GET request against the trading API with Alpaca auth headers.
    pub(crate) async fn get_text(&self, path: &str) -> Result<String, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let auth_headers = self.auth_headers();
        for attempt in 0..=MAX_429_RETRIES {
            if !self.rate_breaker.lock().await.allow_request() {
                return Err(ClientError::RateLimitBreaker);
            }
            self.acquire_rate_token().await;
            let resp = self
                .http
                .get(&url)
                .headers(auth_headers.clone())
                .header("Content-Type", "application/json")
                .header("User-Agent", "aether-venue-alpaca/0.1.0")
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
                let body = resp.text().await.unwrap_or_default();
                return Err(ClientError::Api { status, body });
            }
            self.rate_breaker.lock().await.record_success();
            return Ok(resp.text().await?);
        }
        Err(ClientError::RateLimitBreaker)
    }

    /// Perform a GET request against the Data API.
    pub(crate) async fn data_get_text(&self, path: &str) -> Result<String, ClientError> {
        let url = format!("{}{}", self.data_url, path);
        let auth_headers = self.auth_headers();
        for attempt in 0..=MAX_429_RETRIES {
            if !self.rate_breaker.lock().await.allow_request() {
                return Err(ClientError::RateLimitBreaker);
            }
            self.acquire_rate_token().await;
            let resp = self
                .http
                .get(&url)
                .headers(auth_headers.clone())
                .header("Content-Type", "application/json")
                .header("User-Agent", "aether-venue-alpaca/0.1.0")
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
                let body = resp.text().await.unwrap_or_default();
                return Err(ClientError::Api { status, body });
            }
            self.rate_breaker.lock().await.record_success();
            return Ok(resp.text().await?);
        }
        Err(ClientError::RateLimitBreaker)
    }

    /// Perform a POST request with Alpaca auth headers.
    pub(crate) async fn post_text(&self, path: &str, body: &str) -> Result<String, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let auth_headers = self.auth_headers();
        for attempt in 0..=MAX_429_RETRIES {
            if !self.rate_breaker.lock().await.allow_request() {
                return Err(ClientError::RateLimitBreaker);
            }
            self.acquire_rate_token().await;
            let resp = self
                .http
                .post(&url)
                .headers(auth_headers.clone())
                .header("Content-Type", "application/json")
                .header("User-Agent", "aether-venue-alpaca/0.1.0")
                .header("Accept", "application/json")
                .body(body.to_string())
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
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(ClientError::Api { status, body: resp_body });
            }
            self.rate_breaker.lock().await.record_success();
            return Ok(resp.text().await?);
        }
        Err(ClientError::RateLimitBreaker)
    }

    /// Perform a DELETE request with Alpaca auth headers.
    pub(crate) async fn delete_text(&self, path: &str) -> Result<(), ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let auth_headers = self.auth_headers();
        for attempt in 0..=MAX_429_RETRIES {
            if !self.rate_breaker.lock().await.allow_request() {
                return Err(ClientError::RateLimitBreaker);
            }
            self.acquire_rate_token().await;
            let resp = self
                .http
                .delete(&url)
                .headers(auth_headers.clone())
                .header("Accept", "application/json")
                .header("User-Agent", "aether-venue-alpaca/0.1.0")
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
                let body = resp.text().await.unwrap_or_default();
                return Err(ClientError::Api { status, body });
            }
            self.rate_breaker.lock().await.record_success();
            return Ok(());
        }
        Err(ClientError::RateLimitBreaker)
    }
}

/// Minimal percent-encoding for path segments.
fn url_encode(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserialize_asset_with_all_fields() {
        let json = r#"{
            "id": "904837e3-3b76-47ec-b432-046db621571b",
            "class": "us_equity",
            "exchange": "NASDAQ",
            "symbol": "AAPL",
            "name": "Apple Inc. Common Stock",
            "status": "active",
            "tradable": true,
            "marginable": true,
            "shortable": true,
            "easy_to_borrow": true,
            "fractionable": true,
            "maintenance_margin_requirement": 30
        }"#;

        let asset: AlpacaAsset = serde_json::from_str(json).unwrap();
        assert_eq!(asset.symbol, "AAPL");
        assert_eq!(asset.asset_class, "us_equity");
        assert_eq!(asset.exchange, "NASDAQ");
        assert!(asset.tradable);
        assert!(asset.marginable);
        assert!(asset.shortable);
        assert!(asset.fractionable);
        assert_eq!(asset.maintenance_margin_requirement, Some(30));
    }

    #[test]
    fn deserialize_asset_with_minimal_fields() {
        let json = r#"{
            "id": "abc-123",
            "class": "us_equity",
            "symbol": "SPY",
            "status": "active",
            "tradable": true
        }"#;

        let asset: AlpacaAsset = serde_json::from_str(json).unwrap();
        assert_eq!(asset.symbol, "SPY");
        assert!(asset.tradable);
        assert!(!asset.shortable); // default false
    }

    #[test]
    fn deserialize_account() {
        let json = r#"{
            "id": "d905b07d-240c-4c07-9bb5-707820aae345",
            "account_number": "XXXXXXXXXX",
            "status": "ACTIVE",
            "currency": "USD",
            "buying_power": "400000",
            "cash": "100000",
            "portfolio_value": "100000",
            "equity": "100000",
            "last_equity": "100000",
            "long_market_value": "0",
            "short_market_value": "0",
            "initial_margin": "0",
            "maintenance_margin": "0",
            "multiplier": "4",
            "pattern_day_trader": false,
            "shorting_enabled": true,
            "trade_suspended_by_user": false,
            "trading_blocked": false,
            "transfers_blocked": false,
            "account_blocked": false,
            "daytrade_count": 0
        }"#;

        let acct: AlpacaAccount = serde_json::from_str(json).unwrap();
        assert_eq!(acct.status, "ACTIVE");
        assert_eq!(acct.currency, "USD");
        assert_eq!(acct.buying_power, "400000");
        assert_eq!(acct.cash, "100000");
        assert_eq!(acct.equity, "100000");
    }

    #[test]
    fn deserialize_snapshot() {
        let json = json!({
            "symbol": "AAPL",
            "latestTrade": {
                "t": "2021-05-11T20:00:00.435997104Z",
                "x": "Q",
                "p": 125.91,
                "s": 5589631,
                "i": 179430,
                "z": "C"
            },
            "latestQuote": {
                "t": "2021-05-11T22:05:02.307304704Z",
                "ax": "P",
                "ap": 125.68,
                "as": 12,
                "bx": "P",
                "bp": 125.6,
                "bs": 4
            },
            "dailyBar": {
                "t": "2021-05-11T04:00:00Z",
                "o": 123.5,
                "h": 126.27,
                "l": 122.77,
                "c": 125.91,
                "v": 125863164
            }
        });

        let snap: AlpacaSnapshot = serde_json::from_value(json).unwrap();
        assert_eq!(snap.symbol.as_deref(), Some("AAPL"));
        assert!(snap.latest_trade.is_some());
        assert!(snap.latest_quote.is_some());
        assert!(snap.daily_bar.is_some());
        assert!(snap.minute_bar.is_none());

        let trade = snap.latest_trade.unwrap();
        assert_eq!(trade.p.as_deref(), Some("125.91"));
    }

    #[test]
    fn deserialize_order_response() {
        let json = r#"{
            "id": "61e8e8f8-8f8f-8f8f-8f8f-8f8f8f8f8f8f",
            "client_order_id": "test-123",
            "status": "accepted",
            "symbol": "AAPL",
            "side": "buy",
            "type": "market",
            "qty": "10",
            "time_in_force": "day"
        }"#;

        let resp: AlpacaOrderResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "61e8e8f8-8f8f-8f8f-8f8f-8f8f8f8f8f8f");
        assert_eq!(resp.status.as_deref(), Some("accepted"));
        assert_eq!(resp.symbol.as_deref(), Some("AAPL"));
        assert_eq!(resp.side.as_deref(), Some("buy"));
    }

    #[test]
    fn manifest_rest_budget_is_enforced_before_refill() {
        let budget = manifest_rest_budget();
        let mut bucket = TokenBucket::new(budget);
        for _ in 0..budget {
            assert!(bucket.try_take().is_none());
        }
        let wait = bucket.try_take().expect("request 201 must wait for a refill token");
        assert!(wait > Duration::ZERO);
        assert!(wait <= Duration::from_secs(1));
    }
}

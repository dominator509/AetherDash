//! Polymarket Gamma API REST client.
//!
//! Provides an unauthenticated HTTP client for the Gamma API market discovery
//! endpoints.  Supports listing markets with offset-based pagination, fetching
//! by ID, and fetching by slug.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__POLYMARKET_GAMMA_URL` | Override Gamma API origin (default: prod) |

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
        Ok(manifest) => manifest,
        Err(error) => panic!("embedded Polymarket venue manifest is invalid: {error}"),
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

/// A single market as returned by the Polymarket Gamma API.
///
/// Gamma uses camelCase JSON.  String-encoded arrays like `outcomePrices` and
/// `clobTokenIds` are retained as raw strings and parsed by the normalization
/// layer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GammaMarket {
    /// Unique numeric identifier, e.g. `"572481"`.
    pub id: String,
    /// Human-readable question / title.
    pub question: String,
    /// URL-friendly slug, e.g. `"will-trump-nominate-scott-bessent..."`.
    pub slug: String,
    /// JSON-encoded array of outcome names, e.g. `"[\"Yes\",\"No\"]"`.
    pub outcomes: String,
    /// JSON-encoded array of decimal probability strings,
    /// e.g. `"[\"0.0015\",\"0.9985\"]"`.
    pub outcome_prices: String,
    /// JSON-encoded array of CLOB token IDs (hex),
    /// e.g. `"[\"0x...\",\"0x...\"]"`.
    pub clob_token_ids: String,
    /// Trading volume in the last 24 hours (in USDC).
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub volume24hr: Option<String>,
    /// All-time volume (in USDC).
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub volume_num: Option<String>,
    /// Current liquidity (in USDC).
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub liquidity_num: Option<String>,
    /// ISO 8601 end date, e.g. `"2026-12-31T00:00:00Z"`.
    pub end_date: Option<String>,
    /// Whether the market is currently active and displayed.
    pub active: bool,
    /// Whether the market has closed.
    pub closed: bool,
    /// Whether orders are currently being accepted.
    pub accepting_orders: Option<bool>,
    /// Whether this market uses the neg-risk (no-arb) framework.
    pub neg_risk: Option<bool>,
    /// On-chain condition ID (`0x` + 64 hex chars).
    pub condition_id: Option<String>,
    /// Minimum tick size for order prices.
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub order_price_min_tick_size: Option<String>,
    /// Minimum order size (number of contracts).
    #[serde(default, deserialize_with = "deserialize_optional_decimal_string")]
    pub order_min_size: Option<String>,
    /// Market description or rules text.
    pub description: Option<String>,
    /// ISO 8601 start date.
    pub start_date: Option<String>,
    /// Primary category, e.g. `"Politics"`.
    pub category: Option<String>,
    /// Tags for this market.
    pub tags: Option<Vec<String>>,
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

/// Gamma returns markets as a top-level JSON array (not wrapped in an object).
pub type MarketsResponse = Vec<GammaMarket>;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the Gamma REST client.
#[derive(Error, Debug)]
pub enum ClientError {
    /// HTTP request failed (network, timeout, etc.).
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned a non-success status code.
    #[error("Polymarket Gamma API returned HTTP {status}")]
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
    #[error("Polymarket rate-limit circuit breaker is open")]
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

/// An unauthenticated HTTP client for the Polymarket Gamma API.
///
/// Read-only by design — no order operations are exposed or implemented.
#[derive(Debug)]
pub struct GammaClient {
    /// Base URL for all requests (defaults to production).
    base_url: String,
    /// Shared HTTP client.
    http: reqwest::Client,
    limiter: Mutex<TokenBucket>,
    rate_breaker: Mutex<CircuitBreaker>,
}

impl GammaClient {
    /// Create a new client with the given base URL.
    ///
    /// `base_url` is the origin only, for example
    /// `https://gamma-api.polymarket.com`.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            limiter: Mutex::new(TokenBucket::new(manifest_rest_budget())),
            rate_breaker: Mutex::new(CircuitBreaker::new()),
        }
    }

    /// Create a new client, loading the base URL from
    /// `AETHER_VENUE__POLYMARKET_GAMMA_URL` (falls back to the production
    /// endpoint).
    pub fn from_env() -> Self {
        let base_url = std::env::var("AETHER_VENUE__POLYMARKET_GAMMA_URL")
            .unwrap_or_else(|_| "https://gamma-api.polymarket.com".to_string());
        Self::new(base_url)
    }

    /// Block until a rate-limit token is available.
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
    ///
    /// Gamma uses offset-based pagination.  The response is a top-level JSON
    /// array of [`GammaMarket`] items.
    pub async fn get_markets(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<MarketsResponse, ClientError> {
        let path = format!("/markets?limit={}&offset={}", limit, offset);
        let body = self.get_text(&path).await?;
        serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })
    }

    /// Fetch a single market by its numeric ID.
    ///
    /// Gamma may return the market directly as a JSON object or wrapped in
    /// `{"market": {...}}`.  Both forms are handled.
    pub async fn get_market_by_id(&self, id: &str) -> Result<GammaMarket, ClientError> {
        let path = format!("/markets/{}", id);
        let body = self.get_text(&path).await?;

        // Try the wrapped form first, then direct deserialization.
        #[derive(Deserialize)]
        struct MarketWrapper {
            market: GammaMarket,
        }

        serde_json::from_str::<MarketWrapper>(&body).map(|w| w.market).or_else(|_| {
            serde_json::from_str(&body).map_err(|source| ClientError::Parse {
                detail: source.to_string(),
                source,
                raw: body.into_bytes(),
            })
        })
    }

    /// Fetch markets matching a slug and return the first match, if any.
    pub async fn get_market_by_slug(&self, slug: &str) -> Result<Option<GammaMarket>, ClientError> {
        let path = format!("/markets/slug/{}", urlencoding::encode(slug));
        let body = self.get_text(&path).await?;
        let market = serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })?;
        Ok(Some(market))
    }

    /// Fetch the Gamma market containing one CLOB outcome token.
    pub async fn get_market_by_token_id(
        &self,
        token_id: &str,
    ) -> Result<Option<GammaMarket>, ClientError> {
        let path = format!("/markets?clob_token_ids={}&limit=1", urlencoding::encode(token_id));
        let body = self.get_text(&path).await?;
        let markets: Vec<GammaMarket> = serde_json::from_str(&body).map_err(|source| {
            ClientError::Parse { detail: source.to_string(), source, raw: body.into_bytes() }
        })?;
        Ok(markets.into_iter().next())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Perform an unauthenticated GET request and return the response body as
    /// text.
    ///
    /// Retries on HTTP 429 with exponential backoff and opens a circuit
    /// breaker after the shared SPEC-006 failure threshold.
    async fn get_text(&self, path: &str) -> Result<String, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        for attempt in 0..=MAX_429_RETRIES {
            if !self.rate_breaker.lock().await.allow_request() {
                return Err(ClientError::RateLimitBreaker);
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
                return Err(ClientError::Api { status });
            }
            self.rate_breaker.lock().await.record_success();
            return Ok(resp.text().await?);
        }
        Err(ClientError::RateLimitBreaker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn token_lookup_uses_documented_query_parameter() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 4096];
            let count = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..count]).into_owned();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]")
                .await
                .unwrap();
            request
        });

        let client = GammaClient::new(format!("http://{address}"));
        assert!(client.get_market_by_token_id("abc/def").await.unwrap().is_none());
        let request = server.await.unwrap();
        assert!(request.starts_with("GET /markets?clob_token_ids=abc%2Fdef&limit=1 HTTP/1.1"));
    }

    #[test]
    fn deserialize_gamma_market_with_all_fields() {
        let json = r#"{
            "id": "572481",
            "question": "Will Trump nominate Scott Bessent as the next Fed chair?",
            "slug": "will-trump-nominate-scott-bessent-as-the-next-fed-chair",
            "outcomes": "[\"Yes\",\"No\"]",
            "outcomePrices": "[\"0.0015\",\"0.9985\"]",
            "clobTokenIds": "[\"10749123456789abcdef\",\"88386123456789abcdef\"]",
            "volume24hr": 11394218.4,
            "volumeNum": 35997330.32,
            "liquidityNum": 1270782.68,
            "endDate": "2026-12-31T00:00:00Z",
            "active": true,
            "closed": false,
            "acceptingOrders": true,
            "negRisk": true,
            "conditionId": "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            "orderPriceMinTickSize": 0.001,
            "orderMinSize": 5,
            "description": "This market resolves to Yes if Scott Bessent is nominated as Fed chair by Donald Trump before the end date.",
            "startDate": "2026-01-01T00:00:00Z",
            "category": "Politics",
            "tags": ["Trump", "Fed"]
        }"#;

        let m: GammaMarket = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "572481");
        assert_eq!(m.question, "Will Trump nominate Scott Bessent as the next Fed chair?");
        assert_eq!(m.slug, "will-trump-nominate-scott-bessent-as-the-next-fed-chair");
        assert!(m.outcomes.contains("Yes"));
        assert!(m.outcome_prices.contains("0.0015"));
        assert!(m.clob_token_ids.contains("10749"));
        assert_eq!(m.volume24hr.as_deref(), Some("11394218.4"));
        assert_eq!(m.volume_num.as_deref(), Some("35997330.32"));
        assert_eq!(m.liquidity_num.as_deref(), Some("1270782.68"));
        assert_eq!(m.end_date.as_deref(), Some("2026-12-31T00:00:00Z"));
        assert!(m.active);
        assert!(!m.closed);
        assert_eq!(m.accepting_orders, Some(true));
        assert_eq!(m.neg_risk, Some(true));
        assert!(m.condition_id.as_deref().unwrap_or("").starts_with("0x"));
        assert_eq!(m.order_price_min_tick_size.as_deref(), Some("0.001"));
        assert_eq!(m.order_min_size.as_deref(), Some("5"));
        assert!(m.description.as_deref().unwrap_or("").contains("Scott Bessent"));
        assert_eq!(m.start_date.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(m.category.as_deref(), Some("Politics"));
        assert_eq!(m.tags, Some(vec!["Trump".to_string(), "Fed".to_string()]));
    }

    #[test]
    fn deserialize_minimal_gamma_market() {
        let json = r#"{
            "id": "1",
            "question": "Test?",
            "slug": "test",
            "outcomes": "[\"Yes\",\"No\"]",
            "outcomePrices": "[\"0.5\",\"0.5\"]",
            "clobTokenIds": "[\"0x0\",\"0x1\"]",
            "active": true,
            "closed": false
        }"#;

        let m: GammaMarket = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "1");
        assert_eq!(m.question, "Test?");
        assert!(m.active);
        assert!(!m.closed);
        assert!(m.volume24hr.is_none());
        assert!(m.tags.is_none());
        assert!(m.condition_id.is_none());
    }

    #[test]
    fn deserialize_markets_response_as_top_level_array() {
        let json = r#"[
            {
                "id": "1",
                "question": "Market 1",
                "slug": "market-1",
                "outcomes": "[\"Yes\",\"No\"]",
                "outcomePrices": "[\"0.5\",\"0.5\"]",
                "clobTokenIds": "[\"0xa\",\"0xb\"]",
                "active": true,
                "closed": false
            },
            {
                "id": "2",
                "question": "Market 2",
                "slug": "market-2",
                "outcomes": "[\"Yes\",\"No\"]",
                "outcomePrices": "[\"0.6\",\"0.4\"]",
                "clobTokenIds": "[\"0xc\",\"0xd\"]",
                "active": true,
                "closed": false
            }
        ]"#;

        let markets: MarketsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(markets.len(), 2);
        assert_eq!(markets[0].id, "1");
        assert_eq!(markets[0].question, "Market 1");
        assert_eq!(markets[1].id, "2");
        assert_eq!(markets[1].slug, "market-2");
    }

    #[test]
    fn deserialize_empty_markets_array() {
        let json = r#"[]"#;
        let markets: MarketsResponse = serde_json::from_str(json).unwrap();
        assert!(markets.is_empty());
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

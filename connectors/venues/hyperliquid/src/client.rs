//! Hyperliquid Info API client.
//!
//! Provides an unauthenticated HTTP client for the Hyperliquid JSON-RPC
//! Info API at `https://api.hyperliquid.xyz/info`.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__HYPERLIQUID_INFO_URL` | Override Info API URL (default: `https://api.hyperliquid.xyz/info`) |

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
        Err(error) => panic!("embedded Hyperliquid venue manifest is invalid: {error}"),
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

/// A single perpetual asset as returned by the Hyperliquid `meta` endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlAsset {
    /// Asset symbol, e.g. `"BTC"`, `"ETH"`.
    pub name: String,
    /// Number of decimals for size precision (e.g. 5 means 0.00001 step).
    #[serde(default)]
    pub sz_decimals: Option<i64>,
    /// Maximum leverage for this asset (e.g. 40).
    #[serde(default)]
    pub max_leverage: Option<i64>,
    /// Identifier of the margin tier table.
    #[serde(default)]
    pub margin_table_id: Option<i64>,
    /// If true, asset only supports isolated margin.
    #[serde(default)]
    pub only_isolated: Option<bool>,
    /// If true, asset has been delisted.
    #[serde(default)]
    pub is_delisted: Option<bool>,
}

/// Response from the `meta` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlMetaResponse {
    /// Array of asset definitions.
    #[serde(default)]
    pub universe: Vec<HlAsset>,
    /// Margin tier tables.
    #[serde(default)]
    pub margin_tables: Option<serde_json::Value>,
    /// Collateral token index (0 = USDC).
    #[serde(default)]
    pub collateral_token: Option<i64>,
}

/// A single level in the Hyperliquid L2 order book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlBookLevel {
    /// Price as a string (for precision).
    pub px: String,
    /// Size as a string (for precision).
    pub sz: String,
    /// Number of orders at this level.
    pub n: u64,
}

/// L2 order book snapshot from the Hyperliquid `l2Book` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlBookSnapshot {
    /// Asset symbol, e.g. `"BTC"`.
    pub coin: String,
    /// Snapshot timestamp in milliseconds since Unix epoch.
    pub time: i64,
    /// Two-element array: levels[0] = bids (desc), levels[1] = asks (asc).
    pub levels: Vec<Vec<HlBookLevel>>,
}

/// A single funding history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlFundingEntry {
    /// Asset symbol.
    pub coin: String,
    /// Funding rate as a decimal string.
    #[serde(default)]
    pub funding_rate: Option<String>,
    /// Premium as a decimal string.
    #[serde(default)]
    pub premium: Option<String>,
    /// Timestamp in milliseconds.
    #[serde(default)]
    pub time: Option<i64>,
}

/// Live asset context from `metaAndAssetCtxs`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HlAssetCtx {
    /// Funding rate as a decimal string.
    #[serde(default)]
    pub funding: Option<String>,
    /// Open interest.
    #[serde(default)]
    pub open_interest: Option<String>,
    /// Previous day price.
    #[serde(default)]
    pub prev_day_px: Option<String>,
    /// Day notional volume.
    #[serde(default)]
    pub day_ntl_vlm: Option<String>,
    /// Premium.
    #[serde(default)]
    pub premium: Option<String>,
    /// Oracle price.
    #[serde(default)]
    pub oracle_px: Option<String>,
    /// Mark price.
    #[serde(default)]
    pub mark_px: Option<String>,
    /// Mid price.
    #[serde(default)]
    pub mid_px: Option<String>,
    /// Impact prices [bid, ask].
    #[serde(default)]
    pub impact_pxs: Option<Vec<String>>,
}

/// Combined meta + asset contexts response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlMetaAndAssetCtxsResponse {
    /// Array of asset definitions (same structure as meta).
    pub universe: Vec<HlAsset>,
    /// Margin tier tables.
    #[serde(default)]
    pub margin_tables: Option<serde_json::Value>,
    /// Live per-asset contexts.
    pub asset_ctxs: Vec<HlAssetCtx>,
}

/// Token metadata returned by `spotMetaAndAssetCtxs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlSpotToken {
    pub name: String,
    pub index: u32,
    pub sz_decimals: u32,
    pub wei_decimals: u32,
    #[serde(default)]
    pub token_id: Option<String>,
    #[serde(default)]
    pub is_canonical: Option<bool>,
}

/// Spot pair metadata. `tokens` contains base and quote token indexes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlSpotPair {
    pub name: String,
    pub tokens: Vec<u32>,
    pub index: u32,
    #[serde(default)]
    pub is_canonical: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlSpotMetaResponse {
    #[serde(default)]
    pub universe: Vec<HlSpotPair>,
    #[serde(default)]
    pub tokens: Vec<HlSpotToken>,
}

#[derive(Debug, Clone)]
pub struct HlSpotMetaAndAssetCtxsResponse {
    pub universe: Vec<HlSpotPair>,
    pub tokens: Vec<HlSpotToken>,
    pub asset_ctxs: Vec<HlAssetCtx>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from the Hyperliquid REST client.
#[derive(Error, Debug)]
pub enum ClientError {
    /// HTTP request failed (network, timeout, etc.).
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned a non-success status code.
    #[error("Hyperliquid API returned HTTP {status}")]
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
    #[error("Hyperliquid rate-limit circuit breaker is open")]
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

/// An unauthenticated HTTP client for the Hyperliquid Info API.
#[derive(Debug)]
pub struct HlClient {
    /// Info API URL (default: `https://api.hyperliquid.xyz/info`).
    info_url: String,
    /// Shared HTTP client.
    http: reqwest::Client,
    limiter: Mutex<TokenBucket>,
    rate_breaker: Mutex<CircuitBreaker>,
}

impl HlClient {
    /// Create a new client with the given Info API base URL.
    pub fn new(info_url: impl Into<String>) -> Self {
        Self {
            info_url: info_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            limiter: Mutex::new(TokenBucket::new(manifest_rest_budget())),
            rate_breaker: Mutex::new(CircuitBreaker::new()),
        }
    }

    /// Create a new client, loading URL from
    /// `AETHER_VENUE__HYPERLIQUID_INFO_URL` (falls back to production).
    pub fn from_env() -> Self {
        let info_url = std::env::var("AETHER_VENUE__HYPERLIQUID_INFO_URL")
            .unwrap_or_else(|_| "https://api.hyperliquid.xyz/info".to_string());
        Self::new(info_url)
    }

    /// Remaining requests in the pack-enforced REST budget.
    pub async fn rate_remaining(&self) -> u32 {
        let mut bucket = self.limiter.lock().await;
        bucket.refill();
        bucket.tokens.floor() as u32
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

    /// Fetch the perpetuals universe metadata.
    pub async fn get_meta(&self) -> Result<HlMetaResponse, ClientError> {
        let body = self.post(r#"{"type":"meta"}"#).await?;
        serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })
    }

    /// Fetch all mid prices.
    ///
    /// Returns a map of coin name to mid price string, e.g.
    /// `{"BTC": "67234.5", "ETH": "3456.7"}`.
    pub async fn get_all_mids(
        &self,
    ) -> Result<serde_json::Map<String, serde_json::Value>, ClientError> {
        let body = self.post(r#"{"type":"allMids"}"#).await?;
        let map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&body).map_err(|source| ClientError::Parse {
                detail: source.to_string(),
                source,
                raw: body.into_bytes(),
            })?;
        Ok(map)
    }

    /// Fetch the L2 order book for a coin.
    pub async fn get_l2_book(&self, coin: &str) -> Result<HlBookSnapshot, ClientError> {
        // Hyperliquid's first canonical spot pair is addressed by name; other
        // spot pairs use their stable `@index` identifier.
        let api_coin = if coin == "@0" { "PURR/USDC" } else { coin };
        let payload = format!(r#"{{"type":"l2Book","coin":"{}"}}"#, api_coin);
        let body = self.post(&payload).await?;
        let mut snapshot: HlBookSnapshot = serde_json::from_str(&body).map_err(|source| {
            ClientError::Parse { detail: source.to_string(), source, raw: body.into_bytes() }
        })?;
        if coin == "@0" {
            snapshot.coin = coin.to_string();
        }
        Ok(snapshot)
    }

    /// Fetch funding history for a coin.
    pub async fn get_funding_history(
        &self,
        coin: &str,
        start_time: u64,
    ) -> Result<Vec<HlFundingEntry>, ClientError> {
        let payload =
            format!(r#"{{"type":"fundingHistory","coin":"{}","startTime":{}}}"#, coin, start_time);
        let body = self.post(&payload).await?;
        serde_json::from_str(&body).map_err(|source| ClientError::Parse {
            detail: source.to_string(),
            source,
            raw: body.into_bytes(),
        })
    }

    /// Fetch combined meta + live asset contexts.
    pub async fn get_meta_and_asset_ctxs(&self) -> Result<HlMetaAndAssetCtxsResponse, ClientError> {
        let body = self.post(r#"{"type":"metaAndAssetCtxs"}"#).await?;
        let (meta, asset_ctxs): (HlMetaResponse, Vec<HlAssetCtx>) = serde_json::from_str(&body)
            .map_err(|source| ClientError::Parse {
                detail: source.to_string(),
                source,
                raw: body.into_bytes(),
            })?;
        Ok(HlMetaAndAssetCtxsResponse {
            universe: meta.universe,
            margin_tables: meta.margin_tables,
            asset_ctxs,
        })
    }

    /// Fetch spot pair/token metadata plus live pair contexts.
    pub async fn get_spot_meta_and_asset_ctxs(
        &self,
    ) -> Result<HlSpotMetaAndAssetCtxsResponse, ClientError> {
        let body = self.post(r#"{"type":"spotMetaAndAssetCtxs"}"#).await?;
        let (meta, asset_ctxs): (HlSpotMetaResponse, Vec<HlAssetCtx>) = serde_json::from_str(&body)
            .map_err(|source| ClientError::Parse {
                detail: source.to_string(),
                source,
                raw: body.into_bytes(),
            })?;
        Ok(HlSpotMetaAndAssetCtxsResponse {
            universe: meta.universe,
            tokens: meta.tokens,
            asset_ctxs,
        })
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Perform a POST request to the Info API and return the response body
    /// as text.
    async fn post(&self, body: &str) -> Result<String, ClientError> {
        for attempt in 0..=MAX_429_RETRIES {
            if !self.rate_breaker.lock().await.allow_request() {
                return Err(ClientError::RateLimitBreaker);
            }
            self.acquire_rate_token().await;
            let resp = self
                .http
                .post(&self.info_url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "aether-venue-hyperliquid/0.1.0")
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

    #[test]
    fn deserialize_meta_response() {
        let json = r#"{
            "universe": [
                {
                    "name": "BTC",
                    "szDecimals": 5,
                    "maxLeverage": 40,
                    "marginTableId": 56,
                    "onlyIsolated": true,
                    "isDelisted": false
                },
                {
                    "name": "ETH",
                    "szDecimals": 4,
                    "maxLeverage": 50,
                    "marginTableId": 57
                }
            ],
            "marginTables": [[56, {"description": "", "marginTiers": [{"lowerBound": "0.0", "maxLeverage": 40}]}]],
            "collateralToken": 0
        }"#;

        let resp: HlMetaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.universe.len(), 2);
        assert_eq!(resp.universe[0].name, "BTC");
        assert_eq!(resp.universe[0].sz_decimals, Some(5));
        assert_eq!(resp.universe[0].max_leverage, Some(40));
        assert_eq!(resp.universe[0].only_isolated, Some(true));
        assert_eq!(resp.collateral_token, Some(0));
    }

    #[test]
    fn deserialize_all_mids() {
        let json = r#"{"BTC": "67234.5", "ETH": "3456.7", "SOL": "142.3"}"#;
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(json).unwrap();
        assert_eq!(map.len(), 3);
        assert_eq!(map["BTC"].as_str(), Some("67234.5"));
        assert_eq!(map["ETH"].as_str(), Some("3456.7"));
    }

    #[test]
    fn deserialize_l2_book() {
        let json = r#"{
            "coin": "BTC",
            "time": 1754450974231,
            "levels": [
                [
                    {"px": "113377.0", "sz": "7.6699", "n": 17},
                    {"px": "113376.0", "sz": "4.13714", "n": 8}
                ],
                [
                    {"px": "113397.0", "sz": "0.11543", "n": 3}
                ]
            ]
        }"#;

        let book: HlBookSnapshot = serde_json::from_str(json).unwrap();
        assert_eq!(book.coin, "BTC");
        assert_eq!(book.time, 1754450974231);
        assert_eq!(book.levels.len(), 2);
        assert_eq!(book.levels[0].len(), 2); // 2 bids
        assert_eq!(book.levels[1].len(), 1); // 1 ask
        assert_eq!(book.levels[0][0].px, "113377.0");
        assert_eq!(book.levels[0][0].sz, "7.6699");
        assert_eq!(book.levels[0][0].n, 17);
    }

    #[test]
    fn deserialize_funding_entry() {
        let json = r#"[
            {"coin": "BTC", "fundingRate": "0.00001234", "premium": "0.0001", "time": 1700000000000},
            {"coin": "BTC", "fundingRate": "0.00001111", "premium": "0.00009", "time": 1700000100000}
        ]"#;

        let entries: Vec<HlFundingEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].coin, "BTC");
        assert_eq!(entries[0].funding_rate.as_deref(), Some("0.00001234"));
        assert_eq!(entries[0].time, Some(1_700_000_000_000));
    }

    #[test]
    fn deserialize_meta_and_asset_ctxs() {
        let json = r#"[
            {"universe": [{"name": "BTC", "szDecimals": 5, "maxLeverage": 40}],
             "marginTables": []},
            [
                {
                    "funding": "0.00001234",
                    "openInterest": "1234.5",
                    "prevDayPx": "50000.0",
                    "dayNtlVlm": "1000000.0",
                    "premium": "0.0001",
                    "oraclePx": "50100.0",
                    "markPx": "50150.0",
                    "midPx": "50145.0",
                    "impactPxs": ["50140.0", "50150.0"]
                }
            ]
        ]"#;

        let (meta, asset_ctxs): (HlMetaResponse, Vec<HlAssetCtx>) =
            serde_json::from_str(json).unwrap();
        let resp = HlMetaAndAssetCtxsResponse {
            universe: meta.universe,
            margin_tables: meta.margin_tables,
            asset_ctxs,
        };
        assert_eq!(resp.universe.len(), 1);
        assert_eq!(resp.asset_ctxs.len(), 1);
        assert_eq!(resp.asset_ctxs[0].funding.as_deref(), Some("0.00001234"));
        assert_eq!(resp.asset_ctxs[0].mid_px.as_deref(), Some("50145.0"));
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

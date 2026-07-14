//! Kalshi order management (demo env).
//!
//! Provides methods to place, cancel orders, and check balances against the
//! Kalshi REST API.  All order operations are gated by the
//! configured sandbox endpoint to prevent accidental live trading.
//!
//! # Demo gating
//!
//! Every public method rejects production endpoints before any network call.

use crate::client::{ClientError, KalshiClient};
use aether_core::order::{OrderIntent, Side};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during order operations.
#[derive(Error, Debug)]
pub enum OrderError {
    /// The configured API origin is not Kalshi's sandbox.
    #[error("order operations require the Kalshi sandbox endpoint")]
    DemoRequired,

    /// Underlying client / HTTP error.
    #[error("client error: {0}")]
    Client(#[from] ClientError),

    /// The Kalshi API returned an error or an unexpected response.
    #[error("API error: {0}")]
    Api(String),

    /// The price field could not be converted to cents.
    #[error("invalid price: {0}")]
    InvalidPrice(String),
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Acknowledgement returned when an order is placed.
#[derive(Debug, Clone)]
pub struct OrderAck {
    /// Venue-side order identifier (the `order_id` from Kalshi).
    pub venue_ref: String,
    /// Order status string (e.g. `"placed"`, `"resting"`).
    pub status: String,
}

/// A single asset balance entry.
#[derive(Debug, Clone)]
pub struct BalanceEntry {
    /// Asset symbol (e.g. `"USD"`).
    pub asset: String,
    /// Free (available) balance as a decimal string.
    pub free: String,
    /// Locked (in orders) balance as a decimal string.
    pub locked: String,
}

/// Aggregated balance snapshot.
#[derive(Debug, Clone)]
pub struct Balances {
    /// All non-zero balances.
    pub balances: Vec<BalanceEntry>,
}

// ---------------------------------------------------------------------------
// Kalshi API wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct KalshiPlaceOrderRequest {
    ticker: String,
    side: String,
    price: String,
    count: String,
    client_order_id: String,
    time_in_force: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct KalshiPlaceOrderResponse {
    order_id: String,
    remaining_count: String,
    #[serde(default)]
    client_order_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
struct KalshiBalanceResponse {
    balance: i64,
    #[serde(default)]
    total_value: Option<i64>,
}

// ---------------------------------------------------------------------------
// KalshiOrders
// ---------------------------------------------------------------------------

/// Order-management handle for Kalshi.
///
/// Wraps a [`KalshiClient`] and provides order lifecycle methods.  All methods
/// reject non-sandbox API origins before any order or balance request.
#[derive(Debug)]
pub struct KalshiOrders {
    client: KalshiClient,
}

impl KalshiOrders {
    /// Create a new order handle from an existing authenticated client.
    pub fn new(client: KalshiClient) -> Self {
        Self { client }
    }

    /// Check that the configured API origin is a sandbox (or local test server).
    pub(crate) fn check_demo(&self) -> Result<(), OrderError> {
        if !self.client.is_sandbox_endpoint() {
            return Err(OrderError::DemoRequired);
        }
        Ok(())
    }

    /// Submit a limit order on Kalshi.
    ///
    /// The [`OrderIntent::id`] is used as the `client_order_id` for idempotency.
    /// Only `Side::Buy` and `Side::Sell` are supported; `BuyNo` / `SellNo`
    /// return an error.
    pub async fn submit_order(&self, intent: &OrderIntent) -> Result<OrderAck, OrderError> {
        self.check_demo()?;

        let ticker = intent
            .market
            .as_str()
            .strip_prefix("mkt:kalshi:")
            .filter(|ticker| !ticker.is_empty() && !ticker.contains(':'))
            .ok_or_else(|| OrderError::Api("market key must be mkt:kalshi:{ticker}".into()))?
            .to_string();

        let price = intent
            .limit_price
            .ok_or_else(|| OrderError::InvalidPrice("limit_price is required".into()))?;
        if price <= Decimal::ZERO || price >= Decimal::ONE {
            return Err(OrderError::InvalidPrice("price must be between 0 and 1".into()));
        }

        // Map canonical side to Kalshi side
        let side = match intent.side {
            Side::Buy => "bid",
            Side::Sell => "ask",
            _ => {
                return Err(OrderError::Api(format!(
                    "unsupported side for Kalshi: {side:?}",
                    side = intent.side
                )));
            }
        };

        if intent.size <= Decimal::ZERO {
            return Err(OrderError::InvalidPrice("count must be positive".into()));
        }
        let client_order_id = intent.id.to_string();

        let req_body = KalshiPlaceOrderRequest {
            ticker: ticker.clone(),
            side: side.to_string(),
            price: price.normalize().to_string(),
            count: intent.size.normalize().to_string(),
            client_order_id: client_order_id.clone(),
            time_in_force: "good_till_canceled".into(),
        };

        let json_body =
            serde_json::to_string(&req_body).map_err(|e| OrderError::Api(e.to_string()))?;

        let resp_body =
            self.client.post_text("/trade-api/v2/portfolio/events/orders", &json_body).await?;

        let resp: KalshiPlaceOrderResponse =
            serde_json::from_str(&resp_body).map_err(|e| OrderError::Api(e.to_string()))?;

        if resp.client_order_id.as_deref() != Some(client_order_id.as_str()) {
            return Err(OrderError::Api(
                "Kalshi response client_order_id did not match the submitted intent".into(),
            ));
        }

        let remaining = resp
            .remaining_count
            .parse::<Decimal>()
            .map_err(|error| OrderError::Api(error.to_string()))?;
        Ok(OrderAck {
            venue_ref: resp.order_id,
            status: if remaining > Decimal::ZERO { "resting" } else { "filled" }.into(),
        })
    }

    /// Cancel an order by its venue-side order ID.
    pub async fn cancel_order(&self, venue_ref: &str) -> Result<(), OrderError> {
        self.check_demo()?;
        if venue_ref.trim().is_empty() {
            return Err(OrderError::Api("venue_ref is required".into()));
        }

        let path =
            format!("/trade-api/v2/portfolio/events/orders/{}", urlencoding::encode(venue_ref));
        self.client.delete_text(&path).await?;

        Ok(())
    }

    /// Fetch the current portfolio balances from Kalshi.
    ///
    /// Kalshi returns balances in cents; they are converted to USD decimal
    /// strings (e.g. `"1000.00"`).
    pub async fn get_balances(&self) -> Result<Balances, OrderError> {
        self.check_demo()?;

        let body = self.client.get_text("/trade-api/v2/portfolio/balance").await?;

        let resp: KalshiBalanceResponse =
            serde_json::from_str(&body).map_err(|e| OrderError::Api(e.to_string()))?;

        let free_usd = format!("{:.2}", resp.balance as f64 / 100.0);

        Ok(Balances {
            balances: vec![BalanceEntry {
                asset: "USD".into(),
                free: free_usd,
                locked: "0.00".into(),
            }],
        })
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::ids::{MarketKey, Ulid, VenueId};
    use aether_core::order::{OrderType, Origin, OriginKind, SizeUnit, TimeInForce};
    use aether_core::quote::{Quote, QuoteSource};
    use aether_core::time::UtcTime;

    fn test_market_key() -> MarketKey {
        MarketKey::new(&VenueId::new("kalshi").unwrap(), "BTC-75").unwrap()
    }

    fn test_quote() -> Quote {
        Quote {
            market: test_market_key(),
            bid: Some(Decimal::new(65, 2)),
            ask: Some(Decimal::new(67, 2)),
            mid: Some(Decimal::new(66, 2)),
            last: None,
            bid_size: Some(Decimal::new(1000, 0)),
            ask_size: Some(Decimal::new(500, 0)),
            ts: UtcTime::from_unix_millis(1_783_686_896_789).unwrap(),
            source: QuoteSource::Stream,
            seq: Some(1),
        }
    }

    fn test_intent() -> OrderIntent {
        OrderIntent {
            id: Ulid::new(),
            market: test_market_key(),
            side: Side::Buy,
            order_type: OrderType::Limit,
            limit_price: Some(Decimal::new(65, 2)), // 0.65
            size: Decimal::new(10, 0),              // 10 contracts
            size_unit: SizeUnit::Contracts,
            tif: TimeInForce::Day,
            paper: true,
            origin: Origin::new(OriginKind::Agent, 3, Ulid::new()).unwrap(),
            quote_snapshot: test_quote(),
            caps_version: Ulid::new(),
            created_ts: UtcTime::now(),
        }
    }

    // -- submit_order validation (no HTTP) ---------------------------------

    #[tokio::test]
    async fn submit_order_fails_without_demo_flag() {
        // Even with a dummy client, we should fail before making HTTP due to
        // the demo gate.
        use crate::auth::KalshiAuth;
        let auth = KalshiAuth::from_pem_bytes("test", TEST_KEY_PEM.as_bytes()).unwrap();
        let client = KalshiClient::new("https://external-api.kalshi.com", auth);
        let orders = KalshiOrders::new(client);

        let result = orders.submit_order(&test_intent()).await;
        assert!(matches!(result, Err(OrderError::DemoRequired)));
    }

    const TEST_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDJJgEmkCH8nR55
pqhp/MIFR4hIr/dvbhrY+Ja3VM+qnq9vUD0lvPkPSdvwMVT05n6YVtMMM3ionLcA
bjSX2qjMBQozVih7xZonMKCLryJehbZNLGzPZD4aOv2P8PtctY/pNisa7tG73OvC
OXdlefIz+jMoiHNVNzl/HoVH0HxR4YHASe4lDaPtbbAciw60mpC2G8XWGJFmZGYj
WYfSmZ5tt3nqyQSOZpgzD4TiVXMOGRtjJIk0FdHd1sgo/dDIn6uKoH9j4qV3Mfr8
Z1alWAmt+Pfkwkw6Tcx2Jwtvhh6WNaEXk5+9UyEw+D+U5DqdMOPuD7fFL/y9icDC
fJ+Y9zgfAgMBAAECggEABJkDybfdrwKAYdN3YgTPAoPiD5dGFpvzrSXxe/tKS+IY
rHivDR/GqZzMlC7sfDSQjDbf2BWNGn2KiU37kcUDurYax5Wek0WvAlpQMSEtre9s
fVMYoZzu9naGuTWO6U2VHoWIcrMmxB6GnQfnPMCO0rVTWgfUaww6Gje+YCfZz51H
iJNNrLS9qFiWwO/DbEIOIyKRmAwF+h62Tfc7UQG2HIkJtMagRvCos2/+/gcDJ183
Tnno5XisuJ1B3LVvzh1BNqZaWKiXJZmZA5vpz2cFlaKGFE/IVgzgvKxvDrEt2d7D
j5uYVUb+6oft7BIZem2jkQQLQKez1ZMRmNSXa83BAQKBgQD5O1dcPfvVSczN4/6X
NzrjgkLQ5nNP57PM+gS1LGIVXztFywEQjftTF0R9tKFFqi6rVq5VO5Zjg7P1BiGc
Rk7rZy8mQnZo54MT2JYTpVhX9gUYXwSEOnc9sFyx+ncBPmKkwTvSwZhVdkhCEggw
CZI3VZgpJB0damAWhajQOcOa3wKBgQDOnGNRTYkHiD5Cr76l+CpKONqKiNUaKiZx
ZehBsKMCAfv9z77i/H/Wsbgn/HxDinmIBskF71fAKUOOGcBusJ4cGJRh2B6vBy6g
hm+b+2nSawgWaF7+ttRfVzFGH+nETHClzRaHc3h0p2ccnSxwVu6nW1p1jyMx0c/q
gtynUKVKwQKBgDpVPEY3r7ilFE1gPpdP8vWK6G6ScYzTM08XeYCaCb7s0iessuwX
/ynceUhevZxbj57Eo/sI/lL+YWFI9RbpkdEhDnUK+0HkZdaAS+f/PCUiTOD+ZEU6
lewXWirB75aX7miXXZQfgbMHAzSLmeT8aH+RBhMjA7l9y02aLP/HdVPLAoGAJyR5
rG2ECGlHYlrpQ4hAes9Kl/RUayCRJ+qmlcthFoBJvUweXeJ4VbRVrz2mTSVu4NZo
PzeY6E7o/YLjchUD307IzcCkD4TM0JyniGWZJsQgRB6B4L/CfE2IiECDiSzyKncw
TXkS2QbeAg3E3YOasxobiSoVANs/CK7CHvCoYAECgYEA7+emQFZmbSrWlhn7xeEy
OMQVeC/F6xKe4lGiuXsnjKEO1K6bi3qvltRoUdhH7bnR+k55hbDZG1sRZpl+N5VV
L/pwyKxACFxRoBxJqeozXdOqWB/2nw+byZNtK1KfQLnAyGqADXPnXPBUxVFE+c/2
8jqtMyHz94du+Z7Y/kOyNns=
-----END PRIVATE KEY-----";
}

//! Alpaca paper order management.
//!
//! Provides methods to place, cancel orders, and check balances against the
//! Alpaca REST API. All order operations are gated by the configured paper
//! endpoint to prevent accidental live trading.
//!
//! # Paper gating
//!
//! Every public method rejects production endpoints before any network call.

use crate::client::{AlpacaClient, AlpacaOrderRequest, AlpacaOrderResponse, ClientError};
use aether_core::order::{OrderIntent, Side};
use rust_decimal::Decimal;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during order operations.
#[derive(Error, Debug)]
pub enum OrderError {
    /// The configured API origin is not the Alpaca paper endpoint.
    #[error("order operations require the Alpaca paper endpoint")]
    DemoRequired,

    /// The intent itself was not explicitly marked as paper trading.
    #[error("order intent must set paper=true for the Alpaca paper pack")]
    PaperIntentRequired,

    /// Underlying client / HTTP error.
    #[error("client error: {0}")]
    Client(#[from] ClientError),

    /// The Alpaca API returned an error or an unexpected response.
    #[error("API error: {0}")]
    Api(String),

    /// The price field could not be parsed.
    #[error("invalid price: {0}")]
    InvalidPrice(String),
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Acknowledgement returned when an order is placed.
#[derive(Debug, Clone)]
pub struct OrderAck {
    /// Venue-side order identifier (the `id` from Alpaca).
    pub venue_ref: String,
    /// Order status string (e.g. `"accepted"`, `"filled"`).
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
// AlpacaOrders
// ---------------------------------------------------------------------------

/// Order-management handle for Alpaca.
///
/// Wraps an [`AlpacaClient`] and provides order lifecycle methods. All methods
/// reject non-paper API origins before any order or balance request.
#[derive(Debug)]
pub struct AlpacaOrders {
    client: AlpacaClient,
}

impl AlpacaOrders {
    /// Create a new order handle from an existing authenticated client.
    pub fn new(client: AlpacaClient) -> Self {
        Self { client }
    }

    /// Check that the configured API origin is the paper endpoint.
    pub(crate) fn check_demo(&self) -> Result<(), OrderError> {
        if !self.client.is_sandbox_endpoint() {
            return Err(OrderError::DemoRequired);
        }
        Ok(())
    }

    /// Submit an order to Alpaca.
    ///
    /// The [`OrderIntent::id`] is used as the `client_order_id` for idempotency.
    /// Supports `Side::Buy` and `Side::Sell`.
    pub async fn submit_order(&self, intent: &OrderIntent) -> Result<OrderAck, OrderError> {
        self.check_demo()?;
        if !intent.paper {
            return Err(OrderError::PaperIntentRequired);
        }

        let symbol = intent
            .market
            .as_str()
            .strip_prefix("mkt:alpaca:")
            .filter(|s| !s.is_empty() && !s.contains(':'))
            .ok_or_else(|| OrderError::Api("market key must be mkt:alpaca:{symbol}".into()))?
            .to_uppercase();

        // Map canonical side to Alpaca side
        let side = match intent.side {
            Side::Buy => "buy",
            Side::Sell => "sell",
            _ => {
                return Err(OrderError::Api(format!(
                    "unsupported side for Alpaca: {side:?}",
                    side = intent.side
                )));
            }
        };

        if intent.size <= Decimal::ZERO {
            return Err(OrderError::InvalidPrice("qty must be positive".into()));
        }

        let size_str = intent.size.normalize().to_string();

        // Determine order type and optional limit price
        let order_type = match intent.order_type {
            aether_core::order::OrderType::Limit => "limit",
            aether_core::order::OrderType::Market => "market",
        };

        let limit_price = match intent.limit_price {
            Some(price) if price > Decimal::ZERO => Some(price.normalize().to_string()),
            Some(_) => return Err(OrderError::InvalidPrice("limit price must be positive".into())),
            None if order_type == "limit" => {
                return Err(OrderError::InvalidPrice("limit order requires a limit price".into()));
            }
            None => None,
        };

        let time_in_force = match intent.tif {
            aether_core::order::TimeInForce::Day => "day",
            aether_core::order::TimeInForce::Gtc => "gtc",
            aether_core::order::TimeInForce::Ioc => "ioc",
        };

        let client_order_id = intent.id.to_string();

        let req = AlpacaOrderRequest {
            symbol: symbol.clone(),
            side: side.to_string(),
            qty: size_str,
            order_type: order_type.to_string(),
            time_in_force: time_in_force.to_string(),
            limit_price,
            stop_price: None,
            extended_hours: None,
            client_order_id: Some(client_order_id.clone()),
        };

        let resp: AlpacaOrderResponse = self.client.submit_order(&req).await?;

        if resp.client_order_id.as_deref() != Some(client_order_id.as_str()) {
            return Err(OrderError::Api(
                "Alpaca response client_order_id did not match the submitted intent".into(),
            ));
        }

        let status = resp.status.unwrap_or_else(|| "unknown".into());

        Ok(OrderAck { venue_ref: resp.id, status })
    }

    /// Cancel an order by its venue-side order ID.
    pub async fn cancel_order(&self, venue_ref: &str) -> Result<(), OrderError> {
        self.check_demo()?;
        if venue_ref.trim().is_empty() {
            return Err(OrderError::Api("venue_ref is required".into()));
        }

        self.client.cancel_order(venue_ref).await?;

        Ok(())
    }

    /// Fetch the current portfolio balances from Alpaca (GET /v2/account).
    ///
    /// Returns cash balance as the primary asset.
    pub async fn get_balances(&self) -> Result<Balances, OrderError> {
        self.check_demo()?;

        let account = self.client.get_account().await?;

        let free = account.cash.clone();

        Ok(Balances {
            balances: vec![BalanceEntry { asset: "USD".into(), free, locked: "0.00".into() }],
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AlpacaAuth;
    use aether_core::ids::{MarketKey, Ulid, VenueId};
    use aether_core::order::{OrderType, Origin, OriginKind, SizeUnit, TimeInForce};
    use aether_core::quote::{Quote, QuoteSource};
    use aether_core::time::UtcTime;

    fn test_market_key() -> MarketKey {
        MarketKey::new(&VenueId::new("alpaca").unwrap(), "AAPL").unwrap()
    }

    fn test_quote() -> Quote {
        Quote {
            market: test_market_key(),
            bid: Some(Decimal::new(15000, 2)),
            ask: Some(Decimal::new(15100, 2)),
            mid: Some(Decimal::new(15050, 2)),
            last: None,
            bid_size: Some(Decimal::new(100, 0)),
            ask_size: Some(Decimal::new(50, 0)),
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
            limit_price: Some(Decimal::new(15050, 2)),
            size: Decimal::new(10, 0),
            size_unit: SizeUnit::Contracts,
            tif: TimeInForce::Day,
            paper: true,
            origin: Origin::new(OriginKind::Agent, 3, Ulid::new()).unwrap(),
            quote_snapshot: test_quote(),
            caps_version: Ulid::new(),
            created_ts: UtcTime::now(),
        }
    }

    #[tokio::test]
    async fn submit_order_fails_without_demo_flag() {
        let auth = AlpacaAuth::new("test-key", "test-secret");
        let client =
            AlpacaClient::new("https://api.alpaca.markets", "https://data.alpaca.markets", auth);
        let orders = AlpacaOrders::new(client);

        let result = orders.submit_order(&test_intent()).await;
        assert!(matches!(result, Err(OrderError::DemoRequired)));
    }

    #[tokio::test]
    async fn submit_order_fails_for_crypto_side() {
        let auth = AlpacaAuth::new("test-key", "test-secret");
        let client = AlpacaClient::new(
            "https://paper-api.alpaca.markets",
            "https://data.alpaca.markets",
            auth,
        );
        let orders = AlpacaOrders::new(client);

        let mut intent = test_intent();
        intent.side = Side::BuyNo; // Not supported by Alpaca
        let result = orders.submit_order(&intent).await;
        assert!(matches!(result, Err(OrderError::Api(_))));
    }

    #[tokio::test]
    async fn submit_order_requires_paper_intent_even_on_paper_host() {
        let auth = AlpacaAuth::new("test-key", "test-secret");
        let client = AlpacaClient::new(
            "https://paper-api.alpaca.markets",
            "https://data.alpaca.markets",
            auth,
        );
        let orders = AlpacaOrders::new(client);
        let mut intent = test_intent();
        intent.paper = false;
        assert!(matches!(orders.submit_order(&intent).await, Err(OrderError::PaperIntentRequired)));
    }

    #[tokio::test]
    async fn cancel_order_fails_without_demo_flag() {
        let auth = AlpacaAuth::new("test-key", "test-secret");
        let client =
            AlpacaClient::new("https://api.alpaca.markets", "https://data.alpaca.markets", auth);
        let orders = AlpacaOrders::new(client);

        let result = orders.cancel_order("order-123").await;
        assert!(matches!(result, Err(OrderError::DemoRequired)));
    }

    #[tokio::test]
    async fn get_balances_fails_without_demo_flag() {
        let auth = AlpacaAuth::new("test-key", "test-secret");
        let client =
            AlpacaClient::new("https://api.alpaca.markets", "https://data.alpaca.markets", auth);
        let orders = AlpacaOrders::new(client);

        let result = orders.get_balances().await;
        assert!(matches!(result, Err(OrderError::DemoRequired)));
    }
}

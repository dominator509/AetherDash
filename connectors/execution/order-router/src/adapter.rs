//! Venue adapter client trait and registry for live order dispatch.
//!
//! The router calls venue adapters through this trait, keeping the
//! adapter implementations behind the execution boundary.

use aether_core::ids::VenueId;
use aether_core::order::OrderIntent;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Errors from venue adapter operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum AdapterError {
    #[error("venue adapter is unavailable: {0}")]
    Unavailable(String),
    #[error("venue rejected order: {0}")]
    Rejected(String),
    #[error("order not found: {0}")]
    NotFound(String),
    #[error("timeout waiting for venue response")]
    Timeout,
    #[error("venue capability missing: {0}")]
    CapabilityMissing(String),
}

/// Result of submitting an order to a venue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueOrderResult {
    /// The venue-side order identifier.
    pub venue_ref: String,
    /// The client-supplied idempotency key. For router submissions this must
    /// be exactly [`OrderIntent::id`].
    pub client_order_id: String,
    /// Order status from the venue.
    pub status: String,
    /// Whether the order was accepted.
    pub accepted: bool,
}

impl VenueOrderResult {
    pub fn observation(&self) -> VenueOrderObservation {
        match self.status.to_ascii_lowercase().as_str() {
            "filled" | "done" => VenueOrderObservation::Filled,
            "accepted" | "new" | "open" | "partially_filled" | "pending_new" => {
                VenueOrderObservation::Open
            }
            "canceled" | "cancelled" => VenueOrderObservation::Cancelled,
            "rejected" | "expired" => VenueOrderObservation::Rejected,
            _ if self.accepted => VenueOrderObservation::Open,
            _ => VenueOrderObservation::Rejected,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueOrderObservation {
    Filled,
    Open,
    Cancelled,
    Rejected,
}

/// Trait for venue adapter operations.
/// Implementations wrap venue pack clients (Kalshi, Alpaca, etc.).
pub trait VenueAdapterClient: Send + Sync {
    /// Submit an order to the venue.
    ///
    /// Implementations must send `intent.id` as the venue client order id and
    /// verify that the acknowledgement echoes the same id when the venue
    /// supports echoing client ids.
    fn submit_order(&self, intent: &OrderIntent) -> Result<VenueOrderResult, AdapterError>;

    /// Cancel an order on the venue.
    fn cancel_order(&self, venue_ref: &str) -> Result<(), AdapterError>;

    /// Query the status of an order on the venue.
    fn query_order(&self, venue_ref: &str) -> Result<VenueOrderResult, AdapterError>;

    /// Get current balances from the venue.
    fn get_balances(&self) -> Result<VenueBalances, AdapterError>;

    /// Check if the venue is healthy (can accept orders).
    fn is_healthy(&self) -> bool;
}

/// Deterministic sandbox adapter for router integration tests and operator
/// dry-runs. It accepts order-capable submissions without network access and
/// preserves the router idempotency key as `client_order_id`.
#[derive(Default)]
pub struct SandboxVenueAdapter {
    orders: Mutex<HashMap<String, VenueOrderResult>>,
}

impl SandboxVenueAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_status(&self, client_order_id: &str, status: impl Into<String>, accepted: bool) {
        if let Ok(mut orders) = self.orders.lock() {
            if let Some(order) = orders.get_mut(client_order_id) {
                order.status = status.into();
                order.accepted = accepted;
            }
        }
    }

    pub fn order_count(&self) -> usize {
        self.orders.lock().map_or(0, |orders| orders.len())
    }
}

impl VenueAdapterClient for SandboxVenueAdapter {
    fn submit_order(&self, intent: &OrderIntent) -> Result<VenueOrderResult, AdapterError> {
        let client_order_id = intent.id.to_string();
        let mut orders = self
            .orders
            .lock()
            .map_err(|_| AdapterError::Unavailable("sandbox adapter state unavailable".into()))?;
        if let Some(existing) = orders.get(&client_order_id) {
            return Ok(existing.clone());
        }
        let result = VenueOrderResult {
            venue_ref: format!("sandbox:{}", intent.id),
            client_order_id: client_order_id.clone(),
            status: "accepted".into(),
            accepted: true,
        };
        orders.insert(client_order_id, result.clone());
        Ok(result)
    }

    fn cancel_order(&self, venue_ref: &str) -> Result<(), AdapterError> {
        let mut orders = self
            .orders
            .lock()
            .map_err(|_| AdapterError::Unavailable("sandbox adapter state unavailable".into()))?;
        let Some(order) = orders.values_mut().find(|order| order.venue_ref == venue_ref) else {
            return Err(AdapterError::NotFound(venue_ref.into()));
        };
        order.status = "cancelled".into();
        order.accepted = false;
        Ok(())
    }

    fn query_order(&self, venue_ref: &str) -> Result<VenueOrderResult, AdapterError> {
        let orders = self
            .orders
            .lock()
            .map_err(|_| AdapterError::Unavailable("sandbox adapter state unavailable".into()))?;
        orders
            .get(venue_ref)
            .cloned()
            .or_else(|| orders.values().find(|order| order.venue_ref == venue_ref).cloned())
            .ok_or_else(|| AdapterError::NotFound(venue_ref.into()))
    }

    fn get_balances(&self) -> Result<VenueBalances, AdapterError> {
        Ok(VenueBalances {
            free: Decimal::new(1_000_000, 2),
            locked: Decimal::ZERO,
            currency: "USD".into(),
        })
    }

    fn is_healthy(&self) -> bool {
        self.orders.lock().is_ok()
    }
}

/// Balances returned by a venue adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueBalances {
    pub free: Decimal,
    pub locked: Decimal,
    pub currency: String,
}

/// Stub adapter for venues that don't support orders (read-only).
/// Always returns CapabilityMissing.
pub struct ReadOnlyAdapter {
    pub venue_id: VenueId,
}

impl VenueAdapterClient for ReadOnlyAdapter {
    fn submit_order(&self, _intent: &OrderIntent) -> Result<VenueOrderResult, AdapterError> {
        Err(AdapterError::CapabilityMissing(format!(
            "{} is a read-only venue",
            self.venue_id.as_str()
        )))
    }
    fn cancel_order(&self, _venue_ref: &str) -> Result<(), AdapterError> {
        Err(AdapterError::CapabilityMissing(format!(
            "{} is a read-only venue",
            self.venue_id.as_str()
        )))
    }
    fn query_order(&self, _venue_ref: &str) -> Result<VenueOrderResult, AdapterError> {
        Err(AdapterError::CapabilityMissing(format!(
            "{} is a read-only venue",
            self.venue_id.as_str()
        )))
    }
    fn get_balances(&self) -> Result<VenueBalances, AdapterError> {
        Err(AdapterError::CapabilityMissing(format!(
            "{} is a read-only venue",
            self.venue_id.as_str()
        )))
    }
    fn is_healthy(&self) -> bool {
        true
    }
}

/// Registry of venue adapters keyed by venue ID.
pub struct AdapterRegistry {
    adapters: HashMap<VenueId, Arc<dyn VenueAdapterClient>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self { adapters: HashMap::new() }
    }

    pub fn register(&mut self, venue_id: VenueId, adapter: Arc<dyn VenueAdapterClient>) {
        self.adapters.insert(venue_id, adapter);
    }

    pub fn get(&self, venue_id: &VenueId) -> Option<&Arc<dyn VenueAdapterClient>> {
        self.adapters.get(venue_id)
    }

    /// List all registered venue IDs.
    pub fn venues(&self) -> Vec<&VenueId> {
        self.adapters.keys().collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        // Register read-only adapters for venues without order capability
        let read_only = ["polymarket", "hyperliquid", "openbb"];
        for slug in &read_only {
            if let Ok(vid) = VenueId::new(*slug) {
                registry.register(vid.clone(), Arc::new(ReadOnlyAdapter { venue_id: vid.clone() }));
            }
        }
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::ids::Ulid;
    use aether_core::order::Side;

    #[test]
    fn read_only_adapter_rejects_orders() {
        let vid = VenueId::new("polymarket").unwrap();
        let adapter = ReadOnlyAdapter { venue_id: vid.clone() };
        let intent = OrderIntent {
            id: Ulid::new(),
            market: aether_core::ids::MarketKey::new(&vid, "test").unwrap(),
            side: Side::Buy,
            order_type: aether_core::order::OrderType::Limit,
            limit_price: Some(Decimal::new(50, 2)),
            size: Decimal::new(1, 0),
            size_unit: aether_core::order::SizeUnit::Contracts,
            tif: aether_core::order::TimeInForce::Day,
            paper: false,
            origin: aether_core::order::Origin::new(
                aether_core::order::OriginKind::Agent,
                3,
                Ulid::new(),
            )
            .unwrap(),
            quote_snapshot: aether_core::quote::Quote {
                market: aether_core::ids::MarketKey::new(&vid, "test").unwrap(),
                bid: None,
                ask: None,
                mid: None,
                last: None,
                bid_size: None,
                ask_size: None,
                ts: aether_core::time::UtcTime::now(),
                source: aether_core::quote::QuoteSource::Stream,
                seq: None,
            },
            caps_version: Ulid::new(),
            created_ts: aether_core::time::UtcTime::now(),
        };
        assert!(matches!(adapter.submit_order(&intent), Err(AdapterError::CapabilityMissing(_))));
    }

    #[test]
    fn sandbox_adapter_preserves_client_order_id() {
        let vid = VenueId::new("alpaca").unwrap();
        let adapter = SandboxVenueAdapter::new();
        let intent = OrderIntent {
            id: Ulid::new(),
            market: aether_core::ids::MarketKey::new(&vid, "AAPL").unwrap(),
            side: Side::Buy,
            order_type: aether_core::order::OrderType::Limit,
            limit_price: Some(Decimal::new(15_000, 2)),
            size: Decimal::new(1, 0),
            size_unit: aether_core::order::SizeUnit::Shares,
            tif: aether_core::order::TimeInForce::Day,
            paper: false,
            origin: aether_core::order::Origin::new(
                aether_core::order::OriginKind::Agent,
                3,
                Ulid::new(),
            )
            .unwrap(),
            quote_snapshot: aether_core::quote::Quote {
                market: aether_core::ids::MarketKey::new(&vid, "AAPL").unwrap(),
                bid: Some(Decimal::new(14_900, 2)),
                ask: Some(Decimal::new(15_000, 2)),
                mid: None,
                last: None,
                bid_size: None,
                ask_size: None,
                ts: aether_core::time::UtcTime::now(),
                source: aether_core::quote::QuoteSource::Stream,
                seq: None,
            },
            caps_version: Ulid::new(),
            created_ts: aether_core::time::UtcTime::now(),
        };
        let result = adapter.submit_order(&intent).unwrap();
        assert_eq!(result.client_order_id, intent.id.to_string());
        assert_eq!(adapter.query_order(&result.client_order_id).unwrap(), result);
    }

    #[test]
    fn registry_default_includes_read_only_venues() {
        let registry = AdapterRegistry::default();
        assert!(registry.get(&VenueId::new("polymarket").unwrap()).is_some());
        assert!(registry.get(&VenueId::new("hyperliquid").unwrap()).is_some());
    }

    #[test]
    fn registry_returns_none_for_unknown_venue() {
        let registry = AdapterRegistry::default();
        assert!(registry.get(&VenueId::new("nonexistent").unwrap()).is_none());
    }
}

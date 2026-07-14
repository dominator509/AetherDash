//! AETHER Terminal -- Alpaca venue adapter library.
//!
//! This crate implements the `VenueAdapter` gRPC service for Alpaca Markets
//! (US-regulated equities broker). It provides:
//!
//! - Header-based HTTP client for the Alpaca REST API (`client`, `auth`)
//! - Market normalization from Alpaca raw types to canonical `aether_core::Market`
//! - Quote normalization from snapshot/quotes/trades
//! - WebSocket stream client with reconnection, gap detection, and circuit breaker
//! - Tonic gRPC server exposing the `aether.venue.v1.VenueAdapter` service (`main` binary)
//!
//! # Paper Trading
//!
//! This pack targets the paper trading endpoint
//! (`https://paper-api.alpaca.markets`) exclusively. Order operations are
//! gated to prevent accidental live trading.

pub mod auth;
pub mod client;
pub mod health;
pub mod normalize;
pub mod orders;
pub mod replay;
pub mod ws;

// Re-exports for external consumers (test crate, dashboard, etc.)
pub use auth::AlpacaAuth;
pub use client::AlpacaSnapshot;
pub use client::{AlpacaAsset, AlpacaClient};
pub use health::check_health;
pub use orders::AlpacaOrders;
pub use ws::AlpacaStream;

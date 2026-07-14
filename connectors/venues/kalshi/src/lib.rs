//! AETHER Terminal -- Kalshi venue adapter library.
//!
//! This crate implements the `VenueAdapter` gRPC service for the Kalshi
//! prediction-market exchange (US-regulated).  It provides:
//!
//! - RSA-signed HTTP client for the Kalshi REST API (`client`, `auth`)
//! - Market normalization from Kalshi raw types to canonical `aether_core::Market`
//! - Quote and order-book normalization from WebSocket ticks/snapshots
//! - WebSocket stream client with reconnection, gap detection, and circuit breaker
//! - Tonic gRPC server exposing the `aether.venue.v1.VenueAdapter` service (`main` binary)
//!
//! # M1: Scaffold + manifest
//! # M2: Auth + REST markets (list/get)
//! # M3: Market normalization (cents -> probability, status mapping, MarketKey minting)
//! # M4: WebSocket streams (tick + book normalization, reconnection, gap detection)

pub mod auth;
pub mod client;
pub mod health;
pub mod normalize;
pub mod orders;
pub mod replay;
pub mod stream;

// Re-exports for external consumers (test crate, dashboard, etc.)
pub use auth::KalshiAuth;
pub use client::{KalshiClient, KalshiMarket, MarketsResponse};
pub use health::check_health;
pub use normalize::normalize_market;
pub use orders::KalshiOrders;
pub use stream::KalshiStream;

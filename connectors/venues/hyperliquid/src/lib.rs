//! AETHER Terminal -- Hyperliquid venue adapter library.
//!
//! This crate implements the `VenueAdapter` gRPC service for Hyperliquid,
//! a decentralized perpetuals exchange.
//!
//! # Data sources
//!
//! - **Info API** (`client`): REST market discovery via JSON-RPC POST.
//!   Public; no authentication required.
//! - **Polling stream** (`stream`): polls `allMids` and `l2Book` at
//!   configurable intervals since Hyperliquid does not offer a public
//!   WebSocket for market data.
//!
//! # Caveats
//!
//! - Read-only in Phase 1: no order submission or cancellation.
//! - No authentication required; all info endpoints are public.
//! - Hyperliquid has no sandbox environment; production endpoint is used
//!   for all requests.

pub mod auth;
pub mod client;
pub mod health;
pub mod normalize;
pub mod replay;
pub mod stream;

// Re-exports for external consumers (test crate, dashboard, etc.)
pub use client::{HlAsset, HlClient, HlMetaResponse};
pub use health::check_health;
pub use normalize::normalize_market;
pub use stream::HlStream;

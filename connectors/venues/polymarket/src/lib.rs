//! AETHER Terminal -- Polymarket venue adapter library.
//!
//! This crate implements the `VenueAdapter` gRPC service for Polymarket
//! prediction markets.  It is **read-only** by design: no order submission
//! or cancellation is implemented, and any attempt to invoke order RPCs
//! returns `capability_missing`.
//!
//! # Data sources
//!
//! - **Gamma API** (`client`): REST market discovery — list markets, get by
//!   ID/slug, with pagination.  Public; no authentication required.
//! - **CLOB API** (`clob`, `stream`): REST order-book snapshots +
//!   WebSocket real-time book + price streams.  Public market channel
//!   requires no authentication.
//! - **Polygon RPC** (`rpc`): on-chain resolution/status reads via the
//!   Conditional Token Framework contract.  Public JSON-RPC; no signing.
//!
//! # M1: Scaffold + manifest (read-only capabilities, US execution blocked)
//! # M2: Gamma market discovery -> `Market` rows with condition/outcome mapping
//! # M3: CLOB books + ticks -> `OrderBook`/`Quote` in probability space
//! # M4: Polygon RPC reads -> Market status transitions
//! # M5: Recording + health + registry

pub mod auth;
pub mod client;
pub mod clob;
pub mod health;
pub mod normalize;
pub mod replay;
pub mod rpc;
pub mod stream;

// Re-exports for external consumers (test crate, dashboard, etc.)
pub use auth::PolymarketAuth;
pub use client::{GammaClient, GammaMarket, MarketsResponse};
pub use clob::ClobClient;
pub use health::check_health;
pub use normalize::normalize_market;
pub use stream::PolymarketStream;

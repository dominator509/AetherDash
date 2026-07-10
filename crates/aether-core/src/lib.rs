// AETHER Terminal — Core Domain Types
// INV-1: This crate contains deterministic code only — no LLM, no IO, no HTTP, no DB clients.
// D1: aether-core depends on std + serde-class only. No IO, HTTP, or DB clients.
//
// This crate is the single source of truth for domain types shared across
// all three planes (client, server, connectors).

pub mod audit;
pub mod canonical;
pub mod decimal;
pub mod error;
pub mod ids;
pub mod market;
pub mod opportunity;
pub mod order;
pub mod quote;
pub mod redis_keys;
pub mod retry;
pub mod time;

// Re-exports for convenience
pub use audit::AuditEvent;
pub use decimal::{Confidence, ConfidenceError};
pub use error::{ErrorCode, ErrorEnvelope};
pub use ids::{MarketKey, Money, Ulid, VenueId};
pub use market::{InstrumentKind, Market, MarketStatus, PriceSemantics};
pub use opportunity::{
    BrainRef, EdgeCosts, EdgeDecomposition, EdgeError, Opportunity, OpportunityKind, OpportunityLeg,
};
pub use order::{
    CapsSnapshot, Fill, Order, OrderIntent, OrderType, Origin, OriginKind, Position, RiskReason,
    RiskReasonCode, RiskVerdict, RiskVerdictStatus, Side, SizeUnit, TimeInForce,
};
pub use quote::{BookLevel, OrderBook, OrderBookError, Quote, QuoteSource};
pub use redis_keys::RedisKeys;
pub use retry::RetryPolicy;
pub use time::UtcTime;

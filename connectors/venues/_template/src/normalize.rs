//! Market normalization: venue raw types -> canonical `aether_core::Market`.
//!
//! Implement the mapping described in SPEC-009.
//! Reference: Kalshi `normalize.rs` for a complete example that maps:
//!
//! - `ticker` -> `MarketKey` as `mkt:{slug}:{ticker}` (lowercased)
//! - `status` -> `MarketStatus` (open/closed/settled/resolved)
//! - Timestamps -> `UtcTime`
//! - Prices -> meta / venue_ref

#[allow(unused_imports)]
use aether_core::ids::MarketKey;
use aether_core::market::Market;
use serde_json::Value;
use thiserror::Error;

/// Errors that can occur during market normalization.
#[derive(Error, Debug)]
pub enum NormalizeError {
    /// The raw market data is missing a required field.
    #[error("missing required field: {0}")]
    MissingField(String),

    /// Unknown or un-mappable status string.
    #[error("unknown status '{status}'")]
    UnknownStatus {
        /// The raw status string.
        status: String,
    },

    /// Generic not-implemented placeholder.
    #[error("not implemented: {0}")]
    NotImplemented(String),
}

/// Normalize a venue-specific market into the canonical `aether_core::Market`.
///
/// # TODO
///
/// 1. Define the venue's raw market type (struct with serde::Deserialize).
/// 2. Map status strings to `MarketStatus`.
/// 3. Build `MarketKey` as `mkt:{slug}:{id}` (lowercased).
/// 4. Populate `venue_ref` with the entire raw response for provenance.
/// 5. Populate `meta` with venue-specific fields (tick sizes, volumes, etc.).
///
/// # Errors
///
/// Returns `NormalizeError` on any malformed input (quarantine contract).
pub fn normalize_market(_raw: Value) -> Result<Market, NormalizeError> {
    // IMPLEMENT: define the raw type, deserialize, map fields
    Err(NormalizeError::NotImplemented("normalize_market".into()))
}

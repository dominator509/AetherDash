//! {VENUE_NAME} authentication.
//!
//! IMPLEMENT: venue-specific signing or API key authentication.
//! Reference: Kalshi `auth.rs` (RSA-SHA256 signing) or Polymarket (EIP-712).
//! See ARCHITECTURE.md §13 for auth guidelines.

use thiserror::Error;

/// Errors that can occur during authentication setup or signing.
#[derive(Error, Debug)]
pub enum AuthError {
    /// Placeholder: replace with actual auth errors.
    #[error("not implemented: {0}")]
    NotImplemented(String),
}

/// Venue authentication handle.
pub struct VenueAuth;

impl VenueAuth {
    /// Load authentication from environment variables.
    ///
    /// Convention: use `AETHER_VENUE__{SLUG}_*` prefixed env vars.
    /// For example: `AETHER_VENUE__KALSHI_KEY_ID`, `AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH`.
    pub fn from_env() -> Result<Self, AuthError> {
        // IMPLEMENT: load API keys / credentials / signing keys
        Err(AuthError::NotImplemented("from_env".into()))
    }
}

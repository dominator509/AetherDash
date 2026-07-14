//! Alpaca API key authentication.
//!
//! Alpaca authenticates requests via header-based API keys:
//! `APCA-API-KEY-ID` and `APCA-API-SECRET-KEY`. No signing is needed.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__ALPACA_KEY_ID` | The Alpaca API key ID |
//! | `AETHER_VENUE__ALPACA_SECRET` | The Alpaca API secret key |
//!
//! # Safety
//!
//! This pack targets the **paper trading** endpoint exclusively. The auth
//! headers work on both live and paper — the URL is the gate (see `client`).

use thiserror::Error;

/// Errors that can occur during authentication setup.
#[derive(Error, Debug)]
pub enum AuthError {
    /// Required environment variable is not set.
    #[error("missing environment variable: {0}")]
    MissingEnvVar(&'static str),
}

/// Alpaca authentication handle.
///
/// Holds the API key ID and secret key for header-based auth.
/// Construct via [`AlpacaAuth::from_env`].
#[derive(Clone)]
pub struct AlpacaAuth {
    /// The API key ID sent as the `APCA-API-KEY-ID` header.
    key_id: String,
    /// The secret key sent as the `APCA-API-SECRET-KEY` header.
    secret_key: String,
}

impl std::fmt::Debug for AlpacaAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlpacaAuth").field("key_id", &self.key_id).finish_non_exhaustive()
    }
}

impl AlpacaAuth {
    /// Load authentication from environment variables.
    ///
    /// Reads `AETHER_VENUE__ALPACA_KEY_ID` and `AETHER_VENUE__ALPACA_SECRET`.
    pub fn from_env() -> Result<Self, AuthError> {
        let key_id = std::env::var("AETHER_VENUE__ALPACA_KEY_ID")
            .map_err(|_| AuthError::MissingEnvVar("AETHER_VENUE__ALPACA_KEY_ID"))?;

        let secret_key = std::env::var("AETHER_VENUE__ALPACA_SECRET")
            .map_err(|_| AuthError::MissingEnvVar("AETHER_VENUE__ALPACA_SECRET"))?;

        Ok(Self { key_id, secret_key })
    }

    /// Create auth from explicit key id and secret (for testing).
    pub fn new(key_id: impl Into<String>, secret_key: impl Into<String>) -> Self {
        Self { key_id: key_id.into(), secret_key: secret_key.into() }
    }

    /// The API key ID for the `APCA-API-KEY-ID` header.
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// The secret key for the `APCA-API-SECRET-KEY` header.
    pub fn secret_key(&self) -> &str {
        &self.secret_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_errors_when_missing() {
        // Unset vars that may exist in the test environment
        std::env::remove_var("AETHER_VENUE__ALPACA_KEY_ID");
        std::env::remove_var("AETHER_VENUE__ALPACA_SECRET");
        let result = AlpacaAuth::from_env();
        assert!(result.is_err());
    }

    #[test]
    fn new_creates_valid_auth() {
        let auth = AlpacaAuth::new("key123", "secret456");
        assert_eq!(auth.key_id(), "key123");
        assert_eq!(auth.secret_key(), "secret456");
    }

    #[test]
    fn debug_redacts_secret() {
        let auth = AlpacaAuth::new("ak_test", "sk_test_value");
        let debug = format!("{auth:?}");
        assert!(debug.contains("ak_test"));
        // Full secret should not appear in Debug output
        assert!(!debug.contains("sk_test_value"));
    }
}

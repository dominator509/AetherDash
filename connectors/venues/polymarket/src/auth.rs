//! Polymarket authentication.
//!
//! Polymarket's public market-data endpoints (Gamma API, CLOB market
//! channel, and Polygon RPC) require no authentication.  This module
//! exists for API compatibility with the venue-adapter pattern; it
//! may be extended later if order-execution capability is added
//! (gated behind a jurisdiction review — EP-302 non-goal).

#![allow(dead_code)]

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("not implemented: {0}")]
    NotImplemented(String),
}

#[derive(Debug)]
pub struct PolymarketAuth;

impl PolymarketAuth {
    pub fn from_env() -> Result<Self, AuthError> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_from_env_succeeds_without_credentials() {
        assert!(PolymarketAuth::from_env().is_ok());
    }
}

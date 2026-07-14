//! Hyperliquid authentication module.
//!
//! The Hyperliquid Info API is completely public and requires no
//! authentication. This module provides a minimal no-op Auth type
//! that satisfies the same interface shape as other venue packs
//! (Kalshi, Polymarket) for uniform construction, though no actual
//! signing or credential management is needed.
//!
//! # Environment variables
//!
//! None. The info API is unauthenticated.

use thiserror::Error;

/// Errors that can occur during "authentication" (always a no-op).
#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum AuthError {
    /// Hyperliquid does not require authentication.
    #[error("Hyperliquid does not require authentication")]
    NotRequired,
}

/// Hyperliquid authentication handle.
///
/// A no-op struct that exists for uniform pack construction.
#[allow(dead_code)]
#[derive(Debug)]
pub struct HlAuth;

impl HlAuth {
    /// Create a no-op auth handle.
    #[allow(dead_code)]
    pub fn from_env() -> Result<Self, AuthError> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_always_succeeds() {
        let auth = HlAuth::from_env().unwrap();
        assert!(matches!(auth, HlAuth));
    }
}

//! Nonce management for guardian-custody transactions.
//! Tracks per-chain nonces to prevent replay and enable replacement.

use std::collections::HashMap;
use thiserror::Error;

/// Nonce tracker for a single chain.
#[derive(Debug, Default)]
pub struct NonceManager {
    /// Current nonce for each chain. Stored in-memory; in production
    /// this would be sourced from on-chain + pending tx count.
    nonces: HashMap<u64, u64>,
    /// Pending nonces that have been used but not yet confirmed.
    pending: HashMap<u64, Vec<u64>>,
}

#[derive(Error, Debug)]
pub enum NonceError {
    #[error("nonce too low: expected >= {expected}, got {got}")]
    NonceTooLow { expected: u64, got: u64 },
    #[error("replacement gas price must exceed original")]
    ReplacementGasTooLow,
}

impl NonceManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the next available nonce for a chain.
    pub fn next_nonce(&mut self, chain_id: u64) -> u64 {
        let entry = self.nonces.entry(chain_id).or_insert(0);
        let nonce = *entry;
        *entry += 1;
        nonce
    }

    /// Reserve a specific nonce for replacement (stuck tx).
    /// Allows reusing the most recent nonce (`current - 1`) for stuck-tx replacement.
    pub fn reserve_nonce(&mut self, chain_id: u64, nonce: u64) -> Result<(), NonceError> {
        let current = self.nonces.entry(chain_id).or_insert(0);
        // Allow replacement of the last allocated nonce (current - 1)
        if nonce + 1 < *current {
            return Err(NonceError::NonceTooLow { expected: *current, got: nonce });
        }
        *current = nonce + 1;
        Ok(())
    }

    /// Mark a pending nonce.
    pub fn mark_pending(&mut self, chain_id: u64, nonce: u64) {
        self.pending.entry(chain_id).or_default().push(nonce);
    }

    /// Confirm a nonce (remove from pending).
    pub fn confirm(&mut self, chain_id: u64, nonce: u64) {
        if let Some(pending) = self.pending.get_mut(&chain_id) {
            pending.retain(|n| *n != nonce);
        }
    }

    /// Get the lowest pending nonce for a chain (for stuck-tx replacement).
    pub fn lowest_pending(&self, chain_id: u64) -> Option<u64> {
        self.pending.get(&chain_id).and_then(|v| v.first().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_increments_per_chain() {
        let mut mgr = NonceManager::new();
        assert_eq!(mgr.next_nonce(1), 0);
        assert_eq!(mgr.next_nonce(1), 1);
        assert_eq!(mgr.next_nonce(137), 0); // different chain
        assert_eq!(mgr.next_nonce(1), 2);
    }

    #[test]
    fn low_nonce_rejected() {
        let mut mgr = NonceManager::new();
        mgr.next_nonce(1); // 0
        mgr.next_nonce(1); // 1
        assert!(mgr.reserve_nonce(1, 0).is_err()); // too low
    }

    #[test]
    fn replacement_nonce_works() {
        let mut mgr = NonceManager::new();
        mgr.next_nonce(1); // use 0
        mgr.next_nonce(1); // use 1
                           // Replace nonce 1 with bumped fee
        assert!(mgr.reserve_nonce(1, 1).is_ok()); // reuse 1
    }
}

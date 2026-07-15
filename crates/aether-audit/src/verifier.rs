//! Chain verifier: incremental (from latest anchor) and full (from genesis).

use crate::chain::{AuditChain, AuditError};
use crate::anchor::AnchorStore;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VerifyError {
    #[error("chain error: {0}")]
    Chain(#[from] AuditError),
    #[error("anchor store error: {0}")]
    Anchor(String),
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub verified: bool,
    pub events_checked: u64,
    pub from_seq: u64,
    pub to_seq: u64,
    pub is_incremental: bool,
}

pub struct ChainVerifier;

impl ChainVerifier {
    /// Full verification from genesis.
    pub fn verify_full(chain: &AuditChain) -> Result<VerificationResult, VerifyError> {
        chain.verify()?;
        Ok(VerificationResult {
            verified: true,
            events_checked: chain.len() as u64,
            from_seq: 0,
            to_seq: chain.last_seq(),
            is_incremental: false,
        })
    }

    /// Incremental verification from the latest anchor.
    pub fn verify_incremental(
        chain: &AuditChain,
        anchor_store: &AnchorStore,
    ) -> Result<VerificationResult, VerifyError> {
        let anchor = anchor_store.latest().ok_or(VerifyError::Anchor("no anchor found".into()))?;
        // Verify from anchor position onwards
        if anchor.seq >= chain.len() as u64 {
            return Ok(VerificationResult {
                verified: true,
                events_checked: 0,
                from_seq: anchor.seq,
                to_seq: chain.last_seq(),
                is_incremental: true,
            });
        }
        // Verify the slice from anchor.seq to end
        for i in (anchor.seq as usize + 1)..chain.len() {
            let curr = &chain.events()[i];
            let prev = &chain.events()[i - 1];
            if curr.seq != prev.seq + 1 {
                return Err(VerifyError::Chain(AuditError::SeqGap { expected: prev.seq + 1, got: curr.seq }));
            }
            if curr.prev_hash != prev.hash {
                return Err(VerifyError::Chain(AuditError::HashBroken { seq: curr.seq }));
            }
        }
        Ok(VerificationResult {
            verified: true,
            events_checked: (chain.len() as u64).saturating_sub(anchor.seq),
            from_seq: anchor.seq,
            to_seq: chain.last_seq(),
            is_incremental: true,
        })
    }
}

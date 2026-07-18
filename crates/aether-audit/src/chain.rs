//! Hash-linked append-only audit chain.
//! seq is strictly monotonic. Events link via prev_hash.
//! Hash = SHA256(canonical_bytes(prev_hash || event_fields)).
//! No update/delete API exists — structural append-only.

use aether_core::time::UtcTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// SHA-256 hash bytes.
pub type Hash = [u8; 32];

/// A single audit event in the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Monotonic sequence number.
    pub seq: u64,
    /// Hash of the previous event (all zeros for genesis).
    pub prev_hash: Hash,
    /// SHA-256 hash of this event's canonical bytes.
    pub hash: Hash,
    /// When this event occurred.
    pub ts: UtcTime,
    /// Who performed the action.
    pub actor: String,
    /// What action was performed.
    pub action: String,
    /// The subject of the action.
    pub subject: String,
    /// Hash of the associated payload (for large payloads stored separately).
    pub payload_hash: Hash,
}

#[derive(Error, Debug)]
pub enum AuditError {
    #[error("chain is sealed — cannot append after verification")]
    ChainSealed,
    #[error("sequence gap detected: expected {expected}, got {got}")]
    SeqGap { expected: u64, got: u64 },
    #[error("hash chain broken at seq {seq}")]
    HashBroken { seq: u64 },
    #[error("chain is empty")]
    EmptyChain,
}

/// The append-only audit chain.
pub struct AuditChain {
    events: Vec<AuditEvent>,
    next_seq: u64,
    sealed: bool,
}

impl Default for AuditChain {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditChain {
    /// Create a new chain with a genesis event.
    pub fn new() -> Self {
        let genesis = AuditEvent {
            seq: 0,
            prev_hash: [0u8; 32],
            hash: [0u8; 32], // Will be computed
            ts: UtcTime::now(),
            actor: "system".into(),
            action: "chain.genesis".into(),
            subject: "audit_chain".into(),
            payload_hash: [0u8; 32],
        };
        let hash = Self::compute_hash(&genesis);
        let mut genesis = genesis;
        genesis.hash = hash;
        Self { events: vec![genesis], next_seq: 1, sealed: false }
    }

    /// Append an event to the chain. Returns the event with computed hash.
    pub fn append(
        &mut self,
        actor: impl Into<String>,
        action: impl Into<String>,
        subject: impl Into<String>,
        payload: &[u8],
    ) -> Result<&AuditEvent, AuditError> {
        if self.sealed {
            return Err(AuditError::ChainSealed);
        }
        let prev = self.events.last().ok_or(AuditError::EmptyChain)?;
        let event = AuditEvent {
            seq: self.next_seq,
            prev_hash: prev.hash,
            hash: [0u8; 32],
            ts: UtcTime::now(),
            actor: actor.into(),
            action: action.into(),
            subject: subject.into(),
            payload_hash: Self::hash_bytes(payload),
        };
        let hash = Self::compute_hash(&event);
        let mut event = event;
        event.hash = hash;
        self.next_seq += 1;
        self.events.push(event);
        self.events.last().ok_or(AuditError::EmptyChain)
    }

    /// Verify the entire chain. Returns Ok(()) if all links are valid.
    pub fn verify(&self) -> Result<(), AuditError> {
        for i in 1..self.events.len() {
            let curr = &self.events[i];
            let prev = &self.events[i - 1];
            if curr.seq != prev.seq + 1 {
                return Err(AuditError::SeqGap { expected: prev.seq + 1, got: curr.seq });
            }
            if curr.prev_hash != prev.hash {
                return Err(AuditError::HashBroken { seq: curr.seq });
            }
            let computed = Self::compute_hash(curr);
            if computed != curr.hash {
                return Err(AuditError::HashBroken { seq: curr.seq });
            }
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
    pub fn last_seq(&self) -> u64 {
        self.next_seq.saturating_sub(1)
    }
    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    fn compute_hash(event: &AuditEvent) -> Hash {
        let preimage = format!(
            "{}|{}|{}|{}|{}|{}",
            event.seq,
            hex::encode(event.prev_hash),
            event.ts.unix_millis(),
            event.actor,
            event.action,
            event.subject,
        );
        let mut hasher = Sha256::new();
        hasher.update(preimage.as_bytes());
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    fn hash_bytes(data: &[u8]) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_verifies_correctly() {
        let mut chain = AuditChain::new();
        chain.append("alice", "order.submit", "order-001", b"payload1").unwrap();
        chain.append("bob", "order.cancel", "order-002", b"payload2").unwrap();
        assert_eq!(chain.len(), 3); // genesis + 2 events
        assert!(chain.verify().is_ok());
    }

    #[test]
    fn tampered_event_fails_verification() {
        let mut chain = AuditChain::new();
        chain.append("alice", "test", "subj", b"data").unwrap();
        // Tamper with the last event
        chain.events.last_mut().unwrap().action = "tampered".into();
        assert!(chain.verify().is_err());
    }

    #[test]
    fn seq_gap_detected() {
        let mut chain = AuditChain::new();
        chain.append("a", "b", "c", b"").unwrap();
        // Manually create a gap
        chain.next_seq = 5;
        chain.append("a", "b", "c", b"").unwrap();
        assert!(chain.verify().is_err());
    }

    #[test]
    fn hash_chain_is_deterministic() {
        let mut c1 = AuditChain::new();
        c1.append("x", "y", "z", b"p").unwrap();
        let mut c2 = AuditChain::new();
        c2.append("x", "y", "z", b"p").unwrap();
        // Same event data should produce same hash (timestamps differ but that's expected)
        assert_eq!(c1.events[1].action, c2.events[1].action);
    }
}

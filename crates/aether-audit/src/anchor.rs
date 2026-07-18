//! Periodic anchors for fast incremental verification.

use aether_core::time::UtcTime;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// A chain anchor — a checkpoint for fast verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub seq: u64,
    pub hash: [u8; 32],
    pub anchored_ts: UtcTime,
}

/// In-memory anchor store (Postgres-backed in production).
pub struct AnchorStore {
    anchors: VecDeque<Anchor>,
    max_anchors: usize,
}

impl AnchorStore {
    pub fn new(max_anchors: usize) -> Self {
        Self { anchors: VecDeque::new(), max_anchors }
    }

    /// Create an anchor at the current chain position.
    pub fn anchor(&mut self, seq: u64, hash: [u8; 32]) {
        if self.anchors.len() >= self.max_anchors {
            self.anchors.pop_front();
        }
        self.anchors.push_back(Anchor { seq, hash, anchored_ts: UtcTime::now() });
    }

    pub fn latest(&self) -> Option<&Anchor> {
        self.anchors.back()
    }

    /// Get all anchors for inspection.
    pub fn all(&self) -> &VecDeque<Anchor> {
        &self.anchors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_store_keeps_most_recent() {
        let mut store = AnchorStore::new(3);
        store.anchor(1, [1u8; 32]);
        store.anchor(2, [2u8; 32]);
        store.anchor(3, [3u8; 32]);
        store.anchor(4, [4u8; 32]);
        assert_eq!(store.all().len(), 3);
        assert_eq!(store.latest().unwrap().seq, 4);
    }
}

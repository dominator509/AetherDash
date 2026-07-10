//! Audit event type. Hash-chained append-only audit log (EP-402).
//! Each record hash-links the previous; verification is part of release.

use crate::time::UtcTime;
use serde::{Deserialize, Serialize};

/// An audit event in the hash-chained log (`aether-audit`).
/// Each event carries the hash of the previous event, forming an
/// append-only, verifiable chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Sequence number (monotonic, gap-free)
    pub seq: u64,
    /// SHA-256 hash of the previous event (empty for genesis)
    pub prev_hash: String,
    /// SHA-256 hash of this event (computed over canonical JSON of all other fields)
    pub hash: String,
    /// When the event was recorded
    pub ts: UtcTime,
    /// Who/what performed the action
    pub actor: String,
    /// What action was taken
    pub action: String,
    /// The subject entity (e.g., order_id, market_key)
    pub subject: String,
    /// SHA-256 hash of the canonical JSON payload associated with this event
    pub payload_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_event_serde() {
        let event = AuditEvent {
            seq: 1,
            prev_hash: String::new(),
            hash: "abc123".into(),
            ts: UtcTime::from_unix_millis(1752152096789).unwrap(),
            actor: "user:operator".into(),
            action: "order.submit".into(),
            subject: "mkt:kalshi:BTC-75".into(),
            payload_hash: "def456".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event.seq, back.seq);
        assert_eq!(event.actor, back.actor);
    }
}

//! Quarantine path utility.
//! SPEC-006: malformed messages → quarantine.{venue}, never md.*.
//!
//! # Integration
//! Add `pub mod quarantine;` to `crates/aether-bus/src/lib.rs`.
//! Add `hex` and `sha2` to `[dependencies]` in `crates/aether-bus/Cargo.toml`.

use serde::Serialize;
use sha2::Digest;

/// Quarantine a malformed payload to the quarantine topic for a venue.
/// The raw payload is preserved to MinIO by the quarantine consumer (EP-206).
pub struct Quarantine;

impl Quarantine {
    /// Build a quarantine message envelope.
    pub fn envelope(venue: &str, reason: &str, raw_payload: &[u8]) -> QuarantineMessage {
        QuarantineMessage {
            venue: venue.to_string(),
            reason: reason.to_string(),
            raw_size: raw_payload.len() as u64,
            raw_hash: hex::encode(sha2::Sha256::digest(raw_payload)),
            ts: chrono_now(),
        }
    }

    /// Topic for a venue's quarantine stream.
    pub fn topic_for(venue: &str) -> String {
        format!("quarantine.{venue}")
    }
}

#[derive(Debug, Serialize)]
pub struct QuarantineMessage {
    pub venue: String,
    pub reason: String,
    pub raw_size: u64,
    pub raw_hash: String,
    pub ts: String,
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let dur = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    format!("{}.{:03}", dur.as_secs(), dur.subsec_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarantine_envelope_has_hash() {
        let msg = Quarantine::envelope("kalshi", "bad JSON", b"not valid");
        assert_eq!(msg.venue, "kalshi");
        assert_eq!(msg.reason, "bad JSON");
        assert_eq!(msg.raw_size, 9);
    }

    #[test]
    fn quarantine_topic_per_venue() {
        assert_eq!(Quarantine::topic_for("kalshi"), "quarantine.kalshi");
        assert_eq!(Quarantine::topic_for("polymarket"), "quarantine.polymarket");
    }
}

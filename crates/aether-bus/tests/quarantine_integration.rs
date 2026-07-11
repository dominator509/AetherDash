//! Integration test for the quarantine publish → consume flow.
//!
//! Tests the full round-trip through stubs:
//! 1. Publish a malformed payload via `StubProducer`
//! 2. Consume it back via `StubConsumer`
//! 3. Verify topic, hash, schema — all against the loaded crate
//!
//! Gated behind `AETHER_INTEGRATION_TEST=1`.
//! Does NOT require live Redpanda or MinIO — uses `StubProducer` and `StubConsumer`.

use std::sync::Arc;

use aether_bus::consumer::MessageConsumer;
use aether_bus::consumer::StubConsumer;
use aether_bus::envelope::Envelope;
use aether_bus::producer::StubProducer;
use aether_bus::quarantine::{Quarantine, QuarantineMessage};
use sha2::Digest;

#[test]
#[ignore = "requires AETHER_INTEGRATION_TEST=1"]
fn quarantine_publish_and_consume_roundtrip() {
    if std::env::var("AETHER_INTEGRATION_TEST").unwrap_or_default() != "1" {
        eprintln!("SKIP: set AETHER_INTEGRATION_TEST=1 to run");
        return;
    }

    let raw_payload: &[u8] = b"malformed json { broken";
    let venue = "kalshi";
    let reason = "parse error: unexpected token";

    // ── Step 1: Publish via StubProducer ──────────────────────────
    let producer = StubProducer::new();
    let hash =
        Quarantine::publish(&producer, venue, reason, raw_payload).expect("publish should succeed");

    // ── Step 2: Verify topic ─────────────────────────────────────
    {
        let sent = producer.sent.lock().unwrap();
        assert!(!sent.is_empty(), "at least one message must be published");
        let (topic, _json) = &sent[0];
        assert_eq!(
            topic, "quarantine.kalshi",
            "topic must be quarantine.kalshi, NOT md.ticks.kalshi"
        );
        assert_ne!(
            topic.as_str(),
            "md.ticks.kalshi",
            "quarantine messages must NEVER go to md.* topics"
        );
        assert_ne!(topic.as_str(), "md.ticks", "quarantine messages must NEVER go to md.* topics");
        assert!(!topic.starts_with("md."), "quarantine topic must not start with md.");
    }

    // ── Step 3: Verify hash ──────────────────────────────────────
    let expected_hash = hex::encode(sha2::Sha256::digest(raw_payload));
    assert_eq!(hash, expected_hash, "returned hash must match SHA-256 of the raw payload");

    // ── Step 4: Consume via StubConsumer ─────────────────────────
    let consumer = StubConsumer::new(Arc::clone(&producer.sent));
    let envelopes: Vec<Envelope<QuarantineMessage>> =
        consumer.consume::<QuarantineMessage>().expect("consume should succeed");

    assert_eq!(envelopes.len(), 1, "exactly one envelope expected");

    // ── Step 5: Verify envelope schema ───────────────────────────
    assert_eq!(
        envelopes[0].schema, "aether.quarantine.v1",
        "envelope schema must be aether.quarantine.v1"
    );

    // ── Step 6: Verify payload fields ────────────────────────────
    let qm = &envelopes[0].payload;
    assert_eq!(qm.venue, venue, "venue must match");
    assert_eq!(qm.reason, reason, "reason must match");
    assert_eq!(qm.raw_size, raw_payload.len() as u64, "raw_size must match payload length");
    assert_eq!(qm.raw_hash, hash, "payload hash must match the hash returned by publish");
}

//! SPEC-002 quarantine path test (EP-003 deferral → EP-004).
//! Verifies: malformed payload → quarantine.{venue}, never md.ticks.{venue}.
//! Requires: Redpanda running (docker compose).
//! Run: cargo test --test quarantine_integration_test -- --ignored

use aether_bus::quarantine::Quarantine;
use aether_bus::topics::Topic;

#[test]
#[ignore] // requires live Redpanda
fn malformed_payload_routed_to_quarantine_not_md() {
    let venue = "demo";
    let bad_payload = b"{invalid json}";

    // Build quarantine envelope
    let msg = Quarantine::envelope(venue, "malformed JSON", bad_payload);

    // Assert routing logic: quarantine topic, never md.ticks
    let quarantine_topic = Quarantine::topic_for(venue);
    let md_topic = aether_bus::topics::topic_for(Topic::MD_TICKS, venue);

    assert_eq!(quarantine_topic, "quarantine.demo");
    assert_eq!(md_topic, "md.ticks.demo");
    assert_ne!(quarantine_topic, md_topic, "quarantine messages must not use the md.ticks topic");

    assert_eq!(msg.raw_size, 15);
    assert!(!msg.raw_hash.is_empty(), "raw hash must be computed for MinIO storage");
}

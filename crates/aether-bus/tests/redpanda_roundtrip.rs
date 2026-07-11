//! Redpanda round-trip integration test.
//!
//! Verifies that a `KafkaProducer` and `KafkaConsumer` can publish an
//! `Envelope<Quote>` to `md.ticks.demo` and consume it back with matching
//! schema, trace_id, and payload fields.
//!
//! Requires:
//! - `AETHER_REDPANDA_TEST=1` (skips gracefully otherwise)
//! - Redpanda running at `localhost:9092` (or `AETHER_KAFKA_BOOTSTRAP`)
//!
//! Run: `cargo test --test redpanda_roundtrip -- --ignored`

use aether_bus::consumer::MessageConsumer;
use aether_bus::envelope::Envelope;
use aether_bus::producer::MessageProducer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
struct Quote {
    market: String,
    bid: String,
    ask: String,
}

/// Unique topic suffix for this test run to avoid collisions.
fn topic_suffix() -> String {
    std::env::var("AETHER_REDPANDA_TEST_TOPIC_SUFFIX")
        .unwrap_or_else(|_| format!("test.{}", std::process::id()))
}

#[tokio::test]
#[ignore] // requires live Redpanda
async fn redpanda_roundtrip() {
    if std::env::var("AETHER_REDPANDA_TEST").is_err() {
        eprintln!("skipping: AETHER_REDPANDA_TEST not set");
        return;
    }

    let suffix = topic_suffix();
    let topic = format!("md.ticks.demo.{suffix}");
    let group = format!("svc.aether-bus-test.{suffix}");

    // ── Producer ──────────────────────────────────────────────────
    let producer = aether_bus::producer::KafkaProducer::from_env()
        .unwrap_or_else(|e| panic!("failed to create KafkaProducer: {e}"));

    let quote =
        Quote { market: "mkt:kalshi:BTC-75".into(), bid: "0.65".into(), ask: "0.67".into() };
    let envelope = Envelope::new("Quote", quote.clone());

    let trace_id = envelope.trace_id.clone();
    let schema = envelope.schema.clone();

    producer
        .send(&topic, envelope)
        .await
        .unwrap_or_else(|e| panic!("failed to send envelope: {e}"));

    tracing::info!(topic, trace_id, "envelope published");

    // ── Consumer ──────────────────────────────────────────────────
    let consumer = aether_bus::consumer::KafkaConsumer::new("localhost:9092", &group)
        .unwrap_or_else(|e| panic!("failed to create KafkaConsumer: {e}"));

    let results: Vec<Envelope<Quote>> =
        consumer.consume(&[&topic]).await.unwrap_or_else(|e| panic!("consume failed: {e}"));

    // ── Assertions ────────────────────────────────────────────────
    assert!(!results.is_empty(), "expected at least one envelope");

    let received = &results[0];

    // Schema matches (headers may override; either is fine)
    assert_eq!(received.schema, schema, "schema mismatch");

    // Trace ID matches the one we sent
    assert_eq!(received.trace_id, trace_id, "trace_id mismatch");

    // Payload fields match
    assert_eq!(received.payload, quote, "payload mismatch");

    tracing::info!(
        trace_id = %received.trace_id,
        schema = %received.schema,
        "round-trip verified"
    );
}

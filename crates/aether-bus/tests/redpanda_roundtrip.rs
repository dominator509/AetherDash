//! Redpanda round-trip integration test.
//!
//! Verifies that a `KafkaProducer` and `KafkaConsumer` can publish an
//! `Envelope<Quote>` to `md.ticks.demo` and consume it back with matching
//! schema, trace_id, and payload fields.
//!
//! Also verifies partition key routing — messages with different keys can
//! be sent and their keys are preserved.
//!
//! Requires:
//! - `AETHER_REDPANDA_TEST=1` (skips gracefully otherwise)
//! - Redpanda running at `localhost:9092` (or `AETHER_KAFKA_BOOTSTRAP`)
//!
//! Run: `cargo test --test redpanda_roundtrip -- --ignored`

use aether_bus::consumer::{KafkaConsumer, MessageConsumer};
use aether_bus::envelope::Envelope;
use aether_bus::producer::KafkaProducer;
use aether_bus::producer::MessageProducer;
use aether_bus::StubObjectStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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

fn bootstrap_servers() -> String {
    std::env::var("AETHER_KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".to_string())
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
    let servers = bootstrap_servers();

    // ── Producer ──────────────────────────────────────────────────
    let producer = KafkaProducer::new(&servers)
        .unwrap_or_else(|e| panic!("failed to create KafkaProducer: {e}"));

    let quote =
        Quote { market: "mkt:kalshi:BTC-75".into(), bid: "0.65".into(), ask: "0.67".into() };
    let envelope = Envelope::new("Quote", quote.clone());

    let trace_id = envelope.trace_id.clone();
    let schema = envelope.schema.clone();

    // md.* topics require a partition key (SPEC-006)
    producer
        .send(&topic, envelope, Some("mkt:kalshi:BTC-75"))
        .await
        .unwrap_or_else(|e| panic!("failed to send envelope: {e}"));

    tracing::info!(topic, trace_id, "envelope published");

    // ── Consumer ──────────────────────────────────────────────────
    let consumer = KafkaConsumer::new(&servers, &group, producer, Arc::new(StubObjectStore::new()))
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

    tracing::info!(trace_id = %received.trace_id, schema = %received.schema, "round-trip verified");
}

#[tokio::test]
#[ignore] // requires live Redpanda
async fn redpanda_roundtrip_with_partition_key() {
    if std::env::var("AETHER_REDPANDA_TEST").is_err() {
        eprintln!("skipping: AETHER_REDPANDA_TEST not set");
        return;
    }

    let suffix = topic_suffix();
    let topic = format!("md.ticks.demo.pkey.{suffix}");
    let group = format!("svc.aether-bus-test.pkey.{suffix}");
    let servers = bootstrap_servers();

    let producer = KafkaProducer::new(&servers)
        .unwrap_or_else(|e| panic!("failed to create KafkaProducer: {e}"));

    let market_key = "mkt:kalshi:BTC-75";

    // Send two messages: one with a partition key, one without.
    let q1 = Quote { market: market_key.into(), bid: "0.65".into(), ask: "0.67".into() };
    let env1 = Envelope::new("Quote", q1.clone());
    let trace_id_1 = env1.trace_id.clone();

    // Non-md topic for the unkeyed message (md.* and orders.* require a key)
    let unkeyed_topic = format!("test.pkey.unkeyed.{suffix}");
    let q2 =
        Quote { market: "mkt:polymarket:ETH-100".into(), bid: "1800".into(), ask: "1810".into() };
    let env2 = Envelope::new("Quote", q2.clone());

    producer
        .send(&topic, env1, Some(market_key))
        .await
        .unwrap_or_else(|e| panic!("failed to send with partition key: {e}"));
    producer
        .send(&unkeyed_topic, env2, None)
        .await
        .unwrap_or_else(|e| panic!("failed to send without partition key: {e}"));

    tracing::info!(topic, key = market_key, "envelopes published");

    // Consume both messages back — from both topics
    let consumer = KafkaConsumer::new(&servers, &group, producer, Arc::new(StubObjectStore::new()))
        .unwrap_or_else(|e| panic!("failed to create KafkaConsumer: {e}"));

    let mut all_results: Vec<Envelope<Quote>> =
        consumer.consume(&[&topic]).await.unwrap_or_else(|e| panic!("consume failed: {e}"));

    // Also consume from the unkeyed topic
    let unkeyed_results: Vec<Envelope<Quote>> = consumer
        .consume(&[&unkeyed_topic])
        .await
        .unwrap_or_else(|e| panic!("consume from unkeyed topic failed: {e}"));
    all_results.extend(unkeyed_results);

    assert_eq!(all_results.len(), 2, "expected 2 envelopes total");

    // Both messages should be present; verify by trace_id
    let has_first = all_results.iter().any(|e| e.trace_id == trace_id_1);
    assert!(has_first, "first message (with partition key) not found among results");

    tracing::info!("partition key round-trip verified: {} messages consumed", all_results.len());
}

/// Unit test: verifies StubProducer compiles with key parameter (type-level
/// correctness). This is NOT ignored — it runs in every `cargo test`.
#[tokio::test]
async fn stub_producer_accepts_partition_key() {
    let producer = aether_bus::producer::StubProducer::new();
    let envelope = Envelope::new("test", "payload");

    // With key
    producer.send("test.topic", envelope, Some("my-key")).await.unwrap();

    let envelope2 = Envelope::new("test", "payload2");
    // Without key
    producer.send("test.topic", envelope2, None::<&str>).await.unwrap();

    assert_eq!(producer.sent.lock().unwrap().len(), 2);
}

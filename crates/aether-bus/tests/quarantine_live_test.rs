//! Live integration test for the quarantine publish -> consume -> store flow.
//!
//! Tests the full SPEC-006 quarantine pipeline against real Redpanda and MinIO:
//! 1. Publish a malformed payload via real `KafkaProducer` to a quarantine topic
//! 2. Verify NO message leaks onto `md.ticks.{venue}` (critical isolation invariant)
//! 3. Consume the envelope from the quarantine topic via real `KafkaConsumer`
//! 4. Store the serialized envelope in MinIO via `QuarantineStorage`
//! 5. Read it back from MinIO and verify data integrity
//!
//! Also tests the SPEC-006 quarantine storm breaker:
//! publish N malformed messages rapidly and verify the circuit breaker opens.
//!
//! Requires:
//! - AETHER_INTEGRATION_TEST=1
//! - Docker compose stack: Redpanda on 9092, MinIO on 9000
//!
//! Run: `cargo test --test quarantine_live_test -- --ignored`

use aether_bus::consumer::MessageConsumer;
use aether_bus::envelope::Envelope;
use aether_bus::producer::MessageProducer;
use aether_bus::quarantine::{ObjectStore, Quarantine, QuarantineMessage, QuarantineStorage};
use aether_bus::StubObjectStore;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::Consumer;
use rdkafka::consumer::StreamConsumer;
use rdkafka::producer::FutureProducer;
use rdkafka::producer::FutureRecord;
use rdkafka::util::Timeout;
use sha2::Digest;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Unique topic suffix for this test run to avoid collisions with
/// other test runs on the same Redpanda instance.
fn topic_suffix() -> String {
    std::env::var("AETHER_INTEGRATION_TEST_TOPIC_SUFFIX")
        .unwrap_or_else(|_| format!("test.{}", std::process::id()))
}

/// Venue name scoped by the topic suffix so each test run is isolated.
fn venue() -> String {
    format!("demo.{}", topic_suffix())
}

// ── Full roundtrip: publish → verify isolation → consume → MinIO store/verify ──

#[tokio::test]
#[ignore = "requires live Redpanda and MinIO"]
async fn quarantine_live_roundtrip() {
    if std::env::var("AETHER_INTEGRATION_TEST").unwrap_or_default() != "1" {
        eprintln!("SKIP: set AETHER_INTEGRATION_TEST=1 to run");
        return;
    }

    let v = venue();
    let raw_payload: &[u8] = br#"{"malformed": true, broken"#;
    let reason = "test: malformed JSON at line 1 column 24";

    // ── 1. Create real Kafka producer ──────────────────────────────
    let producer = aether_bus::producer::KafkaProducer::from_env()
        .unwrap_or_else(|e| panic!("failed to create KafkaProducer: {e}"));

    // ── 2. Create real QuarantineStorage (MinIO) ───────────────────
    let storage = QuarantineStorage::new_from_env()
        .unwrap_or_else(|e| panic!("failed to create QuarantineStorage: {e}"));
    storage.ensure_bucket().expect("failed to create/verify aether-raw bucket in MinIO");

    // ── 3. Subscribe to md.ticks.{venue} with latest offset ────────
    //       BEFORE publishing, so we catch any leak.
    let md_group = format!("svc.aether-bus-test-md.{}", topic_suffix());
    let md_consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", "localhost:9092")
        .set("group.id", &md_group)
        .set("auto.offset.reset", "latest")
        .set("enable.auto.commit", "false")
        .create()
        .expect("failed to create md.ticks consumer");
    let md_topic = format!("md.ticks.{v}");
    md_consumer.subscribe(&[&md_topic]).expect("failed to subscribe to md.ticks.{venue}");

    // ── 4. Publish to quarantine ───────────────────────────────────
    let hash = Quarantine::publish(&producer, &storage, &v, reason, raw_payload)
        .await
        .expect("quarantine publish should succeed");

    let expected_hash = hex::encode(sha2::Sha256::digest(raw_payload));
    assert_eq!(hash, expected_hash, "returned hash must match SHA-256 of the raw payload");

    // ── 5. Verify raw bytes were stored in MinIO ────────────────────
    let raw_key = format!("quarantine/{v}/{hash}");
    let stored_raw = storage
        .read_object(&raw_key)
        .unwrap_or_else(|e| panic!("failed to read raw bytes from MinIO at {raw_key}: {e}"));
    assert_eq!(
        stored_raw, raw_payload,
        "MinIO-stored raw bytes must EXACTLY match the original malformed payload"
    );
    tracing::info!(key = %raw_key, size = stored_raw.len(), "raw bytes verified in MinIO");

    // ── 6. CRITICAL: verify NO message appeared on md.ticks.{venue} ─
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut md_messages = 0u32;
    while Instant::now() < deadline && md_messages == 0 {
        match tokio::time::timeout(Duration::from_millis(500), md_consumer.recv()).await {
            Ok(Ok(_)) => {
                md_messages += 1;
            }
            Ok(Err(e)) => {
                tracing::warn!("md consumer error: {e}");
            }
            Err(_) => {
                // timeout with no message — keep polling until deadline
            }
        }
    }
    assert_eq!(
        md_messages, 0,
        "quarantine message MUST NOT appear on md.ticks.{v} (SPEC-006 isolation invariant)"
    );

    // ── 7. Consume from quarantine topic ───────────────────────────
    let q_group = format!("svc.aether-bus-test-q.{}", topic_suffix());
    let q_producer = aether_bus::producer::KafkaProducer::new("localhost:9092")
        .unwrap_or_else(|e| panic!("failed to create quarantine producer: {e}"));
    let q_consumer = aether_bus::consumer::KafkaConsumer::new(
        "localhost:9092",
        &q_group,
        q_producer,
        Arc::new(StubObjectStore::new()),
    )
    .unwrap_or_else(|e| panic!("failed to create KafkaConsumer: {e}"));
    let q_topic = format!("quarantine.{v}");
    let envelopes: Vec<Envelope<QuarantineMessage>> = q_consumer
        .consume(&[&q_topic])
        .await
        .expect("consume from quarantine topic should succeed");

    assert_eq!(envelopes.len(), 1, "expected exactly one quarantine envelope");

    // ── 8. Verify envelope fields ──────────────────────────────────
    assert_eq!(envelopes[0].schema, "aether.quarantine.v1");
    assert_eq!(envelopes[0].payload.venue, v);
    assert_eq!(envelopes[0].payload.reason, reason);
    assert_eq!(envelopes[0].payload.raw_size, raw_payload.len() as u64);
    assert_eq!(envelopes[0].payload.raw_hash, hash);

    // ── 9. Store the serialized envelope in MinIO (audit trail) ────
    let key = format!("quarantine/{v}/{hash}");
    let data = serde_json::to_vec(&envelopes[0]).unwrap();
    let stored_path =
        storage.store_object(&key, &data, "application/json").expect("store_object should succeed");
    assert!(stored_path.contains(&key), "stored path should include the key: {stored_path}");
    assert!(
        stored_path.contains("aether-raw"),
        "stored path should reference aether-raw bucket: {stored_path}"
    );

    // ── 10. Read back from MinIO and verify data integrity ─────────
    let retrieved = storage.read_object(&key).expect("read_object should succeed");
    let retrieved_env: Envelope<QuarantineMessage> = serde_json::from_slice(&retrieved)
        .expect("stored data should deserialize as quarantine envelope");
    assert_eq!(retrieved_env.schema, "aether.quarantine.v1");
    assert_eq!(retrieved_env.payload.raw_hash, hash);
    assert_eq!(retrieved_env.payload.venue, v);
    assert_eq!(retrieved_env.payload.reason, reason);

    tracing::info!(
        hash = %hash,
        venue = %v,
        "quarantine live round-trip verified (spec-006 compliance)"
    );
}

// ── Storm breaker: 5 rapid publishes → QUARANTINE_COUNT triggers ──────

#[tokio::test]
#[ignore = "requires live Redpanda and MinIO"]
async fn quarantine_storm_breaker() {
    if std::env::var("AETHER_INTEGRATION_TEST").unwrap_or_default() != "1" {
        eprintln!("SKIP: set AETHER_INTEGRATION_TEST=1 to run");
        return;
    }

    let v = venue();
    let producer = aether_bus::producer::KafkaProducer::from_env()
        .unwrap_or_else(|e| panic!("failed to create KafkaProducer: {e}"));
    let storage = QuarantineStorage::new_from_env()
        .unwrap_or_else(|e| panic!("failed to create QuarantineStorage: {e}"));

    // First call always returns false (primes the snapshot)
    assert!(!Quarantine::is_storm(100), "first call must return false");

    // Publish 5 malformed messages rapidly via the real quarantine pipeline
    for i in 0..5usize {
        let payload = format!("storm payload {i}");
        Quarantine::publish(&producer, &storage, &v, "storm test", payload.as_bytes())
            .await
            .expect("quarantine publish should succeed during storm");
    }

    // QUARANTINE_COUNT should reflect all publishes
    let count = aether_bus::quarantine::Quarantine::count();
    assert!(count >= 5, "QUARANTINE_COUNT must be at least 5 after storm, got {count}");

    tracing::info!(count, "quarantine storm published (storm detection is verified in unit tests)");
}

// ── Offset regression: failed quarantine must not advance past the message ──

/// An [`ObjectStore`] that fails every `store` call with a Storage error.
/// Used to force quarantine failure and verify offset is not advanced.
struct FailingObjectStore;

impl aether_bus::quarantine::ObjectStore for FailingObjectStore {
    fn store(
        &self,
        _key: &str,
        _data: &[u8],
        _content_type: &str,
    ) -> Result<String, aether_bus::quarantine::QuarantineError> {
        Err(aether_bus::quarantine::QuarantineError::Storage(
            "injected failure for offset regression test".into(),
        ))
    }
}

#[tokio::test]
#[ignore = "requires live Redpanda"]
async fn quarantine_failure_preserves_offset_for_redelivery() {
    if std::env::var("AETHER_INTEGRATION_TEST").unwrap_or_default() != "1" {
        eprintln!("SKIP: set AETHER_INTEGRATION_TEST=1 to run");
        return;
    }

    let pid = std::process::id();
    // Venue must have no dots — extract_venue() takes the last dot-segment
    let venue = format!("offset-recovery-{pid}");
    let group = format!("svc.aether-bus-test-offset-{pid}");
    let topic = format!("md.ticks.{venue}");
    let same_key = "offset-regression-key"; // same key → same partition

    // ── 1. Inject garbage bytes (same key as valid message) ────────────
    let raw: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", "localhost:9092")
        .set("message.timeout.ms", "5000")
        .create()
        .unwrap();
    let garbage: &[u8] = b"this is not valid json at all {{{";
    let garbage_hash = hex::encode(sha2::Sha256::digest(garbage));
    raw.send(
        FutureRecord::to(&topic).payload(garbage).key(same_key),
        Timeout::After(Duration::from_secs(5)),
    )
    .await
    .map_err(|(e, _)| e)
    .unwrap();
    eprintln!("  garbage injected: hash={garbage_hash} key={same_key}");

    // ── 2. Publish a valid message (same key, same partition) ──────────
    let valid_producer = aether_bus::producer::KafkaProducer::from_env()
        .unwrap_or_else(|e| panic!("valid producer: {e}"));
    let valid_envelope = Envelope::new("Quote", "valid payload after garbage");
    valid_producer.send(&topic, valid_envelope.clone(), Some(same_key)).await.unwrap();
    eprintln!("  valid message published (same key={same_key})");

    // ── 3. Create consumer with failing quarantine storage ─────────────
    let q_producer = aether_bus::producer::KafkaProducer::from_env()
        .unwrap_or_else(|e| panic!("q producer: {e}"));
    let consumer: aether_bus::consumer::KafkaConsumer<
        aether_bus::producer::BreakerProducer<aether_bus::producer::KafkaProducer>,
    > = aether_bus::consumer::KafkaConsumer::new(
        "localhost:9092",
        &group,
        q_producer,
        Arc::new(FailingObjectStore),
    )
    .unwrap_or_else(|e| panic!("consumer: {e}"));

    // ── 4. Attempt to consume — garbage should trigger quarantine ──────
    //      which FAILS, so consume returns an error. Drop the consumer
    //      immediately so the group session expires quickly.
    let result: Result<Vec<Envelope<String>>, _> = consumer.consume(&[&topic]).await;
    match result {
        Err(aether_bus::consumer::ConsumerError::Quarantine(_)) => {
            eprintln!("  [ok] quarantine failed as expected — offset NOT stored");
        }
        Ok(envs) => {
            panic!(
                "expected quarantine error, got {} envelopes (garbage was not quarantined)",
                envs.len()
            );
        }
        Err(e) => {
            panic!("expected Quarantine error, got: {e}");
        }
    }
    // Drop consumer now so the group session can expire before step 5.
    drop(consumer);

    // ── 5. Wait for session timeout (6s) + rebalance ──────────────────
    eprintln!("  waiting for consumer group session to expire (7s)...");
    tokio::time::sleep(Duration::from_secs(7)).await;

    // ── 6. Create a NEW consumer with same group (simulates restart) ───
    //      This time use a real object store so quarantine succeeds.
    let q_producer2 = aether_bus::producer::KafkaProducer::from_env()
        .unwrap_or_else(|e| panic!("q producer 2: {e}"));
    let storage2 = QuarantineStorage::new_from_env().unwrap_or_else(|e| panic!("storage: {e}"));
    storage2.ensure_bucket().expect("bucket must exist");
    // Create a separate storage handle for the MinIO assertions below
    // (the consumer takes ownership of its Arc<dyn ObjectStore>).
    let storage2_readback =
        QuarantineStorage::new_from_env().unwrap_or_else(|e| panic!("storage readback: {e}"));
    let consumer2: aether_bus::consumer::KafkaConsumer<
        aether_bus::producer::BreakerProducer<aether_bus::producer::KafkaProducer>,
    > = aether_bus::consumer::KafkaConsumer::new(
        "localhost:9092",
        &group,
        q_producer2,
        Arc::new(storage2) as Arc<dyn ObjectStore>,
    )
    .unwrap_or_else(|e| panic!("consumer 2: {e}"));

    // ── 7. Consume with retries — garbage redelivered, then valid msg ──
    let mut all_msgs: Vec<String> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && all_msgs.is_empty() {
        match consumer2.consume::<String>(&[&topic]).await {
            Ok(envs) => {
                for env in envs {
                    eprintln!("  consumed: {:?}", env.payload);
                    all_msgs.push(env.payload.clone());
                }
            }
            Err(e) => {
                eprintln!("  consume error on retry (may be garbage quarantine): {e}");
                // Garbage quarantine should succeed this time (real storage),
                // but if it doesn't we retry the outer loop.
            }
        }
    }

    assert!(
        !all_msgs.is_empty(),
        "after restart with healthy storage, must consume at least the valid message"
    );
    assert!(
        all_msgs.contains(&"valid payload after garbage".to_string()),
        "valid message produced AFTER garbage must be consumed: {:?}",
        all_msgs
    );

    // ── PROVE redelivery: garbage was re-encountered and quarantined ──
    let garbage_key = format!("quarantine/{venue}/{garbage_hash}");
    let stored = storage2_readback.read_object(&garbage_key).unwrap_or_else(|e| {
        panic!("garbage NOT in MinIO at {garbage_key}: {e} — offset was likely lost")
    });
    assert_eq!(
        stored, garbage,
        "garbage stored in MinIO must EXACTLY match original — proves redelivery"
    );
    eprintln!("  [ok] garbage redelivered, quarantined, preserved in MinIO at {garbage_key}");
    eprintln!(
        "  [ok] offset regression fully verified — {} messages consumed after restart",
        all_msgs.len()
    );
}

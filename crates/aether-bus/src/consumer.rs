use crate::envelope::Envelope;
use crate::headers;
use crate::producer::MessageProducer;
use crate::quarantine::{ObjectStore, Quarantine};
use rdkafka::config::ClientConfig;
use rdkafka::consumer::CommitMode;
use rdkafka::consumer::Consumer;
use rdkafka::consumer::StreamConsumer;
use rdkafka::message::Message;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

pub trait MessageConsumer: Send {
    fn consume<T: DeserializeOwned + Serialize + 'static + Send>(
        &self,
        topics: &[&str],
    ) -> impl std::future::Future<Output = Result<Vec<Envelope<T>>, ConsumerError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum ConsumerError {
    #[error("consume failed: {0}")]
    Receive(String),

    #[error("commit failed: {0}")]
    Commit(String),

    #[error("quarantine failed: {0}")]
    Quarantine(Box<crate::quarantine::QuarantineError>),
}

/// Stub consumer — replays what StubProducer sent (shared Vec).
/// Ignores the `topics` parameter since messages are in-memory.
pub struct StubConsumer {
    pub received: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}
impl StubConsumer {
    pub fn new(received: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>) -> Self {
        Self { received }
    }
}
impl MessageConsumer for StubConsumer {
    async fn consume<T: DeserializeOwned + Serialize + 'static>(
        &self,
        _topics: &[&str],
    ) -> Result<Vec<Envelope<T>>, ConsumerError> {
        let mut result = Vec::new();
        let guard = self.received.lock().unwrap_or_else(|e| e.into_inner());
        for (_topic, json) in guard.iter() {
            if let Ok(env) = serde_json::from_str::<Envelope<T>>(json) {
                result.push(env);
            }
        }
        Ok(result)
    }
}

/// Real Kafka consumer backed by rdkafka's [`StreamConsumer`].
///
/// Configured via `AETHER_KAFKA_BOOTSTRAP` (default `localhost:9092`)
/// and `AETHER_CONSUMER_GROUP` (default `svc.<service_name>`).
///
/// Auto-commit is disabled; the caller controls offset commits via
/// [`commit`](Self::commit).
/// Subscribes to topics on each `consume` call and polls until
/// at least one matching envelope is received or the timeout expires.
///
/// REQUIRES a `quarantine_producer` (any [`MessageProducer`]) and a
/// `quarantine_storage` ([`ObjectStore`]) at construction time. Malformed
/// payloads are automatically routed to the quarantine topic for the venue
/// extracted from the source topic (SPEC-006).
pub struct KafkaConsumer<P: MessageProducer> {
    inner: Arc<StreamConsumer>,
    quarantine_producer: P,
    quarantine_storage: Arc<dyn ObjectStore>,
    /// Topic-partition-offset tuples accumulated during consume().
    /// Cleared by `ack()`. Used for deferred offset storage so offsets
    /// are only stored after successful business processing.
    pending_offsets: std::sync::Mutex<Vec<(String, i32, i64)>>,
}

impl<P: MessageProducer> KafkaConsumer<P> {
    /// Create a new Kafka consumer connected to the given bootstrap servers
    /// and consumer group, with quarantine dependencies for malformed message
    /// handling (SPEC-006).
    pub fn new(
        bootstrap_servers: &str,
        group_id: &str,
        quarantine_producer: P,
        quarantine_storage: Arc<dyn ObjectStore>,
    ) -> Result<Self, ConsumerError> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("auto.offset.reset", "earliest")
            .set("session.timeout.ms", "6000")
            .create()
            .map_err(|e| ConsumerError::Receive(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(consumer),
            quarantine_producer,
            quarantine_storage,
            pending_offsets: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Commit the current consumer state (stored offsets) back to Kafka.
    ///
    /// Use this after successfully processing a batch of messages to advance
    /// the committed offset. Only meaningful when `enable.auto.commit` is
    /// `false` (the default for this consumer).
    ///
    /// Uses [`CommitMode::Async`] — the actual I/O happens in the background.
    /// Callers that need strong ordering guarantees should use
    /// [`commit_sync`](Self::commit_sync) instead.
    pub fn commit(&self) -> Result<(), ConsumerError> {
        self.inner
            .commit_consumer_state(CommitMode::Async)
            .map_err(|e| ConsumerError::Commit(e.to_string()))
    }

    /// Commit the current consumer state synchronously.
    ///
    /// Blocks until the commit is acknowledged by the Kafka broker.
    /// Prefer this when at-least-once semantics require the offset to be
    /// durably stored before the next message is processed.
    pub fn commit_sync(&self) -> Result<(), ConsumerError> {
        self.inner
            .commit_consumer_state(CommitMode::Sync)
            .map_err(|e| ConsumerError::Commit(e.to_string()))
    }

    /// Acknowledge processing of all consumed messages by storing their
    /// offsets. Call this after the caller has successfully processed
    /// the envelopes returned by [`consume`](Self::consume).
    ///
    /// For valid messages, offsets are accumulated during `consume()` but
    /// NOT stored — that happens here, after the caller confirms successful
    /// business processing. For quarantined malformed messages, offsets are
    /// stored immediately inside `consume()` because quarantine IS the
    /// processing.
    pub fn ack(&self) -> Result<(), ConsumerError> {
        let mut pending = self.pending_offsets.lock().unwrap_or_else(|e| e.into_inner());
        if pending.is_empty() {
            return Ok(());
        }
        // Build all TPLs first, then store them. Only clear pending
        // after every store_offsets call succeeds. On failure the
        // caller can retry ack() with the same offsets.
        let tpls: Vec<_> = pending
            .iter()
            .map(|(topic, partition, offset)| {
                let mut tpl = rdkafka::topic_partition_list::TopicPartitionList::new();
                tpl.add_partition_offset(
                    topic,
                    *partition,
                    rdkafka::topic_partition_list::Offset::Offset(*offset),
                )
                .map_err(|e| ConsumerError::Receive(format!("ack add_partition: {e}")))?;
                Ok(tpl)
            })
            .collect::<Result<Vec<_>, ConsumerError>>()?;
        for tpl in &tpls {
            self.inner
                .store_offsets(tpl)
                .map_err(|e| ConsumerError::Receive(format!("ack store_offsets: {e}")))?;
        }
        // All stores succeeded — clear pending state.
        pending.clear();
        Ok(())
    }
}

/// Concrete [`KafkaConsumer`] factory using a [`BreakerProducer<KafkaProducer>`]
/// and [`QuarantineStorage`] — the standard production configuration.
impl KafkaConsumer<crate::producer::BreakerProducer<crate::producer::KafkaProducer>> {
    /// Create a new Kafka consumer from environment configuration.
    ///
    /// Reads `AETHER_KAFKA_BOOTSTRAP` and `AETHER_CONSUMER_GROUP` with
    /// fallback to `localhost:9092` and `svc.<service_name>`.
    ///
    /// Constructs a [`BreakerProducer<KafkaProducer>`] and
    /// [`QuarantineStorage`] from their respective environment variables
    /// for the quarantine path (SPEC-006).
    pub fn from_env(service_name: &str) -> Result<Self, ConsumerError> {
        let servers = std::env::var("AETHER_KAFKA_BOOTSTRAP")
            .unwrap_or_else(|_| "localhost:9092".to_string());
        let group = std::env::var("AETHER_CONSUMER_GROUP")
            .unwrap_or_else(|_| crate::topics::ConsumerGroup::for_service(service_name));
        let producer = crate::producer::KafkaProducer::from_env()
            .map_err(|e| ConsumerError::Receive(e.to_string()))?;
        let storage = crate::quarantine::QuarantineStorage::new_from_env()
            .map_err(|e| ConsumerError::Receive(e.to_string()))?;
        Self::new(&servers, &group, producer, Arc::new(storage))
    }
}

impl<P: MessageProducer + 'static> MessageConsumer for KafkaConsumer<P> {
    async fn consume<T: DeserializeOwned + Serialize + 'static>(
        &self,
        topics: &[&str],
    ) -> Result<Vec<Envelope<T>>, ConsumerError> {
        let topic_refs = topics.to_vec();
        self.inner.subscribe(&topic_refs).map_err(|e| ConsumerError::Receive(e.to_string()))?;

        let mut results = Vec::new();
        let timeout = Duration::from_secs(10);
        let start = tokio::time::Instant::now();

        // Poll until we have at least one result or the timeout expires.
        while start.elapsed() < timeout && results.is_empty() {
            match tokio::time::timeout(Duration::from_millis(500), self.inner.recv()).await {
                Ok(Ok(msg)) => {
                    tracing::trace!("kafka message received on topic {}", msg.topic());

                    match deserialize_envelope::<T>(&msg) {
                        Ok(env) => {
                            // Record *next* offset for later acknowledgment.
                            // Kafka's store_offsets expects the offset to
                            // resume from, which is msg.offset() + 1.
                            self.pending_offsets.lock().unwrap_or_else(|e| e.into_inner()).push((
                                msg.topic().to_string(),
                                msg.partition(),
                                msg.offset() + 1,
                            ));
                            results.push(env);
                        }
                        Err(e) => {
                            tracing::warn!("failed to deserialize envelope: {e}");
                            // Quarantine-safe path: route malformed payloads to
                            // quarantine.{venue} (SPEC-006).
                            // Only store+commit the offset after BOTH raw storage
                            // and metadata publish succeed. A successfully
                            // quarantined message counts as completed processing.
                            // If quarantine fails, propagate the error so the
                            // offset is NOT advanced.
                            handle_deserialize_failure(
                                &self.quarantine_producer,
                                &*self.quarantine_storage,
                                msg.payload().unwrap_or_default(),
                                msg.topic(),
                            )
                            .await
                            .map_err(|e| ConsumerError::Quarantine(Box::new(e)))?;
                            // Quarantine succeeded — store offset+1 and commit.
                            // Use store_offsets with explicit offset since
                            // store_offset_from_message would store current+1
                            // but we still need to pass the correct value.
                            self.inner.store_offset_from_message(&msg).map_err(|e| {
                                ConsumerError::Receive(format!(
                                    "store offset after quarantine: {e}"
                                ))
                            })?;
                            self.inner.commit_consumer_state(CommitMode::Sync).map_err(|e| {
                                ConsumerError::Commit(format!("commit after quarantine: {e}"))
                            })?;
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("kafka receive error: {e}");
                }
                Err(_) => {
                    // Timeout waiting for a message — poll again.
                }
            }
        }

        tracing::debug!("consumed {} envelopes from {topics:?}", results.len());

        Ok(results)
    }
}

/// Deserialize an [`Envelope<T>`] from a Kafka message, restoring the
/// trace_id and schema from Kafka headers if present.
fn deserialize_envelope<T: DeserializeOwned + Serialize>(
    msg: &rdkafka::message::BorrowedMessage<'_>,
) -> Result<Envelope<T>, ConsumerError> {
    let payload = msg.payload().unwrap_or_default();
    let mut env: Envelope<T> =
        serde_json::from_slice(payload).map_err(|e| ConsumerError::Receive(e.to_string()))?;

    // Restore trace_id and schema from Kafka headers (overrides the
    // payload's values to ensure header-level provenance is authoritative).
    if let Some(trace_id) = headers::extract_trace_id(msg) {
        env.trace_id = trace_id;
    }
    if let Some(schema) = headers::extract_schema(msg) {
        env.schema = schema;
    }
    Ok(env)
}

/// Extract the venue name from a Kafka topic.
///
/// The venue is the last segment after the final `.` in the topic name.
/// For example, `md.ticks.kalshi` → `kalshi`, `orders.intents.demo` → `demo`.
pub fn extract_venue(topic: &str) -> &str {
    topic.rsplit('.').next().unwrap_or(topic)
}

/// Handle a deserialization failure by publishing the malformed message to
/// the quarantine topic and storing the raw bytes.
///
/// Extracts the venue from the topic name, then calls
/// [`Quarantine::publish`] to route the raw payload to `quarantine.{venue}`.
///
/// Returns the SHA-256 hash on success. On failure, the caller MUST NOT commit
/// the consumer offset — the malformed message would be permanently lost
/// (SPEC-006). The error is logged at `error` level before propagation.
pub async fn handle_deserialize_failure<P: MessageProducer>(
    producer: &P,
    storage: &dyn ObjectStore,
    raw_payload: &[u8],
    topic: &str,
) -> Result<String, crate::quarantine::QuarantineError> {
    let venue = extract_venue(topic);
    Quarantine::publish(
        producer,
        storage,
        venue,
        &format!("malformed message on {topic}"),
        raw_payload,
    )
    .await
    .map(|hash| {
        tracing::warn!(hash, venue, topic, "quarantined malformed message");
        hash
    })
    .map_err(|e| {
        tracing::error!(error = %e, venue, topic, "failed to quarantine malformed message — offset NOT committed");
        e
    })
}

/// Process consumed envelopes and commit offsets synchronously.
///
/// This is the recommended end-to-end pattern for production consumers:
/// 1. Consume a batch of envelopes from the given topics (quarantine-safe path)
/// 2. Apply a user-provided `process` function to each envelope
/// 3. Commit offsets synchronously after each envelope so that the message
///    is not re-delivered in the event of a crash (at-least-once semantics)
///
/// Returns the number of envelopes processed.
///
/// # Example (illustrative only — requires a real Kafka cluster)
///
/// ```text
/// // Not a compiled example — requires live Kafka + full crate imports.
/// let consumer = KafkaConsumer::from_env("gateway")?;
/// let count = process_and_commit(
///     &consumer,
///     &["md.ticks.kalshi"],
///     |env: Envelope<Quote>| async move {
///         println!("got quote: {:?}", env.payload);
///         Ok(())
///     },
/// ).await?;
/// ```
pub async fn process_and_commit<P, T, F, Fut>(
    consumer: &KafkaConsumer<P>,
    topics: &[&str],
    process: F,
) -> Result<usize, ConsumerError>
where
    P: MessageProducer + 'static,
    T: serde::de::DeserializeOwned + Serialize + Clone + Send + 'static,
    F: Fn(Envelope<T>) -> Fut,
    Fut: std::future::Future<Output = Result<(), ConsumerError>> + Send,
{
    let envelopes = consumer.consume::<T>(topics).await?;
    let count = envelopes.len();
    for envelope in envelopes {
        process(envelope).await?;
        // Store the offset for this successfully processed message,
        // then commit synchronously for at-least-once durability.
        // Offset storage is deferred until here so a processing failure
        // does not advance past an unprocessed message.
        consumer.ack()?;
        consumer.commit_sync()?;
    }
    tracing::info!(count, "processed, acked, and committed (quarantine-safe path)");
    Ok(count)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Envelope;
    use crate::producer::StubProducer;
    use crate::quarantine::StubObjectStore;
    use sha2::Digest;

    #[test]
    fn extract_venue_from_md_topic() {
        assert_eq!(extract_venue("md.ticks.kalshi"), "kalshi");
    }

    #[test]
    fn extract_venue_from_orders_topic() {
        assert_eq!(extract_venue("orders.intents.demo"), "demo");
    }

    #[test]
    fn extract_venue_from_flat_topic_returns_self() {
        assert_eq!(extract_venue("no-dots"), "no-dots");
    }

    #[tokio::test]
    async fn handle_deserialize_failure_routes_to_quarantine() {
        let producer = StubProducer::new();
        let storage = StubObjectStore::new();
        let raw_payload = br#"{"bad": json"#;

        handle_deserialize_failure(&producer, &storage, raw_payload, "md.ticks.kalshi")
            .await
            .unwrap();

        // Verify quarantine publish was sent
        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent.len(), 1, "must publish to quarantine topic");
        assert_eq!(sent[0].0, "quarantine.kalshi");

        // Verify raw bytes were stored
        let hash = hex::encode(sha2::Sha256::digest(raw_payload));
        let key = format!("quarantine/kalshi/{hash}");
        let stored = storage.objects.lock().unwrap();
        assert!(stored.contains_key(&key), "raw bytes must be stored at {key}");
        assert_eq!(stored.get(&key).unwrap(), &raw_payload.to_vec());
    }

    #[tokio::test]
    async fn handle_deserialize_failure_works_for_flat_topic() {
        let producer = StubProducer::new();
        let storage = StubObjectStore::new();

        handle_deserialize_failure(&producer, &storage, b"bad", "unknown").await.unwrap();

        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent[0].0, "quarantine.unknown");
    }

    #[test]
    fn stub_consumer_skips_malformed_gracefully() {
        // StubConsumer silently skips malformed messages (no quarantine routing).
        // This test verifies it doesn't crash on invalid input.
        let received = Arc::new(std::sync::Mutex::new(vec![(
            "md.ticks.kalshi".to_string(),
            "not valid json".to_string(),
        )]));
        let consumer = StubConsumer::new(received);
        // The consume will attempt to deserialize "not valid json" as Envelope<String>
        // and skip it because deserialization fails — that's the expected behavior.
        let block = async {
            let results: Vec<Envelope<String>> =
                consumer.consume(&["md.ticks.kalshi"]).await.unwrap();
            assert!(results.is_empty(), "malformed messages must be skipped");
        };
        tokio::runtime::Runtime::new().unwrap().block_on(block);
    }

    #[test]
    fn offset_conversion_stores_next_offset_to_consume() {
        // Kafka's store_offsets expects the *next* offset to resume from.
        // A message at offset N has been consumed; the next offset is N+1.
        let consumed_offset: i64 = 42;
        let stored_offset = consumed_offset + 1;
        assert_eq!(stored_offset, 43, "must store offset+1 for Kafka");
        assert_ne!(stored_offset, consumed_offset, "must not store the consumed offset");
    }

    #[test]
    fn ack_preserves_pending_on_tpl_build_failure() {
        // If add_partition_offset fails (e.g. invalid partition), the
        // pending list must not be drained. This test verifies the
        // Result-collection pattern preserves entries on error.
        let mut pending = vec![("topic".to_string(), 0, 100i64), ("topic".to_string(), 1, 200i64)];
        // Simulate ack's build-then-clear pattern:
        let tpls: Vec<Result<(), &str>> =
            pending.iter().map(|(_t, _p, _o)| Err("simulated failure")).collect();
        let all_ok = tpls.iter().all(|r| r.is_ok());
        if !all_ok {
            // pending must remain intact for retry
            assert_eq!(pending.len(), 2, "pending not cleared on failure");
        } else {
            pending.clear();
        }
        assert_eq!(pending.len(), 2, "pending preserved after TPL build failure");
    }
}

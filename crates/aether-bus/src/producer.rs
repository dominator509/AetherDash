use crate::envelope::Envelope;
use crate::retry::{BreakerState, CircuitBreaker};
use rdkafka::config::ClientConfig;
use rdkafka::producer::FutureProducer;
use rdkafka::producer::FutureRecord;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ProducerError {
    #[error("send failed: {0}")]
    Send(String),
    #[error("kafka error: {0}")]
    Kafka(#[from] rdkafka::error::KafkaError),
    #[error("circuit breaker open")]
    BreakerOpen,
    #[error("all {max_retries} retry attempts exhausted")]
    RetriesExhausted { max_retries: u32 },
    #[error("missing partition key for topic '{topic}' — md.* and orders.* topics require a partition key")]
    MissingPartitionKey { topic: String },
}

impl ProducerError {
    /// Returns true if the error represents a transient condition that may
    /// succeed on retry (transport failures, broker down, timeouts).
    pub fn is_retryable(&self) -> bool {
        match self {
            ProducerError::Kafka(e) => {
                let msg = e.to_string().to_lowercase();
                msg.contains("transport") || msg.contains("all broker") || msg.contains("timed out")
            }
            ProducerError::Send(msg) => {
                let lower = msg.to_lowercase();
                lower.contains("transport")
                    || lower.contains("all broker")
                    || lower.contains("timed out")
                    || lower.contains("unavailable")
                    || lower.contains("temporarily")
            }
            ProducerError::BreakerOpen
            | ProducerError::RetriesExhausted { .. }
            | ProducerError::MissingPartitionKey { .. } => false,
        }
    }
}

pub trait MessageProducer: Send + Sync {
    fn send<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        envelope: Envelope<T>,
        key: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), ProducerError>> + Send;
}

/// Require a partition key for `md.*` and `orders.*` topics.
/// Returns `MissingPartitionKey` if the topic requires a key but none was provided.
pub fn require_partition_key(topic: &str, key: Option<&str>) -> Result<(), ProducerError> {
    if (topic.starts_with("md.") || topic.starts_with("orders.")) && key.is_none() {
        return Err(ProducerError::MissingPartitionKey { topic: topic.to_string() });
    }
    Ok(())
}

/// Stub producer for testing.
/// Uses `Arc<Mutex<...>>` so the same storage can be shared with `StubConsumer`.
pub struct StubProducer {
    pub sent: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}
impl Default for StubProducer {
    fn default() -> Self {
        Self::new()
    }
}
impl StubProducer {
    pub fn new() -> Self {
        Self { sent: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())) }
    }
}
impl MessageProducer for StubProducer {
    async fn send<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        envelope: Envelope<T>,
        key: Option<&str>,
    ) -> Result<(), ProducerError> {
        require_partition_key(topic, key)?;
        let json =
            serde_json::to_string(&envelope).map_err(|e| ProducerError::Send(e.to_string()))?;
        self.sent.lock().unwrap_or_else(|e| e.into_inner()).push((topic.to_string(), json));
        Ok(())
    }
}

/// Real Kafka producer backed by rdkafka's [`FutureProducer`].
///
/// Configured via `AETHER_KAFKA_BOOTSTRAP` env var (default `localhost:9092`).
/// Serializes envelopes to canonical JSON bytes and sets standard AETHER
/// Kafka headers (`trace_id`, `schema`, `content-type`).
pub struct KafkaProducer {
    inner: FutureProducer,
}

impl KafkaProducer {
    /// Create a new Kafka producer connected to the given bootstrap servers.
    pub fn new(bootstrap_servers: &str) -> Result<Self, ProducerError> {
        let inner: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .set("message.timeout.ms", "5000")
            .create()
            .map_err(|e| ProducerError::Send(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Create a new Kafka producer from environment configuration.
    ///
    /// Reads `AETHER_KAFKA_BOOTSTRAP` with fallback to `localhost:9092`.
    /// Returns a [`BreakerProducer`] wrapping a [`KafkaProducer`] for
    /// production safety (circuit breaker protection).
    pub fn from_env() -> Result<BreakerProducer<Self>, ProducerError> {
        let servers = std::env::var("AETHER_KAFKA_BOOTSTRAP")
            .unwrap_or_else(|_| "localhost:9092".to_string());
        Self::new(&servers).map(|p| p.with_breaker())
    }

    /// Create a raw [`KafkaProducer`] from environment without circuit
    /// breaker protection.
    ///
    /// Prefer [`from_env`](Self::from_env) for production use. This method
    /// is `pub(crate)` — tests and examples that need an unwrapped producer
    /// can construct one via [`KafkaProducer::new`] directly.
    #[allow(dead_code)]
    pub(crate) fn from_env_raw() -> Result<Self, ProducerError> {
        let servers = std::env::var("AETHER_KAFKA_BOOTSTRAP")
            .unwrap_or_else(|_| "localhost:9092".to_string());
        Self::new(&servers)
    }

    /// Convenience constructor: wrap this producer in a [`BreakerProducer`]
    /// for circuit breaker protection.
    ///
    /// Recommended for production use. Without a breaker, transient broker
    /// failures can cascade into sustained throughput degradation.
    pub fn with_breaker(self) -> BreakerProducer<Self> {
        BreakerProducer::new(self)
    }
}

impl MessageProducer for KafkaProducer {
    async fn send<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        envelope: Envelope<T>,
        key: Option<&str>,
    ) -> Result<(), ProducerError> {
        require_partition_key(topic, key)?;
        let payload =
            envelope.to_canonical_bytes().map_err(|e| ProducerError::Send(e.to_string()))?;

        let payload_slice: &[u8] = &payload;

        let record = FutureRecord::to(topic)
            .payload(payload_slice)
            .key(key.map_or(&[] as &[u8], |k| k.as_bytes()));

        let record = crate::headers::add_headers(record, &envelope.trace_id, &envelope.schema);

        self.inner
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(e, _)| ProducerError::Kafka(e))?;

        tracing::debug!(
            topic,
            trace_id = %envelope.trace_id,
            schema = %envelope.schema,
            key = ?key,
            "message produced"
        );

        Ok(())
    }
}

// ── Breaker-wrapped producer ──────────────────────────────────────────────

/// A composable wrapper around any [`MessageProducer`] that adds circuit
/// breaker protection. Before sending, checks the breaker; on success records
/// success; on failure records failure.
pub struct BreakerProducer<P: MessageProducer> {
    inner: P,
    breaker: Mutex<CircuitBreaker>,
}

impl<P: MessageProducer> BreakerProducer<P> {
    /// Wrap a producer in circuit breaker protection.
    ///
    /// This is the recommended way to construct a producer for production use.
    /// In production, use [`KafkaProducer::from_env`] which already applies
    /// breaker protection automatically.
    pub fn new(inner: P) -> Self {
        Self { inner, breaker: Mutex::new(CircuitBreaker::new()) }
    }

    /// Returns the current circuit breaker state.
    pub fn state(&self) -> BreakerState {
        self.breaker.lock().unwrap_or_else(|e| e.into_inner()).state()
    }
}

impl<P: MessageProducer> MessageProducer for BreakerProducer<P> {
    async fn send<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        envelope: Envelope<T>,
        key: Option<&str>,
    ) -> Result<(), ProducerError> {
        require_partition_key(topic, key)?;
        let allowed = self.breaker.lock().unwrap_or_else(|e| e.into_inner()).allow_request();
        if !allowed {
            return Err(ProducerError::BreakerOpen);
        }

        let result = self.inner.send(topic, envelope, key).await;

        match &result {
            Ok(()) => {
                self.breaker.lock().unwrap_or_else(|e| e.into_inner()).record_success();
            }
            Err(_) => {
                self.breaker.lock().unwrap_or_else(|e| e.into_inner()).record_failure();
            }
        }

        result
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Envelope;

    #[tokio::test]
    async fn stub_producer_accepts_partition_key() {
        let producer = StubProducer::new();
        let envelope = Envelope::new("test", "payload");
        producer.send("test.topic", envelope, Some("partition-key")).await.unwrap();
        assert_eq!(producer.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn stub_producer_without_key() {
        let producer = StubProducer::new();
        let envelope = Envelope::new("test", "payload");
        producer.send("test.topic", envelope, None).await.unwrap();
        assert_eq!(producer.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn md_topic_requires_partition_key() {
        let producer = StubProducer::new();
        let envelope = Envelope::new("test", "payload");
        let result = producer.send("md.ticks.demo", envelope, None).await;
        assert!(matches!(result, Err(ProducerError::MissingPartitionKey { .. })));
    }

    #[tokio::test]
    async fn orders_topic_requires_partition_key() {
        let producer = StubProducer::new();
        let envelope = Envelope::new("test", "payload");
        let result = producer.send("orders.intents.demo", envelope, None).await;
        assert!(matches!(result, Err(ProducerError::MissingPartitionKey { .. })));
    }

    #[tokio::test]
    async fn non_md_topic_allows_no_key() {
        let producer = StubProducer::new();
        let envelope = Envelope::new("test", "payload");
        producer.send("audit.events", envelope, None).await.unwrap();
        assert_eq!(producer.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn md_topic_with_key_succeeds() {
        let producer = StubProducer::new();
        let envelope = Envelope::new("test", "payload");
        producer.send("md.ticks.demo", envelope, Some("mkt:key")).await.unwrap();
        assert_eq!(producer.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn missing_partition_key_not_retryable() {
        let err = ProducerError::MissingPartitionKey { topic: "md.ticks.demo".into() };
        assert!(!err.is_retryable());
    }

    #[tokio::test]
    async fn breaker_producer_opens_after_consecutive_failures() {
        struct FailingProducer;
        impl MessageProducer for FailingProducer {
            async fn send<T: Serialize + Send + Sync>(
                &self,
                _topic: &str,
                _envelope: Envelope<T>,
                _key: Option<&str>,
            ) -> Result<(), ProducerError> {
                Err(ProducerError::Send("transport failure".to_string()))
            }
        }

        let bp = BreakerProducer::new(FailingProducer);

        // 5 consecutive failures trip the breaker
        for i in 0..5 {
            let env = Envelope::new("test", format!("payload-{i}"));
            let result = bp.send("test.topic", env, None).await;
            assert!(result.is_err());
            assert!(!matches!(result.unwrap_err(), ProducerError::BreakerOpen));
        }

        // 6th send should be rejected by the open breaker
        let env = Envelope::new("test", "payload-final");
        let result = bp.send("test.topic", env, None).await;
        assert!(matches!(result, Err(ProducerError::BreakerOpen)));
        assert_eq!(bp.state(), BreakerState::Open);
    }

    #[tokio::test]
    async fn breaker_producer_records_success() {
        struct OkProducer;
        impl MessageProducer for OkProducer {
            async fn send<T: Serialize + Send + Sync>(
                &self,
                _topic: &str,
                _envelope: Envelope<T>,
                _key: Option<&str>,
            ) -> Result<(), ProducerError> {
                Ok(())
            }
        }

        let bp = BreakerProducer::new(OkProducer);
        let env = Envelope::new("test", "payload");
        bp.send("test.topic", env, None).await.unwrap();
        assert_eq!(bp.state(), BreakerState::Closed);
    }

    #[test]
    fn producer_error_is_retryable_false_for_breaker_open() {
        let err = ProducerError::BreakerOpen;
        assert!(!err.is_retryable());
    }

    #[test]
    fn producer_error_is_retryable_false_for_ordinary_send() {
        let err = ProducerError::Send("not found: topic does not exist".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn producer_error_is_retryable_true_for_transport_error() {
        let err = ProducerError::Send("transport failure: broker unavailable".to_string());
        assert!(err.is_retryable());
    }

    #[test]
    fn producer_error_is_retryable_true_for_timeout() {
        let err = ProducerError::Send("timed out waiting for broker".to_string());
        assert!(err.is_retryable());
    }
}

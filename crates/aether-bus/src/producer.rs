use crate::envelope::Envelope;
use rdkafka::config::ClientConfig;
use rdkafka::producer::FutureProducer;
use rdkafka::producer::FutureRecord;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ProducerError {
    #[error("send failed: {0}")]
    Send(String),
}

pub trait MessageProducer: Send + Sync {
    fn send<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        envelope: Envelope<T>,
    ) -> impl std::future::Future<Output = Result<(), ProducerError>> + Send;
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
        Self {
            sent: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}
impl MessageProducer for StubProducer {
    async fn send<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        envelope: Envelope<T>,
    ) -> Result<(), ProducerError> {
        let json =
            serde_json::to_string(&envelope).map_err(|e| ProducerError::Send(e.to_string()))?;
        self.sent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((topic.to_string(), json));
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
    pub fn from_env() -> Result<Self, ProducerError> {
        let servers = std::env::var("AETHER_KAFKA_BOOTSTRAP")
            .unwrap_or_else(|_| "localhost:9092".to_string());
        Self::new(&servers)
    }
}

impl MessageProducer for KafkaProducer {
    async fn send<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        envelope: Envelope<T>,
    ) -> Result<(), ProducerError> {
        let payload =
            envelope.to_canonical_bytes().map_err(|e| ProducerError::Send(e.to_string()))?;

        let payload_slice: &[u8] = &payload;
        let empty_key: &[u8] = &[];

        let record = FutureRecord::to(topic).payload(payload_slice).key(empty_key);

        let record = crate::headers::add_headers(record, &envelope.trace_id, &envelope.schema);

        self.inner
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(e, _)| ProducerError::Send(format!("send failed: {e}")))?;

        tracing::debug!(
            topic,
            trace_id = %envelope.trace_id,
            schema = %envelope.schema,
            "message produced"
        );

        Ok(())
    }
}

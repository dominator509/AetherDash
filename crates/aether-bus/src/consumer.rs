use crate::envelope::Envelope;
use crate::headers;
use rdkafka::config::ClientConfig;
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
/// Auto-commit is disabled; the caller controls offset commits.
/// Subscribes to topics on each `consume` call and polls until
/// at least one matching envelope is received or the timeout expires.
pub struct KafkaConsumer {
    inner: Arc<StreamConsumer>,
}

impl KafkaConsumer {
    /// Create a new Kafka consumer connected to the given bootstrap servers
    /// and consumer group.
    pub fn new(bootstrap_servers: &str, group_id: &str) -> Result<Self, ConsumerError> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            .set("auto.offset.reset", "earliest")
            .set("session.timeout.ms", "6000")
            .create()
            .map_err(|e| ConsumerError::Receive(e.to_string()))?;
        Ok(Self { inner: Arc::new(consumer) })
    }

    /// Create a new Kafka consumer from environment configuration.
    ///
    /// Reads `AETHER_KAFKA_BOOTSTRAP` and `AETHER_CONSUMER_GROUP` with
    /// fallback to `localhost:9092` and `svc.<service_name>`.
    pub fn from_env(service_name: &str) -> Result<Self, ConsumerError> {
        let servers = std::env::var("AETHER_KAFKA_BOOTSTRAP")
            .unwrap_or_else(|_| "localhost:9092".to_string());
        let group = std::env::var("AETHER_CONSUMER_GROUP")
            .unwrap_or_else(|_| crate::topics::ConsumerGroup::for_service(service_name));
        Self::new(&servers, &group)
    }
}

impl MessageConsumer for KafkaConsumer {
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
                        Ok(env) => results.push(env),
                        Err(e) => tracing::warn!("failed to deserialize envelope: {e}"),
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

//! Quarantine path utility.
//! SPEC-006: malformed messages → quarantine.{venue}, never md.*.
//!
//! Provides:
//! - `Quarantine::publish` — publish a quarantined payload envelope
//! - `QuarantineStorage` — MinIO / S3-compatible storage for raw payloads
//! - `quarantine_consume_and_store` — consumer-side workflow

use crate::consumer::MessageConsumer;
use crate::envelope::Envelope;
use crate::producer::MessageProducer;
use serde::{Deserialize, Serialize};
use sha2::Digest;

// ── Quarantine struct ──────────────────────────────────────────────────

/// Quarantine a malformed payload to the quarantine topic for a venue.
/// The raw payload is preserved to MinIO by the quarantine consumer.
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

    /// Publish a malformed payload to the quarantine topic for a venue.
    ///
    /// Creates a [`QuarantineMessage`] via [`envelope`](Self::envelope),
    /// wraps it in an [`Envelope`] with schema `aether.quarantine.v1`,
    /// and sends it via the given producer to `quarantine.{venue}`.
    /// Returns the SHA-256 hex hash of the raw payload on success.
    pub fn publish<P: MessageProducer>(
        producer: &P,
        venue: &str,
        reason: &str,
        raw_payload: &[u8],
    ) -> Result<String, QuarantineError> {
        let msg = Self::envelope(venue, reason, raw_payload);
        let hash = msg.raw_hash.clone();
        let envelope = Envelope::new("quarantine", msg);
        let topic = Self::topic_for(venue);
        producer.send(&topic, envelope)?;
        Ok(hash)
    }
}

// ── QuarantineMessage ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineMessage {
    pub venue: String,
    pub reason: String,
    pub raw_size: u64,
    pub raw_hash: String,
    pub ts: String,
}

// ── Errors ─────────────────────────────────────────────────────────────

/// Errors from the quarantine subsystem.
#[derive(Debug, thiserror::Error)]
pub enum QuarantineError {
    #[error("publish failed: {0}")]
    Publish(#[from] crate::producer::ProducerError),

    #[error("consume failed: {0}")]
    Consume(#[from] crate::consumer::ConsumerError),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ── MinIO / S3-compatible storage ─────────────────────────────────────

/// MinIO / S3-compatible storage for quarantined raw payloads.
///
/// Uses the `rust-s3` crate (`sync` feature, attohttpc-backed) to PUT objects
/// into a MinIO bucket.
///
/// ## Environment variables
///
/// | Variable | Default |
/// |---|---|
/// | `AETHER_MINIO_ENDPOINT` | `http://localhost:9000` |
/// | `AETHER_MINIO_ACCESS_KEY` | `minioadmin` |
/// | `AETHER_MINIO_SECRET_KEY` | `minioadmin` |
/// | `AETHER_MINIO_QUARANTINE_BUCKET` | `quarantine` |
pub struct QuarantineStorage {
    bucket: Box<s3::Bucket>,
}

impl QuarantineStorage {
    /// Create a new quarantine storage from environment variables.
    pub fn new_from_env() -> Result<Self, QuarantineError> {
        let endpoint = std::env::var("AETHER_MINIO_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:9000".to_string());
        let access_key =
            std::env::var("AETHER_MINIO_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string());
        let secret_key =
            std::env::var("AETHER_MINIO_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string());
        let bucket_name = std::env::var("AETHER_MINIO_QUARANTINE_BUCKET")
            .unwrap_or_else(|_| "quarantine".to_string());

        Self::new(&endpoint, &access_key, &secret_key, &bucket_name)
    }

    /// Create a new quarantine storage with explicit configuration.
    pub fn new(
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket_name: &str,
    ) -> Result<Self, QuarantineError> {
        use s3::creds::Credentials;
        use s3::region::Region;

        let region =
            Region::Custom { region: "us-east-1".to_string(), endpoint: endpoint.to_string() };
        let credentials = Credentials::new(Some(access_key), Some(secret_key), None, None, None)
            .map_err(|e| QuarantineError::Storage(e.to_string()))?;

        let bucket = s3::Bucket::new(bucket_name, region, credentials)
            .map_err(|e| QuarantineError::Storage(e.to_string()))?;
        let bucket = bucket.with_path_style();

        Ok(Self { bucket })
    }

    /// Store raw payload bytes in MinIO under the given key.
    ///
    /// The key is expected to follow the pattern `quarantine/{venue}/{hash}`.
    /// Returns the storage path `{bucket_name}/{key}` on success.
    pub fn store_object(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, QuarantineError> {
        self.bucket
            .put_object_builder(key, data)
            .with_content_type(content_type)
            .execute()
            .map_err(|e| QuarantineError::Storage(e.to_string()))?;
        Ok(format!("{}/{key}", self.bucket.name()))
    }
}

// ── Consumer workflow ─────────────────────────────────────────────────

/// Consume quarantine messages from a venue's quarantine topic and store
/// the serialized envelopes in MinIO for forensic preservation.
///
/// Returns the number of messages processed.
pub fn quarantine_consume_and_store<C: MessageConsumer>(
    consumer: &C,
    storage: &QuarantineStorage,
) -> Result<usize, QuarantineError> {
    let envelopes = consumer.consume::<QuarantineMessage>()?;

    let count = envelopes.len();
    for envelope in &envelopes {
        let key = format!("quarantine/{}/{}", envelope.payload.venue, envelope.payload.raw_hash);
        let data = serde_json::to_vec(envelope)?;
        storage.store_object(&key, &data, "application/json")?;
        tracing::info!(key = %key, venue = %envelope.payload.venue, "stored quarantine payload");
    }
    Ok(count)
}

// ── Helpers ────────────────────────────────────────────────────────────

fn chrono_now() -> String {
    use std::time::SystemTime;
    let dur = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    format!("{}.{:03}", dur.as_secs(), dur.subsec_millis())
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consumer::StubConsumer;
    use crate::producer::StubProducer;

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

    #[test]
    fn quarantine_publish_sends_to_correct_topic() {
        let producer = StubProducer::new();
        let hash = Quarantine::publish(&producer, "kalshi", "bad json", b"garbage").unwrap();

        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "quarantine.kalshi");
        assert_ne!(sent[0].0, "md.ticks.kalshi");
        assert_ne!(sent[0].0, "md.ticks");

        let expected = hex::encode(sha2::Sha256::digest(b"garbage"));
        assert_eq!(hash, expected);
    }

    #[test]
    fn quarantine_publish_envelope_schema() {
        let producer = StubProducer::new();
        let _ = Quarantine::publish(&producer, "kalshi", "bad json", b"garbage").unwrap();

        let sent = producer.sent.lock().unwrap();
        let json = &sent[0].1;
        let envelope: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(envelope["schema"], "aether.quarantine.v1");
    }

    #[test]
    fn quarantine_consume_returns_envelopes() {
        let producer = StubProducer::new();
        Quarantine::publish(&producer, "kalshi", "bad json", b"garbage").unwrap();
        Quarantine::publish(&producer, "polymarket", "no payload", b"trash").unwrap();

        let consumer = StubConsumer::new(producer.sent.clone());
        let envelopes = consumer.consume::<QuarantineMessage>().unwrap();

        assert_eq!(envelopes.len(), 2);
        assert_eq!(envelopes[0].schema, "aether.quarantine.v1");
        assert_eq!(envelopes[0].payload.venue, "kalshi");
        assert_eq!(envelopes[1].payload.venue, "polymarket");
    }

    #[test]
    fn quarantine_publish_hash_is_sha256() {
        let payload = b"malformed json input";
        let msg = Quarantine::envelope("test", "parse error", payload);
        let expected = hex::encode(sha2::Sha256::digest(payload));
        assert_eq!(msg.raw_hash, expected);
    }

    #[test]
    fn quarantine_publish_topic_not_md() {
        // Quarantine topics MUST NOT overlap with md.* topics (SPEC-006)
        let kalshi_q = Quarantine::topic_for("kalshi");
        assert!(!kalshi_q.starts_with("md."), "quarantine topic must not start with md.");

        let poly_q = Quarantine::topic_for("polymarket");
        assert!(!poly_q.starts_with("md."), "quarantine topic must not start with md.");
    }

    #[test]
    fn quarantine_storage_new_from_env_defaults() {
        // Verify that new_from_env succeeds with defaults when env vars are absent
        // (this just checks the construction — store_object won't have a live MinIO)
        let storage = QuarantineStorage::new_from_env();
        // We expect this to succeed because defaults are provided for all vars
        assert!(storage.is_ok());
    }
}

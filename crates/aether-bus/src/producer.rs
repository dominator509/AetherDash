use crate::envelope::Envelope;
use serde::Serialize;

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
    ) -> Result<(), ProducerError>;
}

/// Stub producer for testing
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
    fn send<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        envelope: Envelope<T>,
    ) -> Result<(), ProducerError> {
        let json =
            serde_json::to_string(&envelope).map_err(|e| ProducerError::Send(e.to_string()))?;
        self.sent
            .lock()
            .map_err(|e| ProducerError::Send(e.to_string()))?
            .push((topic.to_string(), json));
        Ok(())
    }
}

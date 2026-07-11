use crate::envelope::Envelope;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub trait MessageConsumer: Send {
    fn consume<T: DeserializeOwned + Serialize>(&self) -> Result<Vec<Envelope<T>>, ConsumerError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ConsumerError {
    #[error("consume failed: {0}")]
    Receive(String),
}

/// Stub consumer — replays what StubProducer sent (shared Vec)
pub struct StubConsumer {
    pub received: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}
impl StubConsumer {
    pub fn new(received: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>) -> Self {
        Self { received }
    }
}
impl MessageConsumer for StubConsumer {
    fn consume<T: DeserializeOwned + Serialize>(&self) -> Result<Vec<Envelope<T>>, ConsumerError> {
        let received = self.received.lock().map_err(|e| ConsumerError::Receive(e.to_string()))?;
        let mut result = Vec::new();
        for (_topic, json) in received.iter() {
            if let Ok(env) = serde_json::from_str::<Envelope<T>>(json) {
                result.push(env);
            }
        }
        Ok(result)
    }
}

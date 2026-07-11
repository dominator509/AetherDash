use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T: Serialize> {
    pub schema: String,
    pub trace_id: String,
    pub ts: String,
    pub payload: T,
}

impl<T: Serialize> Envelope<T> {
    pub fn new(type_name: &str, payload: T) -> Self {
        Self {
            schema: format!("aether.{type_name}.v1"),
            trace_id: uuid::Uuid::new_v4().to_string(),
            ts: chrono_now_iso(),
            payload,
        }
    }

    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

fn chrono_now_iso() -> String {
    // Simple ISO-8601 timestamp without chrono dependency
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let millis = dur.subsec_millis();
    format!("{secs}.{millis:03}")
}

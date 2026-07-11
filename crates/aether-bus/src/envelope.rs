use aether_core::canonical::canonical_json_bytes;
use aether_core::time::UtcTime;
use serde::{Deserialize, Serialize};

/// SPEC-003 message envelope: every bus message carries
/// { schema, trace_id, ts, payload } in canonical JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T: Serialize> {
    pub schema: String,
    pub trace_id: String,
    pub ts: String,
    pub payload: T,
}

impl<T: Serialize> Envelope<T> {
    /// Create a new envelope with the given type name.
    /// The schema field is set to `aether.<type_name>.v1`.
    /// Timestamp uses aether-core UtcTime (RFC3339 with millisecond precision).
    pub fn new(type_name: &str, payload: T) -> Self {
        Self {
            schema: format!("aether.{type_name}.v1"),
            trace_id: ulid_string(),
            ts: UtcTime::now().to_string(),
            payload,
        }
    }

    /// Serialize to canonical bytes via aether-core's canonical serialization.
    /// Uses deterministic field order and decimal-string encoding.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, aether_core::canonical::CanonicalError> {
        canonical_json_bytes(self)
    }
}

fn ulid_string() -> String {
    aether_core::ids::Ulid::new().to_string()
}

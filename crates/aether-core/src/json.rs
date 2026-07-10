//! Validated JSON object wrapper. Rejects non-object values on deserialization.
//! All keys are recursively sorted on construction for deterministic canonical serialization.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonObject(serde_json::Value);

impl JsonObject {
    pub fn new(value: serde_json::Value) -> Result<Self, JsonObjectError> {
        if !value.is_object() {
            return Err(JsonObjectError);
        }
        let mut v = value;
        sort_json_keys(&mut v);
        Ok(Self(v))
    }
    pub fn as_value(&self) -> &serde_json::Value {
        &self.0
    }
    /// Returns true if the wrapped object has no entries.
    pub fn is_empty(&self) -> bool {
        self.0.as_object().map_or(true, |m| m.is_empty())
    }
}

impl Default for JsonObject {
    fn default() -> Self {
        Self(serde_json::Value::Object(serde_json::Map::new()))
    }
}

/// Recursively sort all keys in a JSON value for deterministic serialization.
/// Objects have their entries sorted by key; arrays have each element recursively sorted.
fn sort_json_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            // Collect entries, recursively sort values, sort by key, rebuild
            let mut entries: Vec<(String, serde_json::Value)> = map
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (_, v) in &mut entries {
                sort_json_keys(v);
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            map.clear();
            for (k, v) in entries {
                map.insert(k, v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                sort_json_keys(v);
            }
        }
        _ => {}
    }
}

#[derive(Debug, thiserror::Error)]
#[error("value must be a JSON object")]
pub struct JsonObjectError;

impl Serialize for JsonObject {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}
impl<'de> Deserialize<'de> for JsonObject {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsonObject::new(serde_json::Value::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

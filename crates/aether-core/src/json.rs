//! Validated JSON object wrapper. Rejects non-object values on deserialization.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonObject(serde_json::Value);

impl JsonObject {
    pub fn new(value: serde_json::Value) -> Result<Self, JsonObjectError> {
        if !value.is_object() {
            return Err(JsonObjectError);
        }
        Ok(Self(value))
    }
    pub fn as_value(&self) -> &serde_json::Value {
        &self.0
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

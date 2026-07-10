//! Canonical serialization: `canonical_json_bytes<T>()` produces deterministic
//! byte-identical output across all three languages for provenance and audit hashing.
//! SPEC-001 canonical serialization rules:
//!   - Declared struct-field order
//!   - Omit only explicitly optional fields (None)
//!   - Decimals as JSON strings
//!   - Timestamps as UTC RFC3339 with exactly millisecond precision (YYYY-MM-DDTHH:MM:SS.mmmZ)
//!   - Deterministic ordering for map/object keys (sorted by key)

use serde::Serialize;
use std::collections::BTreeMap;

/// Produce canonical JSON bytes for a value.
///
/// Uses serde_json with `preserve_order` for struct field ordering.
/// Map keys are sorted via BTreeMap. Decimals are serialized as strings
/// by each type's custom Serialize impl.
///
/// This is the single function used for computing provenance and audit hashes.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, CanonicalError> {
    let json = serde_json::to_string(value)?;
    Ok(json.into_bytes())
}

/// Canonical JSON string (for debugging/display).
pub fn canonical_json_string<T: Serialize>(value: &T) -> Result<String, CanonicalError> {
    Ok(serde_json::to_string(value)?)
}

/// SHA-256 hash of canonical bytes — used for provenance and audit.
pub fn canonical_sha256<T: Serialize>(value: &T) -> Result<String, CanonicalError> {
    use sha2::{Digest, Sha256};
    let bytes = canonical_json_bytes(value)?;
    let hash = Sha256::digest(&bytes);
    Ok(hex::encode(hash))
}

#[derive(Debug, thiserror::Error)]
pub enum CanonicalError {
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Deterministic map for canonical output — keys are always sorted.
pub type CanonicalMap<K, V> = BTreeMap<K, V>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{MarketKey, Money};
    use rust_decimal::Decimal;
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestQuote {
        market: MarketKey,
        bid: String,
        ask: String,
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        let q = TestQuote {
            market: MarketKey::from_string_unchecked("mkt:kalshi:BTC-75"),
            bid: "0.65".into(),
            ask: "0.67".into(),
        };
        let bytes1 = canonical_json_bytes(&q).unwrap();
        let bytes2 = canonical_json_bytes(&q).unwrap();
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn canonical_bytes_produce_valid_utf8() {
        let m = Money::new(Decimal::new(12345, 2), "USD");
        let bytes = canonical_json_bytes(&m).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("123.45"));
        assert!(s.contains("USD"));
    }

    #[test]
    fn canonical_sha256_is_hex_string() {
        let m = Money::new(Decimal::new(100, 0), "USD");
        let hash = canonical_sha256(&m).unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn reverse_insertion_order_produces_same_hash() {
        use crate::json::JsonObject;

        // Same keys inserted in opposite orders
        let obj_a =
            JsonObject::new(serde_json::json!({"a": 1, "b": 2, "c": 3})).unwrap();
        let obj_b =
            JsonObject::new(serde_json::json!({"c": 3, "b": 2, "a": 1})).unwrap();

        let hash_a = canonical_sha256(&obj_a).unwrap();
        let hash_b = canonical_sha256(&obj_b).unwrap();
        assert_eq!(hash_a, hash_b, "different insertion orders must produce same hash");
    }

    #[test]
    fn nested_maps_are_sorted() {
        use crate::json::JsonObject;

        // Nested objects; outer keys are reverse-alphabetical
        let obj = JsonObject::new(serde_json::json!({
            "z_level": {"nested_b": 1, "nested_a": 2},
            "a_level": {"inner": {"sub_z": "last", "sub_a": "first"}}
        }))
        .unwrap();

        let json_str = canonical_json_string(&obj).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        // Top-level keys must be sorted: "a_level" < "z_level"
        if let serde_json::Value::Object(map) = &parsed {
            let keys: Vec<&String> = map.keys().collect();
            assert_eq!(keys, vec!["a_level", "z_level"], "top-level keys not sorted");
        } else {
            panic!("expected object");
        }

        // Nested keys within "z_level" must be sorted
        let z = &parsed["z_level"];
        if let serde_json::Value::Object(map) = z {
            let keys: Vec<&String> = map.keys().collect();
            assert_eq!(keys, vec!["nested_a", "nested_b"], "nested keys not sorted");
        } else {
            panic!("expected object");
        }

        // Deeply nested keys in "a_level" -> "inner" must be sorted
        let inner = &parsed["a_level"]["inner"];
        if let serde_json::Value::Object(map) = inner {
            let keys: Vec<&String> = map.keys().collect();
            assert_eq!(keys, vec!["sub_a", "sub_z"], "deeply nested keys not sorted");
        } else {
            panic!("expected object");
        }
    }
}

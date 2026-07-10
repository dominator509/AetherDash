//! Decimal serialization helpers. SPEC-001: all money/price/size/fee/edge values
//! are arbitrary-precision decimals. Wire format (JSON and proto) is decimal STRING.
//! Floats are FORBIDDEN for these values in all three languages.

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Serialize/deserialize a Decimal as a JSON string. Rejects numeric JSON values.
pub mod decimal_string {
    use super::*;

    pub fn serialize<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&d.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Decimal, D::Error> {
        let s = String::deserialize(d)?;
        Decimal::from_str_exact(&s).map_err(serde::de::Error::custom)
    }
}

/// Serialize/deserialize Option<Decimal> as optional JSON string. Rejects numeric values.
pub mod decimal_option_string {
    use super::*;

    pub fn serialize<S: Serializer>(d: &Option<Decimal>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(val) => s.serialize_some(&val.to_string()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Decimal>, D::Error> {
        match Option::<String>::deserialize(d)? {
            Some(s) => Ok(Some(Decimal::from_str_exact(&s).map_err(serde::de::Error::custom)?)),
            None => Ok(None),
        }
    }
}

// ── Confidence ─────────────────────────────────────────────────────

/// Confidence value: Decimal constrained to [0, 1].
/// Constructor enforces the invariant. Custom Deserialize prevents bypass.
/// SPEC-001: confidence 0..=1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Confidence(Decimal);

impl Confidence {
    pub fn new(value: Decimal) -> Result<Self, ConfidenceError> {
        if value < Decimal::ZERO || value > Decimal::ONE {
            return Err(ConfidenceError { value });
        }
        Ok(Self(value))
    }

    pub fn value(&self) -> Decimal {
        self.0
    }
}

impl Serialize for Confidence {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Confidence {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let d = Decimal::from_str_exact(&s).map_err(serde::de::Error::custom)?;
        Confidence::new(d).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("confidence must be in [0, 1], got {value}")]
pub struct ConfidenceError {
    pub value: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_string_round_trip() {
        let d = Decimal::new(12345, 2);
        #[derive(Serialize, Deserialize)]
        struct W {
            #[serde(with = "decimal_string")]
            v: Decimal,
        }
        let w = W { v: d };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains(r#""123.45""#));
        let w2: W = serde_json::from_str(&json).unwrap();
        assert_eq!(w.v, w2.v);
    }

    #[test]
    fn decimal_string_rejects_numeric_json() {
        #[derive(Serialize, Deserialize)]
        struct W {
            #[serde(with = "decimal_string")]
            v: Decimal,
        }
        let result: Result<W, _> = serde_json::from_str(r#"{"v": 123.45}"#);
        assert!(result.is_err(), "numeric JSON decimal should be rejected");
    }

    #[test]
    fn decimal_string_rejects_invalid_string() {
        #[derive(Serialize, Deserialize)]
        struct W {
            #[serde(with = "decimal_string")]
            v: Decimal,
        }
        let result: Result<W, _> = serde_json::from_str(r#"{"v": "not-a-number"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn confidence_valid() {
        let c = Confidence::new(Decimal::new(5, 1)).unwrap();
        assert_eq!(c.value(), Decimal::new(5, 1));
    }

    #[test]
    fn confidence_invalid_above_one() {
        assert!(Confidence::new(Decimal::new(15, 1)).is_err());
    }

    #[test]
    fn confidence_invalid_negative() {
        assert!(Confidence::new(Decimal::NEGATIVE_ONE).is_err());
    }

    #[test]
    fn confidence_deserialize_rejects_above_one() {
        let result: Result<Confidence, _> = serde_json::from_str(r#""1.5""#);
        assert!(result.is_err(), "confidence > 1 should be rejected on deserialize");
    }

    #[test]
    fn confidence_deserialize_rejects_negative() {
        let result: Result<Confidence, _> = serde_json::from_str(r#""-0.1""#);
        assert!(result.is_err(), "negative confidence should be rejected on deserialize");
    }

    #[test]
    fn confidence_deserialize_rejects_numeric() {
        let result: Result<Confidence, _> = serde_json::from_str("0.5");
        assert!(result.is_err(), "numeric confidence should be rejected");
    }

    #[test]
    fn confidence_serde_valid() {
        let c = Confidence::new(Decimal::new(5, 1)).unwrap();
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, r#""0.5""#);
        let c2: Confidence = serde_json::from_str(&json).unwrap();
        assert_eq!(c, c2);
    }
}

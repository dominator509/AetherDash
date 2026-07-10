//! Decimal serialization helpers. SPEC-001: all money/price/size/fee/edge values
//! are arbitrary-precision decimals. Wire format (JSON and proto) is decimal STRING.
//! Floats are FORBIDDEN for these values in all three languages.

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Serialize a Decimal as a string (JSON wire format per SPEC-001).
pub mod decimal_string {
    use super::*;

    pub fn serialize<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
        d.to_string().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Decimal, D::Error> {
        let s = String::deserialize(d)?;
        Decimal::from_str_exact(&s).map_err(serde::de::Error::custom)
    }
}

/// Serialize an Option<Decimal> as a string, omitting None (canonical rule: omit-none).
pub mod decimal_option_string {
    use super::*;

    pub fn serialize<S: Serializer>(d: &Option<Decimal>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(val) => val.to_string().serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Decimal>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|s| Decimal::from_str_exact(&s).map_err(serde::de::Error::custom))
            .transpose()
    }
}

/// Confidence value: Decimal constrained to [0, 1].
/// Constructor enforces the invariant. SPEC-001: confidence 0..=1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Confidence(#[serde(with = "decimal_string")] Decimal);

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
        let d = Decimal::new(12345, 2); // 123.45
        #[derive(Serialize, Deserialize)]
        struct Wrapper {
            #[serde(with = "decimal_string")]
            value: Decimal,
        }
        let w = Wrapper { value: d };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains(r#""123.45""#));
        let w2: Wrapper = serde_json::from_str(&json).unwrap();
        assert_eq!(w.value, w2.value);
    }

    #[test]
    fn confidence_valid() {
        let c = Confidence::new(Decimal::new(5, 1)).unwrap(); // 0.5
        assert_eq!(c.value(), Decimal::new(5, 1));
    }

    #[test]
    fn confidence_invalid() {
        assert!(Confidence::new(Decimal::new(15, 1)).is_err()); // 1.5
        assert!(Confidence::new(Decimal::NEGATIVE_ONE).is_err());
    }
}

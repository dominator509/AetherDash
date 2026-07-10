//! Market types: InstrumentKind, Market, PriceSemantics. SPEC-001 market data.

use crate::ids::{MarketKey, VenueId};
use crate::time::UtcTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    BinaryContract,
    CategoricalContract,
    ScalarContract,
    Equity,
    Option,
    Perp,
    Spot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketStatus {
    Open,
    Halted,
    Closed,
    Resolved,
}

/// PriceSemantics with validated decimal string fields.
/// tick_size, min, max are decimal strings — validated on deserialization.
#[derive(Debug, Clone, PartialEq)]
pub enum PriceSemantics {
    Probability { tick_size: DecimalString },
    Scalar { unit: String, min: DecimalString, max: DecimalString },
    Currency,
}

/// A validated decimal string — always parses as a valid Decimal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecimalString(String);

impl DecimalString {
    pub fn new(s: impl Into<String>) -> Result<Self, rust_decimal::Error> {
        let s = s.into();
        rust_decimal::Decimal::from_str_exact(&s)?;
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for DecimalString {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DecimalString {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        rust_decimal::Decimal::from_str_exact(&s)
            .map_err(|e| serde::de::Error::custom(format!("invalid decimal string: {e}")))?;
        Ok(Self(s))
    }
}

// Custom serde for PriceSemantics through validation
impl Serialize for PriceSemantics {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            PriceSemantics::Probability { tick_size } => {
                let mut st = serializer.serialize_struct("PriceSemantics", 2)?;
                st.serialize_field("kind", "probability")?;
                st.serialize_field("tick_size", tick_size)?;
                st.end()
            }
            PriceSemantics::Scalar { unit, min, max } => {
                let mut st = serializer.serialize_struct("PriceSemantics", 4)?;
                st.serialize_field("kind", "scalar")?;
                st.serialize_field("unit", unit)?;
                st.serialize_field("min", min)?;
                st.serialize_field("max", max)?;
                st.end()
            }
            PriceSemantics::Currency => {
                let mut st = serializer.serialize_struct("PriceSemantics", 1)?;
                st.serialize_field("kind", "currency")?;
                st.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for PriceSemantics {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            kind: String,
            #[serde(default)]
            tick_size: Option<DecimalString>,
            #[serde(default)]
            unit: Option<String>,
            #[serde(default)]
            min: Option<DecimalString>,
            #[serde(default)]
            max: Option<DecimalString>,
        }
        let w = Wire::deserialize(deserializer)?;
        match w.kind.as_str() {
            "probability" => Ok(PriceSemantics::Probability {
                tick_size: w
                    .tick_size
                    .ok_or_else(|| serde::de::Error::missing_field("tick_size"))?,
            }),
            "scalar" => Ok(PriceSemantics::Scalar {
                unit: w.unit.ok_or_else(|| serde::de::Error::missing_field("unit"))?,
                min: w.min.ok_or_else(|| serde::de::Error::missing_field("min"))?,
                max: w.max.ok_or_else(|| serde::de::Error::missing_field("max"))?,
            }),
            "currency" => Ok(PriceSemantics::Currency),
            other => Err(serde::de::Error::custom(format!("unknown PriceSemantics kind: {other}"))),
        }
    }
}

/// A trading market. SPEC-001: description_ref is NOT optional per spec.
/// jurisdiction_flags is required (emit empty array, never omit).
/// venue_ref and meta are required JSON objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Market {
    pub key: MarketKey,
    pub venue: VenueId,
    pub kind: InstrumentKind,
    pub title: String,
    pub description_ref: String,
    pub status: MarketStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_ts: Option<UtcTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_ts: Option<UtcTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    pub jurisdiction_flags: Vec<String>,
    pub venue_ref: serde_json::Value,
    pub meta: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_kind_serde_snake_case() {
        let kind = InstrumentKind::BinaryContract;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""binary_contract""#);
        let back: InstrumentKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, InstrumentKind::BinaryContract);
    }

    #[test]
    fn unknown_instrument_kind_is_error() {
        let result: Result<InstrumentKind, _> = serde_json::from_str(r#""future""#);
        assert!(result.is_err());
    }

    #[test]
    fn market_serde_includes_required_fields() {
        let m = Market {
            key: MarketKey::from_string("mkt:kalshi:BTC-75"),
            venue: VenueId::new("kalshi"),
            kind: InstrumentKind::BinaryContract,
            title: "BTC above $75k?".into(),
            description_ref: "BTC-75K-JUL10".into(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec!["US".into()],
            venue_ref: serde_json::json!({"ticker": "BTC-75K-JUL10"}),
            meta: serde_json::json!({"tick_size": "0.01"}),
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("description_ref"));
        assert!(json.contains("jurisdiction_flags"));
    }
}

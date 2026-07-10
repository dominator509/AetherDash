//! Internal identifiers: ULIDs, MarketKey, VenueId, Money. SPEC-001 scalars.
use crate::decimal::decimal_string;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

// ── Ulid ──
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ulid(pub ulid::Ulid);
impl Default for Ulid {
    fn default() -> Self {
        Self(ulid::Ulid::new())
    }
}
impl Ulid {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn from_string(s: &str) -> Result<Self, ulid::DecodeError> {
        Ok(Self(ulid::Ulid::from_string(s)?))
    }
}
impl fmt::Display for Ulid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Serialize for Ulid {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for Ulid {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        ulid::Ulid::from_string(&s)
            .map(Ulid)
            .map_err(|e| serde::de::Error::custom(format!("invalid ULID: {e}")))
    }
}

// ── VenueId ──
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VenueId(String);
#[derive(Debug, thiserror::Error)]
#[error("VenueId must be lowercase alphanumeric, got: {raw}")]
pub struct VenueIdError {
    pub raw: String,
}

impl VenueId {
    pub fn new(s: impl Into<String>) -> Result<Self, VenueIdError> {
        let s = s.into();
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
            return Err(VenueIdError { raw: s });
        }
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for VenueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Serialize for VenueId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for VenueId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        VenueId::new(s).map_err(serde::de::Error::custom)
    }
}

// ── MarketKey ──
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MarketKey(String);
#[derive(Debug, thiserror::Error)]
#[error("MarketKey must match mkt:{{venue}}:{{native_id}}, got: {raw}")]
pub struct MarketKeyError {
    pub raw: String,
}

impl From<VenueIdError> for MarketKeyError {
    fn from(e: VenueIdError) -> Self {
        MarketKeyError { raw: e.raw }
    }
}

impl MarketKey {
    pub fn new(venue: &VenueId, native_id: &str) -> Result<Self, MarketKeyError> {
        if native_id.is_empty() {
            return Err(MarketKeyError { raw: format!("mkt:{}:", venue.0) });
        }
        Ok(Self(format!("mkt:{}:{}", venue.0, native_id)))
    }
    pub fn from_string(s: impl Into<String>) -> Result<Self, MarketKeyError> {
        let s = s.into();
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() != 3 || parts[0] != "mkt" || parts[1].is_empty() || parts[2].is_empty() {
            return Err(MarketKeyError { raw: s });
        }
        Ok(Self(s))
    }
    #[cfg(any(test, feature = "golden-gen"))]
    pub fn from_string_unchecked(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for MarketKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Serialize for MarketKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for MarketKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        MarketKey::from_string(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

// ── Money ──
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    #[serde(with = "decimal_string")]
    pub amount: Decimal,
    pub currency: String,
}
impl Money {
    pub fn new(amount: Decimal, currency: impl Into<String>) -> Self {
        Self { amount, currency: currency.into() }
    }
    pub fn zero(currency: impl Into<String>) -> Self {
        Self { amount: Decimal::ZERO, currency: currency.into() }
    }
}

// ── Tests ──
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ulid_round_trip() {
        let u = Ulid::new();
        let j = serde_json::to_string(&u).unwrap();
        assert_eq!(serde_json::from_str::<Ulid>(&j).unwrap(), u);
    }
    #[test]
    fn ulid_rejects_invalid() {
        assert!(serde_json::from_str::<Ulid>(r#""bad""#).is_err());
    }
    #[test]
    fn venue_id_rejects_empty() {
        assert!(VenueId::new("").is_err());
    }
    #[test]
    fn venue_id_rejects_upper() {
        assert!(VenueId::new("Kalshi").is_err());
    }
    #[test]
    fn venue_id_valid() {
        assert!(VenueId::new("kalshi").is_ok());
    }
    #[test]
    fn mk_rejects_empty_native() {
        let v = VenueId::new("kalshi").unwrap();
        assert!(MarketKey::new(&v, "").is_err());
    }
    #[test]
    fn mk_new_valid() {
        let v = VenueId::new("kalshi").unwrap();
        let mk = MarketKey::new(&v, "BTC-75").unwrap();
        assert_eq!(mk.as_str(), "mkt:kalshi:BTC-75");
    }
    #[test]
    fn mk_from_string_rejects_bad() {
        assert!(MarketKey::from_string("bad").is_err());
    }
    #[test]
    fn mk_from_string_ok() {
        assert!(MarketKey::from_string("mkt:kalshi:BTC-75").is_ok());
    }
    #[test]
    fn mk_deserialize_rejects_bad() {
        assert!(serde_json::from_str::<MarketKey>(r#""bad""#).is_err());
    }
    #[test]
    fn money_serde_string() {
        let m = Money::new(Decimal::new(12345, 2), "USD");
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains(r#""123.45""#));
    }
    #[test]
    fn money_rejects_numeric() {
        assert!(serde_json::from_str::<Money>(r#"{"amount":1,"currency":"USD"}"#).is_err());
    }
}

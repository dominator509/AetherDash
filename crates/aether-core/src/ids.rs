//! Internal identifiers: ULIDs for entities, MarketKey for universal market join, VenueId slugs.
//! SPEC-001 scalar rules: IDs use ULID (sortable), venue-native IDs are NEVER internal IDs.

use crate::decimal::decimal_string;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

// ── Ulid ───────────────────────────────────────────────────────────

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

// ── VenueId ────────────────────────────────────────────────────────

/// Venue identifier slug. Validated: lowercase alphanumeric only.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VenueId(String);

impl VenueId {
    pub const KALSHI: &str = "kalshi";
    pub const POLYMARKET: &str = "polymarket";
    pub const HYPERLIQUID: &str = "hyperliquid";
    pub const OPENBB: &str = "openbb";
    pub const ALPACA: &str = "alpaca";

    pub fn new(s: impl Into<String>) -> Self {
        let s = s.into();
        debug_assert!(
            s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "VenueId must be lowercase alphanumeric slug: {s}"
        );
        Self(s)
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
        if !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
            return Err(serde::de::Error::custom(format!(
                "VenueId must be lowercase alphanumeric slug, got: {s}"
            )));
        }
        Ok(Self(s))
    }
}

// ── MarketKey ──────────────────────────────────────────────────────

/// MarketKey = `mkt:{venue}:{native_id}` — the universal join key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MarketKey(String);

impl MarketKey {
    pub fn new(venue: &VenueId, native_id: &str) -> Self {
        Self(format!("mkt:{}:{}", venue.0, native_id))
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        let s = s.into();
        debug_assert!(
            s.starts_with("mkt:") && s.split(':').count() >= 3,
            "MarketKey must match mkt:{{venue}}:{{native_id}}: {s}"
        );
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn venue(&self) -> Option<VenueId> {
        let parts: Vec<&str> = self.0.splitn(3, ':').collect();
        if parts.len() == 3 && parts[0] == "mkt" {
            Some(VenueId::new(parts[1]))
        } else {
            None
        }
    }

    pub fn native_id(&self) -> Option<&str> {
        let parts: Vec<&str> = self.0.splitn(3, ':').collect();
        if parts.len() == 3 {
            Some(parts[2])
        } else {
            None
        }
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
        let s = String::deserialize(d)?;
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() != 3 || parts[0] != "mkt" || parts[1].is_empty() || parts[2].is_empty() {
            return Err(serde::de::Error::custom(format!(
                "MarketKey must match mkt:{{venue}}:{{native_id}}, got: {s}"
            )));
        }
        Ok(Self(s))
    }
}

// ── Money ──────────────────────────────────────────────────────────

/// Money value with currency. SPEC-001: decimals as strings on the wire.
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

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_round_trip() {
        let u = Ulid::new();
        let json = serde_json::to_string(&u).unwrap();
        let u2: Ulid = serde_json::from_str(&json).unwrap();
        assert_eq!(u, u2);
    }

    #[test]
    fn ulid_rejects_invalid() {
        let result: Result<Ulid, _> = serde_json::from_str(r#""not-a-ulid""#);
        assert!(result.is_err());
    }

    #[test]
    fn venue_id_rejects_uppercase() {
        let result: Result<VenueId, _> = serde_json::from_str(r#""Kalshi""#);
        assert!(result.is_err());
    }

    #[test]
    fn venue_id_rejects_special_chars() {
        let result: Result<VenueId, _> = serde_json::from_str(r#""kalshi!""#);
        assert!(result.is_err());
    }

    #[test]
    fn venue_id_accepts_valid() {
        let v: VenueId = serde_json::from_str(r#""kalshi""#).unwrap();
        assert_eq!(v.as_str(), "kalshi");
    }

    #[test]
    fn market_key_format() {
        let venue = VenueId::new("kalshi");
        let mk = MarketKey::new(&venue, "BTC-75");
        assert_eq!(mk.as_str(), "mkt:kalshi:BTC-75");
        assert_eq!(mk.venue().unwrap().as_str(), "kalshi");
        assert_eq!(mk.native_id().unwrap(), "BTC-75");
    }

    #[test]
    fn market_key_rejects_invalid_format() {
        let result: Result<MarketKey, _> = serde_json::from_str(r#""not-a-market-key""#);
        assert!(result.is_err());
    }

    #[test]
    fn market_key_rejects_missing_parts() {
        let result: Result<MarketKey, _> = serde_json::from_str(r#""mkt:onlytwo""#);
        assert!(result.is_err());
    }

    #[test]
    fn market_key_accepts_valid() {
        let mk: MarketKey = serde_json::from_str(r#""mkt:kalshi:BTC-75""#).unwrap();
        assert_eq!(mk.as_str(), "mkt:kalshi:BTC-75");
    }

    #[test]
    fn market_key_serde_round_trip() {
        let mk = MarketKey::from_string("mkt:kalshi:BTC-75");
        let json = serde_json::to_string(&mk).unwrap();
        assert_eq!(json, r#""mkt:kalshi:BTC-75""#);
        let mk2: MarketKey = serde_json::from_str(&json).unwrap();
        assert_eq!(mk, mk2);
    }

    #[test]
    fn money_serde_decimal_is_string() {
        let m = Money::new(Decimal::new(12345, 2), "USD");
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains(r#""123.45""#));
        // Verify it's a JSON string, not a number
        assert!(json.contains(r#""amount":"123.45""#));
        let m2: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(m, m2);
    }

    #[test]
    fn money_rejects_numeric_decimal() {
        let result: Result<Money, _> =
            serde_json::from_str(r#"{"amount": 123.45, "currency": "USD"}"#);
        assert!(result.is_err(), "numeric decimal in Money should be rejected");
    }
}

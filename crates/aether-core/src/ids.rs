//! Internal identifiers: ULIDs for entities, MarketKey for universal market join, VenueId slugs.
//! SPEC-001 scalar rules: IDs use ULID (sortable), venue-native IDs are NEVER internal IDs.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A ULID — universally unique, lexicographically sortable identifier.
/// Internal entity IDs use ULID; venue-native identifiers live in `venue_ref` pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ulid(#[serde(with = "ulid_serde")] pub ulid::Ulid);

mod ulid_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(u: &ulid::Ulid, s: S) -> Result<S::Ok, S::Error> {
        u.to_string().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ulid::Ulid, D::Error> {
        let s = String::deserialize(d)?;
        ulid::Ulid::from_string(&s).map_err(serde::de::Error::custom)
    }
}

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

/// MarketKey = `mkt:{venue}:{native_id}` — the universal join key.
/// Stable, unique, used across all DBs, the bus, and the UI.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarketKey(String);

impl MarketKey {
    pub fn new(venue: &VenueId, native_id: &str) -> Self {
        Self(format!("mkt:{}:{}", venue.0, native_id))
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn venue(&self) -> Option<VenueId> {
        let parts: Vec<&str> = self.0.splitn(3, ':').collect();
        if parts.len() == 3 && parts[0] == "mkt" {
            Some(VenueId(parts[1].to_string()))
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

/// Venue identifier slug (kalshi, polymarket, hyperliquid, openbb, alpaca).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VenueId(pub String);

impl VenueId {
    pub const KALSHI: &str = "kalshi";
    pub const POLYMARKET: &str = "polymarket";
    pub const HYPERLIQUID: &str = "hyperliquid";
    pub const OPENBB: &str = "openbb";
    pub const ALPACA: &str = "alpaca";

    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
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

/// Money value with currency. Decimals as rust_decimal::Decimal; wire format is decimal string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
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
    fn market_key_format() {
        let venue = VenueId::new("kalshi");
        let mk = MarketKey::new(&venue, "BTC-75");
        assert_eq!(mk.as_str(), "mkt:kalshi:BTC-75");
        assert_eq!(mk.venue().unwrap().as_str(), "kalshi");
        assert_eq!(mk.native_id().unwrap(), "BTC-75");
    }

    #[test]
    fn market_key_serde() {
        let mk = MarketKey::from_string("mkt:kalshi:BTC-75");
        let json = serde_json::to_string(&mk).unwrap();
        assert_eq!(json, r#""mkt:kalshi:BTC-75""#);
        let mk2: MarketKey = serde_json::from_str(&json).unwrap();
        assert_eq!(mk, mk2);
    }

    #[test]
    fn money_serde_decimal_string() {
        let m = Money::new(Decimal::new(12345, 2), "USD");
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("123.45"));
        let m2: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(m, m2);
    }
}

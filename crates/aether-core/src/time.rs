//! UTC timestamp wrapper. All `ts` fields are UTC.
//! JSON wire: RFC3339 with millisecond precision. Proto: google.protobuf.Timestamp.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A UTC timestamp. All AETHER timestamps are UTC.
/// JSON format: RFC3339 with milliseconds (e.g., "2026-07-10T12:34:56.789Z").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UtcTime(time::OffsetDateTime);

impl UtcTime {
    pub fn now() -> Self {
        Self(time::OffsetDateTime::now_utc())
    }

    pub fn from_unix_millis(ms: i64) -> Result<Self, time::error::ComponentRange> {
        let nanos: i128 = (ms as i128) * 1_000_000;
        let dt = time::OffsetDateTime::from_unix_timestamp_nanos(nanos)?;
        Ok(Self(dt))
    }

    pub fn unix_millis(&self) -> i64 {
        (self.0.unix_timestamp_nanos() / 1_000_000) as i64
    }

    pub fn inner(&self) -> time::OffsetDateTime {
        self.0
    }
}

impl fmt::Display for UtcTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // RFC3339 with milliseconds
        write!(
            f,
            "{}",
            self.0
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|_| fmt::Error)?
        )
    }
}

impl Serialize for UtcTime {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for UtcTime {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let dt = time::OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339)
            .map_err(serde::de::Error::custom)?;
        Ok(Self(dt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_time_round_trip() {
        let t = UtcTime::from_unix_millis(1752152096789).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        let t2: UtcTime = serde_json::from_str(&json).unwrap();
        assert_eq!(t, t2);
    }

    #[test]
    fn utc_time_display_is_rfc3339() {
        let t = UtcTime::from_unix_millis(1752152096789).unwrap();
        let s = t.to_string();
        assert!(s.contains("T"));
        assert!(s.contains("Z") || s.contains("+00:00"));
    }
}

//! UTC timestamp wrapper. All `ts` fields are UTC.
//! JSON wire: YYYY-MM-DDTHH:MM:SS.mmmZ. Non-UTC offsets rejected on deserialization.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A UTC timestamp normalized to millisecond precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UtcTime(time::OffsetDateTime);

impl UtcTime {
    pub fn now() -> Self {
        let now = time::OffsetDateTime::now_utc();
        let ms = now.unix_timestamp_nanos() / 1_000_000;
        let nanos = ms * 1_000_000;
        // SAFETY: nanos is always a valid timestamp (normalized from current time)
        #[allow(clippy::expect_used)]
        Self(
            time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
                .expect("millisecond-normalized current timestamp"),
        )
    }

    pub fn from_unix_millis(ms: i64) -> Result<Self, time::error::ComponentRange> {
        let nanos = (ms as i128) * 1_000_000;
        Ok(Self(time::OffsetDateTime::from_unix_timestamp_nanos(nanos)?))
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
        let t = self.0.time();
        let d = self.0.date();
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            d.year(),
            d.month() as u8,
            d.day(),
            t.hour(),
            t.minute(),
            t.second(),
            t.nanosecond() / 1_000_000
        )
    }
}

impl Serialize for UtcTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for UtcTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let dt = time::OffsetDateTime::parse(&s, &time::format_description::well_known::Rfc3339)
            .map_err(|e| serde::de::Error::custom(format!("invalid RFC3339 timestamp: {e}")))?;
        if dt.offset() != time::UtcOffset::UTC {
            return Err(serde::de::Error::custom(format!(
                "timestamp must be UTC, got offset: {}",
                dt.offset()
            )));
        }
        // Normalize to millisecond precision
        let ms = dt.unix_timestamp_nanos() / 1_000_000;
        let nanos = ms * 1_000_000;
        // SAFETY: nanos is always valid (derived from successfully-parsed timestamp)
        #[allow(clippy::expect_used)]
        let normalized = time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .expect("millisecond-normalized timestamp");
        Ok(Self(normalized))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_time_serializes_millis_z() {
        let t = UtcTime::from_unix_millis(1752150896789).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.starts_with('"'));
        assert!(json.ends_with("Z\""));
        assert!(json.contains('T'));
        assert!(json.contains(':'));
        assert!(json.contains('.'));
        // "YYYY-MM-DDTHH:MM:SS.mmmZ" = 24 chars + 2 quotes = 26
        assert_eq!(json.len(), 26);
    }

    #[test]
    fn utc_time_accepts_z_suffix() {
        let t: UtcTime = serde_json::from_str(r#""2026-07-10T12:34:56.789Z""#).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#""2026-07-10T12:34:56.789Z""#);
    }

    #[test]
    fn utc_time_accepts_utc_offset() {
        let t: UtcTime = serde_json::from_str(r#""2026-07-10T12:34:56.789+00:00""#).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#""2026-07-10T12:34:56.789Z""#);
    }

    #[test]
    fn utc_time_rejects_non_utc_offset() {
        let result: Result<UtcTime, _> = serde_json::from_str(r#""2026-07-10T08:34:56.789-04:00""#);
        assert!(result.is_err(), "non-UTC offset should be rejected");
    }

    #[test]
    fn utc_time_round_trip_millis() {
        let t = UtcTime::from_unix_millis(1752150896789).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        let t2: UtcTime = serde_json::from_str(&json).unwrap();
        assert_eq!(t, t2);
        assert_eq!(t.unix_millis(), t2.unix_millis());
    }

    #[test]
    fn utc_time_normalizes_sub_millis() {
        let t: UtcTime = serde_json::from_str(r#""2026-07-10T12:34:56.789123Z""#).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#""2026-07-10T12:34:56.789Z""#);
    }
}

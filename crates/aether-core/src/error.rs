//! Error envelope type with closed code set. SPEC-003/SPEC-006 error handling.

use crate::ids::Ulid;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidArgument,
    Unauthenticated,
    PermissionDenied,
    NotFound,
    FailedPrecondition,
    Unavailable,
    DeadlineExceeded,
    Quarantined,
    Internal,
}

impl ErrorCode {
    pub fn is_retryable(&self) -> bool {
        matches!(self, ErrorCode::Unavailable | ErrorCode::DeadlineExceeded)
    }
}

/// Standard error envelope. retryable is derived from code during deserialization —
/// an incoming retryable field that contradicts the code is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorEnvelope {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub trace_id: Ulid,
    pub details: Option<String>,
}

impl ErrorEnvelope {
    pub fn new(code: ErrorCode, message: impl Into<String>, trace_id: Ulid) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: code.is_retryable(),
            trace_id,
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

// Custom serde — validate code/retryable consistency on deserialize
impl Serialize for ErrorEnvelope {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("ErrorEnvelope", 5)?;
        st.serialize_field("code", &self.code)?;
        st.serialize_field("message", &self.message)?;
        st.serialize_field("retryable", &self.retryable)?;
        st.serialize_field("trace_id", &self.trace_id)?;
        if let Some(ref d) = self.details {
            st.serialize_field("details", d)?;
        }
        st.end()
    }
}

impl<'de> Deserialize<'de> for ErrorEnvelope {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            code: ErrorCode,
            message: String,
            retryable: bool,
            trace_id: Ulid,
            details: Option<String>,
        }
        let w = Wire::deserialize(d)?;
        let expected = w.code.is_retryable();
        if w.retryable != expected {
            return Err(serde::de::Error::custom(format!(
                "retryable field ({}) contradicts code {:?} (is_retryable={})",
                w.retryable, w.code, expected
            )));
        }
        Ok(Self {
            code: w.code,
            message: w.message,
            retryable: w.retryable,
            trace_id: w.trace_id,
            details: w.details,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_retryable_table() {
        // SPEC-006: only Unavailable and DeadlineExceeded are retryable
        let table = [
            (ErrorCode::InvalidArgument, false),
            (ErrorCode::Unauthenticated, false),
            (ErrorCode::PermissionDenied, false),
            (ErrorCode::NotFound, false),
            (ErrorCode::FailedPrecondition, false),
            (ErrorCode::Unavailable, true),
            (ErrorCode::DeadlineExceeded, true),
            (ErrorCode::Quarantined, false),
            (ErrorCode::Internal, false),
        ];
        for (code, expected) in &table {
            assert_eq!(code.is_retryable(), *expected, "wrong retryable for {code:?}");
        }
    }

    #[test]
    fn error_envelope_round_trip() {
        let e =
            ErrorEnvelope::new(ErrorCode::FailedPrecondition, "live trading disabled", Ulid::new());
        let json = serde_json::to_string(&e).unwrap();
        let back: ErrorEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(e.code, back.code);
        assert_eq!(e.retryable, back.retryable);
    }

    #[test]
    fn error_envelope_rejects_contradictory_retryable() {
        let json = r#"{"code":"invalid_argument","message":"bad","retryable":true,"trace_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#;
        let result: Result<ErrorEnvelope, _> = serde_json::from_str(json);
        assert!(result.is_err(), "contradictory retryable should be rejected");
    }
}

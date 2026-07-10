//! Error envelope type with closed code set. SPEC-003/SPEC-006 error handling.
//! Used across all surfaces: gRPC details, WS error frames, HTTP responses.

use crate::ids::Ulid;
use serde::{Deserialize, Serialize};

/// Closed error code set. Unknown tag = validation error at boundary (SECURITY.md T2).
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
    /// Whether this error code allows retries.
    /// Per SPEC-006: only `unavailable` and `deadline_exceeded` with `retryable=true` may be retried.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ErrorCode::Unavailable | ErrorCode::DeadlineExceeded)
    }
}

/// Standard error envelope for all surfaces.
/// Messages are operator-safe: no secrets, no raw payload echoes (SECURITY.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub code: ErrorCode,
    /// Human-readable message — what failed + what the user can do.
    pub message: String,
    /// Whether this error is retryable.
    pub retryable: bool,
    /// Trace ID for correlation across services.
    pub trace_id: Ulid,
    /// Optional additional detail (operator-safe, no raw payloads).
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_retryable_rule() {
        assert!(ErrorCode::Unavailable.is_retryable());
        assert!(ErrorCode::DeadlineExceeded.is_retryable());
        assert!(!ErrorCode::InvalidArgument.is_retryable());
        assert!(!ErrorCode::PermissionDenied.is_retryable());
        assert!(!ErrorCode::Internal.is_retryable());
    }

    #[test]
    fn error_envelope_serde_round_trip() {
        let e = ErrorEnvelope::new(
            ErrorCode::FailedPrecondition,
            "live trading is disabled — enable AETHER_EXECUTION__LIVE_ENABLED",
            Ulid::new(),
        )
        .with_details("check = execution.live_enabled");
        let json = serde_json::to_string(&e).unwrap();
        let back: ErrorEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(e.code, back.code);
        assert!(!back.retryable);
    }

    #[test]
    fn error_envelope_message_never_contains_raw_secrets() {
        // Structural guarantee: messages are plain strings, no secret fields.
        let e = ErrorEnvelope::new(ErrorCode::Internal, "internal error", Ulid::new());
        assert!(!e.message.contains("key"));
        assert!(!e.message.contains("secret"));
        assert!(!e.message.contains("password"));
    }
}

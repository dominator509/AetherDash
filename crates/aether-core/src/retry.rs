//! Retry policy configuration type. SPEC-006 retry rules.
//! Trading path: NO automatic retry on submit. Understanding path: exponential backoff.

use std::time::Duration;

/// Retry policy configuration.
/// SPEC-006: exponential backoff with full jitter, base 200ms, cap 30s, max 5 attempts.
#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    /// Base backoff duration (default: 200ms per SPEC-006)
    pub base: Duration,
    /// Maximum backoff cap (default: 30s per SPEC-006)
    pub cap: Duration,
    /// Maximum number of attempts (default: 5 per SPEC-006)
    pub max_attempts: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { base: Duration::from_millis(200), cap: Duration::from_secs(30), max_attempts: 5 }
    }
}

impl RetryPolicy {
    /// Create the standard understanding-path retry policy per SPEC-006.
    pub fn standard() -> Self {
        Self::default()
    }

    /// Compute the backoff duration for attempt `n` (1-indexed).
    /// Uses exponential backoff: base * 2^(n-1), capped at `cap`.
    /// Full jitter should be applied by the caller (not included here).
    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let raw = self.base * 2u32.pow(attempt.saturating_sub(1));
        raw.min(self.cap)
    }

    /// Whether attempt `n` should be attempted.
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt <= self.max_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_defaults_match_spec() {
        let p = RetryPolicy::default();
        assert_eq!(p.base, Duration::from_millis(200));
        assert_eq!(p.cap, Duration::from_secs(30));
        assert_eq!(p.max_attempts, 5);
    }

    #[test]
    fn retry_policy_exponential_backoff() {
        let p = RetryPolicy::default();
        // attempt 1: 200ms
        assert_eq!(p.backoff_for_attempt(1), Duration::from_millis(200));
        // attempt 2: 400ms
        assert_eq!(p.backoff_for_attempt(2), Duration::from_millis(400));
        // attempt 3: 800ms
        assert_eq!(p.backoff_for_attempt(3), Duration::from_millis(800));
        // attempt 8: 25.6s
        assert_eq!(p.backoff_for_attempt(8), Duration::from_millis(25_600));
        // attempt 9: capped at 30s
        assert_eq!(p.backoff_for_attempt(9), Duration::from_secs(30));
    }

    #[test]
    fn retry_policy_max_attempts() {
        let p = RetryPolicy::default();
        assert!(p.should_retry(1));
        assert!(p.should_retry(5));
        assert!(!p.should_retry(6));
    }
}

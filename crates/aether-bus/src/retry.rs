//! SPEC-006 retry policy and circuit breaker.
//!
//! Retry policy: 200 ms base, 30-second cap, 5 attempts,
//! full jitter on each attempt, retryable-only gating.
//!
//! Circuit breaker: per-stream isolation, 5 consecutive failures,
//! 30-second error-rate window, single half-open probe.

use std::collections::VecDeque;
use std::time::Duration;

// ── Retry policy ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// Compute the delay for attempt `n` (0-indexed) with full jitter.
    /// capped delay = min(max_delay, base_delay * 2^n)
    /// actual delay = uniformly random in [0, capped_delay]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let capped = self.base_delay.saturating_mul(1u32 << attempt.min(31)).min(self.max_delay);
        // Full jitter: uniformly random in [0, capped]
        let capped_ns = capped.as_nanos();
        let jitter_ns: u128 = if capped_ns > 0 {
            rand::Rng::gen_range(&mut rand::thread_rng(), 0..=capped_ns)
        } else {
            0
        };
        Duration::from_nanos(jitter_ns as u64)
    }

    /// Returns true if the error is retryable.
    /// SPEC-006: both the error code must be retryable AND the retryable flag
    /// must be set. An `unavailable` error with `retryable=false` is NOT retried.
    pub fn is_retryable(code: &aether_core::error::ErrorCode, retryable: bool) -> bool {
        retryable && code.is_retryable()
    }
}

// ── Circuit breaker ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    state: BreakerState,
    consecutive_failures: u32,
    failure_window_start: Option<std::time::Instant>,
    /// Timestamp when the breaker last transitioned to Open
    opened_at: Option<std::time::Instant>,
    /// Whether the single half-open probe has been sent (prevents multiple probes)
    half_open_probe_sent: bool,
    /// Rolling window: timestamped outcomes for error-rate triggering.
    /// Entries older than SPEC_ERROR_WINDOW are trimmed on each push.
    recent_outcomes: VecDeque<(std::time::Instant, bool)>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreaker {
    /// SPEC-006: 5 consecutive failures, 15-second reset timeout.
    pub const SPEC_DEFAULT_THRESHOLD: u32 = 5;
    pub const SPEC_DEFAULT_RESET: Duration = Duration::from_secs(15);
    pub const SPEC_ERROR_WINDOW: Duration = Duration::from_secs(30);

    pub fn new() -> Self {
        Self {
            state: BreakerState::Closed,
            consecutive_failures: 0,
            failure_window_start: None,
            opened_at: None,
            half_open_probe_sent: false,
            recent_outcomes: VecDeque::new(),
        }
    }

    pub fn state(&self) -> BreakerState {
        self.state
    }

    /// Record a successful request.
    /// SPEC-006: in HalfOpen, a single success transitions to Closed.
    /// In Closed, successes always reset the consecutive failure count.
    pub fn record_success(&mut self) {
        // Track outcome for rolling-window error-rate trigger
        self.push_outcome(false);

        match self.state {
            BreakerState::HalfOpen => {
                self.state = BreakerState::Closed;
                self.consecutive_failures = 0;
                self.failure_window_start = None;
                self.half_open_probe_sent = false;
            }
            BreakerState::Closed => {
                // Always reset consecutive failures on success in Closed
                self.consecutive_failures = 0;
                self.failure_window_start = None;
            }
            BreakerState::Open => { /* ignore successes while open */ }
        }
    }

    /// Record a failed request.
    /// SPEC-006: 5 consecutive failures in a 30-second window → Open.
    /// Or >50% errors in the rolling 30-second window → Open.
    /// In HalfOpen, a single failure transitions back to Open.
    pub fn record_failure(&mut self) {
        // Track outcome for rolling-window error-rate trigger
        self.push_outcome(true);

        // HalfOpen probe failed → back to Open immediately
        if self.state == BreakerState::HalfOpen {
            self.state = BreakerState::Open;
            self.opened_at = Some(std::time::Instant::now());
            self.consecutive_failures = 0;
            self.half_open_probe_sent = false;
            return;
        }

        let now = std::time::Instant::now();

        // Reset the failure window if it has elapsed
        if let Some(start) = self.failure_window_start {
            if now.duration_since(start) >= Self::SPEC_ERROR_WINDOW {
                self.consecutive_failures = 0;
                self.failure_window_start = Some(now);
            }
        } else {
            self.failure_window_start = Some(now);
        }

        self.consecutive_failures += 1;

        // Trigger 1: consecutive failures threshold
        let consecutive_trigger = self.consecutive_failures >= Self::SPEC_DEFAULT_THRESHOLD;

        // Trigger 2: >50% error rate in rolling window
        let error_rate_trigger = self.error_rate_exceeded();

        if self.state == BreakerState::Closed && (consecutive_trigger || error_rate_trigger) {
            self.state = BreakerState::Open;
            self.opened_at = Some(now);
        }
    }

    /// Push a timestamped outcome and trim entries older than SPEC_ERROR_WINDOW.
    fn push_outcome(&mut self, was_error: bool) {
        let now = std::time::Instant::now();
        let cutoff = now - Self::SPEC_ERROR_WINDOW;
        while self.recent_outcomes.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
            self.recent_outcomes.pop_front();
        }
        self.recent_outcomes.push_back((now, was_error));
    }

    /// Returns true if >50% of outcomes in the rolling window are errors.
    /// Requires at least SPEC_DEFAULT_THRESHOLD entries to avoid tripping
    /// on tiny sample sizes.
    fn error_rate_exceeded(&self) -> bool {
        let total = self.recent_outcomes.len();
        if total < Self::SPEC_DEFAULT_THRESHOLD as usize {
            return false;
        }
        let errors = self.recent_outcomes.iter().filter(|(_, e)| *e).count();
        // >50%: strictly more errors than successes
        errors > total / 2
    }

    /// Returns true if a request should be allowed.
    /// SPEC-006: in Open, wait for reset_timeout then allow a single HalfOpen probe.
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            BreakerState::Closed => true,
            BreakerState::Open => {
                if let Some(opened) = self.opened_at {
                    if opened.elapsed() >= Self::SPEC_DEFAULT_RESET {
                        // Transition to HalfOpen and mark the single probe as sent —
                        // prevents a second caller from also getting a probe.
                        self.state = BreakerState::HalfOpen;
                        self.half_open_probe_sent = true;
                        return true;
                    }
                }
                false
            }
            BreakerState::HalfOpen => {
                // Only one probe at a time
                if self.half_open_probe_sent {
                    false
                } else {
                    self.half_open_probe_sent = true;
                    true
                }
            }
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Retry policy ──────────────────────────────────────────────────

    #[test]
    fn retry_policy_spec_006_defaults() {
        let p = RetryPolicy::default();
        assert_eq!(p.max_attempts, 5);
        assert_eq!(p.base_delay, Duration::from_millis(200));
        assert_eq!(p.max_delay, Duration::from_secs(30));
    }

    #[test]
    fn retry_policy_is_retryable_table() {
        use aether_core::error::ErrorCode;

        // Retryable code + retryable flag = retryable
        assert!(RetryPolicy::is_retryable(&ErrorCode::Unavailable, true));
        assert!(RetryPolicy::is_retryable(&ErrorCode::DeadlineExceeded, true));

        // Non-retryable codes are never retried, regardless of flag
        assert!(!RetryPolicy::is_retryable(&ErrorCode::InvalidArgument, true));
        assert!(!RetryPolicy::is_retryable(&ErrorCode::Internal, true));
        assert!(!RetryPolicy::is_retryable(&ErrorCode::NotFound, true));

        // Retryable code but retryable=false → NOT retried
        assert!(
            !RetryPolicy::is_retryable(&ErrorCode::Unavailable, false),
            "unavailable with retryable=false must NOT be retried"
        );
        assert!(
            !RetryPolicy::is_retryable(&ErrorCode::DeadlineExceeded, false),
            "deadline_exceeded with retryable=false must NOT be retried"
        );
    }

    #[test]
    fn retry_policy_capped_at_max() {
        let p = RetryPolicy::default();
        // attempt 100 would be huge without cap
        let d = p.delay_for_attempt(100);
        assert!(d <= p.max_delay);
    }

    #[test]
    fn retry_delay_always_within_bounds() {
        let p = RetryPolicy::default();
        for attempt in 0..5 {
            let d = p.delay_for_attempt(attempt);
            let cap = p.base_delay.saturating_mul(1u32 << attempt.min(31)).min(p.max_delay);
            assert!(d <= cap, "delay {d:?} exceeds cap {cap:?} at attempt {attempt}");
        }
    }

    #[test]
    fn retry_jitter_spans_full_range_for_large_cap() {
        // With a 100ms cap, take 1000 samples. At least one should be >90ms
        // (probability of all samples ≤90ms is (0.9)^1000 ≈ 10^-46).
        let p = RetryPolicy {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(100),
            max_attempts: 5,
        };
        let mut max_seen = Duration::ZERO;
        for _ in 0..1000 {
            let d = p.delay_for_attempt(0);
            max_seen = max_seen.max(d);
        }
        assert!(
            max_seen > Duration::from_millis(90),
            "jitter never exceeded 90ms in 1000 samples (max={max_seen:?}); \
             likely not uniformly distributed across the full interval"
        );
    }

    // ── Circuit breaker ───────────────────────────────────────────────

    #[test]
    fn breaker_starts_closed() {
        let mut b = CircuitBreaker::new();
        assert_eq!(b.state(), BreakerState::Closed);
        assert!(b.allow_request());
    }

    #[test]
    fn breaker_opens_after_spec_threshold() {
        let mut b = CircuitBreaker::new();
        for _ in 0..5 {
            assert!(b.allow_request());
            b.record_failure();
        }
        assert_eq!(b.state(), BreakerState::Open);
        assert!(!b.allow_request());
    }

    #[test]
    fn breaker_half_open_after_reset_timeout() {
        let mut b = CircuitBreaker::new();
        // Override to use a very short reset for test speed
        b.consecutive_failures = 5;
        b.state = BreakerState::Open;
        b.opened_at = Some(std::time::Instant::now() - Duration::from_secs(60));
        assert!(b.allow_request());
        assert_eq!(b.state(), BreakerState::HalfOpen);
    }

    #[test]
    fn breaker_single_success_closes_from_half_open() {
        let mut b = CircuitBreaker::new();
        b.state = BreakerState::HalfOpen;
        b.consecutive_failures = 5;
        b.record_success();
        assert_eq!(b.state(), BreakerState::Closed);
        assert_eq!(b.consecutive_failures, 0);
    }

    #[test]
    fn breaker_failure_in_half_open_reopens() {
        let mut b = CircuitBreaker::new();
        b.state = BreakerState::HalfOpen;
        b.half_open_probe_sent = true;
        b.record_failure();
        // A failed half-open probe transitions back to Open
        assert_eq!(b.state(), BreakerState::Open);
        assert!(!b.allow_request());
    }

    #[test]
    fn breaker_success_resets_window_in_closed() {
        let mut b = CircuitBreaker::new();
        b.record_failure();
        b.record_failure();
        assert_eq!(b.consecutive_failures, 2);
        // Any success in Closed resets the counter (no window check needed)
        b.record_success();
        assert_eq!(b.consecutive_failures, 0);
    }

    #[test]
    fn breaker_half_open_single_probe_only() {
        // Two callers after Open expiry: first gets a probe, second is refused.
        let mut b = CircuitBreaker::new();
        b.consecutive_failures = 5;
        b.state = BreakerState::Open;
        b.opened_at = Some(std::time::Instant::now() - Duration::from_secs(60));
        // First call — transition to HalfOpen, probe sent
        assert!(b.allow_request(), "first caller must get a probe");
        assert_eq!(b.state(), BreakerState::HalfOpen);
        // Second call — probe already sent, must be refused
        assert!(!b.allow_request(), "second caller must be refused");
    }

    #[test]
    fn breaker_error_rate_triggers_open() {
        // >50% errors in 30s rolling window, fewer than 5 consecutive.
        // 6 requests: 4 errors, 2 successes = 66% error rate → Open.
        let mut b = CircuitBreaker::new();
        // Pattern: S, F, S, F, F, F (4 errors out of 6, but not 5 consecutive)
        b.record_success(); // 1: ok
        b.record_failure(); // 2: error
        b.record_success(); // 3: ok — resets consecutive to 0
        b.record_failure(); // 4: error
        b.record_failure(); // 5: error
        b.record_failure(); // 6: error — 4/6=66% > 50% → trip
        assert_eq!(
            b.state(),
            BreakerState::Open,
            "breaker must open on >50% error rate in rolling window"
        );
    }

    #[test]
    fn breaker_error_rate_below_threshold_does_not_trip() {
        // Fewer than 5 total outcomes → error-rate check is skipped.
        let mut b = CircuitBreaker::new();
        b.record_failure();
        b.record_failure();
        b.record_failure();
        b.record_success();
        // 3 errors out of 4, but total < 5 → should not trip
        assert_eq!(b.state(), BreakerState::Closed);
    }

    #[test]
    fn breaker_consecutive_and_error_rate_triggers_independent() {
        // Consecutive trigger: trip without any successes interspersed.
        {
            let mut b = CircuitBreaker::new();
            for _ in 0..5 {
                b.record_failure();
            }
            assert_eq!(b.state(), BreakerState::Open);
        }

        // Error-rate trigger: trip with mixed outcomes but >50% errors.
        {
            let mut b = CircuitBreaker::new();
            // 7 outcomes: 5 errors, 2 successes = 71% → trip
            b.record_success();
            b.record_failure(); // 1
            b.record_failure(); // 2
            b.record_success(); // resets consecutive count
            b.record_failure(); // 3
            b.record_failure(); // 4
            b.record_failure(); // 5 errors / 7 total = 71% → trip
            assert_eq!(
                b.state(),
                BreakerState::Open,
                "error-rate trigger must work independently of consecutive trigger"
            );
        }
    }
}

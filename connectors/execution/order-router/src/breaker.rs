//! Circuit breaker integration for order routing.
//!
//! Each venue gets its own breaker. The router checks breaker state
//! before dispatching live orders. Failures open the breaker;
//! successes reset it. This is execution-path code — fail-closed.

use aether_bus::retry::CircuitBreaker;
use aether_core::ids::VenueId;
use std::collections::HashMap;
use std::sync::Mutex;

/// Router-level configuration for circuit breakers.
///
/// The underlying [`CircuitBreaker`] currently uses its own SPEC-006
/// hardcoded thresholds (5 consecutive failures, 30s error-rate window,
/// 15s reset). This struct is stored for future per-venue overrides
/// once the breaker supports configurable thresholds.
#[derive(Debug, Clone, Copy)]
pub struct BreakerConfig {
    pub consecutive_failure_threshold: u32,
    pub error_rate_threshold: f64,
    pub error_window_secs: f64,
    pub half_open_timeout_secs: f64,
}

impl BreakerConfig {
    /// Defaults matching the SPEC-006 hardcoded thresholds.
    pub const fn spec_defaults() -> Self {
        Self {
            consecutive_failure_threshold: CircuitBreaker::SPEC_DEFAULT_THRESHOLD,
            error_rate_threshold: 0.5,
            error_window_secs: 30.0,
            half_open_timeout_secs: 15.0,
        }
    }
}

/// Router-level circuit breaker registry.
///
/// Each known venue gets its own [`CircuitBreaker`] instance. The router
/// queries [`is_allowed`](Self::is_allowed) before dispatching and calls
/// [`record_success`](Self::record_success) or
/// [`record_failure`](Self::record_failure) after each attempt.
pub struct RouterBreakers {
    breakers: Mutex<HashMap<VenueId, CircuitBreaker>>,
    #[allow(dead_code)]
    base_config: BreakerConfig,
}

impl RouterBreakers {
    pub fn new(base_config: BreakerConfig) -> Self {
        Self { breakers: Mutex::new(HashMap::new()), base_config }
    }

    /// Default config matching the shared SPEC-006 breaker implementation.
    pub fn with_defaults() -> Self {
        Self::new(BreakerConfig::spec_defaults())
    }

    /// Check whether a venue is accepting traffic.
    ///
    /// Returns `true` when the breaker is `Closed` or when a single
    /// half-open probe is permitted. Returns `false` when the breaker
    /// is `Open` and the reset timeout has not elapsed yet.
    pub fn is_allowed(&self, venue_id: &VenueId) -> bool {
        let Ok(mut breakers) = self.breakers.lock() else {
            return false;
        };
        breakers.get_mut(venue_id).is_none_or(|breaker| breaker.allow_request())
    }

    /// Record a successful submission.
    ///
    /// Resets the consecutive-failure counter. If the breaker was in
    /// `HalfOpen` it transitions back to `Closed`.
    pub fn record_success(&self, venue_id: &VenueId) {
        if let Ok(mut breakers) = self.breakers.lock() {
            breakers.entry(venue_id.clone()).or_insert_with(CircuitBreaker::new).record_success();
        }
    }

    /// Record a failed submission.
    ///
    /// Increments the consecutive-failure counter and may transition
    /// the breaker to `Open` if the threshold is exceeded.
    pub fn record_failure(&self, venue_id: &VenueId) {
        if let Ok(mut breakers) = self.breakers.lock() {
            breakers.entry(venue_id.clone()).or_insert_with(CircuitBreaker::new).record_failure();
        }
    }

    /// Get the breaker state for a venue (for health reporting).
    ///
    /// Returns [`BreakerState::Closed`] when the breaker is `Closed`
    /// (fully accepting traffic). All other states (`Open`, `HalfOpen`,
    /// or an unknown venue) are reported as [`BreakerState::Open`].
    pub fn state_for(&self, venue_id: &VenueId) -> BreakerState {
        let Ok(breakers) = self.breakers.lock() else {
            return BreakerState::Open;
        };
        match breakers.get(venue_id) {
            Some(b) if b.state() == aether_bus::retry::BreakerState::Closed => BreakerState::Closed,
            Some(_) => BreakerState::Open,
            None => BreakerState::Closed,
        }
    }
}

/// Simplified breaker state for health reporting.
///
/// Unlike [`aether_bus::retry::BreakerState`] which has three states,
/// this type collapses `HalfOpen` into `Open` for the router's
/// fail-closed health check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_venue_is_allowed() {
        let breakers = RouterBreakers::with_defaults();
        let vid = VenueId::new("test").unwrap();
        assert!(breakers.is_allowed(&vid));
    }

    #[test]
    fn consecutive_failures_open_breaker() {
        // CircuitBreaker uses a hardcoded SPEC-006 threshold of 5.
        let breakers = RouterBreakers::with_defaults();
        let vid = VenueId::new("test").unwrap();
        for _ in 0..5 {
            assert!(breakers.is_allowed(&vid));
            breakers.record_failure(&vid);
        }
        assert!(!breakers.is_allowed(&vid));
    }

    #[test]
    fn success_after_failures_keeps_breaker_closed() {
        let breakers = RouterBreakers::with_defaults();
        let vid = VenueId::new("test").unwrap();
        breakers.record_failure(&vid);
        breakers.record_failure(&vid);
        breakers.record_success(&vid);
        assert!(breakers.is_allowed(&vid));
    }

    #[test]
    fn state_for_returns_closed_when_unknown() {
        let breakers = RouterBreakers::with_defaults();
        let vid = VenueId::new("unknown").unwrap();
        assert_eq!(breakers.state_for(&vid), BreakerState::Closed);
    }

    #[test]
    fn state_for_returns_open_after_failures() {
        let breakers = RouterBreakers::with_defaults();
        let vid = VenueId::new("kalshi").unwrap();
        for _ in 0..5 {
            breakers.record_failure(&vid);
        }
        assert_eq!(breakers.state_for(&vid), BreakerState::Open);
    }

    #[test]
    fn multi_venue_breakers_are_independent() {
        let breakers = RouterBreakers::with_defaults();
        let a = VenueId::new("venuea").unwrap();
        let b = VenueId::new("venueb").unwrap();

        // Open venue A, keep B closed.
        for _ in 0..5 {
            breakers.record_failure(&a);
        }
        assert!(!breakers.is_allowed(&a));
        assert!(breakers.is_allowed(&b));
    }
}

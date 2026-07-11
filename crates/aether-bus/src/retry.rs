use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let d = self.base_delay.mul_f64(self.multiplier.powi(attempt as i32));
        d.min(self.max_delay)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    state: BreakerState,
    failure_count: u32,
    threshold: u32,
    reset_timeout: Duration,
    last_failure: Option<std::time::Instant>,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            state: BreakerState::Closed,
            failure_count: 0,
            threshold,
            reset_timeout,
            last_failure: None,
        }
    }

    pub fn state(&self) -> BreakerState {
        self.state
    }

    pub fn record_success(&mut self) {
        if self.state == BreakerState::HalfOpen {
            self.state = BreakerState::Closed;
            self.failure_count = 0;
        }
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(std::time::Instant::now());
        if self.failure_count >= self.threshold {
            self.state = BreakerState::Open;
        }
    }

    pub fn allow_request(&mut self) -> bool {
        match self.state {
            BreakerState::Closed => true,
            BreakerState::Open => {
                if let Some(t) = self.last_failure {
                    if t.elapsed() >= self.reset_timeout {
                        self.state = BreakerState::HalfOpen;
                        return true;
                    }
                }
                false
            }
            BreakerState::HalfOpen => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_policy_exponential_backoff() {
        let p = RetryPolicy::default();
        assert!(p.delay_for_attempt(0) <= Duration::from_millis(200));
        assert!(p.delay_for_attempt(3) >= Duration::from_millis(400));
    }
    #[test]
    fn breaker_opens_after_threshold() {
        let mut b = CircuitBreaker::new(2, Duration::from_secs(60));
        assert!(b.allow_request());
        b.record_failure();
        b.record_failure();
        assert!(!b.allow_request());
    }
    #[test]
    fn breaker_half_open_after_timeout() {
        let mut b = CircuitBreaker::new(2, Duration::from_millis(1));
        b.record_failure();
        b.record_failure();
        assert!(!b.allow_request());
        std::thread::sleep(Duration::from_millis(2));
        assert!(b.allow_request());
    }
}

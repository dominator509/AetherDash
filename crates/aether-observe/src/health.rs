//! Health and readiness endpoint helpers.
//! /healthz: dependency-free, always responds 200 if the process is alive.
//! /readyz: reflects real dependency state, flips within 10s of loss.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Health status for a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Ok,
    Degraded,
    Down,
}

/// A health checker for a single dependency.
pub trait HealthChecker: Send + Sync {
    fn check(&self) -> HealthStatus;
    fn name(&self) -> &str;
}

/// Simple HTTP health checker that pings a URL.
pub struct HttpHealthChecker {
    name: String,
    url: String,
}

impl HttpHealthChecker {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self { name: name.into(), url: url.into() }
    }
}

impl HealthChecker for HttpHealthChecker {
    fn check(&self) -> HealthStatus {
        match reqwest::blocking::get(&self.url) {
            Ok(resp) if resp.status().is_success() => HealthStatus::Ok,
            Ok(_) => HealthStatus::Degraded,
            Err(_) => HealthStatus::Down,
        }
    }
    fn name(&self) -> &str {
        &self.name
    }
}

/// Readiness tracker with dependency checks.
pub struct ReadyChecker {
    checkers: Vec<Box<dyn HealthChecker>>,
    last_check: Mutex<Instant>,
    last_results: Mutex<HashMap<String, HealthStatus>>,
    check_interval: Duration,
}

impl ReadyChecker {
    pub fn new(check_interval: Duration) -> Self {
        Self {
            checkers: Vec::new(),
            last_check: Mutex::new(Instant::now() - check_interval),
            last_results: Mutex::new(HashMap::new()),
            check_interval,
        }
    }

    pub fn add_checker(&mut self, checker: Box<dyn HealthChecker>) {
        self.checkers.push(checker);
    }

    /// Check all dependencies. Returns true if all are Ok.
    pub fn is_ready(&self) -> bool {
        let mut last = self.last_check.lock().unwrap_or_else(|error| error.into_inner());
        if last.elapsed() < self.check_interval {
            let results = self.last_results.lock().unwrap_or_else(|error| error.into_inner());
            return results.values().all(|s| *s == HealthStatus::Ok);
        }
        *last = Instant::now();
        let mut results = self.last_results.lock().unwrap_or_else(|error| error.into_inner());
        results.clear();
        for checker in &self.checkers {
            let status = checker.check();
            results.insert(checker.name().to_string(), status);
        }
        results.values().all(|s| *s == HealthStatus::Ok)
    }

    /// Get the health status for Prometheus export.
    pub fn health_metric(&self) -> f64 {
        if self.is_ready() {
            1.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockChecker {
        name: String,
        status: HealthStatus,
    }
    impl HealthChecker for MockChecker {
        fn check(&self) -> HealthStatus {
            self.status.clone()
        }
        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn ready_when_all_healthy() {
        let mut rc = ReadyChecker::new(Duration::from_millis(1));
        rc.add_checker(Box::new(MockChecker { name: "db".into(), status: HealthStatus::Ok }));
        assert!(rc.is_ready());
    }

    #[test]
    fn not_ready_when_any_down() {
        let mut rc = ReadyChecker::new(Duration::from_millis(1));
        rc.add_checker(Box::new(MockChecker { name: "db".into(), status: HealthStatus::Ok }));
        rc.add_checker(Box::new(MockChecker { name: "redis".into(), status: HealthStatus::Down }));
        assert!(!rc.is_ready());
    }
}

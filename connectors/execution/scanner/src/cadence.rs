//! Scan cadence controller with cost-aware shedding.
//! SPEC-012: ~500ms target, sheds expensive tail under load.

use std::time::Duration;

pub const SCAN_CYCLE_METRIC: &str = "aether_scan_cycle_ms";

#[derive(Debug, Clone)]
pub struct CadenceController {
    target_cycle_ms: u64,
    shed_thresholds: Vec<u64>,
    pub cycles_completed: u64,
    pub cycles_shed: u64,
    pub last_cycle_ms: u64,
}

impl CadenceController {
    pub fn new(target_cycle_ms: u64) -> Self {
        let pct = |x| (target_cycle_ms as f64 * x) as u64;
        Self {
            target_cycle_ms,
            // Aggressive shedding in the tail: 80%, 90%, 96%, 100% of target
            shed_thresholds: vec![pct(0.80), pct(0.90), pct(0.96), target_cycle_ms],
            cycles_completed: 0,
            cycles_shed: 0,
            last_cycle_ms: 0,
        }
    }

    /// Begin a new scan cycle. Returns which shed level to use (0 = full, 1+ = increasingly aggressive).
    pub fn begin_cycle(&mut self) -> usize {
        let elapsed = self.last_cycle_ms;
        self.cycles_completed += 1;
        let shed_level =
            self.shed_thresholds.iter().filter(|&&threshold| elapsed > threshold).count();
        if shed_level > 0 {
            self.cycles_shed += 1;
        }
        shed_level
    }

    /// End a cycle and record the elapsed time.
    pub fn end_cycle(&mut self, elapsed: Duration) {
        self.last_cycle_ms = elapsed.as_millis() as u64;
    }

    /// Export the scan cycle metric.
    pub fn cycle_metric(&self) -> u64 {
        self.last_cycle_ms
    }

    /// Return the configured target cycle time in milliseconds.
    pub fn target_ms(&self) -> u64 {
        self.target_cycle_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_scan_when_under_budget() {
        let mut cc = CadenceController::new(500);
        cc.last_cycle_ms = 200;
        assert_eq!(cc.begin_cycle(), 0); // full scan
        assert_eq!(cc.cycles_completed, 1);
    }

    #[test]
    fn shed_when_over_budget() {
        let mut cc = CadenceController::new(500);
        cc.last_cycle_ms = 510; // over budget
        assert_eq!(cc.begin_cycle(), 4); // all thresholds exceeded
        assert_eq!(cc.cycles_shed, 1);
        assert_eq!(cc.cycles_completed, 1);
    }

    #[test]
    fn target_ms_returns_configured_value() {
        let cc = CadenceController::new(1000);
        assert_eq!(cc.target_ms(), 1000);
    }

    #[test]
    fn cycle_timing_is_accurate() {
        let mut cc = CadenceController::new(500);
        cc.end_cycle(Duration::from_millis(350));
        assert_eq!(cc.cycle_metric(), 350);
    }
}

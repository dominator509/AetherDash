//! Prometheus-compatible metric registry.
//! All aether_ prefixed metrics are registered here.

use std::collections::HashMap;
use std::sync::Mutex;

/// A simple gauge metric.
#[derive(Debug, Clone)]
pub struct Gauge {
    pub name: String,
    pub help: String,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

/// Metric registry holding all AETHER metrics.
pub struct MetricRegistry {
    gauges: Mutex<HashMap<String, Gauge>>,
}

impl MetricRegistry {
    pub fn new() -> Self {
        Self { gauges: Mutex::new(HashMap::new()) }
    }

    /// Register or update a gauge.
    pub fn set_gauge(&self, name: &str, help: &str, value: f64, labels: HashMap<String, String>) {
        let mut gauges = self.gauges.lock().unwrap();
        gauges.insert(name.to_string(), Gauge { name: name.into(), help: help.into(), value, labels });
    }

    /// Get all registered gauges in Prometheus text format.
    pub fn export_prometheus(&self) -> String {
        let gauges = self.gauges.lock().unwrap();
        let mut output = String::new();
        for gauge in gauges.values() {
            output.push_str(&format!("# HELP {} {}\n", gauge.name, gauge.help));
            output.push_str(&format!("# TYPE {} gauge\n", gauge.name));
            let label_str: Vec<String> = gauge.labels.iter()
                .map(|(k, v)| format!("{}=\"{}\"", k, v))
                .collect();
            let labels = if label_str.is_empty() { String::new() } else { format!("{{{}}}", label_str.join(",")) };
            output.push_str(&format!("{}{} {}\n", gauge.name, labels, gauge.value));
        }
        output
    }

    /// Create the standard OBSERVABILITY.md metrics.
    pub fn register_standard_metrics(&self) {
        self.set_gauge("aether_feed_lag_ms", "Age of most recent normalized tick", 0.0, HashMap::new());
        self.set_gauge("aether_scan_cycle_ms", "Scanner cycle duration", 0.0, HashMap::new());
        self.set_gauge("aether_scan_shed_count", "Scanner shed count", 0.0, HashMap::new());
        self.set_gauge("aether_router_decisions", "Router decisions by reason", 0.0, HashMap::from([("reason".into(), "total".into())]));
        self.set_gauge("aether_order_latency_ms", "Order submission latency", 0.0, HashMap::new());
        self.set_gauge("aether_guardian_proposals", "Guardian proposal count", 0.0, HashMap::from([("state".into(), "pending".into())]));
        self.set_gauge("aether_audit_chain_verified", "Audit chain verified flag", 1.0, HashMap::new());
        self.set_gauge("aether_audit_last_verify_ts", "Last audit verify timestamp", 0.0, HashMap::new());
        self.set_gauge("aether_job_last_success_ts", "Last successful job run", 0.0, HashMap::from([("job".into(), "audit-verify".into())]));
        self.set_gauge("aether_lifecycle_open", "Open opportunity lifecycle count", 0.0, HashMap::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_includes_help_and_type() {
        let registry = MetricRegistry::new();
        registry.set_gauge("aether_test", "test metric", 42.0, HashMap::new());
        let exported = registry.export_prometheus();
        assert!(exported.contains("# HELP aether_test"));
        assert!(exported.contains("# TYPE aether_test gauge"));
        assert!(exported.contains("aether_test 42"));
    }

    #[test]
    fn export_with_labels() {
        let registry = MetricRegistry::new();
        let mut labels = HashMap::new();
        labels.insert("venue".into(), "kalshi".into());
        registry.set_gauge("aether_feed_lag_ms", "Feed lag", 150.0, labels);
        let exported = registry.export_prometheus();
        assert!(exported.contains("venue=\"kalshi\""));
        assert!(exported.contains("aether_feed_lag_ms{venue=\"kalshi\"} 150"));
    }
}

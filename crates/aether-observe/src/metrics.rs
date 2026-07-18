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

#[derive(Debug, Clone)]
pub struct Counter {
    pub name: String,
    pub help: String,
    pub value: u64,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Histogram {
    pub name: String,
    pub help: String,
    pub sum: f64,
    pub count: u64,
    pub buckets: Vec<(f64, u64)>,
    pub labels: HashMap<String, String>,
}

/// Metric registry holding all AETHER metrics.
pub struct MetricRegistry {
    gauges: Mutex<HashMap<String, Gauge>>,
    counters: Mutex<HashMap<String, Counter>>,
    histograms: Mutex<HashMap<String, Histogram>>,
}

impl MetricRegistry {
    pub fn new() -> Self {
        Self {
            gauges: Mutex::new(HashMap::new()),
            counters: Mutex::new(HashMap::new()),
            histograms: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_counter(&self, name: &str, help: &str, value: u64, labels: HashMap<String, String>) {
        let mut counters = self.counters.lock().unwrap_or_else(|error| error.into_inner());
        counters.insert(
            name.to_owned(),
            Counter { name: name.to_owned(), help: help.to_owned(), value, labels },
        );
    }

    pub fn observe_histogram(
        &self,
        name: &str,
        help: &str,
        value: f64,
        bucket_bounds: &[f64],
        labels: HashMap<String, String>,
    ) {
        let mut histograms = self.histograms.lock().unwrap_or_else(|error| error.into_inner());
        let histogram = histograms
            .entry(name.to_owned())
            .or_insert_with(|| empty_histogram(name, help, bucket_bounds, labels));
        histogram.sum += value;
        histogram.count += 1;
        for (bound, count) in &mut histogram.buckets {
            if value <= *bound {
                *count += 1;
            }
        }
    }

    /// Register a histogram without recording a synthetic observation.
    pub fn register_histogram(
        &self,
        name: &str,
        help: &str,
        bucket_bounds: &[f64],
        labels: HashMap<String, String>,
    ) {
        let mut histograms = self.histograms.lock().unwrap_or_else(|error| error.into_inner());
        histograms
            .entry(name.to_owned())
            .or_insert_with(|| empty_histogram(name, help, bucket_bounds, labels));
    }

    /// Register or update a gauge.
    pub fn set_gauge(&self, name: &str, help: &str, value: f64, labels: HashMap<String, String>) {
        let mut gauges = self.gauges.lock().unwrap_or_else(|error| error.into_inner());
        gauges.insert(
            name.to_string(),
            Gauge { name: name.into(), help: help.into(), value, labels },
        );
    }

    /// Get all registered gauges in Prometheus text format.
    pub fn export_prometheus(&self) -> String {
        let gauges = self.gauges.lock().unwrap_or_else(|error| error.into_inner());
        let counters = self.counters.lock().unwrap_or_else(|error| error.into_inner());
        let histograms = self.histograms.lock().unwrap_or_else(|error| error.into_inner());
        let mut output = String::new();
        let mut gauge_values: Vec<_> = gauges.values().collect();
        gauge_values.sort_by_key(|gauge| &gauge.name);
        for gauge in gauge_values {
            output.push_str(&format!("# HELP {} {}\n", gauge.name, gauge.help));
            output.push_str(&format!("# TYPE {} gauge\n", gauge.name));
            let labels = format_labels(&gauge.labels, None);
            output.push_str(&format!("{}{} {}\n", gauge.name, labels, gauge.value));
        }
        let mut counter_values: Vec<_> = counters.values().collect();
        counter_values.sort_by_key(|counter| &counter.name);
        for counter in counter_values {
            output.push_str(&format!("# HELP {} {}\n", counter.name, counter.help));
            output.push_str(&format!("# TYPE {} counter\n", counter.name));
            output.push_str(&format!(
                "{}{} {}\n",
                counter.name,
                format_labels(&counter.labels, None),
                counter.value
            ));
        }
        let mut histogram_values: Vec<_> = histograms.values().collect();
        histogram_values.sort_by_key(|histogram| &histogram.name);
        for histogram in histogram_values {
            output.push_str(&format!("# HELP {} {}\n", histogram.name, histogram.help));
            output.push_str(&format!("# TYPE {} histogram\n", histogram.name));
            for (bound, count) in &histogram.buckets {
                output.push_str(&format!(
                    "{}_bucket{} {}\n",
                    histogram.name,
                    format_labels(&histogram.labels, Some(("le", &bound.to_string()))),
                    count
                ));
            }
            output.push_str(&format!(
                "{}_bucket{} {}\n",
                histogram.name,
                format_labels(&histogram.labels, Some(("le", "+Inf"))),
                histogram.count
            ));
            let labels = format_labels(&histogram.labels, None);
            output.push_str(&format!("{}_sum{} {}\n", histogram.name, labels, histogram.sum));
            output.push_str(&format!("{}_count{} {}\n", histogram.name, labels, histogram.count));
        }
        output
    }

    /// Create the standard OBSERVABILITY.md metrics.
    pub fn register_standard_metrics(&self) {
        self.set_gauge(
            "aether_feed_lag_ms",
            "Age of most recent normalized tick",
            0.0,
            HashMap::new(),
        );
        self.register_histogram(
            "aether_scan_cycle_ms",
            "Scanner cycle duration",
            &[100.0, 250.0, 500.0, 1_000.0],
            HashMap::new(),
        );
        self.set_counter("aether_scan_shed_total", "Scanner shed count", 0, HashMap::new());
        self.set_counter(
            "aether_scan_fill_failures_total",
            "Scanner candidates rejected by missing or unfillable books",
            0,
            HashMap::new(),
        );
        self.set_gauge(
            "aether_router_decisions",
            "Router decisions by reason",
            0.0,
            HashMap::from([("reason".into(), "total".into())]),
        );
        self.set_gauge("aether_order_latency_ms", "Order submission latency", 0.0, HashMap::new());
        self.set_gauge(
            "aether_guardian_proposals",
            "Guardian proposal count",
            0.0,
            HashMap::from([("state".into(), "pending".into())]),
        );
        self.set_gauge(
            "aether_audit_chain_verified",
            "Audit chain verified flag",
            1.0,
            HashMap::new(),
        );
        self.set_gauge(
            "aether_audit_last_verify_ts",
            "Last audit verify timestamp",
            0.0,
            HashMap::new(),
        );
        self.set_gauge(
            "aether_job_last_success_ts",
            "Last successful job run",
            0.0,
            HashMap::from([("job".into(), "audit-verify".into())]),
        );
        self.set_gauge(
            "aether_opportunity_lifecycle_open",
            "Open opportunity lifecycle count",
            0.0,
            HashMap::new(),
        );
    }
}

fn empty_histogram(
    name: &str,
    help: &str,
    bucket_bounds: &[f64],
    labels: HashMap<String, String>,
) -> Histogram {
    Histogram {
        name: name.to_owned(),
        help: help.to_owned(),
        sum: 0.0,
        count: 0,
        buckets: bucket_bounds.iter().copied().map(|bound| (bound, 0)).collect(),
        labels,
    }
}

fn format_labels(labels: &HashMap<String, String>, extra: Option<(&str, &str)>) -> String {
    let mut values: Vec<(String, String)> =
        labels.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
    if let Some((key, value)) = extra {
        values.push((key.to_owned(), value.to_owned()));
    }
    values.sort_by(|left, right| left.0.cmp(&right.0));
    if values.is_empty() {
        String::new()
    } else {
        format!(
            "{{{}}}",
            values
                .into_iter()
                .map(|(key, value)| format!("{key}=\"{value}\""))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl Default for MetricRegistry {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn histogram_exports_cumulative_buckets() {
        let registry = MetricRegistry::new();
        for value in [100.0, 400.0, 600.0] {
            registry.observe_histogram(
                "aether_scan_cycle_ms",
                "Cycle",
                value,
                &[250.0, 500.0],
                HashMap::new(),
            );
        }
        let exported = registry.export_prometheus();
        assert!(exported.contains("aether_scan_cycle_ms_bucket{le=\"250\"} 1"));
        assert!(exported.contains("aether_scan_cycle_ms_bucket{le=\"500\"} 2"));
        assert!(exported.contains("aether_scan_cycle_ms_bucket{le=\"+Inf\"} 3"));
        assert!(exported.contains("aether_scan_cycle_ms_count 3"));
    }

    #[test]
    fn standard_histogram_starts_without_a_fake_sample() {
        let registry = MetricRegistry::new();
        registry.register_standard_metrics();
        let exported = registry.export_prometheus();
        assert!(exported.contains("aether_scan_cycle_ms_count 0"));
        assert!(exported.contains("aether_scan_cycle_ms_sum 0"));
    }
}

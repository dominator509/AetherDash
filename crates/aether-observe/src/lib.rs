//! AETHER Observability — shared instrumentation.
//! Provides structured logging with redaction, metrics, and health helpers.

pub mod health;
pub mod metrics;
pub mod redact;
pub mod trace;

pub use health::{HealthChecker, HealthStatus, ReadyChecker};
pub use metrics::MetricRegistry;
pub use redact::RedactionLayer;
pub use trace::{new_trace_id, TraceContext};

//! AETHER Observability — shared instrumentation.
//! Provides structured logging with redaction, metrics, and health helpers.

pub mod redact;
pub mod metrics;
pub mod health;
pub mod trace;

pub use redact::RedactionLayer;
pub use metrics::MetricRegistry;
pub use health::{HealthStatus, HealthChecker, ReadyChecker};
pub use trace::{new_trace_id, TraceContext};

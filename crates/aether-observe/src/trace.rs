//! trace_id end-to-end propagation.
//! Gateway/scheduler -> gRPC metadata -> bus envelopes -> DB rows.

use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

static TRACE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a new trace ID. Format: ULID-like for sortability.
pub fn new_trace_id() -> String {
    let counter = TRACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let uuid = Uuid::new_v4();
    format!("trace-{}-{}", counter, uuid.to_string().chars().take(8).collect::<String>())
}

/// Thread-local trace context for propagating trace_id through the call stack.
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
}

impl TraceContext {
    pub fn new() -> Self {
        Self {
            trace_id: new_trace_id(),
            span_id: format!(
                "span-{}",
                Uuid::new_v4().to_string().chars().take(8).collect::<String>()
            ),
        }
    }

    /// Create a child span with the same trace_id.
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: format!(
                "span-{}",
                Uuid::new_v4().to_string().chars().take(8).collect::<String>()
            ),
        }
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_span_shares_trace_id() {
        let parent = TraceContext::new();
        let child = parent.child();
        assert_eq!(parent.trace_id, child.trace_id);
        assert_ne!(parent.span_id, child.span_id);
    }

    #[test]
    fn trace_ids_are_unique() {
        let t1 = new_trace_id();
        let t2 = new_trace_id();
        assert_ne!(t1, t2);
    }
}

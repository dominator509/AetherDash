use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginAuditEvent {
    pub plugin: String,
    pub version: String,
    pub stage: &'static str,
    pub allowed: bool,
    pub reason: &'static str,
}

pub trait PluginAuditSink: Send + Sync {
    fn record(&self, event: PluginAuditEvent);
}

#[derive(Debug, Default)]
pub struct StderrPluginAudit;

impl PluginAuditSink for StderrPluginAudit {
    fn record(&self, event: PluginAuditEvent) {
        eprintln!(
            "plugin_audit plugin={} version={} stage={} allowed={} reason={}",
            event.plugin, event.version, event.stage, event.allowed, event.reason
        );
    }
}

#[derive(Debug, Default)]
pub struct MemoryPluginAudit {
    events: Mutex<Vec<PluginAuditEvent>>,
}

impl MemoryPluginAudit {
    #[must_use]
    pub fn events(&self) -> Vec<PluginAuditEvent> {
        self.events.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }
}

impl PluginAuditSink for MemoryPluginAudit {
    fn record(&self, event: PluginAuditEvent) {
        self.events.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(event);
    }
}

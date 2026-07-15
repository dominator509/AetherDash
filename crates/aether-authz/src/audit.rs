use crate::{Action, ActorKind, EnforcementPoint, Tier, UnixSeconds, Verdict};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub at: UnixSeconds,
    pub actor_id: String,
    pub actor_kind: ActorKind,
    pub action: Action,
    pub enforcement_point: EnforcementPoint,
    pub grant_id: Option<String>,
    pub effective_tier: Option<Tier>,
    pub verdict: Verdict,
    pub deciding_rule: &'static str,
}

pub trait AuditSink {
    type Error;
    fn emit(&self, record: &AuditRecord) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditedDecision {
    pub decision: crate::Decision,
    pub audit: AuditRecord,
}

#[derive(Debug, Default)]
pub struct MemoryAuditSink {
    records: Mutex<Vec<AuditRecord>>,
}

impl MemoryAuditSink {
    #[must_use]
    pub fn records(&self) -> Vec<AuditRecord> {
        self.records.lock().map_or_else(|_| Vec::new(), |records| records.clone())
    }
}

impl AuditSink for MemoryAuditSink {
    type Error = ();

    fn emit(&self, record: &AuditRecord) -> Result<(), Self::Error> {
        self.records.lock().map_err(|_| ())?.push(record.clone());
        Ok(())
    }
}

use crate::{ActorKind, UnixSeconds};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapsLimits {
    /// Monetary limits use canonical minor units at this policy boundary.
    pub per_order_minor: u64,
    pub daily_minor: u64,
}

impl CapsLimits {
    #[must_use]
    pub const fn lower_of(self, other: Self) -> Self {
        Self {
            per_order_minor: if self.per_order_minor <= other.per_order_minor {
                self.per_order_minor
            } else {
                other.per_order_minor
            },
            daily_minor: if self.daily_minor <= other.daily_minor {
                self.daily_minor
            } else {
                other.daily_minor
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsVersionState {
    Draft,
    Active,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsSnapshot {
    pub version: u64,
    pub limits: CapsLimits,
    pub state: CapsVersionState,
    pub drafted_by: String,
    pub drafted_by_kind: ActorKind,
    pub activated_by: Option<String>,
    pub created_at: UnixSeconds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapsDiff {
    pub from: Option<CapsLimits>,
    pub to: CapsLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapsError {
    #[error("caps version not found")]
    NotFound,
    #[error("caps version is not a draft")]
    NotDraft,
    #[error("caps activation requires a human actor")]
    HumanRequired,
    #[error("caps activation requires step-up")]
    StepUpRequired,
    #[error("caps version overflow")]
    VersionOverflow,
}

#[derive(Debug, Default)]
pub struct CapsStore {
    versions: HashMap<u64, CapsSnapshot>,
    active_version: Option<u64>,
    next_version: u64,
}

impl CapsStore {
    pub fn draft(
        &mut self,
        limits: CapsLimits,
        actor_id: &str,
        actor_kind: ActorKind,
        now: UnixSeconds,
    ) -> Result<u64, CapsError> {
        let version = self.next_version.checked_add(1).ok_or(CapsError::VersionOverflow)?;
        self.next_version = version;
        self.versions.insert(
            version,
            CapsSnapshot {
                version,
                limits,
                state: CapsVersionState::Draft,
                drafted_by: actor_id.to_owned(),
                drafted_by_kind: actor_kind,
                activated_by: None,
                created_at: now,
            },
        );
        Ok(version)
    }

    pub fn diff(&self, version: u64) -> Result<CapsDiff, CapsError> {
        let draft = self.versions.get(&version).ok_or(CapsError::NotFound)?;
        Ok(CapsDiff { from: self.active().map(|active| active.limits), to: draft.limits })
    }

    pub fn activate(
        &mut self,
        version: u64,
        actor_id: &str,
        actor_kind: ActorKind,
        step_up_satisfied: bool,
    ) -> Result<(), CapsError> {
        if actor_kind != ActorKind::Human {
            return Err(CapsError::HumanRequired);
        }
        if !step_up_satisfied {
            return Err(CapsError::StepUpRequired);
        }
        let draft = self.versions.get(&version).ok_or(CapsError::NotFound)?;
        if draft.state != CapsVersionState::Draft {
            return Err(CapsError::NotDraft);
        }
        if let Some(previous) = self.active_version {
            if let Some(snapshot) = self.versions.get_mut(&previous) {
                snapshot.state = CapsVersionState::Superseded;
            }
        }
        let draft = self.versions.get_mut(&version).ok_or(CapsError::NotFound)?;
        draft.state = CapsVersionState::Active;
        draft.activated_by = Some(actor_id.to_owned());
        self.active_version = Some(version);
        Ok(())
    }

    #[must_use]
    pub fn active(&self) -> Option<&CapsSnapshot> {
        self.active_version.and_then(|version| self.versions.get(&version))
    }

    /// Router rule: an intent's older snapshot can only tighten the current
    /// active limits. It can never loosen them retroactively.
    pub fn execution_limits(&self, intent_version: u64) -> Result<CapsLimits, CapsError> {
        let intent = self.versions.get(&intent_version).ok_or(CapsError::NotFound)?;
        let active = self.active().ok_or(CapsError::NotFound)?;
        Ok(intent.limits.lower_of(active.limits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_diff_step_up_activate_and_lower_of_two() {
        let mut store = CapsStore::default();
        let v1 = store
            .draft(
                CapsLimits { per_order_minor: 10_000, daily_minor: 50_000 },
                "operator",
                ActorKind::Human,
                1,
            )
            .expect("draft");
        assert_eq!(
            store.activate(v1, "operator", ActorKind::Human, false),
            Err(CapsError::StepUpRequired)
        );
        store.activate(v1, "operator", ActorKind::Human, true).expect("activate v1");

        let v2 = store
            .draft(
                CapsLimits { per_order_minor: 8_000, daily_minor: 60_000 },
                "agent",
                ActorKind::Agent,
                2,
            )
            .expect("draft");
        let diff = store.diff(v2).expect("diff");
        assert_eq!(diff.from.expect("active").daily_minor, 50_000);
        store.activate(v2, "operator", ActorKind::Human, true).expect("activate v2");
        assert_eq!(
            store.execution_limits(v1).expect("lower"),
            CapsLimits { per_order_minor: 8_000, daily_minor: 50_000 }
        );
    }

    #[test]
    fn automation_cannot_activate() {
        let mut store = CapsStore::default();
        let version = store
            .draft(
                CapsLimits { per_order_minor: 1, daily_minor: 1 },
                "automation",
                ActorKind::Automation,
                1,
            )
            .expect("draft");
        assert_eq!(
            store.activate(version, "automation", ActorKind::Automation, true),
            Err(CapsError::HumanRequired)
        );
    }
}

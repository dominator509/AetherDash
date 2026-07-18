//! Deterministic assertions for the SPEC-012 opportunity lifecycle.

use aether_core::ids::Ulid;
use aether_core::opportunity::{Opportunity, OpportunityKind};
use aether_core::time::UtcTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The canonical states stored in `opportunities.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Detected,
    Scored,
    Surfaced,
    Accepted,
    Ignored,
    Expired,
    Executed,
    Closed,
}

impl LifecycleState {
    pub fn is_terminal(self) -> bool {
        self == Self::Closed
    }

    pub fn can_transition_to(self, to: Self) -> bool {
        self.allowed_next_states().contains(&to)
    }

    pub const fn allowed_next_states(self) -> &'static [Self] {
        use LifecycleState::*;
        match self {
            Detected => &[Scored, Expired],
            Scored => &[Surfaced, Expired],
            Surfaced => &[Accepted, Ignored, Expired],
            Accepted => &[Executed],
            Ignored | Expired | Executed => &[Closed],
            Closed => &[],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub opportunity_id: Ulid,
    pub from: LifecycleState,
    pub to: LifecycleState,
    pub timestamp: UtcTime,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionTrace {
    pub opportunity_id: Ulid,
    pub opportunity_kind: OpportunityKind,
    pub transitions: Vec<Transition>,
    pub created_ts: UtcTime,
}

impl TransitionTrace {
    pub fn new(opportunity: &Opportunity) -> Self {
        Self {
            opportunity_id: opportunity.id,
            opportunity_kind: opportunity.kind,
            transitions: Vec::new(),
            created_ts: opportunity.detected_ts,
        }
    }

    pub fn record(&mut self, to: LifecycleState, reason: impl Into<String>) -> LifecycleAssertion {
        self.record_at(to, reason, UtcTime::now())
    }

    pub fn record_at(
        &mut self,
        to: LifecycleState,
        reason: impl Into<String>,
        timestamp: UtcTime,
    ) -> LifecycleAssertion {
        let from = self.current_state();
        let transition = Transition {
            opportunity_id: self.opportunity_id,
            from,
            to,
            timestamp,
            reason: reason.into(),
        };
        let valid =
            from.can_transition_to(to) && timestamp.unix_millis() >= self.created_ts.unix_millis();
        let errors = if valid {
            Vec::new()
        } else if timestamp.unix_millis() < self.created_ts.unix_millis() {
            vec!["transition timestamp precedes detection".to_owned()]
        } else {
            vec![format!("illegal transition: {from:?} -> {to:?}")]
        };
        if valid {
            self.transitions.push(transition.clone());
        }
        LifecycleAssertion { valid, transition, errors }
    }

    pub fn is_terminated(&self) -> bool {
        self.current_state().is_terminal()
    }

    pub fn current_state(&self) -> LifecycleState {
        self.transitions.last().map_or(LifecycleState::Detected, |transition| transition.to)
    }

    pub fn transition_count(&self) -> usize {
        self.transitions.len()
    }

    pub fn check_ttl(&self, max_ttl_ms: i64) -> Vec<LifecycleAssertion> {
        self.transitions
            .iter()
            .filter_map(|transition| {
                let elapsed = transition.timestamp.unix_millis() - self.created_ts.unix_millis();
                (elapsed > max_ttl_ms).then(|| LifecycleAssertion {
                    valid: false,
                    transition: transition.clone(),
                    errors: vec![format!(
                        "transition {:?} -> {:?} exceeded TTL: {elapsed}ms > {max_ttl_ms}ms",
                        transition.from, transition.to
                    )],
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct LifecycleAssertion {
    pub valid: bool,
    pub transition: Transition,
    pub errors: Vec<String>,
}

pub struct LifecycleChecker {
    traces: HashMap<Ulid, TransitionTrace>,
    default_max_ttl_ms: i64,
}

impl LifecycleChecker {
    pub fn new(default_max_ttl_ms: i64) -> Self {
        Self { traces: HashMap::new(), default_max_ttl_ms }
    }

    pub fn register(&mut self, opportunity: &Opportunity) {
        self.traces.insert(opportunity.id, TransitionTrace::new(opportunity));
    }

    pub fn transition(
        &mut self,
        opportunity_id: Ulid,
        to: LifecycleState,
        reason: impl Into<String>,
    ) -> Option<LifecycleAssertion> {
        self.traces.get_mut(&opportunity_id).map(|trace| trace.record(to, reason))
    }

    pub fn transition_at(
        &mut self,
        opportunity_id: Ulid,
        to: LifecycleState,
        reason: impl Into<String>,
        timestamp: UtcTime,
    ) -> Option<LifecycleAssertion> {
        self.traces.get_mut(&opportunity_id).map(|trace| trace.record_at(to, reason, timestamp))
    }

    pub fn get_trace(&self, opportunity_id: &Ulid) -> Option<&TransitionTrace> {
        self.traces.get(opportunity_id)
    }

    pub fn check_all_ttl(&self) -> Vec<(Ulid, Vec<LifecycleAssertion>)> {
        self.traces
            .iter()
            .filter_map(|(id, trace)| {
                let violations = trace.check_ttl(self.default_max_ttl_ms);
                (!violations.is_empty()).then_some((*id, violations))
            })
            .collect()
    }

    pub fn check_ttl(&self, opportunity_id: &Ulid) -> Option<Vec<LifecycleAssertion>> {
        self.traces.get(opportunity_id).map(|trace| trace.check_ttl(self.default_max_ttl_ms))
    }

    pub fn all_traces(&self) -> &HashMap<Ulid, TransitionTrace> {
        &self.traces
    }

    pub fn len(&self) -> usize {
        self.traces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.traces.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_matrix_is_complete_and_closed() {
        let states = [
            LifecycleState::Detected,
            LifecycleState::Scored,
            LifecycleState::Surfaced,
            LifecycleState::Accepted,
            LifecycleState::Ignored,
            LifecycleState::Expired,
            LifecycleState::Executed,
            LifecycleState::Closed,
        ];
        for from in states {
            for to in states {
                let expected = match from {
                    LifecycleState::Detected => {
                        matches!(to, LifecycleState::Scored | LifecycleState::Expired)
                    }
                    LifecycleState::Scored => {
                        matches!(to, LifecycleState::Surfaced | LifecycleState::Expired)
                    }
                    LifecycleState::Surfaced => matches!(
                        to,
                        LifecycleState::Accepted
                            | LifecycleState::Ignored
                            | LifecycleState::Expired
                    ),
                    LifecycleState::Accepted => to == LifecycleState::Executed,
                    LifecycleState::Ignored
                    | LifecycleState::Expired
                    | LifecycleState::Executed => to == LifecycleState::Closed,
                    LifecycleState::Closed => false,
                };
                assert_eq!(from.can_transition_to(to), expected, "{from:?} -> {to:?}");
            }
        }
    }
}

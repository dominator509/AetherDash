//! Lifecycle assertion framework for opportunity state machines per SPEC-012.
//!
//! Validates legal/illegal transitions, TTL expiry, and chain closure
//! (terminal state enforcement).  The state machine is:
//!
//! ```text
//! Discovered ──► EdgeValidated ──► RiskApproved ──► Executing ──► Filled ──► Closed
//!      │               │                │               │
//!      └──► Expired ◄──┘                │               ├──► PartialFill ──► Filled
//!                                        │               └──► Expired
//!                                        └──► Closed (rejected by risk)
//! Expired ──► Closed
//! ```
//!
//! Terminal states: `Closed`, `Expired`.

use aether_core::ids::Ulid;
use aether_core::opportunity::{Opportunity, OpportunityKind};
use aether_core::time::UtcTime;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Lifecycle state machine
// ---------------------------------------------------------------------------

/// SPEC-012 opportunity lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    /// Opportunity initially detected by a brain.
    Discovered,
    /// Edge decomposition computed and sum-law validated.
    EdgeValidated,
    /// Risk engine approved the opportunity for execution.
    RiskApproved,
    /// Order placement in progress.
    Executing,
    /// Order fully filled.
    Filled,
    /// Order partially filled (non-terminal — may yet fill).
    PartialFill,
    /// Opportunity expired (TTL reached or stale). Terminal.
    Expired,
    /// Lifecycle complete. Terminal.
    Closed,
}

impl LifecycleState {
    /// Returns `true` for terminal states (no further transitions allowed).
    pub fn is_terminal(&self) -> bool {
        matches!(self, LifecycleState::Closed | LifecycleState::Expired)
    }

    /// Returns `true` if this state can legally transition to `to`.
    pub fn can_transition_to(&self, to: LifecycleState) -> bool {
        legal_next_states(*self).contains(&to)
    }

    /// List all states reachable from this one in a single step.
    pub fn allowed_next_states(&self) -> &'static [LifecycleState] {
        legal_next_states(*self)
    }
}

/// Legal transition matrix encoded as pure function.
const fn legal_next_states(from: LifecycleState) -> &'static [LifecycleState] {
    use LifecycleState::*;
    match from {
        Discovered => &[EdgeValidated, Expired],
        EdgeValidated => &[RiskApproved, Expired],
        RiskApproved => &[Executing, Closed],
        Executing => &[Filled, PartialFill, Expired],
        PartialFill => &[Filled, Expired],
        Filled => &[Closed],
        Expired => &[Closed],
        Closed => &[],
    }
}

// ---------------------------------------------------------------------------
// Transition trace
// ---------------------------------------------------------------------------

/// A single state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    /// The opportunity being tracked.
    pub opportunity_id: Ulid,
    /// Source state.
    pub from: LifecycleState,
    /// Target state.
    pub to: LifecycleState,
    /// Wall-clock timestamp of the transition.
    pub timestamp: UtcTime,
    /// Human-readable reason (e.g. "edge computed", "risk denied").
    pub reason: String,
}

/// Complete transition history for a single opportunity.
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
            created_ts: UtcTime::now(),
        }
    }

    /// Record a transition.  Performs legality and terminal-state checks.
    pub fn record(&mut self, to: LifecycleState, reason: impl Into<String>) -> LifecycleAssertion {
        let from = self
            .transitions
            .last()
            .map(|t| t.to)
            .unwrap_or(LifecycleState::Discovered);

        let mut errors = Vec::new();

        if !from.can_transition_to(to) {
            errors.push(format!(
                "illegal transition: {from:?} -> {to:?}"
            ));
        }

        let transition = Transition {
            opportunity_id: self.opportunity_id,
            from,
            to,
            timestamp: UtcTime::now(),
            reason: reason.into(),
        };

        let assertion = LifecycleAssertion {
            valid: errors.is_empty(),
            transition: transition.clone(),
            errors,
        };

        if assertion.valid {
            self.transitions.push(transition);
        }

        assertion
    }

    /// Check whether the trace has reached a terminal state.
    pub fn is_terminated(&self) -> bool {
        self.transitions
            .last()
            .map(|t| t.to.is_terminal())
            .unwrap_or(false)
    }

    /// Check whether any transition exceeded the allowed TTL (in milliseconds)
    /// between creation timestamp and the last recorded transition.
    pub fn check_ttl(&self, max_ttl_ms: i64) -> Vec<LifecycleAssertion> {
        let mut results = Vec::new();
        for (i, t) in self.transitions.iter().enumerate() {
            let elapsed = t.timestamp.unix_millis() - self.created_ts.unix_millis();
            if elapsed > max_ttl_ms {
                results.push(LifecycleAssertion {
                    valid: false,
                    transition: t.clone(),
                    errors: vec![format!(
                        "transition[{}] ({:?} -> {:?}) exceeded TTL: \
                         elapsed={}ms > max_ttl={max_ttl_ms}ms",
                        i, t.from, t.to, elapsed
                    )],
                });
            }
        }
        results
    }

    /// Return the current state, or `None` if no transitions occurred yet.
    pub fn current_state(&self) -> Option<LifecycleState> {
        self.transitions.last().map(|t| t.to)
    }

    /// Return the number of recorded transitions.
    pub fn transition_count(&self) -> usize {
        self.transitions.len()
    }
}

// ---------------------------------------------------------------------------
// LifecycleAssertion
// ---------------------------------------------------------------------------

/// The result of validating a single transition.
#[derive(Debug, Clone)]
pub struct LifecycleAssertion {
    /// Whether the transition is legal.
    pub valid: bool,
    /// The transition that was checked.
    pub transition: Transition,
    /// Human-readable error messages (empty when `valid` is `true`).
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// LifecycleChecker
// ---------------------------------------------------------------------------

/// Validates opportunity lifecycle state machine transitions.
///
/// Maintains a set of `TransitionTrace`s keyed by opportunity ULID.
pub struct LifecycleChecker {
    traces: std::collections::HashMap<Ulid, TransitionTrace>,
    default_max_ttl_ms: i64,
}

impl LifecycleChecker {
    /// Create a new checker.  `default_max_ttl_ms` sets the TTL threshold
    /// used by `check_all_ttl()`.
    pub fn new(default_max_ttl_ms: i64) -> Self {
        Self {
            traces: std::collections::HashMap::new(),
            default_max_ttl_ms,
        }
    }

    /// Register an opportunity to begin tracking it.
    pub fn register(&mut self, opportunity: &Opportunity) {
        let id = opportunity.id;
        let trace = TransitionTrace::new(opportunity);
        self.traces.insert(id, trace);
    }

    /// Attempt to transition an opportunity to a new state.
    ///
    /// Returns `None` if the opportunity has not been registered.
    /// Returns the assertion whether the transition was accepted.
    pub fn transition(
        &mut self,
        opportunity_id: Ulid,
        to: LifecycleState,
        reason: impl Into<String>,
    ) -> Option<LifecycleAssertion> {
        let trace = self.traces.get_mut(&opportunity_id)?;
        Some(trace.record(to, reason))
    }

    /// Get the trace for an opportunity, if registered.
    pub fn get_trace(&self, opportunity_id: &Ulid) -> Option<&TransitionTrace> {
        self.traces.get(opportunity_id)
    }

    /// Check TTL for all registered traces against `default_max_ttl_ms`.
    pub fn check_all_ttl(&self) -> Vec<(Ulid, Vec<LifecycleAssertion>)> {
        let mut results = Vec::new();
        for (id, trace) in &self.traces {
            let violations = trace.check_ttl(self.default_max_ttl_ms);
            if !violations.is_empty() {
                results.push((*id, violations));
            }
        }
        results
    }

    /// Check only a single trace's TTL.
    pub fn check_ttl(&self, opportunity_id: &Ulid) -> Option<Vec<LifecycleAssertion>> {
        self.traces.get(opportunity_id).map(|t| t.check_ttl(self.default_max_ttl_ms))
    }

    /// Return all traces.
    pub fn all_traces(&self) -> &std::collections::HashMap<Ulid, TransitionTrace> {
        &self.traces
    }

    /// Return the number of registered opportunities.
    pub fn len(&self) -> usize {
        self.traces.len()
    }

    /// Returns `true` if no opportunities are registered.
    pub fn is_empty(&self) -> bool {
        self.traces.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::decimal::Confidence;
    use aether_core::ids::MarketKey;
    use aether_core::opportunity::{
        BrainRef, EdgeCosts, EdgeDecomposition, Opportunity, OpportunityLeg,
    };
    use rust_decimal::Decimal;

    fn sample_opportunity() -> Opportunity {
        use aether_core::ids::VenueId;
        let venue = VenueId::new("kalshi").expect("valid venue");
        let mkt = MarketKey::new(&venue, "TEST-100").expect("valid market");
        Opportunity {
            id: Ulid::new(),
            kind: OpportunityKind::Arbitrage,
            legs: vec![OpportunityLeg {
                market: mkt,
                side: aether_core::order::Side::Buy,
                target_price: None,
                size_hint: None,
            }],
            gross_edge: Decimal::new(100, 2),
            edge: EdgeDecomposition::compute(
                Decimal::new(100, 2),
                EdgeCosts {
                    fees: Decimal::new(10, 2),
                    slippage_est: Decimal::ZERO,
                    funding_cost: Decimal::ZERO,
                    gas_cost: Decimal::ZERO,
                    bridge_cost: Decimal::ZERO,
                    settlement_mismatch_discount: Decimal::ZERO,
                    liquidity_haircut: Decimal::ZERO,
                    staleness_penalty: Decimal::ZERO,
                    confidence_penalty: Decimal::ZERO,
                },
            ),
            confidence: Confidence::new(Decimal::new(8, 1)).unwrap(),
            detected_ts: UtcTime::from_unix_millis(1752152096789).unwrap(),
            expires_ts: None,
            explain_ref: BrainRef {
                object_id: Ulid::new(),
                provenance_hash: "abc123".into(),
            },
            trace_id: Ulid::new(),
        }
    }

    #[test]
    fn legal_transition_succeeds() {
        let opp = sample_opportunity();
        let mut checker = LifecycleChecker::new(60_000);
        checker.register(&opp);

        let assertion = checker
            .transition(opp.id, LifecycleState::EdgeValidated, "edge computed")
            .unwrap();

        assert!(assertion.valid);
        assert!(assertion.errors.is_empty());
        assert!(!assertion.transition.from.is_terminal());
    }

    #[test]
    fn full_lifecycle_succeeds() {
        let opp = sample_opportunity();
        let mut checker = LifecycleChecker::new(60_000);
        checker.register(&opp);

        let steps = [
            (LifecycleState::EdgeValidated, "edge computed"),
            (LifecycleState::RiskApproved, "risk OK"),
            (LifecycleState::Executing, "placing order"),
            (LifecycleState::Filled, "fully filled"),
            (LifecycleState::Closed, "settled"),
        ];

        for &(state, reason) in &steps {
            let assertion = checker.transition(opp.id, state, reason).unwrap();
            assert!(assertion.valid, "transition to {state:?} failed: {:?}", assertion.errors);
        }

        let trace = checker.get_trace(&opp.id).unwrap();
        assert!(trace.is_terminated());
        assert_eq!(trace.current_state(), Some(LifecycleState::Closed));
        assert_eq!(trace.transition_count(), steps.len());
    }

    #[test]
    fn illegal_transition_is_rejected() {
        let opp = sample_opportunity();
        let mut checker = LifecycleChecker::new(60_000);
        checker.register(&opp);

        // Discovered -> Filled is illegal (skips EdgeValidated, RiskApproved, Executing).
        let assertion = checker.transition(opp.id, LifecycleState::Filled, "shortcut").unwrap();

        assert!(!assertion.valid);
        assert!(!assertion.errors.is_empty());
        // The transition must not have been recorded.
        let trace = checker.get_trace(&opp.id).unwrap();
        assert_eq!(trace.transition_count(), 0);
    }

    #[test]
    fn terminal_state_prevents_further_transitions() {
        let opp = sample_opportunity();
        let mut checker = LifecycleChecker::new(60_000);
        checker.register(&opp);

        checker.transition(opp.id, LifecycleState::Expired, "TTL expired").unwrap();

        // Try to transition from Expired -> Filled (illegal, should be Expired -> Closed).
        let assertion = checker.transition(opp.id, LifecycleState::Filled, "late fill").unwrap();
        assert!(!assertion.valid);
        assert!(!assertion.errors.is_empty());

        // Now Expired -> Closed is legal.
        let assertion = checker.transition(opp.id, LifecycleState::Closed, "cleanup").unwrap();
        assert!(assertion.valid);
    }

    #[test]
    fn partial_fill_then_full_fill() {
        let opp = sample_opportunity();
        let mut checker = LifecycleChecker::new(60_000);
        checker.register(&opp);

        let steps = [
            (LifecycleState::EdgeValidated, "edge OK"),
            (LifecycleState::RiskApproved, "risk OK"),
            (LifecycleState::Executing, "placed"),
            (LifecycleState::PartialFill, "partial"),
            (LifecycleState::Filled, "rest filled"),
            (LifecycleState::Closed, "done"),
        ];

        for &(state, reason) in &steps {
            let assertion = checker.transition(opp.id, state, reason).unwrap();
            assert!(assertion.valid, "transition to {state:?} failed: {:?}", assertion.errors);
        }

        let trace = checker.get_trace(&opp.id).unwrap();
        assert!(trace.is_terminated());
        assert_eq!(trace.current_state(), Some(LifecycleState::Closed));
    }

    #[test]
    fn unregistered_opportunity_returns_none() {
        let mut checker = LifecycleChecker::new(60_000);
        let id = Ulid::new();
        assert!(checker.get_trace(&id).is_none());
        assert!(checker.transition(id, LifecycleState::Closed, "no-op").is_none());
    }

    #[test]
    fn transition_trace_records_order() {
        let opp = sample_opportunity();
        let mut trace = TransitionTrace::new(&opp);

        let a1 = trace.record(LifecycleState::EdgeValidated, "first");
        assert!(a1.valid);
        assert_eq!(trace.transition_count(), 1);

        let a2 = trace.record(LifecycleState::RiskApproved, "second");
        assert!(a2.valid);
        assert_eq!(trace.transition_count(), 2);

        assert_eq!(trace.current_state(), Some(LifecycleState::RiskApproved));
    }

    #[test]
    fn ttl_violation_detected() {
        let opp = sample_opportunity();
        let mut checker = LifecycleChecker::new(1); // 1 ms TTL
        checker.register(&opp);

        // Force elapsed time to exceed 1ms TTL
        std::thread::sleep(std::time::Duration::from_millis(5));
        checker.transition(opp.id, LifecycleState::EdgeValidated, "slow").unwrap();

        let violations = checker.check_all_ttl();
        assert!(!violations.is_empty(), "expected TTL violations with 1ms threshold");

        let (vid, assertions) = &violations[0];
        assert_eq!(*vid, opp.id);
        assert!(!assertions.is_empty());
        assert!(!assertions[0].valid);
    }

    #[test]
    fn no_ttl_violation_with_large_threshold() {
        let opp = sample_opportunity();
        let mut checker = LifecycleChecker::new(600_000); // 10 minutes
        checker.register(&opp);

        checker.transition(opp.id, LifecycleState::EdgeValidated, "within TTL").unwrap();

        let violations = checker.check_all_ttl();
        assert!(violations.is_empty(), "should not have TTL violations with 10min threshold");
    }

    #[test]
    fn checker_length_and_empty() {
        let checker = LifecycleChecker::new(60_000);
        assert!(checker.is_empty());
        assert_eq!(checker.len(), 0);
    }

    #[test]
    fn state_is_terminal_correctness() {
        assert!(LifecycleState::Closed.is_terminal());
        assert!(LifecycleState::Expired.is_terminal());
        assert!(!LifecycleState::Discovered.is_terminal());
        assert!(!LifecycleState::EdgeValidated.is_terminal());
        assert!(!LifecycleState::RiskApproved.is_terminal());
        assert!(!LifecycleState::Executing.is_terminal());
        assert!(!LifecycleState::Filled.is_terminal());
        assert!(!LifecycleState::PartialFill.is_terminal());
    }

    #[test]
    fn allowed_next_states_non_empty() {
        let next = LifecycleState::Discovered.allowed_next_states();
        assert!(next.contains(&LifecycleState::EdgeValidated));
        assert!(next.contains(&LifecycleState::Expired));
    }

    #[test]
    fn terminal_state_has_no_allowed_next() {
        assert!(LifecycleState::Closed.allowed_next_states().is_empty());
        assert!(LifecycleState::Expired.allowed_next_states().contains(&LifecycleState::Closed));
    }
}

//! Integration tests for the lifecycle assertion framework.
//!
//! Exercises the state machine via `LifecycleChecker` with realistic
//! `Opportunity` objects.

use aether_core::decimal::Confidence;
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::opportunity::{
    BrainRef, EdgeCosts, EdgeDecomposition, Opportunity, OpportunityKind, OpportunityLeg,
};
use aether_core::order::Side;
use aether_core::time::UtcTime;
use aether_testing::lifecycle::{LifecycleChecker, LifecycleState};
use rust_decimal::Decimal;

fn sample_opportunity() -> Opportunity {
    let venue = VenueId::new("polymarket").expect("valid venue");
    let market = MarketKey::new(&venue, "BTC-75").expect("valid market");
    Opportunity {
        id: Ulid::new(),
        kind: OpportunityKind::Catalyst,
        legs: vec![OpportunityLeg {
            market,
            side: Side::Buy,
            target_price: None,
            size_hint: None,
        }],
        gross_edge: Decimal::new(200, 2),
        edge: EdgeDecomposition::compute(
            Decimal::new(200, 2),
            EdgeCosts {
                fees: Decimal::new(15, 2),
                slippage_est: Decimal::new(5, 2),
                funding_cost: Decimal::ZERO,
                gas_cost: Decimal::new(1, 3),
                bridge_cost: Decimal::ZERO,
                settlement_mismatch_discount: Decimal::ZERO,
                liquidity_haircut: Decimal::ZERO,
                staleness_penalty: Decimal::ZERO,
                confidence_penalty: Decimal::ZERO,
            },
        ),
        confidence: Confidence::new(Decimal::new(9, 1)).expect("valid confidence"),
        detected_ts: UtcTime::from_unix_millis(1752152096789).expect("valid ts"),
        expires_ts: None,
        explain_ref: BrainRef {
            object_id: Ulid::new(),
            provenance_hash: "integration-test".into(),
        },
        trace_id: Ulid::new(),
    }
}

#[test]
fn happy_path_lifecycle_succeeds() {
    let opp = sample_opportunity();
    let mut checker = LifecycleChecker::new(300_000); // 5 minute TTL

    checker.register(&opp);

    // Full legal lifecycle
    let steps = [
        (LifecycleState::EdgeValidated, "edge decomposition computed"),
        (LifecycleState::RiskApproved, "risk check passed"),
        (LifecycleState::Executing, "order sent to venue"),
        (LifecycleState::Filled, "order fully filled"),
        (LifecycleState::Closed, "settlement confirmed"),
    ];

    for &(state, reason) in &steps {
        let a = checker
            .transition(opp.id, state, reason)
            .unwrap_or_else(|| panic!("opportunity {} should be registered", opp.id));
        assert!(a.valid, "transition to {state:?} failed: {:?}", a.errors);
    }

    let trace = checker.get_trace(&opp.id).expect("trace exists");
    assert!(trace.is_terminated());
    assert_eq!(trace.current_state(), Some(LifecycleState::Closed));
    assert_eq!(trace.transition_count(), steps.len());
}

#[test]
fn partial_fill_then_complete() {
    let opp = sample_opportunity();
    let mut checker = LifecycleChecker::new(300_000);
    checker.register(&opp);

    let steps = [
        (LifecycleState::EdgeValidated, "computed"),
        (LifecycleState::RiskApproved, "risk OK"),
        (LifecycleState::Executing, "sent"),
        (LifecycleState::PartialFill, "partial fill (50%)"),
        (LifecycleState::Filled, "rest filled"),
        (LifecycleState::Closed, "closed"),
    ];

    for &(state, reason) in &steps {
        let a = checker.transition(opp.id, state, reason).unwrap();
        assert!(a.valid, "transition to {state:?} failed: {:?}", a.errors);
    }

    let trace = checker.get_trace(&opp.id).unwrap();
    assert!(trace.is_terminated());
}

#[test]
fn illegal_transition_discovered_to_closed() {
    let opp = sample_opportunity();
    let mut checker = LifecycleChecker::new(60_000);
    checker.register(&opp);

    // Discovered -> Closed is illegal
    let a = checker.transition(opp.id, LifecycleState::Closed, "skip all steps").unwrap();
    assert!(!a.valid, "skipping to Closed must be rejected");
    assert!(!a.errors.is_empty());

    // Trace should be empty since no valid transitions occurred
    let trace = checker.get_trace(&opp.id).unwrap();
    assert_eq!(trace.transition_count(), 0);
}

#[test]
fn illegal_transition_filled_back_to_executing() {
    let opp = sample_opportunity();
    let mut checker = LifecycleChecker::new(60_000);
    checker.register(&opp);

    checker.transition(opp.id, LifecycleState::EdgeValidated, "edge OK").unwrap();
    checker.transition(opp.id, LifecycleState::RiskApproved, "risk OK").unwrap();
    checker.transition(opp.id, LifecycleState::Executing, "sent").unwrap();
    checker.transition(opp.id, LifecycleState::Filled, "filled").unwrap();

    // Filled -> Executing is illegal
    let a = checker.transition(opp.id, LifecycleState::Executing, "re-open").unwrap();
    assert!(!a.valid, "Filled -> Executing must be rejected");
}

#[test]
fn expired_opportunity_can_only_close() {
    let opp = sample_opportunity();
    let mut checker = LifecycleChecker::new(60_000);
    checker.register(&opp);

    checker.transition(opp.id, LifecycleState::Expired, "TTL expired").unwrap();

    // Expired -> Filled is illegal
    let a = checker.transition(opp.id, LifecycleState::Filled, "late fill").unwrap();
    assert!(!a.valid);

    // Expired -> Closed is legal (cleanup)
    let a = checker.transition(opp.id, LifecycleState::Closed, "cleanup").unwrap();
    assert!(a.valid);
}

#[test]
fn risk_rejected_opportunity_goes_to_closed() {
    let opp = sample_opportunity();
    let mut checker = LifecycleChecker::new(60_000);
    checker.register(&opp);

    checker.transition(opp.id, LifecycleState::EdgeValidated, "edge OK").unwrap();
    checker.transition(opp.id, LifecycleState::RiskApproved, "risk check initiated").unwrap();

    // Risk rejects the opportunity
    let a = checker.transition(opp.id, LifecycleState::Closed, "risk rejected: cap exceeded").unwrap();
    assert!(a.valid, "RiskApproved -> Closed should be legal (risk rejection)");

    let trace = checker.get_trace(&opp.id).unwrap();
    assert!(trace.is_terminated());
}

#[test]
fn multiple_opportunities_tracked_independently() {
    let opp1 = sample_opportunity();
    let opp2 = sample_opportunity();

    let mut checker = LifecycleChecker::new(60_000);
    checker.register(&opp1);
    checker.register(&opp2);

    assert_eq!(checker.len(), 2);

    // Progress opp1 fully
    checker.transition(opp1.id, LifecycleState::EdgeValidated, "edge OK").unwrap();
    checker.transition(opp1.id, LifecycleState::RiskApproved, "risk OK").unwrap();
    checker.transition(opp1.id, LifecycleState::Executing, "sent").unwrap();
    checker.transition(opp1.id, LifecycleState::Filled, "filled").unwrap();
    checker.transition(opp1.id, LifecycleState::Closed, "done").unwrap();

    // opp2 stays at Discovered
    let trace2 = checker.get_trace(&opp2.id).unwrap();
    assert_eq!(trace2.transition_count(), 0);
    assert_eq!(trace2.current_state(), None);

    let trace1 = checker.get_trace(&opp1.id).unwrap();
    assert!(trace1.is_terminated());
    assert_eq!(trace1.transition_count(), 5);
}

#[test]
fn unregistered_opportunity_returns_none() {
    let mut checker = LifecycleChecker::new(60_000);
    let unknown_id = Ulid::new();
    assert!(checker.get_trace(&unknown_id).is_none());
    assert!(checker.transition(unknown_id, LifecycleState::Closed, "who?").is_none());
}

#[test]
fn ttl_violation_reported() {
    let opp = sample_opportunity();
    let mut checker = LifecycleChecker::new(1); // 1 ms — must trigger
    checker.register(&opp);

    // Force elapsed time to exceed 1ms TTL
    std::thread::sleep(std::time::Duration::from_millis(5));
    checker.transition(opp.id, LifecycleState::EdgeValidated, "slow edge").unwrap();

    let violations = checker.check_all_ttl();
    assert!(!violations.is_empty(), "expected TTL violations with 1ms threshold");
}

#[test]
fn lifecycle_checker_is_empty_initially() {
    let checker = LifecycleChecker::new(60_000);
    assert!(checker.is_empty());
    assert_eq!(checker.len(), 0);
}

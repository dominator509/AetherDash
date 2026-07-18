#![allow(clippy::expect_used, clippy::unwrap_used)]

use aether_core::decimal::Confidence;
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::opportunity::{
    BrainRef, EdgeCosts, EdgeDecomposition, Opportunity, OpportunityKind, OpportunityLeg,
};
use aether_core::order::Side;
use aether_core::time::UtcTime;
use aether_testing::lifecycle::{LifecycleChecker, LifecycleState};
use rust_decimal::Decimal;

const DETECTED_MS: i64 = 1_752_152_096_789;

fn sample_opportunity() -> Opportunity {
    let venue = VenueId::new("polymarket").unwrap();
    let market = MarketKey::new(&venue, "BTC-75").unwrap();
    Opportunity {
        id: Ulid::new(),
        kind: OpportunityKind::Catalyst,
        legs: vec![OpportunityLeg { market, side: Side::Buy, target_price: None, size_hint: None }],
        gross_edge: Decimal::new(200, 2),
        edge: EdgeDecomposition::compute(Decimal::new(200, 2), EdgeCosts::zero()),
        confidence: Confidence::new(Decimal::new(9, 1)).unwrap(),
        detected_ts: UtcTime::from_unix_millis(DETECTED_MS).unwrap(),
        expires_ts: UtcTime::from_unix_millis(DETECTED_MS + 30_000).ok(),
        explain_ref: BrainRef { object_id: Ulid::new(), provenance_hash: "fixture".into() },
        trace_id: Ulid::new(),
    }
}

fn at(offset_ms: i64) -> UtcTime {
    UtcTime::from_unix_millis(DETECTED_MS + offset_ms).unwrap()
}

#[test]
fn accepted_execution_lifecycle_closes() {
    let opportunity = sample_opportunity();
    let mut checker = LifecycleChecker::new(30_000);
    checker.register(&opportunity);
    for (offset, state) in [
        (1, LifecycleState::Scored),
        (2, LifecycleState::Surfaced),
        (3, LifecycleState::Accepted),
        (4, LifecycleState::Executed),
        (5, LifecycleState::Closed),
    ] {
        assert!(checker.transition_at(opportunity.id, state, "fixture", at(offset)).unwrap().valid);
    }
    let trace = checker.get_trace(&opportunity.id).unwrap();
    assert!(trace.is_terminated());
    assert_eq!(trace.current_state(), LifecycleState::Closed);
}

#[test]
fn ignored_and_expired_paths_require_closed() {
    for terminal_preclose in [LifecycleState::Ignored, LifecycleState::Expired] {
        let opportunity = sample_opportunity();
        let mut checker = LifecycleChecker::new(30_000);
        checker.register(&opportunity);
        assert!(
            checker
                .transition_at(opportunity.id, LifecycleState::Scored, "scored", at(1))
                .unwrap()
                .valid
        );
        if terminal_preclose == LifecycleState::Ignored {
            assert!(
                checker
                    .transition_at(opportunity.id, LifecycleState::Surfaced, "surfaced", at(2))
                    .unwrap()
                    .valid
            );
        }
        assert!(
            checker
                .transition_at(opportunity.id, terminal_preclose, "terminal", at(3))
                .unwrap()
                .valid
        );
        assert!(!checker.get_trace(&opportunity.id).unwrap().is_terminated());
        assert!(
            checker
                .transition_at(opportunity.id, LifecycleState::Closed, "attributed", at(4))
                .unwrap()
                .valid
        );
    }
}

#[test]
fn illegal_shortcuts_are_not_recorded() {
    let opportunity = sample_opportunity();
    let mut checker = LifecycleChecker::new(30_000);
    checker.register(&opportunity);
    let result =
        checker.transition_at(opportunity.id, LifecycleState::Accepted, "shortcut", at(1)).unwrap();
    assert!(!result.valid);
    assert_eq!(checker.get_trace(&opportunity.id).unwrap().transition_count(), 0);
}

#[test]
fn ttl_uses_injected_time_without_sleeping() {
    let opportunity = sample_opportunity();
    let mut checker = LifecycleChecker::new(30_000);
    checker.register(&opportunity);
    assert!(
        checker
            .transition_at(opportunity.id, LifecycleState::Scored, "late", at(30_001))
            .unwrap()
            .valid
    );
    assert_eq!(checker.check_all_ttl().len(), 1);
}

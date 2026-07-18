#![allow(clippy::expect_used)]

//! Integration tests for the deterministic replay harness.
//!
//! Tests capture, persist, load, replay, and hash-verification using
//! realistic domain types from `aether-core`.

use aether_core::decimal::Confidence;
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::opportunity::{
    BrainRef, EdgeCosts, EdgeDecomposition, Opportunity, OpportunityKind, OpportunityLeg,
};
use aether_core::order::Side;
use aether_core::time::UtcTime;
use aether_testing::replay::{CapturedEvent, ReplayHarness};
use rust_decimal::Decimal;
use std::path::Path;

/// Create a realistic `Opportunity` for use as a test payload.
fn sample_opportunity() -> Opportunity {
    let venue = VenueId::new("kalshi").expect("valid venue");
    let market = MarketKey::new(&venue, "INTC-50").expect("valid market");
    Opportunity {
        id: Ulid::new(),
        kind: OpportunityKind::Arbitrage,
        legs: vec![OpportunityLeg { market, side: Side::Buy, target_price: None, size_hint: None }],
        gross_edge: Decimal::new(150, 2),
        edge: EdgeDecomposition::compute(
            Decimal::new(150, 2),
            EdgeCosts {
                fees: Decimal::new(20, 2),
                slippage_est: Decimal::ZERO,
                funding_cost: Decimal::ZERO,
                gas_cost: Decimal::new(5, 4),
                bridge_cost: Decimal::ZERO,
                settlement_mismatch_discount: Decimal::ZERO,
                liquidity_haircut: Decimal::new(10, 2),
                staleness_penalty: Decimal::ZERO,
                confidence_penalty: Decimal::new(5, 2),
            },
        ),
        confidence: Confidence::new(Decimal::new(85, 2)).expect("valid confidence"),
        detected_ts: UtcTime::from_unix_millis(1752152096789).expect("valid timestamp"),
        expires_ts: Some(UtcTime::from_unix_millis(1752155696789).expect("valid timestamp")),
        explain_ref: BrainRef { object_id: Ulid::new(), provenance_hash: "a1b2c3d4e5f6".into() },
        trace_id: Ulid::new(),
    }
}

#[test]
fn capture_domain_type_and_replay_identity() {
    let opp = sample_opportunity();
    let mut harness = ReplayHarness::new_capture();

    harness
        .record_event("opportunity", &opp.trace_id.to_string(), &opp)
        .expect("capture should succeed");

    assert_eq!(harness.events().len(), 1);
    assert_eq!(harness.events()[0].event_type, "opportunity");

    // Replay with identity handler — return same bytes
    let result = harness
        .replay_events(&mut |ev: &CapturedEvent| Ok(ev.payload_bytes.clone()))
        .expect("replay should succeed");

    assert!(result.matched, "identity handler on domain type must match");
    assert!(result.mismatches.is_empty());
}

#[test]
fn capture_multiple_domain_events_and_replay_in_order() {
    let mut harness = ReplayHarness::new_capture();

    // Record three opportunities to simulate a stream
    for i in 0..3 {
        let mut opp = sample_opportunity();
        opp.gross_edge = Decimal::new(100 + i, 2);
        harness
            .record_event("opportunity", &opp.trace_id.to_string(), &opp)
            .expect("capture opportunity");
    }

    assert_eq!(harness.events().len(), 3);

    let result = harness
        .replay_events(&mut |ev: &CapturedEvent| Ok(ev.payload_bytes.clone()))
        .expect("replay should succeed");

    assert!(result.matched);
    assert_eq!(result.event_count, 3);
}

#[test]
fn persist_and_load_with_domain_types() {
    let opp = sample_opportunity();
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("opp_capture.json");

    // Capture and persist
    {
        let mut harness = ReplayHarness::new_capture();
        harness.record_event("opportunity", &opp.trace_id.to_string(), &opp).expect("capture");
        harness.persist(&path).expect("persist");
    }

    // Load and verify
    let loaded = ReplayHarness::load(&path).expect("load");
    assert_eq!(loaded.events().len(), 1);
    assert_eq!(loaded.events()[0].trace_id, opp.trace_id.to_string());

    // Replay the loaded capture
    let result = loaded
        .replay_events(&mut |ev: &CapturedEvent| Ok(ev.payload_bytes.clone()))
        .expect("replay");

    assert!(result.matched);
}

#[test]
fn hash_mismatch_from_modified_handler() {
    let opp = sample_opportunity();
    let mut harness = ReplayHarness::new_capture();

    harness.record_event("opportunity", &opp.trace_id.to_string(), &opp).expect("capture");

    // Handler that returns a different payload
    let result = harness
        .replay_events(&mut |_ev: &CapturedEvent| Ok(br#"{"type":"tampered"}"#.to_vec()))
        .expect("replay");

    assert!(!result.matched, "tampered output should not match");
    assert!(!result.mismatches.is_empty(), "should have mismatch details");
}

#[test]
fn load_nonexistent_file_fails() {
    let err = ReplayHarness::load(Path::new("/tmp/nonexistent_capture.json"));
    assert!(err.is_err(), "loading nonexistent file must return error");
}

#[test]
fn sha256_of_domain_payload_is_consistent() {
    let opp = sample_opportunity();
    let bytes = serde_json::to_vec(&opp).expect("serialize");

    let hash1 = ReplayHarness::sha256(&bytes);
    let hash2 = ReplayHarness::sha256(&bytes);

    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64);
}

#[test]
fn captured_event_metadata_is_correct() {
    let opp = sample_opportunity();
    let mut harness = ReplayHarness::new_capture();

    harness.record_event("opportunity.v1", &opp.trace_id.to_string(), &opp).expect("capture");

    let ev = &harness.events()[0];
    assert_eq!(ev.event_type, "opportunity.v1");
    assert_eq!(ev.trace_id, opp.trace_id.to_string());
    assert_eq!(ev.seq, 0);
    assert!(ev.wall_clock_offset_ms >= 0, "wall clock offset should be non-negative");
}

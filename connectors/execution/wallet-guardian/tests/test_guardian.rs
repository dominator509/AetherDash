#![allow(clippy::unwrap_used)]

use aether_authz::{Actor, ActorKind, EvaluationContext};
use aether_wallet_guardian::keystore::KeyStore;
use aether_wallet_guardian::nonce::NonceManager;
use aether_wallet_guardian::policy::allowlist::AllowList;
use aether_wallet_guardian::policy::engine::{PolicyConfig, PolicyEngine};
use aether_wallet_guardian::proposal::{CustodyMode, ProposalState, ProposalStore, TxSpec};
use aether_wallet_guardian::service::{ApprovalRequest, GuardianService};
use aether_wallet_guardian::wc::PairingClient;
use rust_decimal::Decimal;

fn make_tx() -> TxSpec {
    TxSpec {
        chain_id: 137,
        to: "0x1234567890123456789012345678901234567890".into(),
        value: "0x0".into(),
        data: "0x".into(),
        gas_limit: 100000,
        max_fee_per_gas: "0x3b9aca00".into(),
        max_priority_fee_per_gas: "0x3b9aca00".into(),
    }
}

fn make_approval(proposal_id: aether_core::ids::Ulid) -> ApprovalRequest {
    let mut evaluation = EvaluationContext::new(0, None);
    evaluation.step_up_satisfied = true;
    evaluation.fresh_human_wallet_approval = true;
    ApprovalRequest {
        proposal_id,
        expected_proposal_hash: String::new(),
        actor: Actor { id: "test-actor".into(), kind: ActorKind::Human },
        evaluation,
        step_up_code: None,
    }
}

fn policy_engine() -> PolicyEngine {
    PolicyEngine {
        allowlist: AllowList::new()
            .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
        ..PolicyEngine::new(PolicyConfig::default())
    }
}

#[test]
fn m1_keystore_sign_and_lock() {
    let mut ks = KeyStore::new("/tmp/test.key");
    assert!(ks.is_available());
    ks.lock();
    assert!(!ks.is_available());
}

#[test]
fn m2_proposal_lifecycle_complete() {
    let mut store = ProposalStore::new();
    let p = store.create(make_tx(), CustodyMode::GuardianCustody);
    assert_eq!(p.state, ProposalState::Pending);

    store.transition(&p.id, ProposalState::Approved).unwrap();
    store.transition(&p.id, ProposalState::Broadcast).unwrap();
    store.transition(&p.id, ProposalState::Confirmed).unwrap();
    assert!(store.get(&p.id).unwrap().state.is_terminal());
}

#[test]
fn m2_proposal_expiry() {
    let mut store = ProposalStore::new();
    let mut p = store.create(make_tx(), CustodyMode::GuardianCustody);
    p.expires_at = aether_core::time::UtcTime::from_unix_millis(0).unwrap();
    store.proposals.insert(p.id, p);
    let expired = store.expire_stale();
    assert_eq!(expired.len(), 1);
}

#[test]
fn m3_policy_chain_denial() {
    let engine = PolicyEngine::new(PolicyConfig::default());
    let mut tx = make_tx();
    tx.chain_id = 999;
    let result = engine.evaluate(&tx, Decimal::ZERO, false, 4);
    assert!(!result.allowed);
}

#[test]
fn m3_policy_withdrawal_always_human() {
    let engine = PolicyEngine {
        allowlist: AllowList::new()
            .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
        ..PolicyEngine::new(PolicyConfig::default())
    };
    let result = engine.evaluate(&make_tx(), Decimal::new(1, 2), true, 5);
    let routing = result.trace.iter().find(|s| s.rule == "approval_routing").unwrap();
    assert_eq!(routing.result, "pending_human");
}

#[test]
fn m4_simulation_revert_marker_denies() {
    use aether_wallet_guardian::policy::simulation::simulate;
    let mut tx = make_tx();
    tx.data = "0xdead".into();
    let result = simulate(&tx, 137, Decimal::ZERO);
    assert!(!result.success);
}

#[test]
fn m4_nonzero_transfer_with_stale_price_denies_at_limits() {
    use aether_wallet_guardian::policy::simulation::SimulationResult;
    let engine = policy_engine();
    let mut tx = make_tx();
    tx.value = "0x1".into();
    let result = engine.evaluate_with_simulation(
        &tx,
        SimulationResult {
            success: true,
            gas_used: Some(21_000),
            error: None,
            value_delta_usd: Decimal::ZERO,
        },
        false,
        5,
    );
    assert!(!result.allowed);
    assert!(result.trace.iter().any(|step| {
        step.rule == "limits" && step.detail.contains("stale or unavailable price")
    }));
}

#[test]
fn m5_nonce_tracking_per_chain() {
    let mut mgr = NonceManager::new();
    assert_eq!(mgr.next_nonce(1), 0);
    assert_eq!(mgr.next_nonce(137), 0);
    assert_eq!(mgr.next_nonce(1), 1);
}

#[test]
fn m6_wc_pairing_and_propose() {
    let mut client = PairingClient::new();
    assert!(!client.is_paired());
    let uri = client.create_pairing();
    assert!(uri.as_str().starts_with("wc:"));

    client.complete_pairing("test-topic");
    assert!(client.is_paired());

    let tx = make_tx();
    let result = client.propose_wc_transaction(&tx);
    assert!(result.is_ok());
}

#[test]
fn m6_wc_request_requires_policy_approved_proposal() {
    let mut svc = GuardianService::with_policy(KeyStore::new("/tmp/test.key"), policy_engine());
    let mut client = PairingClient::new();
    client.create_pairing();
    client.complete_pairing("test-topic");

    let approved = svc.propose_transaction(make_tx(), CustodyMode::WalletConnect).unwrap();
    let req = svc.build_walletconnect_request(&approved.id, &client).unwrap();
    assert!(req.contains("eth_sendTransaction"));

    let mut tx = make_tx();
    tx.value = "0x1".into();
    let pending = svc.propose_transaction(tx, CustodyMode::WalletConnect).unwrap();
    assert!(svc.build_walletconnect_request(&pending.id, &client).is_err());
}

#[test]
fn guardian_service_propose_approve_flow() {
    let mut svc = GuardianService::with_policy(KeyStore::new("/tmp/test.key"), policy_engine());
    let mut tx = make_tx();
    tx.value = "0x1".into();
    let p = svc.propose_transaction(tx, CustodyMode::GuardianCustody).unwrap();
    let mut approval = make_approval(p.id);
    approval.expected_proposal_hash = p.proposal_hash;
    let approved = svc.approve_proposal(approval).unwrap();
    assert_eq!(approved.state, ProposalState::Approved);
}

#[test]
fn guardian_service_rejects_hash_mismatch() {
    let mut svc = GuardianService::with_policy(KeyStore::new("/tmp/test.key"), policy_engine());
    let mut tx = make_tx();
    tx.value = "0x1".into();
    let p = svc.propose_transaction(tx, CustodyMode::GuardianCustody).unwrap();
    let mut approval = make_approval(p.id);
    approval.expected_proposal_hash = "wrong".into();
    assert!(svc.approve_proposal(approval).is_err());
}

#[test]
fn guardian_service_rejects_missing_step_up() {
    let mut svc = GuardianService::with_policy(KeyStore::new("/tmp/test.key"), policy_engine());
    let mut tx = make_tx();
    tx.value = "0x1".into();
    let p = svc.propose_transaction(tx, CustodyMode::GuardianCustody).unwrap();
    let approval = ApprovalRequest {
        proposal_id: p.id,
        expected_proposal_hash: p.proposal_hash,
        actor: Actor { id: "test-actor".into(), kind: ActorKind::Human },
        evaluation: EvaluationContext::new(0, None),
        step_up_code: None,
    };
    assert!(svc.approve_proposal(approval).is_err());
}

#[test]
fn guardian_service_auto_approves_only_policy_allowed_small_non_withdrawal() {
    let mut svc = GuardianService::with_policy(KeyStore::new("/tmp/test.key"), policy_engine());
    let approved = svc.propose_transaction(make_tx(), CustodyMode::GuardianCustody).unwrap();
    assert_eq!(approved.state, ProposalState::AutoApproved);
}

#[test]
fn guardian_service_refuses_when_keystore_locked() {
    let mut ks = KeyStore::new("/tmp/test.key");
    ks.lock();
    let mut svc = GuardianService::new(ks);
    assert!(matches!(
        svc.propose_transaction(make_tx(), CustodyMode::GuardianCustody),
        Err(aether_wallet_guardian::service::GuardianError::KeystoreUnavailable)
    ));
}

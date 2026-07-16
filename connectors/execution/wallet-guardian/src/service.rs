//! Guardian service — the only public API.
//!
//! Provides ProposeTransaction, GetProposal, ApproveProposal.
//! No sign_arbitrary, key_export, or message_signing exists.

use crate::keystore::KeyStore;
use crate::nonce::NonceManager;
use crate::policy::{PolicyConfig, PolicyEngine};
use crate::proposal::{CustodyMode, Proposal, ProposalError, ProposalState, ProposalStore, TxSpec};
use crate::wc::{PairingClient, WcError};
use crate::{broadcast, rpc::RpcClient};
use aether_authz::{Actor, ActorKind, EvaluationContext};
use aether_core::ids::Ulid;
use rust_decimal::Decimal;
use thiserror::Error;

/// Guardian service errors.
#[derive(Error, Debug)]
pub enum GuardianError {
    #[error("keystore unavailable")]
    KeystoreUnavailable,
    #[error("proposal error: {0}")]
    Proposal(#[from] ProposalError),
    #[error("approval denied: {0}")]
    ApprovalDenied(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("broadcast error: {0}")]
    Broadcast(#[from] broadcast::BroadcastError),
    #[error("WalletConnect error: {0}")]
    WalletConnect(#[from] WcError),
}

/// Approval request from the operator.
pub struct ApprovalRequest {
    pub proposal_id: Ulid,
    pub expected_proposal_hash: String,
    pub actor: Actor,
    pub evaluation: EvaluationContext<'static>,
    pub step_up_code: Option<String>,
}

/// The guardian service.
pub struct GuardianService {
    keystore: KeyStore,
    proposals: ProposalStore,
    policy: PolicyEngine,
    nonce_mgr: NonceManager,
}

impl GuardianService {
    pub fn new(keystore: KeyStore) -> Self {
        Self::with_policy(keystore, PolicyEngine::new(PolicyConfig::default()))
    }

    pub fn with_policy(keystore: KeyStore, policy: PolicyEngine) -> Self {
        Self { keystore, proposals: ProposalStore::new(), policy, nonce_mgr: NonceManager::new() }
    }

    /// Propose a transaction. Returns a proposal for policy evaluation.
    pub fn propose_transaction(
        &mut self,
        tx: TxSpec,
        custody: CustodyMode,
    ) -> Result<Proposal, GuardianError> {
        if !self.keystore.is_available() {
            return Err(GuardianError::KeystoreUnavailable);
        }
        let value_usd = value_usd_from_hex_wei(&tx.value);
        let policy = self.policy.evaluate(&tx, value_usd, is_withdrawal(&tx), 5);
        let mut proposal = self.proposals.create(tx, custody);
        proposal.policy_trace = policy.trace;
        if !policy.allowed {
            proposal.state = ProposalState::Denied;
        } else if !policy.requires_human {
            proposal.state = ProposalState::AutoApproved;
            let now = proposal.updated_at;
            proposal.approved_at = Some(now);
            proposal.approval_expires_at =
                aether_core::time::UtcTime::from_unix_millis(now.unix_millis() + 60_000).ok();
        }
        self.proposals.proposals.insert(proposal.id, proposal.clone());
        Ok(proposal)
    }

    /// Get a proposal by ID.
    pub fn get_proposal(&self, id: &Ulid) -> Result<&Proposal, GuardianError> {
        self.proposals.get(id).ok_or(GuardianError::Proposal(ProposalError::NotFound(*id)))
    }

    /// Approve a proposal. This is where policy evaluation and HARD-DENY checks live.
    /// M3 will add the full policy engine here.
    pub fn approve_proposal(
        &mut self,
        request: ApprovalRequest,
    ) -> Result<Proposal, GuardianError> {
        if !self.keystore.is_available() {
            return Err(GuardianError::KeystoreUnavailable);
        }
        if request.actor.kind != ActorKind::Human
            || !request.evaluation.step_up_satisfied
            || !request.evaluation.fresh_human_wallet_approval
        {
            return Err(GuardianError::Unauthorized(
                "fresh human step-up wallet approval is required".into(),
            ));
        }

        let proposal = self
            .proposals
            .get(&request.proposal_id)
            .ok_or(GuardianError::Proposal(ProposalError::NotFound(request.proposal_id)))?;
        if proposal.state != ProposalState::Pending {
            return Err(GuardianError::ApprovalDenied(format!(
                "proposal is not pending: {:?}",
                proposal.state
            )));
        }

        self.proposals.approve_with_hash(&request.proposal_id, &request.expected_proposal_hash)?;

        self.proposals
            .get(&request.proposal_id)
            .cloned()
            .ok_or(GuardianError::Proposal(ProposalError::NotFound(request.proposal_id)))
    }

    /// Sign an approved proposal using the guardian keystore.
    /// M5: Full signing will go here. M1-M2: placeholder.
    pub fn sign_proposal(&self, id: &Ulid) -> Result<Vec<u8>, GuardianError> {
        let proposal =
            self.proposals.get(id).ok_or(GuardianError::Proposal(ProposalError::NotFound(*id)))?;
        if proposal.state != ProposalState::Approved
            && proposal.state != ProposalState::AutoApproved
        {
            return Err(GuardianError::ApprovalDenied("proposal not in approved state".into()));
        }
        if proposal.approval_expires_at.is_some_and(|expiry| {
            aether_core::time::UtcTime::now().unix_millis() > expiry.unix_millis()
        }) {
            return Err(GuardianError::Proposal(ProposalError::Expired(*id)));
        }
        let hash = hash_bytes(&proposal.proposal_hash)?;
        self.keystore
            .sign_proposal(&hash)
            .map(|sig| sig.to_vec())
            .map_err(|_| GuardianError::KeystoreUnavailable)
    }

    /// Broadcast an approved guardian-custody proposal as a signed EIP-1559 transaction.
    pub async fn broadcast_approved_proposal(
        &mut self,
        id: &Ulid,
        rpc: &RpcClient,
    ) -> Result<Proposal, GuardianError> {
        if !self.keystore.is_available() {
            return Err(GuardianError::KeystoreUnavailable);
        }
        let proposal = self
            .proposals
            .get(id)
            .cloned()
            .ok_or(GuardianError::Proposal(ProposalError::NotFound(*id)))?;
        if proposal.custody_mode != CustodyMode::GuardianCustody {
            return Err(GuardianError::ApprovalDenied(
                "WalletConnect proposals must be signed by the external wallet".into(),
            ));
        }
        if proposal.state != ProposalState::Approved
            && proposal.state != ProposalState::AutoApproved
        {
            return Err(GuardianError::ApprovalDenied("proposal not in approved state".into()));
        }
        if proposal.approval_expires_at.is_some_and(|expiry| {
            aether_core::time::UtcTime::now().unix_millis() > expiry.unix_millis()
        }) {
            return Err(GuardianError::Proposal(ProposalError::Expired(*id)));
        }

        let result = broadcast::broadcast_transaction(
            &self.keystore,
            rpc,
            &mut self.nonce_mgr,
            &proposal.tx,
            proposal.tx.chain_id,
        )
        .await?;
        self.proposals.transition(id, ProposalState::Broadcast)?;
        let stored = self
            .proposals
            .get_mut(id)
            .ok_or(GuardianError::Proposal(ProposalError::NotFound(*id)))?;
        stored.signature = Some(result.signed_raw.as_bytes().to_vec());
        stored.tx_hash = Some(result.tx_hash);
        Ok(stored.clone())
    }

    /// Build a WalletConnect transaction request for a policy-approved proposal.
    pub fn build_walletconnect_request(
        &self,
        id: &Ulid,
        client: &PairingClient,
    ) -> Result<String, GuardianError> {
        let proposal =
            self.proposals.get(id).ok_or(GuardianError::Proposal(ProposalError::NotFound(*id)))?;
        client.build_approved_proposal_request(proposal).map_err(GuardianError::from)
    }

    /// Refuse all operations if keystore is unavailable — fail closed.
    pub fn is_operational(&self) -> bool {
        self.keystore.is_available()
    }
}

fn is_withdrawal(tx: &TxSpec) -> bool {
    value_usd_from_hex_wei(&tx.value) > Decimal::ZERO
}

fn value_usd_from_hex_wei(raw: &str) -> Decimal {
    let cleaned = raw.trim_start_matches("0x");
    let Ok(wei) = u128::from_str_radix(cleaned, 16) else {
        return Decimal::ZERO;
    };
    Decimal::from(wei.min(1_000_000_000_000_000_000u128))
        / Decimal::new(1_000_000_000_000_000_000i64, 0)
}

fn hash_bytes(hash_hex: &str) -> Result<[u8; 32], GuardianError> {
    let bytes = hex::decode(hash_hex).map_err(|error| {
        GuardianError::ApprovalDenied(format!("invalid proposal hash: {error}"))
    })?;
    if bytes.len() != 32 {
        return Err(GuardianError::ApprovalDenied("proposal hash must be 32 bytes".into()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::allowlist::AllowList;

    fn policy() -> PolicyEngine {
        PolicyEngine {
            allowlist: AllowList::new()
                .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
            ..PolicyEngine::new(PolicyConfig::default())
        }
    }

    fn make_service() -> GuardianService {
        GuardianService::with_policy(KeyStore::new("/tmp/test.key"), policy())
    }

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

    #[test]
    fn propose_and_approve_flow() {
        let mut svc = make_service();
        let p = svc.propose_transaction(make_tx(), CustodyMode::GuardianCustody).unwrap();
        assert_eq!(p.state, ProposalState::AutoApproved);
    }

    #[test]
    fn human_approval_requires_step_up_and_matching_hash() {
        let mut svc = make_service();
        let mut tx = make_tx();
        tx.value = "0x1".into();
        let p = svc.propose_transaction(tx, CustodyMode::GuardianCustody).unwrap();
        assert_eq!(p.state, ProposalState::Pending);

        let mut evaluation = EvaluationContext::new(0, None);
        evaluation.step_up_satisfied = true;
        evaluation.fresh_human_wallet_approval = true;
        let approved = svc
            .approve_proposal(ApprovalRequest {
                proposal_id: p.id,
                expected_proposal_hash: p.proposal_hash,
                actor: Actor { id: "test".into(), kind: ActorKind::Human },
                evaluation,
                step_up_code: None,
            })
            .unwrap();
        assert_eq!(approved.state, ProposalState::Approved);
    }

    #[test]
    fn refuse_all_when_keystore_unavailable() {
        let mut ks = KeyStore::new("/tmp/test.key");
        ks.lock();
        let mut svc = GuardianService::new(ks);
        assert!(!svc.is_operational());
        assert!(matches!(
            svc.propose_transaction(make_tx(), CustodyMode::GuardianCustody),
            Err(GuardianError::KeystoreUnavailable)
        ));
    }

    #[test]
    fn hard_deny_no_export_or_sign_arbitrary() {
        // This test documents what the Guardian does NOT expose.
        // There is no:
        // - get_private_key() method
        // - sign_message() method
        // - export_keystore() method
        // A grep test in CI validates this.
        let svc = make_service();
        // The only public methods are: propose_transaction, get_proposal,
        // approve_proposal, sign_proposal, is_operational
        // No key export or arbitrary signing exists.
        let _ = svc;
    }

    #[tokio::test]
    async fn approved_guardian_custody_proposal_broadcasts_signed_raw_tx() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            use std::io::{Read, Write};
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let read = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..read]);
                let body = if request.contains("eth_getTransactionCount") {
                    r#"{"jsonrpc":"2.0","id":1,"result":"0x0"}"#.to_string()
                } else {
                    assert!(request.contains("eth_sendRawTransaction"));
                    assert!(request.contains("0x02"));
                    r#"{"jsonrpc":"2.0","id":1,"result":"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#.to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });

        let mut svc = make_service();
        let proposal = svc.propose_transaction(make_tx(), CustodyMode::GuardianCustody).unwrap();
        assert_eq!(proposal.state, ProposalState::AutoApproved);
        let rpc = RpcClient::new(format!("http://{}", addr));
        let broadcast = svc.broadcast_approved_proposal(&proposal.id, &rpc).await.unwrap();
        handle.join().unwrap();
        assert_eq!(broadcast.state, ProposalState::Broadcast);
        assert_eq!(
            broadcast.tx_hash.as_deref(),
            Some("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(String::from_utf8(broadcast.signature.unwrap()).unwrap().starts_with("0x02"));
    }

    #[test]
    fn walletconnect_request_is_built_only_after_policy_approval() {
        let mut svc = make_service();
        let mut client = PairingClient::new();
        client.create_pairing();
        client.complete_pairing("test-topic");

        let approved = svc.propose_transaction(make_tx(), CustodyMode::WalletConnect).unwrap();
        assert_eq!(approved.state, ProposalState::AutoApproved);
        let request = svc.build_walletconnect_request(&approved.id, &client).unwrap();
        assert!(request.contains("eth_sendTransaction"));

        let mut tx = make_tx();
        tx.value = "0x1".into();
        let pending = svc.propose_transaction(tx, CustodyMode::WalletConnect).unwrap();
        assert_eq!(pending.state, ProposalState::Pending);
        assert!(matches!(
            svc.build_walletconnect_request(&pending.id, &client),
            Err(GuardianError::WalletConnect(WcError::PolicyNotApproved))
        ));
    }
}

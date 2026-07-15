//! Guardian proposal lifecycle (SPEC-010).
//!
//! Proposals flow: pending -> approved|auto_approved|denied|expired -> broadcast -> confirmed|failed

use aether_core::ids::Ulid;
use aether_core::time::UtcTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Ethereum transaction specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSpec {
    pub chain_id: u64,
    pub to: String,
    pub value: String, // hex-encoded wei
    pub data: String,  // hex-encoded calldata
    pub gas_limit: u64,
    pub max_fee_per_gas: String, // hex wei
    pub max_priority_fee_per_gas: String,
}

/// A single step in the policy evaluation trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStep {
    pub rule: String,
    pub result: String, // "allow" | "deny"
    pub detail: String,
}

/// Proposal state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalState {
    Pending,
    Approved,
    AutoApproved,
    Denied,
    Expired,
    Broadcast,
    Confirmed,
    Failed,
}

impl ProposalState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Denied | Self::Expired | Self::Confirmed | Self::Failed)
    }
}

/// A guardian proposal for a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: Ulid,
    pub tx: TxSpec,
    pub state: ProposalState,
    pub policy_trace: Vec<PolicyStep>,
    pub custody_mode: CustodyMode,
    pub proposal_hash: String,
    pub approved_at: Option<UtcTime>,
    pub approval_expires_at: Option<UtcTime>,
    pub signature: Option<Vec<u8>>,
    pub tx_hash: Option<String>,
    pub created_at: UtcTime,
    pub expires_at: UtcTime,
    pub updated_at: UtcTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustodyMode {
    GuardianCustody,
    WalletConnect,
}

/// Proposal store errors.
#[derive(Error, Debug)]
pub enum ProposalError {
    #[error("proposal not found: {0}")]
    NotFound(Ulid),
    #[error("proposal expired: {0}")]
    Expired(Ulid),
    #[error("invalid state transition: {from:?} -> {to:?}")]
    InvalidTransition { from: ProposalState, to: ProposalState },
    #[error("approval hash mismatch: proposal was modified after approval")]
    HashMismatch,
}

/// In-memory proposal store (production would use Postgres).
#[derive(Debug, Default)]
pub struct ProposalStore {
    pub proposals: HashMap<Ulid, Proposal>,
}

impl ProposalStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, tx: TxSpec, custody: CustodyMode) -> Proposal {
        let now = UtcTime::now();
        let proposal_hash = proposal_hash(&tx, custody);
        let proposal = Proposal {
            id: Ulid::new(),
            tx,
            state: ProposalState::Pending,
            policy_trace: Vec::new(),
            custody_mode: custody,
            proposal_hash,
            approved_at: None,
            approval_expires_at: None,
            signature: None,
            tx_hash: None,
            created_at: now,
            expires_at: UtcTime::from_unix_millis(now.unix_millis() + 600_000).unwrap_or(now), // 10 min
            updated_at: now,
        };
        self.proposals.insert(proposal.id, proposal.clone());
        proposal
    }

    pub fn get(&self, id: &Ulid) -> Option<&Proposal> {
        self.proposals.get(id)
    }

    pub fn get_mut(&mut self, id: &Ulid) -> Option<&mut Proposal> {
        self.proposals.get_mut(id)
    }

    pub fn expire_stale(&mut self) -> Vec<Ulid> {
        let now = UtcTime::now();
        let mut expired = Vec::new();
        for p in self.proposals.values_mut() {
            if !p.state.is_terminal() && now.unix_millis() > p.expires_at.unix_millis() {
                p.state = ProposalState::Expired;
                p.updated_at = now;
                expired.push(p.id);
            }
        }
        expired
    }

    pub fn transition(&mut self, id: &Ulid, to: ProposalState) -> Result<(), ProposalError> {
        let p = self.proposals.get_mut(id).ok_or(ProposalError::NotFound(*id))?;
        let now = UtcTime::now();
        if now.unix_millis() > p.expires_at.unix_millis() {
            p.state = ProposalState::Expired;
            p.updated_at = now;
            return Err(ProposalError::Expired(*id));
        }
        match (p.state, to) {
            (ProposalState::Pending, ProposalState::Approved)
            | (ProposalState::Pending, ProposalState::AutoApproved) => {
                p.state = to;
                p.approved_at = Some(now);
                p.approval_expires_at = UtcTime::from_unix_millis(now.unix_millis() + 60_000).ok();
                p.updated_at = now;
                Ok(())
            }
            (ProposalState::Pending, ProposalState::Denied)
            | (ProposalState::Approved, ProposalState::Broadcast)
            | (ProposalState::AutoApproved, ProposalState::Broadcast)
            | (ProposalState::Broadcast, ProposalState::Confirmed)
            | (ProposalState::Broadcast, ProposalState::Failed) => {
                if matches!(p.state, ProposalState::Approved | ProposalState::AutoApproved)
                    && to == ProposalState::Broadcast
                    && p.approval_expires_at
                        .is_some_and(|expiry| now.unix_millis() > expiry.unix_millis())
                {
                    p.state = ProposalState::Expired;
                    p.updated_at = now;
                    return Err(ProposalError::Expired(*id));
                }
                p.state = to;
                p.updated_at = now;
                Ok(())
            }
            (from, to) => Err(ProposalError::InvalidTransition { from, to }),
        }
    }

    pub fn approve_with_hash(
        &mut self,
        id: &Ulid,
        expected_hash: &str,
    ) -> Result<(), ProposalError> {
        let proposal = self.proposals.get(id).ok_or(ProposalError::NotFound(*id))?;
        if proposal.proposal_hash != expected_hash {
            return Err(ProposalError::HashMismatch);
        }
        self.transition(id, ProposalState::Approved)
    }
}

pub fn proposal_hash(tx: &TxSpec, custody: CustodyMode) -> String {
    use sha2::{Digest, Sha256};
    let wire = serde_json::json!({
        "chain_id": tx.chain_id,
        "to": tx.to.to_lowercase(),
        "value": tx.value,
        "data": tx.data,
        "gas_limit": tx.gas_limit,
        "max_fee_per_gas": tx.max_fee_per_gas,
        "max_priority_fee_per_gas": tx.max_priority_fee_per_gas,
        "custody_mode": custody,
    });
    let bytes = serde_json::to_vec(&wire).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn proposal_lifecycle_pending_to_approved_to_broadcast() {
        let mut store = ProposalStore::new();
        let p = store.create(make_tx(), CustodyMode::GuardianCustody);
        assert_eq!(p.state, ProposalState::Pending);

        store.approve_with_hash(&p.id, &p.proposal_hash).unwrap();
        assert_eq!(store.get(&p.id).unwrap().state, ProposalState::Approved);

        store.transition(&p.id, ProposalState::Broadcast).unwrap();
        assert_eq!(store.get(&p.id).unwrap().state, ProposalState::Broadcast);
    }

    #[test]
    fn proposal_expires_after_10_minutes() {
        let mut store = ProposalStore::new();
        let mut p = store.create(make_tx(), CustodyMode::GuardianCustody);
        // Force expiry
        p.expires_at = UtcTime::from_unix_millis(0).unwrap();
        let id = p.id;
        store.proposals.insert(id, p);
        let result = store.transition(&id, ProposalState::Approved);
        assert!(matches!(result, Err(ProposalError::Expired(_))));
    }

    #[test]
    fn invalid_transition_rejected() {
        let mut store = ProposalStore::new();
        let p = store.create(make_tx(), CustodyMode::GuardianCustody);
        // Can't go directly from Pending to Confirmed
        let result = store.transition(&p.id, ProposalState::Confirmed);
        assert!(matches!(result, Err(ProposalError::InvalidTransition { .. })));
    }

    #[test]
    fn auto_approved_path_works() {
        let mut store = ProposalStore::new();
        let p = store.create(make_tx(), CustodyMode::GuardianCustody);
        store.transition(&p.id, ProposalState::AutoApproved).unwrap();
        store.transition(&p.id, ProposalState::Broadcast).unwrap();
        store.transition(&p.id, ProposalState::Confirmed).unwrap();
        assert!(store.get(&p.id).unwrap().state.is_terminal());
    }

    #[test]
    fn approval_hash_mismatch_rejected() {
        let mut store = ProposalStore::new();
        let p = store.create(make_tx(), CustodyMode::GuardianCustody);
        assert!(matches!(
            store.approve_with_hash(&p.id, "wrong"),
            Err(ProposalError::HashMismatch)
        ));
    }
}

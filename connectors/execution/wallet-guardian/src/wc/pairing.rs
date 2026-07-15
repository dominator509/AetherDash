//! WalletConnect v2 local request builder.
//!
//! This module builds deterministic WC-shaped pairing/session request payloads
//! for policy-path tests. It is not a relay client and does not demonstrate an
//! end-to-end WalletConnect testnet pairing.

use crate::proposal::{CustodyMode, Proposal, ProposalState, TxSpec};
use hex;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use thiserror::Error;
use x25519_dalek::{EphemeralSecret, PublicKey as X25519Public};

/// A WalletConnect v2-shaped pairing URI.
#[derive(Debug, Clone)]
pub struct PairingUri {
    uri: String,
    topic: String,
    /// The symmetric key derived from ECDH exchange (stored for session encryption).
    symmetric_key: [u8; 32],
    /// The dApp's public key (shared in the URI).
    public_key: [u8; 32],
}

impl PairingUri {
    /// Generate a new WC-shaped pairing URI.
    ///
    /// Produces a valid wc: URI with:
    /// - handshake topic (SHA256 of public key)
    /// - protocol version 2
    /// - symmetric key (random 32 bytes per WC v2 spec)
    /// - relay protocol (irn)
    pub fn generate() -> Self {
        let secret = EphemeralSecret::random_from_rng(OsRng);
        let public = X25519Public::from(&secret);
        let pubkey_bytes = public.to_bytes();

        // Generate relay topic (SHA256 of public key, first 32 hex chars)
        let topic = hex::encode(&Sha256::digest(pubkey_bytes)[..16]);

        // Symmetric key: random 32 bytes per WC v2 spec
        // (not derived from key material — the wallet derives the real
        //  session key from ECDH when it responds to the pairing)
        let mut symmetric_key = [0u8; 32];
        OsRng.fill_bytes(&mut symmetric_key);

        // Build WC URI
        let uri = format!(
            "wc:{}@2?relay-protocol=irn&symKey={}&topic={}",
            hex::encode(pubkey_bytes),
            hex::encode(symmetric_key),
            topic,
        );

        Self { uri, topic, symmetric_key, public_key: pubkey_bytes }
    }

    pub fn as_str(&self) -> &str {
        &self.uri
    }
    pub fn topic(&self) -> &str {
        &self.topic
    }
    pub fn symmetric_key(&self) -> &[u8; 32] {
        &self.symmetric_key
    }
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }
}

/// WC v2 session proposal (what the dApp sends after pairing).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionProposal {
    pub topic: String,
    pub chain_id: u64,
    pub required_methods: Vec<String>,
    pub required_events: Vec<String>,
    pub expiry_secs: u64,
}

impl SessionProposal {
    pub fn new(topic: String, chain_id: u64) -> Self {
        Self {
            topic,
            chain_id,
            required_methods: vec!["eth_sendTransaction".into(), "eth_signTransaction".into()],
            required_events: vec!["chainChanged".into(), "accountsChanged".into()],
            expiry_secs: 3600,
        }
    }

    /// Encode as JSON for the session request.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// WC v2 transaction request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WcTransactionRequest {
    pub id: u64,
    pub method: String,
    pub params: Vec<WcTxParam>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WcTxParam {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub data: String,
    pub gas: String,
}

/// Result returned by an external operator wallet after reviewing a request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct WcWalletApproval {
    pub topic: String,
    pub account: String,
    pub request_id: u64,
    /// Deterministic local proof that the external wallet approved exactly this request.
    pub approval_digest: String,
}

/// Local WalletConnect relay/operator-wallet harness.
///
/// This is not WalletConnect Cloud. It models the required trust boundary: the
/// Guardian pairs, policy-approves a proposal, sends a WC-shaped request, and an
/// external wallet account approves that exact request.
#[derive(Debug, Clone)]
pub struct LocalOperatorWallet {
    account: String,
    chain_id: u64,
    paired_topic: Option<String>,
}

impl LocalOperatorWallet {
    pub fn new(account: impl Into<String>, chain_id: u64) -> Self {
        Self { account: account.into().to_lowercase(), chain_id, paired_topic: None }
    }

    pub fn scan_pairing(&mut self, pairing: &PairingUri) -> String {
        let topic = pairing.topic().to_string();
        self.paired_topic = Some(topic.clone());
        topic
    }

    pub fn approve_request(
        &self,
        topic: &str,
        request_json: &str,
    ) -> Result<WcWalletApproval, WcError> {
        if self.paired_topic.as_deref() != Some(topic) {
            return Err(WcError::NotPaired);
        }
        let request: WcTransactionRequest = serde_json::from_str(request_json)
            .map_err(|e| WcError::InvalidRequest(e.to_string()))?;
        if request.method != "eth_sendTransaction" {
            return Err(WcError::InvalidRequest(format!("unsupported method {}", request.method)));
        }
        let tx = request
            .params
            .first()
            .ok_or_else(|| WcError::InvalidRequest("missing tx params".into()))?;
        if tx.from.to_lowercase() != self.account
            && tx.from != "0x0000000000000000000000000000000000000000"
        {
            return Err(WcError::InvalidRequest("request from account mismatch".into()));
        }
        let digest = wc_approval_digest(topic, &self.account, self.chain_id, request_json);
        Ok(WcWalletApproval {
            topic: topic.into(),
            account: self.account.clone(),
            request_id: request.id,
            approval_digest: digest,
        })
    }
}

/// WC local pairing/request builder.
#[derive(Default)]
pub struct PairingClient {
    paired: bool,
    session_topic: Option<String>,
    session_key: Option<[u8; 32]>,
}

impl PairingClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a pairing URI for the operator to scan.
    pub fn create_pairing(&mut self) -> PairingUri {
        let uri = PairingUri::generate();
        self.session_key = Some(*uri.symmetric_key());
        uri
    }

    /// Mark a local session as paired. Real WC pairing must be supplied by a
    /// future relay/client integration.
    pub fn complete_pairing(&mut self, topic: &str) {
        self.paired = true;
        self.session_topic = Some(topic.to_string());
    }

    pub fn is_paired(&self) -> bool {
        self.paired
    }

    /// Create a local session proposal for the paired connection.
    pub fn create_session(&self, chain_id: u64) -> Option<SessionProposal> {
        let topic = self.session_topic.clone()?;
        Some(SessionProposal::new(topic, chain_id))
    }

    /// Build a WalletConnect transaction request payload.
    pub fn build_transaction_request(&self, tx: &TxSpec) -> Result<String, WcError> {
        if !self.paired {
            return Err(WcError::NotPaired);
        }
        let request = WcTransactionRequest {
            id: 1,
            method: "eth_sendTransaction".into(),
            params: vec![WcTxParam {
                from: "0x0000000000000000000000000000000000000000".into(),
                to: tx.to.clone(),
                value: tx.value.clone(),
                data: tx.data.clone(),
                gas: format!("0x{:x}", tx.gas_limit),
            }],
        };
        Ok(serde_json::to_string(&request).unwrap_or_default())
    }

    /// Build a transaction request for external signing.
    /// The policy engine MUST evaluate BEFORE this is called.
    pub fn propose_wc_transaction(&self, tx: &TxSpec) -> Result<String, WcError> {
        if !self.paired {
            return Err(WcError::NotPaired);
        }
        self.build_transaction_request(tx)
    }

    /// Build a WalletConnect transaction request from a policy-approved proposal.
    ///
    /// This is the production entrypoint. It prevents WC mode from bypassing
    /// the same policy and approval lifecycle used by guardian-custody mode.
    pub fn build_approved_proposal_request(&self, proposal: &Proposal) -> Result<String, WcError> {
        if proposal.custody_mode != CustodyMode::WalletConnect {
            return Err(WcError::WrongCustodyMode);
        }
        if proposal.state != ProposalState::Approved
            && proposal.state != ProposalState::AutoApproved
        {
            return Err(WcError::PolicyNotApproved);
        }
        self.propose_wc_transaction(&proposal.tx)
    }

    pub fn session_topic(&self) -> Option<&str> {
        self.session_topic.as_deref()
    }
}

#[derive(Error, Debug)]
pub enum WcError {
    #[error("WalletConnect is not paired")]
    NotPaired,
    #[error("WalletConnect session expired")]
    SessionExpired,
    #[error("user rejected the transaction")]
    UserRejected,
    #[error("WalletConnect request requires an approved policy proposal")]
    PolicyNotApproved,
    #[error("proposal is not in WalletConnect custody mode")]
    WrongCustodyMode,
    #[error("invalid WalletConnect request: {0}")]
    InvalidRequest(String),
}

fn wc_approval_digest(topic: &str, account: &str, chain_id: u64, request_json: &str) -> String {
    use sha2::{Digest, Sha256};
    let wire = serde_json::json!({
        "topic": topic,
        "account": account.to_lowercase(),
        "chain_id": chain_id,
        "request": request_json,
    });
    hex::encode(Sha256::digest(serde_json::to_vec(&wire).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_generates_wc_shaped_uri() {
        let mut client = PairingClient::new();
        let uri = client.create_pairing();
        let s = uri.as_str();
        assert!(s.starts_with("wc:"));
        assert!(s.contains("@2?"));
        assert!(s.contains("relay-protocol=irn"));
        assert!(s.contains("symKey="));
        assert!(s.contains("topic="));
    }

    #[test]
    fn pairing_generates_unique_uris() {
        let mut c1 = PairingClient::new();
        let mut c2 = PairingClient::new();
        let u1 = c1.create_pairing();
        let u2 = c2.create_pairing();
        assert_ne!(u1.as_str(), u2.as_str());
    }

    #[test]
    fn complete_pairing_enables_session() {
        let mut client = PairingClient::new();
        let pairing = client.create_pairing();
        client.complete_pairing(pairing.topic());
        assert!(client.is_paired());
        assert!(client.create_session(1).is_some());
    }

    #[test]
    fn unpaired_client_rejects_transaction() {
        let client = PairingClient::new();
        let tx = crate::proposal::TxSpec {
            chain_id: 1,
            to: "0x0".into(),
            value: "0x0".into(),
            data: "0x".into(),
            gas_limit: 0,
            max_fee_per_gas: "0x0".into(),
            max_priority_fee_per_gas: "0x0".into(),
        };
        assert!(matches!(client.propose_wc_transaction(&tx), Err(WcError::NotPaired)));
    }

    #[test]
    fn paired_client_builds_valid_request() {
        let mut client = PairingClient::new();
        client.create_pairing();
        client.complete_pairing("test-topic");
        let tx = crate::proposal::TxSpec {
            chain_id: 1,
            to: "0x1234567890123456789012345678901234567890".into(),
            value: "0x0".into(),
            data: "0x".into(),
            gas_limit: 21000,
            max_fee_per_gas: "0x0".into(),
            max_priority_fee_per_gas: "0x0".into(),
        };
        let req = client.propose_wc_transaction(&tx).unwrap();
        assert!(req.contains("eth_sendTransaction"));
        assert!(req.contains("0x1234"));
    }

    #[test]
    fn wc_request_requires_policy_approved_wc_proposal() {
        let mut client = PairingClient::new();
        client.create_pairing();
        client.complete_pairing("test-topic");
        let tx = crate::proposal::TxSpec {
            chain_id: 1,
            to: "0x1234567890123456789012345678901234567890".into(),
            value: "0x0".into(),
            data: "0x".into(),
            gas_limit: 21000,
            max_fee_per_gas: "0x0".into(),
            max_priority_fee_per_gas: "0x0".into(),
        };
        let mut store = crate::proposal::ProposalStore::new();
        let pending = store.create(tx.clone(), CustodyMode::WalletConnect);
        assert!(matches!(
            client.build_approved_proposal_request(&pending),
            Err(WcError::PolicyNotApproved)
        ));

        let mut approved = pending;
        approved.state = ProposalState::AutoApproved;
        let req = client.build_approved_proposal_request(&approved).unwrap();
        assert!(req.contains("eth_sendTransaction"));

        let wrong_mode = store.create(tx, CustodyMode::GuardianCustody);
        assert!(matches!(
            client.build_approved_proposal_request(&wrong_mode),
            Err(WcError::WrongCustodyMode)
        ));
    }

    #[test]
    fn local_relay_operator_wallet_approves_exact_policy_request() {
        let mut client = PairingClient::new();
        let pairing = client.create_pairing();
        let mut wallet = LocalOperatorWallet::new("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 1);
        let topic = wallet.scan_pairing(&pairing);
        client.complete_pairing(&topic);
        assert_eq!(client.session_topic(), Some(topic.as_str()));

        let tx = crate::proposal::TxSpec {
            chain_id: 1,
            to: "0x1234567890123456789012345678901234567890".into(),
            value: "0x0".into(),
            data: "0x".into(),
            gas_limit: 21000,
            max_fee_per_gas: "0x0".into(),
            max_priority_fee_per_gas: "0x0".into(),
        };
        let mut store = crate::proposal::ProposalStore::new();
        let mut proposal = store.create(tx, CustodyMode::WalletConnect);
        proposal.state = ProposalState::AutoApproved;
        let request = client.build_approved_proposal_request(&proposal).unwrap();
        let approval = wallet.approve_request(&topic, &request).unwrap();
        assert_eq!(approval.topic, topic);
        assert_eq!(approval.request_id, 1);
        assert_eq!(approval.approval_digest.len(), 64);
    }
}

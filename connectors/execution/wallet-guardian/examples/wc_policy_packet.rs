//! Emits one policy-approved WalletConnect transaction request for the live
//! relay adapter. This process never receives a project id or wallet secret.

use aether_wallet_guardian::keystore::KeyStore;
use aether_wallet_guardian::policy::allowlist::AllowList;
use aether_wallet_guardian::policy::engine::{PolicyConfig, PolicyEngine};
use aether_wallet_guardian::proposal::{CustodyMode, TxSpec};
use aether_wallet_guardian::service::GuardianService;
use aether_wallet_guardian::wc::PairingClient;

fn required_env(name: &str) -> Result<String, std::io::Error> {
    std::env::var(name)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, format!("missing {name}")))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let account = required_env("AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT")?.to_lowercase();
    let chain_id: u64 = required_env("AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID").and_then(|value| {
        value.parse().map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID must be a u64: {error}"),
            )
        })
    })?;

    let policy = PolicyEngine {
        allowlist: AllowList::new().with_allowed_destinations(vec![account.as_str()]),
        ..PolicyEngine::new(PolicyConfig {
            allowed_chains: vec![chain_id],
            ..PolicyConfig::default()
        })
    };
    let mut service = GuardianService::with_policy(KeyStore::new("/tmp/wc-live.key"), policy);
    let tx = TxSpec {
        chain_id,
        to: account.clone(),
        value: "0x0".into(),
        data: "0x".into(),
        gas_limit: 21_000,
        max_fee_per_gas: "0x3b9aca00".into(),
        max_priority_fee_per_gas: "0x3b9aca00".into(),
    };
    let proposal = service.propose_transaction(tx, CustodyMode::WalletConnect)?;

    let mut client = PairingClient::new();
    client.create_pairing();
    client.complete_pairing("live-session-account-binding");
    let request_json =
        service.build_walletconnect_request_for_account(&proposal.id, &client, &account)?;
    let request: serde_json::Value = serde_json::from_str(&request_json)?;

    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "proposal_id": proposal.id.to_string(),
            "proposal_hash": proposal.proposal_hash,
            "guardian_policy_state": proposal.state,
            "policy_trace": proposal.policy_trace,
            "chain_id": chain_id,
            "operator_account": account,
            "request": request,
        }))?
    );
    Ok(())
}

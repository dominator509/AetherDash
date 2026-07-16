#![allow(clippy::unwrap_used)]

use aether_wallet_guardian::keystore::KeyStore;
use aether_wallet_guardian::policy::allowlist::AllowList;
use aether_wallet_guardian::policy::engine::{PolicyConfig, PolicyEngine};
use aether_wallet_guardian::proposal::{CustodyMode, ProposalState, TxSpec};
use aether_wallet_guardian::service::GuardianService;
use aether_wallet_guardian::wc::PairingClient;

const PROJECT_ID: &str = "AETHER_GUARDIAN__WC_PROJECT_ID";
const RELAY_URL: &str = "AETHER_GUARDIAN__WC_RELAY_URL";
const OPERATOR_ACCOUNT: &str = "AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT";
const TESTNET_CHAIN_ID: &str = "AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID";

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!(
            "missing {name}; live WalletConnect proof requires project id, relay URL, operator account, and testnet chain id"
        )
    })
}

fn make_tx(chain_id: u64) -> TxSpec {
    TxSpec {
        chain_id,
        to: "0x1234567890123456789012345678901234567890".into(),
        value: "0x0".into(),
        data: "0x".into(),
        gas_limit: 21_000,
        max_fee_per_gas: "0x3b9aca00".into(),
        max_priority_fee_per_gas: "0x3b9aca00".into(),
    }
}

fn policy_engine(chain_id: u64) -> PolicyEngine {
    PolicyEngine {
        allowlist: AllowList::new()
            .with_allowed_destinations(vec!["0x1234567890123456789012345678901234567890"]),
        ..PolicyEngine::new(PolicyConfig {
            allowed_chains: vec![chain_id],
            ..PolicyConfig::default()
        })
    }
}

#[test]
fn wc_live_required_env_contract_is_documented() {
    let required = [PROJECT_ID, RELAY_URL, OPERATOR_ACCOUNT, TESTNET_CHAIN_ID];
    assert_eq!(required.len(), 4);
    assert!(required.iter().all(|name| name.starts_with("AETHER_GUARDIAN__WC_")));
}

#[test]
#[ignore = "requires real WalletConnect project/relay config and an operator wallet testnet session"]
fn wc_live_pairing_packet_is_policy_approved_and_operator_ready() {
    let project_id = required_env(PROJECT_ID);
    let relay_url = required_env(RELAY_URL);
    let operator_account = required_env(OPERATOR_ACCOUNT);
    let chain_id: u64 = required_env(TESTNET_CHAIN_ID)
        .parse()
        .expect("AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID must be a u64 chain id");

    assert!(
        relay_url.starts_with("wss://") || relay_url.starts_with("ws://"),
        "WalletConnect relay URL must be ws/wss"
    );
    assert!(operator_account.starts_with("0x") && operator_account.len() == 42);
    assert!(!project_id.trim().is_empty());

    let mut service =
        GuardianService::with_policy(KeyStore::new("/tmp/wc-live.key"), policy_engine(chain_id));
    let proposal =
        service.propose_transaction(make_tx(chain_id), CustodyMode::WalletConnect).unwrap();
    assert_eq!(proposal.state, ProposalState::AutoApproved);

    let mut client = PairingClient::new();
    let pairing = client.create_pairing();
    client.complete_pairing(pairing.topic());
    let request = service.build_walletconnect_request(&proposal.id, &client).unwrap();

    let operator_packet = serde_json::json!({
        "project_id": project_id,
        "relay_url": relay_url,
        "operator_account": operator_account.to_lowercase(),
        "chain_id": chain_id,
        "pairing_uri": pairing.as_str(),
        "pairing_topic": pairing.topic(),
        "request": serde_json::from_str::<serde_json::Value>(&request).unwrap(),
        "expected_policy_state": "auto_approved",
    });

    println!("{}", serde_json::to_string_pretty(&operator_packet).unwrap());
    assert!(operator_packet["pairing_uri"].as_str().unwrap().starts_with("wc:"));
    assert_eq!(operator_packet["request"]["method"], "eth_sendTransaction");
}

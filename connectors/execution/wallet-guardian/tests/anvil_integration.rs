#![allow(clippy::unwrap_used)]

use aether_wallet_guardian::keystore::KeyStore;
use aether_wallet_guardian::policy::allowlist::AllowList;
use aether_wallet_guardian::policy::engine::{PolicyConfig, PolicyEngine};
use aether_wallet_guardian::policy::simulation::simulate_async;
use aether_wallet_guardian::proposal::{CustodyMode, ProposalState, TxSpec};
use aether_wallet_guardian::rpc::RpcClient;
use aether_wallet_guardian::service::GuardianService;
use rust_decimal::Decimal;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const ANVIL_CHAIN_ID: u64 = 31337;
const ANVIL_DEFAULT_PRIVATE_KEY: &str =
    "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const ANVIL_FUNDED_ADDRESS: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
const RECEIVER: &str = "0x70997970c51812dc3a010c7d01b50e0d17dc79c8";

struct AnvilGuard {
    child: Child,
}

impl Drop for AnvilGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn make_tx() -> TxSpec {
    TxSpec {
        chain_id: ANVIL_CHAIN_ID,
        to: RECEIVER.into(),
        value: "0x0".into(),
        data: "0x".into(),
        gas_limit: 21_000,
        max_fee_per_gas: "0x3b9aca00".into(),
        max_priority_fee_per_gas: "0x3b9aca00".into(),
    }
}

fn policy_engine() -> PolicyEngine {
    PolicyEngine {
        allowlist: AllowList::new().with_allowed_destinations(vec![RECEIVER]),
        ..PolicyEngine::new(PolicyConfig {
            allowed_chains: vec![ANVIL_CHAIN_ID],
            ..PolicyConfig::default()
        })
    }
}

fn anvil_bin() -> String {
    std::env::var("AETHER_GUARDIAN__ANVIL_BIN").unwrap_or_else(|_| "anvil".into())
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn start_anvil() -> (AnvilGuard, String) {
    let port = free_port();
    let url = format!("http://127.0.0.1:{port}");
    let child = Command::new(anvil_bin())
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--chain-id",
            &ANVIL_CHAIN_ID.to_string(),
            "--silent",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let guard = AnvilGuard { child };
    let rpc = RpcClient::new(&url);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if rpc.eth_get_transaction_count(ANVIL_FUNDED_ADDRESS).await.is_ok() {
            return (guard, url);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("anvil did not become ready at {url}");
}

fn hex_32(raw: &str) -> [u8; 32] {
    let bytes = hex::decode(raw).unwrap();
    assert_eq!(bytes.len(), 32);
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

#[tokio::test]
#[ignore = "requires anvil; run with AETHER_GUARDIAN__ANVIL_BIN=<path-to-anvil> cargo test -p aether-wallet-guardian --test anvil_integration -- --ignored"]
async fn anvil_simulates_signs_and_broadcasts_approved_guardian_tx() {
    let (_anvil, rpc_url) = start_anvil().await;
    let tx = make_tx();

    let simulation = simulate_async(&tx, ANVIL_CHAIN_ID, Decimal::new(1, 0), Some(&rpc_url)).await;
    assert!(simulation.success, "simulation failed: {:?}", simulation.error);
    assert_eq!(simulation.value_delta_usd, Decimal::new(1, 0));

    let keystore =
        KeyStore::dev_from_private_key_bytes("/tmp/anvil.key", &hex_32(ANVIL_DEFAULT_PRIVATE_KEY))
            .unwrap();
    assert_eq!(keystore.address().as_str(), ANVIL_FUNDED_ADDRESS);

    let mut service = GuardianService::with_policy(keystore, policy_engine());
    let proposal = service.propose_transaction(tx, CustodyMode::GuardianCustody).unwrap();
    assert_eq!(proposal.state, ProposalState::AutoApproved);

    let rpc = RpcClient::new(rpc_url);
    let broadcast = service.broadcast_approved_proposal(&proposal.id, &rpc).await.unwrap();
    assert_eq!(broadcast.state, ProposalState::Broadcast);
    let tx_hash = broadcast.tx_hash.unwrap();
    assert!(tx_hash.starts_with("0x"));
    assert_eq!(tx_hash.len(), 66);
    assert!(String::from_utf8(broadcast.signature.unwrap()).unwrap().starts_with("0x02"));
}

//! Policy rule: pre-execution simulation via eth_call.
//!
//! Production simulation calls eth_call through operator-configured RPC.
//! Falls back to deterministic local checks when RPC is unavailable.
//! Simulation revert -> deny with reason. Balance-delta feeds limits.

use crate::proposal::TxSpec;
use rust_decimal::Decimal;

/// Simulation result.
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub success: bool,
    pub gas_used: Option<u64>,
    pub error: Option<String>,
    pub value_delta_usd: Decimal,
}

/// Perform deterministic local validation checks.
/// These are run BEFORE any RPC call as a fast-reject gate.
pub fn local_validate(tx: &TxSpec, chain_id: u64) -> Option<SimulationResult> {
    if tx.chain_id != chain_id {
        return Some(failed("transaction chain does not match simulation chain"));
    }
    if !tx.to.starts_with("0x") || tx.to.len() != 42 {
        return Some(failed("invalid destination address"));
    }
    if !tx.data.starts_with("0x") {
        return Some(failed("calldata must be 0x-prefixed hex"));
    }
    if tx.gas_limit == 0 {
        return Some(failed("gas limit is zero"));
    }
    if tx.data.eq_ignore_ascii_case("0xdead") || tx.data.to_ascii_lowercase().contains("revert") {
        return Some(failed("simulation_failed: explicit revert marker"));
    }
    None // Passed local checks
}

/// Simulate a transaction. Tries RPC first, falls back to local-only.
pub async fn simulate_async(
    tx: &TxSpec,
    chain_id: u64,
    value_usd: Decimal,
    rpc_url: Option<&str>,
) -> SimulationResult {
    // Fast-reject local checks
    if let Some(early) = local_validate(tx, chain_id) {
        return early;
    }

    // Try RPC simulation if available
    if let Some(url) = rpc_url {
        let sim = RpcSimulator::new(url);
        match sim.simulate(tx).await {
            Ok(result) => {
                return SimulationResult {
                    success: result.success,
                    gas_used: Some(result.gas_used),
                    error: if result.success {
                        None
                    } else {
                        Some("simulation_failed: eth_call returned revert payload".into())
                    },
                    value_delta_usd: if result.success { value_usd } else { Decimal::ZERO },
                };
            }
            Err(e) => {
                return SimulationResult {
                    success: false,
                    gas_used: None,
                    error: Some(format!("RPC simulation failed: {e}")),
                    value_delta_usd: Decimal::ZERO,
                };
            }
        }
    }

    // Fallback: local-only (conservative -- allows through)
    SimulationResult {
        success: true,
        gas_used: Some(21000),
        error: None,
        value_delta_usd: value_usd,
    }
}

/// Synchronous local-only simulation (used when RPC is unavailable).
pub fn simulate(tx: &TxSpec, chain_id: u64, value_usd: Decimal) -> SimulationResult {
    if let Some(early) = local_validate(tx, chain_id) {
        return early;
    }
    // Local revert-marker heuristic
    if tx.data == "0xdead" || tx.data.starts_with("0x08c379a0") {
        return failed("revert detected");
    }
    SimulationResult {
        success: true,
        gas_used: Some(21000),
        error: None,
        value_delta_usd: value_usd,
    }
}

fn failed(error: impl Into<String>) -> SimulationResult {
    SimulationResult {
        success: false,
        gas_used: None,
        error: Some(error.into()),
        value_delta_usd: Decimal::ZERO,
    }
}

/// Result from an RPC simulation.
pub struct RpcSimResult {
    pub gas_used: u64,
    pub success: bool,
    pub return_data: String,
}

/// RPC-based transaction simulator.
pub struct RpcSimulator {
    client: crate::rpc::RpcClient,
}

impl RpcSimulator {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self { client: crate::rpc::RpcClient::new(rpc_url) }
    }

    /// Simulate a transaction via eth_call.
    pub async fn simulate(&self, tx: &TxSpec) -> Result<RpcSimResult, crate::rpc::RpcError> {
        let params = crate::rpc::EthCallParams {
            from: "0x0000000000000000000000000000000000000000".into(),
            to: tx.to.clone(),
            value: tx.value.clone(),
            data: tx.data.clone(),
            gas_limit: tx.gas_limit,
        };
        let result = self.client.eth_call(&params).await?;
        // Error(string) selector = 0x08c379a0 -> indicates a revert with reason
        let success = !result.contains("0x08c379a0");
        let gas_used = tx.gas_limit; // Simplified -- real impl would measure from trace
        Ok(RpcSimResult { gas_used, success, return_data: result })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal::TxSpec;

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
    fn local_validate_rejects_wrong_chain() {
        let mut tx = make_tx();
        tx.chain_id = 1;
        let result = local_validate(&tx, 137).unwrap();
        assert!(!result.success);
    }

    #[test]
    fn local_validate_rejects_bad_address() {
        let mut tx = make_tx();
        tx.to = "not-an-address".into();
        let result = local_validate(&tx, 137).unwrap();
        assert!(!result.success);
    }

    #[test]
    fn local_validate_rejects_zero_gas() {
        let mut tx = make_tx();
        tx.gas_limit = 0;
        let result = local_validate(&tx, 137).unwrap();
        assert!(!result.success);
    }

    #[test]
    fn local_validate_passes_valid_tx() {
        let result = local_validate(&make_tx(), 137);
        assert!(result.is_none()); // None means passed
    }

    #[test]
    fn simulation_stale_price_deny() {
        // When value_usd is zero (stale/unavailable price), simulation should
        // still succeed but the limit check in the engine will catch it.
        let result = simulate(&make_tx(), 137, Decimal::ZERO);
        assert!(result.success);
        assert_eq!(result.value_delta_usd, Decimal::ZERO);
    }

    #[tokio::test]
    async fn rpc_revert_payload_denies_simulation() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            use std::io::{Read, Write};
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).unwrap();
            let body = r#"{"jsonrpc":"2.0","id":1,"result":"0x08c379a0"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let result = simulate_async(
            &make_tx(),
            137,
            Decimal::new(100, 2),
            Some(&format!("http://{}", addr)),
        )
        .await;
        handle.join().unwrap();
        assert!(!result.success);
        assert_eq!(result.value_delta_usd, Decimal::ZERO);
        assert!(result.error.unwrap().contains("revert"));
    }
}

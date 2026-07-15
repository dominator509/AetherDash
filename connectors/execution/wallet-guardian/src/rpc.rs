//! JSON-RPC transport for chain interactions.
//! Used by simulation (eth_call) and broadcast (eth_sendRawTransaction).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RpcError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON-RPC error {code}: {message}")]
    RpcError { code: i64, message: String },
    #[error("parse error: {0}")]
    Parse(String),
}

/// A JSON-RPC request.
#[derive(Debug, Serialize)]
struct RpcRequest {
    jsonrpc: &'static str,
    method: String,
    params: Vec<Value>,
    id: u64,
}

/// A JSON-RPC response.
#[derive(Debug, Deserialize)]
struct RpcResponse {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcErrorBody>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    code: i64,
    message: String,
}

/// JSON-RPC client for a single chain endpoint.
pub struct RpcClient {
    url: String,
    http: reqwest::Client,
}

impl RpcClient {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into(), http: reqwest::Client::new() }
    }

    /// Create from environment: reads AETHER_GUARDIAN__RPC_{CHAIN_ID}
    pub fn for_chain(chain_id: u64) -> Self {
        let var = format!("AETHER_GUARDIAN__RPC_{}", chain_id);
        let url = std::env::var(&var).unwrap_or_else(|_| "http://127.0.0.1:8545".to_string());
        Self::new(url)
    }

    /// Perform an eth_call.
    pub async fn eth_call(&self, tx: &EthCallParams) -> Result<String, RpcError> {
        let tx_obj = serde_json::json!({
            "from": tx.from,
            "to": tx.to,
            "value": tx.value,
            "data": tx.data,
            "gas": format!("0x{:x}", tx.gas_limit),
        });
        let params = vec![tx_obj, serde_json::json!("latest")];
        let result = self.call("eth_call", params).await?;
        result
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| RpcError::Parse("eth_call returned non-string".into()))
    }

    /// Call eth_sendRawTransaction.
    pub async fn eth_send_raw_transaction(&self, raw_tx: &str) -> Result<String, RpcError> {
        let params = vec![serde_json::json!(raw_tx)];
        let result = self.call("eth_sendRawTransaction", params).await?;
        result
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| RpcError::Parse("sendRawTransaction returned non-string".into()))
    }

    /// Call eth_getTransactionCount.
    pub async fn eth_get_transaction_count(&self, address: &str) -> Result<u64, RpcError> {
        let params = vec![serde_json::json!(address), serde_json::json!("latest")];
        let result = self.call("eth_getTransactionCount", params).await?;
        let count = result.as_str().unwrap_or("0x0");
        u64::from_str_radix(count.trim_start_matches("0x"), 16)
            .map_err(|e| RpcError::Parse(format!("invalid nonce: {e}")))
    }

    /// Call eth_gasPrice.
    pub async fn eth_gas_price(&self) -> Result<u64, RpcError> {
        let result = self.call("eth_gasPrice", vec![]).await?;
        let price = result.as_str().unwrap_or("0x0");
        u64::from_str_radix(price.trim_start_matches("0x"), 16)
            .map_err(|e| RpcError::Parse(format!("invalid gas price: {e}")))
    }

    async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value, RpcError> {
        let req = RpcRequest { jsonrpc: "2.0", method: method.into(), params, id: 1 };
        let resp = self.http.post(&self.url).json(&req).send().await?;
        let body: RpcResponse = resp.json().await?;
        if let Some(err) = body.error {
            return Err(RpcError::RpcError { code: err.code, message: err.message });
        }
        Ok(body.result.unwrap_or(Value::Null))
    }
}

/// Parameters for an eth_call.
#[derive(Debug, Clone)]
pub struct EthCallParams {
    pub from: String,
    pub to: String,
    pub value: String,
    pub data: String,
    pub gas_limit: u64,
}

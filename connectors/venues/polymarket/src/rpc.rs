//! Polygon RPC client for Polymarket on-chain resolution reads.
//!
//! Queries the Conditional Token Framework (CTF) contract on Polygon
//! to determine market resolution status.  Read-only; no wallet or
//! signing dependencies.

#![allow(dead_code)]
//!
//! # Environment variables
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__POLYGON_RPC_URL` | Polygon RPC endpoint (default: <https://polygon-rpc.com>) |
//!
//! # ABI note
//!
//! Function selectors match the public getters in the deployed Gnosis CTF ABI.

use std::time::Duration;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Gnosis Conditional Token Framework contract address on Polygon (chain ID 137).
const CTF_CONTRACT: &str = "0x4D97DCd97eC945f40cF65F87097ACe5EA0476045";

/// Default Polygon RPC endpoint.
const DEFAULT_RPC_URL: &str = "https://polygon-rpc.com";

/// RPC call timeout.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// ABI selector: keccak256("payoutNumerators(bytes32,uint256)")[0..4].
///
const PAYOUT_NUMERATORS_SEL: &str = "0504c814";

/// ABI selector: keccak256("payoutDenominator(bytes32)")[0..4].
///
const PAYOUT_DENOMINATOR_SEL: &str = "dd34de67";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Whether a Polymarket condition has resolved on-chain.
///
/// The CTF contract stores a payout numerator per outcome index.  If all
/// numerators are zero the condition is unresolved; otherwise the index
/// with the winning numerator (equal to the denominator) determines the
/// outcome.
///
/// For binary (Yes/No) markets outcome index 0 = Yes, index 1 = No.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolutionStatus {
    /// Not yet resolved (payout vector is all zeros).
    Unresolved,
    /// Resolved with a specific outcome index (0 = first outcome won, etc.).
    Resolved { outcome_index: Option<u8> },
    /// Resolution check failed (RPC error, contract revert, etc.).
    Unknown,
}

/// Errors from the Polygon JSON-RPC client.
#[derive(Error, Debug)]
pub enum RpcError {
    /// HTTP transport failure (timeout, connection refused, DNS, etc.).
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON-RPC protocol-level error returned by the node.
    #[error("JSON-RPC error ({code}): {message}")]
    JsonRpc {
        /// Error code as defined by the JSON-RPC spec or the node.
        code: i64,
        /// Human-readable error message.
        message: String,
    },

    /// Response payload could not be parsed.
    #[error("invalid response: {0}")]
    InvalidResponse(String),

    /// The `0x`-prefixed hex string received from eth_call is malformed.
    #[error("malformed hex in RPC response: {0}")]
    MalformedHex(String),
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Lightweight JSON-RPC client for the Polygon chain.
///
/// Communicates with a standard Ethereum JSON-RPC endpoint to call view
/// functions on the CTF contract.  No wallet, signing, or transaction
/// submission capabilities.
pub struct RpcClient {
    /// Polygon RPC endpoint URL.
    rpc_url: String,
    /// Shared HTTP client with timeout.
    http: reqwest::Client,
}

impl RpcClient {
    /// Create a new client pointing at the given RPC endpoint.
    ///
    /// `rpc_url` should be a full URL such as `"https://polygon-rpc.com"`.
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            http: {
                #[allow(clippy::expect_used)]
                reqwest::Client::builder()
                    .timeout(RPC_TIMEOUT)
                    .build()
                    .expect("reqwest Client builder should succeed")
            },
        }
    }

    /// Create a new client from the environment.
    ///
    /// Reads `AETHER_VENUE__POLYGON_RPC_URL` or falls back to
    /// `https://polygon-rpc.com`.
    pub fn from_env() -> Self {
        let rpc_url = std::env::var("AETHER_VENUE__POLYGON_RPC_URL")
            .unwrap_or_else(|_| DEFAULT_RPC_URL.to_string());
        Self::new(rpc_url)
    }

    /// Check the CTF denominator and every outcome numerator.
    pub async fn check_resolution(
        &self,
        condition_id: &str,
        outcome_count: usize,
    ) -> Result<ResolutionStatus, RpcError> {
        if !(2..=256).contains(&outcome_count) {
            return Err(RpcError::InvalidResponse(format!(
                "outcome count must be in 2..=256, got {outcome_count}"
            )));
        }
        let denominator = self.get_payout_denominator(condition_id).await?;
        if denominator == 0 {
            return Ok(ResolutionStatus::Unresolved);
        }

        let mut winner = None;
        for index in 0..outcome_count {
            let data = encode_payout_numerators_call(condition_id, index as u64)?;
            let hex = self.eth_call(CTF_CONTRACT, &data).await?;
            let raw = hex.strip_prefix("0x").unwrap_or(&hex);
            let numerator = parse_uint256(raw).map_err(|error| {
                RpcError::MalformedHex(format!("payoutNumerators[{index}] returned {raw}: {error}"))
            })?;
            if numerator == denominator {
                winner = Some(index as u8);
            }
        }
        Ok(ResolutionStatus::Resolved { outcome_index: winner })
    }

    /// Get the payout denominator for a condition.
    ///
    /// Calls `payoutDenominator(bytes32 conditionId)` on the CTF contract
    /// and returns the scaling factor used for payout numerators.
    ///
    /// A condition is fully resolved when one outcome index has a numerator
    /// equal to the denominator and all others have zero.
    pub async fn get_payout_denominator(&self, condition_id: &str) -> Result<u64, RpcError> {
        let data = encode_payout_denominator_call(condition_id)?;
        let hex = self.eth_call(CTF_CONTRACT, &data).await?;

        let raw = hex.strip_prefix("0x").unwrap_or(&hex);
        if raw.is_empty() {
            return Err(RpcError::InvalidResponse("empty response from payoutDenominator".into()));
        }

        parse_uint256(raw)
            .map_err(|e| RpcError::MalformedHex(format!("payoutDenominator returned {raw}: {e}")))
    }

    // -----------------------------------------------------------------------
    // JSON-RPC helpers
    // -----------------------------------------------------------------------

    /// Perform a raw `eth_call` via JSON-RPC.
    ///
    /// Sends `{"jsonrpc":"2.0","method":"eth_call","params":[{"to":...,"data":...},"latest"],"id":1}`
    /// and returns the `result` field as a hex string (with `0x` prefix).
    async fn eth_call(&self, to: &str, data: &str) -> Result<String, RpcError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [
                {"to": to, "data": data},
                "latest"
            ],
            "id": 1
        });

        let resp: serde_json::Value =
            self.http.post(&self.rpc_url).json(&body).send().await?.json().await?;

        // JSON-RPC error object
        if let Some(err) = resp.get("error") {
            return Err(RpcError::JsonRpc {
                code: err["code"].as_i64().unwrap_or(-1),
                message: err["message"].as_str().unwrap_or("unknown").to_string(),
            });
        }

        resp["result"].as_str().map(|s| s.to_string()).ok_or_else(|| {
            RpcError::InvalidResponse("eth_call response missing 'result' field".into())
        })
    }
}

// ---------------------------------------------------------------------------
// ABI encoding helpers
// ---------------------------------------------------------------------------

/// ABI-encode a call to `payoutNumerators(bytes32 conditionId, uint256 index)`.
///
/// Layout:
/// ```text
/// | selector (4 bytes) | conditionId (32 bytes, zero-left-padded) | index (32 bytes, zero-left-padded) |
/// ```
/// Output is a hex string with `0x` prefix, ready to use as the `data` field
/// of an `eth_call`.
fn encode_payout_numerators_call(condition_id: &str, index: u64) -> Result<String, RpcError> {
    let cond = validate_condition_id(condition_id)?;
    Ok(format!("0x{sel}{cond}{idx:064x}", sel = PAYOUT_NUMERATORS_SEL, cond = cond, idx = index))
}

/// ABI-encode a call to `payoutDenominator(bytes32 conditionId)`.
///
/// Layout:
/// ```text
/// | selector (4 bytes) | conditionId (32 bytes, zero-left-padded) |
/// ```
fn encode_payout_denominator_call(condition_id: &str) -> Result<String, RpcError> {
    let cond = validate_condition_id(condition_id)?;
    Ok(format!("0x{sel}{cond}", sel = PAYOUT_DENOMINATOR_SEL, cond = cond))
}

/// Strip `0x` (or `0X`) prefix from a hex string if present.
fn strip_hex(s: &str) -> &str {
    s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s)
}

fn validate_condition_id(condition_id: &str) -> Result<&str, RpcError> {
    let condition_id = strip_hex(condition_id);
    if condition_id.len() != 64 || !condition_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RpcError::MalformedHex(
            "condition_id must be exactly 32 bytes of hexadecimal".into(),
        ));
    }
    Ok(condition_id)
}

/// Parse an Ethereum `uint256` (up to 78 hex digits) into a `u64`.
///
/// Returns an error if the hex string is empty, exceeds `u64::MAX`, or
/// contains invalid characters.
fn parse_uint256(hex: &str) -> Result<u64, String> {
    if hex.is_empty() {
        return Err("empty hex string".into());
    }
    // Strip leading zeros but keep at least one character.
    let trimmed = hex.trim_start_matches('0');
    let significant = if trimmed.is_empty() { "0" } else { trimmed };

    // u64::MAX = 0xffffffffffffffff (16 hex digits)
    if significant.len() > 16 {
        return Err(format!("value 0x{hex} exceeds u64::MAX (0xffffffffffffffff)"));
    }

    u64::from_str_radix(significant, 16).map_err(|e| format!("{e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn resolution_reads_denominator_and_every_outcome() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0_u8; 8192];
                let count = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..count]).into_owned();
                let value = if request.contains(PAYOUT_DENOMINATOR_SEL)
                    || request.contains(&format!("{PAYOUT_NUMERATORS_SEL}{}", "11".repeat(32)))
                        && request.contains(&format!("{:064x}", 1_u64))
                {
                    1_u64
                } else {
                    0_u64
                };
                let body = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": format!("0x{value:064x}")
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                requests.push(request);
            }
            requests
        });

        let client = RpcClient::new(format!("http://{address}"));
        let status = client.check_resolution(&"11".repeat(32), 2).await.unwrap();
        assert_eq!(status, ResolutionStatus::Resolved { outcome_index: Some(1) });
        let requests = server.await.unwrap();
        assert_eq!(
            requests.iter().filter(|request| request.contains(PAYOUT_NUMERATORS_SEL)).count(),
            2
        );
        assert!(requests.iter().any(|request| request.contains(PAYOUT_DENOMINATOR_SEL)));
    }

    // -----------------------------------------------------------------------
    // ABI encoding tests
    // -----------------------------------------------------------------------

    #[test]
    fn encode_numerators_call_without_0x_prefix() {
        // condition_id without 0x, 64 hex chars
        let cid = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let data = encode_payout_numerators_call(cid, 0).unwrap();
        assert!(data.starts_with("0x"), "should have 0x prefix");
        // selector = 8 hex chars, conditionId = 64, index = 64  => total hex = 8 + 64 + 64 = 136
        assert_eq!(data.len(), 2 + 8 + 64 + 64, "selector + cond + index");
        assert!(data.contains(PAYOUT_NUMERATORS_SEL), "should contain selector");
    }

    #[test]
    fn encode_numerators_call_with_0x_prefix() {
        let cid = "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let with = encode_payout_numerators_call(cid, 1).unwrap();
        let without = encode_payout_numerators_call(&cid[2..], 1).unwrap();
        assert_eq!(with, without, "0x prefix should be stripped");
    }

    #[test]
    fn encode_numerators_call_index_one() {
        let cid = "aa".repeat(32); // 64 hex chars
        let data = encode_payout_numerators_call(&cid, 1).unwrap();
        // trailing 64 hex chars should encode u64::from(1)
        assert!(
            data.ends_with("0000000000000000000000000000000000000000000000000000000000000001"),
            "index=1 should be right-padded as 0000...0001"
        );
    }

    #[test]
    fn encode_numerators_call_index_max() {
        let cid = "bb".repeat(32);
        let data = encode_payout_numerators_call(&cid, u64::MAX).unwrap();
        // {idx:064x} produces 64 hex chars: 48 zeros + 16 f's for u64::MAX
        assert!(
            data.ends_with("000000000000000000000000000000000000000000000000ffffffffffffffff"),
            "index=u64::MAX should encode correctly"
        );
    }

    #[test]
    fn encode_denominator_call() {
        let cid = "cc".repeat(32);
        let data = encode_payout_denominator_call(&cid).unwrap();
        assert!(data.starts_with("0x"), "should have 0x prefix");
        // selector = 8, conditionId = 64 => total hex = 8 + 64 = 72
        assert_eq!(data.len(), 2 + 8 + 64, "selector + cond");
        assert!(data.contains(PAYOUT_DENOMINATOR_SEL), "should contain denominator selector");
    }

    // -----------------------------------------------------------------------
    // Hex parsing tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_zero() {
        assert_eq!(parse_uint256("0").unwrap(), 0);
        assert_eq!(parse_uint256("00").unwrap(), 0);
        assert_eq!(
            parse_uint256("0000000000000000000000000000000000000000000000000000000000000000")
                .unwrap(),
            0
        );
    }

    #[test]
    fn parse_one() {
        assert_eq!(parse_uint256("1").unwrap(), 1);
        assert_eq!(parse_uint256("01").unwrap(), 1);
    }

    #[test]
    fn parse_u64_max() {
        assert_eq!(parse_uint256("ffffffffffffffff").unwrap(), u64::MAX);
    }

    #[test]
    fn parse_exceeds_u64() {
        // 0x1_00000000000000 = 2^64, one more than u64::MAX
        let err = parse_uint256("10000000000000000").unwrap_err();
        assert!(err.contains("u64::MAX"), "error should mention overflow");
    }

    #[test]
    fn parse_empty_returns_err() {
        assert!(parse_uint256("").is_err());
    }

    #[test]
    fn parse_invalid_hex() {
        assert!(parse_uint256("xyz").is_err());
    }

    // -----------------------------------------------------------------------
    // Resolution status construction
    // -----------------------------------------------------------------------

    #[test]
    fn resolution_unresolved() {
        let s = ResolutionStatus::Unresolved;
        assert_eq!(s, ResolutionStatus::Unresolved);
    }

    #[test]
    fn resolution_resolved() {
        let s = ResolutionStatus::Resolved { outcome_index: Some(0) };
        assert_eq!(s, ResolutionStatus::Resolved { outcome_index: Some(0) });
        assert_ne!(s, ResolutionStatus::Unresolved);
    }

    #[test]
    fn resolution_unknown() {
        let s = ResolutionStatus::Unknown;
        assert_eq!(s, ResolutionStatus::Unknown);
    }

    // -----------------------------------------------------------------------
    // RPC client construction
    // -----------------------------------------------------------------------

    #[test]
    fn client_default_url_when_env_unset() {
        // When AETHER_VENUE__POLYGON_RPC_URL is not set, from_env() should
        // fall back to the default.
        let client = RpcClient::from_env();
        // from_env reads the actual environment — on CI and dev machines
        // without the var this will be the default; if the var _is_ set
        // the assertion still holds because the test runner's env is the
        // source of truth.
        let expected = std::env::var("AETHER_VENUE__POLYGON_RPC_URL")
            .unwrap_or_else(|_| DEFAULT_RPC_URL.to_string());
        assert_eq!(client.rpc_url, expected);
    }

    #[test]
    fn client_custom_url() {
        let client = RpcClient::new("http://localhost:8545");
        assert_eq!(client.rpc_url, "http://localhost:8545");
    }
}

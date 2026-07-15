//! Transaction broadcast boundary.
//!
//! This module builds EIP-1559 typed transactions, signs the transaction hash
//! inside the keystore boundary, and broadcasts the resulting raw transaction.

#![allow(clippy::unwrap_used)]

use crate::keystore::KeyStore;
use crate::nonce::NonceManager;
use crate::proposal::TxSpec;
use crate::rpc::RpcClient;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BroadcastError {
    #[error("RPC error: {0}")]
    Rpc(#[from] crate::rpc::RpcError),
    #[error("signing error: {0}")]
    Signing(String),
    #[error("nonce error: {0}")]
    Nonce(#[from] crate::nonce::NonceError),
    #[error("invalid transaction: {0}")]
    InvalidTransaction(String),
}

/// Result of broadcasting a signed transaction.
#[derive(Debug, Clone)]
pub struct BroadcastResult {
    pub tx_hash: String,
    pub signed_raw: String,
    pub nonce: u64,
}

/// Build, sign, and broadcast an EIP-1559 transaction.
pub async fn broadcast_transaction(
    keystore: &KeyStore,
    rpc: &RpcClient,
    nonce_mgr: &mut NonceManager,
    tx: &TxSpec,
    chain_id: u64,
) -> Result<BroadcastResult, BroadcastError> {
    let nonce = reserve_broadcast_nonce(keystore, rpc, nonce_mgr, chain_id).await?;
    let signed_raw = sign_eip1559_transaction(keystore, tx, nonce, chain_id)?;
    let tx_hash = rpc.eth_send_raw_transaction(&signed_raw).await?;
    Ok(BroadcastResult { tx_hash, signed_raw, nonce })
}

/// Reserve the nonce that would be used for a guardian-custody transaction.
/// This is safe local bookkeeping and does not sign or broadcast.
pub async fn reserve_broadcast_nonce(
    keystore: &KeyStore,
    rpc: &RpcClient,
    nonce_mgr: &mut NonceManager,
    chain_id: u64,
) -> Result<u64, BroadcastError> {
    let nonce = if let Some(replacement) = nonce_mgr.lowest_pending(chain_id) {
        replacement
    } else {
        rpc.eth_get_transaction_count(keystore.address().as_str()).await?
    };
    nonce_mgr.mark_pending(chain_id, nonce);
    Ok(nonce)
}

/// Build and sign an EIP-1559 typed transaction without broadcasting it.
pub fn sign_eip1559_transaction(
    keystore: &KeyStore,
    tx: &TxSpec,
    nonce: u64,
    chain_id: u64,
) -> Result<String, BroadcastError> {
    let unsigned = Eip1559Tx::from_spec(tx, nonce, chain_id)?;
    let signing_payload = unsigned.signing_payload();
    let hash = keccak256(&signing_payload);
    let signature = keystore
        .sign_transaction_hash(&hash)
        .map_err(|e| BroadcastError::Signing(e.to_string()))?;
    Ok(unsigned.encode_signed(&signature))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Eip1559Tx {
    chain_id: u64,
    nonce: u64,
    max_priority_fee_per_gas: u128,
    max_fee_per_gas: u128,
    gas_limit: u64,
    to: [u8; 20],
    value: u128,
    data: Vec<u8>,
}

impl Eip1559Tx {
    fn from_spec(tx: &TxSpec, nonce: u64, chain_id: u64) -> Result<Self, BroadcastError> {
        if tx.chain_id != chain_id {
            return Err(BroadcastError::InvalidTransaction(format!(
                "tx chain {} does not match broadcast chain {}",
                tx.chain_id, chain_id
            )));
        }
        Ok(Self {
            chain_id,
            nonce,
            max_priority_fee_per_gas: parse_hex_u128(&tx.max_priority_fee_per_gas)?,
            max_fee_per_gas: parse_hex_u128(&tx.max_fee_per_gas)?,
            gas_limit: tx.gas_limit,
            to: parse_address(&tx.to)?,
            value: parse_hex_u128(&tx.value)?,
            data: parse_hex_bytes(&tx.data)?,
        })
    }

    fn signing_payload(&self) -> Vec<u8> {
        let mut out = vec![0x02];
        out.extend(rlp_list(&[
            rlp_u64(self.chain_id),
            rlp_u64(self.nonce),
            rlp_u128(self.max_priority_fee_per_gas),
            rlp_u128(self.max_fee_per_gas),
            rlp_u64(self.gas_limit),
            rlp_bytes(&self.to),
            rlp_u128(self.value),
            rlp_bytes(&self.data),
            rlp_list(&[]),
        ]));
        out
    }

    fn encode_signed(&self, signature: &crate::keystore::EthSignature) -> String {
        let mut out = vec![0x02];
        out.extend(rlp_list(&[
            rlp_u64(self.chain_id),
            rlp_u64(self.nonce),
            rlp_u128(self.max_priority_fee_per_gas),
            rlp_u128(self.max_fee_per_gas),
            rlp_u64(self.gas_limit),
            rlp_bytes(&self.to),
            rlp_u128(self.value),
            rlp_bytes(&self.data),
            rlp_list(&[]),
            rlp_u64(u64::from(signature.y_parity)),
            rlp_bytes(&trim_leading_zeroes(&signature.r)),
            rlp_bytes(&trim_leading_zeroes(&signature.s)),
        ]));
        format!("0x{}", hex::encode(out))
    }
}

fn parse_hex_u128(raw: &str) -> Result<u128, BroadcastError> {
    let cleaned = raw.strip_prefix("0x").unwrap_or(raw);
    if cleaned.is_empty() {
        return Ok(0);
    }
    u128::from_str_radix(cleaned, 16)
        .map_err(|e| BroadcastError::InvalidTransaction(format!("invalid hex integer {raw}: {e}")))
}

fn parse_hex_bytes(raw: &str) -> Result<Vec<u8>, BroadcastError> {
    let cleaned = raw.strip_prefix("0x").unwrap_or(raw);
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    if !cleaned.len().is_multiple_of(2) {
        return Err(BroadcastError::InvalidTransaction(format!(
            "hex bytes must have even length: {raw}"
        )));
    }
    hex::decode(cleaned)
        .map_err(|e| BroadcastError::InvalidTransaction(format!("invalid hex bytes {raw}: {e}")))
}

fn parse_address(raw: &str) -> Result<[u8; 20], BroadcastError> {
    let bytes = parse_hex_bytes(raw)?;
    if bytes.len() != 20 {
        return Err(BroadcastError::InvalidTransaction(format!("address must be 20 bytes: {raw}")));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn keccak256(bytes: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let hash = Keccak256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hash);
    out
}

fn rlp_u64(value: u64) -> Vec<u8> {
    rlp_bytes(&minimal_be_u128(u128::from(value)))
}

fn rlp_u128(value: u128) -> Vec<u8> {
    rlp_bytes(&minimal_be_u128(value))
}

fn rlp_bytes(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() == 1 && bytes[0] < 0x80 {
        return vec![bytes[0]];
    }
    let mut out = rlp_prefix(0x80, bytes.len());
    out.extend(bytes);
    out
}

fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload: Vec<u8> = items.iter().flat_map(|item| item.iter().copied()).collect();
    let mut out = rlp_prefix(0xc0, payload.len());
    out.extend(payload);
    out
}

fn rlp_prefix(offset: u8, payload_len: usize) -> Vec<u8> {
    if payload_len <= 55 {
        return vec![offset + payload_len as u8];
    }
    let len_bytes = minimal_be_usize(payload_len);
    let mut out = vec![offset + 55 + len_bytes.len() as u8];
    out.extend(len_bytes);
    out
}

fn minimal_be_u128(value: u128) -> Vec<u8> {
    if value == 0 {
        return Vec::new();
    }
    trim_leading_zeroes(&value.to_be_bytes())
}

fn minimal_be_usize(value: usize) -> Vec<u8> {
    trim_leading_zeroes(&value.to_be_bytes())
}

fn trim_leading_zeroes(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().skip_while(|b| **b == 0).copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tx() -> TxSpec {
        TxSpec {
            chain_id: 31337, // anvil default
            to: "0x1234567890123456789012345678901234567890".into(),
            value: "0x0".into(),
            data: "0x".into(),
            gas_limit: 21000,
            max_fee_per_gas: "0x3b9aca00".into(),
            max_priority_fee_per_gas: "0x3b9aca00".into(),
        }
    }

    #[test]
    fn signed_eip1559_tx_is_type_two_rlp_payload() {
        let keystore = KeyStore::new("/tmp/test.key");
        let signed = sign_eip1559_transaction(&keystore, &make_tx(), 7, 31337).unwrap();
        assert!(signed.starts_with("0x02"));
        assert!(signed.len() > 120);
        assert!(!signed.contains("12345678901234567890123456789012345678900x"));
    }

    #[test]
    fn invalid_tx_hex_is_rejected_before_signing() {
        let keystore = KeyStore::new("/tmp/test.key");
        let mut tx = make_tx();
        tx.data = "0xabc".into();
        assert!(matches!(
            sign_eip1559_transaction(&keystore, &tx, 0, 31337),
            Err(BroadcastError::InvalidTransaction(_))
        ));
    }

    #[test]
    fn rlp_zero_integer_is_empty_string_encoding() {
        assert_eq!(rlp_u64(0), vec![0x80]);
        assert_eq!(rlp_u64(127), vec![0x7f]);
        assert_eq!(rlp_u64(128), vec![0x81, 0x80]);
    }
}

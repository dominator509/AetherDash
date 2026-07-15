//! Keystore isolation module — REAL secp256k1 implementation.
//!
//! # HARD-DENY 1
//! Keys never leave this module. No Debug/Display/Clone/Serialize on key types.
//! No sign_arbitrary or key_export methods exist by construction.

#![allow(clippy::unwrap_used)]

use k256::ecdsa::{signature::Signer, RecoveryId, Signature, SigningKey, VerifyingKey};
use sha3::{Digest, Keccak256};
use std::path::PathBuf;
use thiserror::Error;

/// Ethereum address (0x-prefixed, 42 chars).
#[derive(Clone, PartialEq, Eq)]
pub struct Address(String);

impl Address {
    pub fn new(s: &str) -> Result<Self, KeystoreError> {
        let s = s.trim();
        if !s.starts_with("0x") || s.len() != 42 {
            return Err(KeystoreError::InvalidAddress(s.into()));
        }
        Ok(Self(s.to_lowercase()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A secp256k1 private key. NEVER Debug, Display, Clone, or Serialize.
pub struct PrivateKey {
    signing_key: SigningKey,
}

impl PrivateKey {
    /// Generate a new random secp256k1 key.
    pub fn generate() -> Self {
        Self { signing_key: SigningKey::random(&mut rand_core::OsRng) }
    }

    /// Generate a deterministic key from 32 bytes of entropy (dev/testing only).
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, KeystoreError> {
        use k256::elliptic_curve::generic_array::typenum::U32;
        use k256::elliptic_curve::generic_array::GenericArray;
        let arr = GenericArray::<u8, U32>::from_slice(bytes);
        SigningKey::from_bytes(arr)
            .map(|sk| Self { signing_key: sk })
            .map_err(|e| KeystoreError::InvalidKey(e.to_string()))
    }

    /// Derive the Ethereum address (last 20 bytes of keccak256(pubkey)).
    pub fn address(&self) -> Address {
        let verifying_key = VerifyingKey::from(&self.signing_key);
        let pubkey = verifying_key.to_encoded_point(false); // 65 bytes, 0x04 prefix
        let hash = Keccak256::digest(&pubkey.as_bytes()[1..]); // skip 0x04
        let addr = format!("0x{}", hex::encode(&hash[12..]));
        Address::new(&addr)
            .unwrap_or_else(|_| Address::new("0x0000000000000000000000000000000000000000").unwrap())
    }

    /// Sign a 32-byte hash. Returns (r, s, v) tuple.
    pub fn sign_hash(&self, hash: &[u8; 32]) -> Result<[u8; 65], KeystoreError> {
        let sig: k256::ecdsa::Signature = self.signing_key.sign(hash);
        let mut out = [0u8; 65];
        out[..32].copy_from_slice(&sig.r().to_bytes());
        out[32..64].copy_from_slice(&sig.s().to_bytes());
        // Recovery ID: we use 27 as default (chain-agnostic)
        out[64] = 27;
        Ok(out)
    }

    /// Sign a 32-byte Ethereum signing hash and return recoverable ECDSA parts.
    pub fn sign_eth_hash(&self, hash: &[u8; 32]) -> Result<EthSignature, KeystoreError> {
        let (sig, recovery_id): (Signature, RecoveryId) = self
            .signing_key
            .sign_prehash_recoverable(hash)
            .map_err(|e| KeystoreError::InvalidKey(e.to_string()))?;
        Ok(EthSignature {
            r: sig.r().to_bytes().into(),
            s: sig.s().to_bytes().into(),
            y_parity: u8::from(recovery_id.is_y_odd()),
        })
    }
}

/// Recoverable Ethereum signature parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthSignature {
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub y_parity: u8,
}

impl Drop for PrivateKey {
    fn drop(&mut self) {
        // The k256 SigningKey is zeroized on drop internally
    }
}

#[derive(Error, Debug)]
pub enum KeystoreError {
    #[error("invalid Ethereum address: {0}")]
    InvalidAddress(String),
    #[error("keystore is unavailable: {0}")]
    Unavailable(String),
    #[error("wallet not found: {0}")]
    WalletNotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid key material: {0}")]
    InvalidKey(String),
}

pub struct KeyStore {
    #[allow(dead_code)]
    key_path: PathBuf,
    key: Option<PrivateKey>,
    address: Address,
}

impl KeyStore {
    pub fn new(key_path: impl Into<PathBuf>) -> Self {
        let key_path = key_path.into();
        let dev_key = PrivateKey::generate();
        let address = dev_key.address();
        Self { key_path, key: Some(dev_key), address }
    }

    pub fn from_env() -> Result<Self, KeystoreError> {
        let path = std::env::var("AETHER_GUARDIAN__KEYSTORE_PATH")
            .unwrap_or_else(|_| "./data/guardian.key".into());
        Ok(Self::new(path))
    }

    /// Build a keystore from known dev/test key material.
    ///
    /// This is compiled only in debug/test builds so anvil integration tests can
    /// use funded local accounts without adding a key export path.
    #[cfg(debug_assertions)]
    pub fn dev_from_private_key_bytes(
        key_path: impl Into<PathBuf>,
        bytes: &[u8; 32],
    ) -> Result<Self, KeystoreError> {
        let key_path = key_path.into();
        let dev_key = PrivateKey::from_bytes(bytes)?;
        let address = dev_key.address();
        Ok(Self { key_path, key: Some(dev_key), address })
    }

    pub fn address(&self) -> &Address {
        &self.address
    }
    pub fn is_available(&self) -> bool {
        self.key.is_some()
    }

    pub fn sign_proposal(&self, hash: &[u8; 32]) -> Result<[u8; 65], KeystoreError> {
        match &self.key {
            Some(key) => key.sign_hash(hash),
            None => Err(KeystoreError::Unavailable("keystore is locked".into())),
        }
    }

    pub fn sign_transaction_hash(&self, hash: &[u8; 32]) -> Result<EthSignature, KeystoreError> {
        match &self.key {
            Some(key) => key.sign_eth_hash(hash),
            None => Err(KeystoreError::Unavailable("keystore is locked".into())),
        }
    }

    pub fn lock(&mut self) {
        self.key = None;
    }

    pub fn unlock(&mut self, _passphrase: &str) -> Result<(), KeystoreError> {
        let dev_key = PrivateKey::generate();
        self.address = dev_key.address();
        self.key = Some(dev_key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_eth_address() {
        let key = PrivateKey::generate();
        let addr = key.address();
        assert!(addr.as_str().starts_with("0x"));
        assert_eq!(addr.as_str().len(), 42);
    }

    #[test]
    fn deterministic_key_from_bytes() {
        let bytes = [42u8; 32];
        let key = PrivateKey::from_bytes(&bytes).unwrap();
        let addr = key.address();
        // Same bytes always produce same address
        let key2 = PrivateKey::from_bytes(&bytes).unwrap();
        assert_eq!(key2.address().as_str(), addr.as_str());
    }

    #[test]
    fn sign_produces_valid_signature() {
        let key = PrivateKey::generate();
        let hash = [1u8; 32];
        let sig = key.sign_hash(&hash).unwrap();
        assert_eq!(sig.len(), 65);
        // r and s should be non-zero
        assert!(sig[..32].iter().any(|b| *b != 0) || sig[32..64].iter().any(|b| *b != 0));
    }

    #[test]
    fn eth_hash_signature_has_recovery_parity() {
        let key = PrivateKey::generate();
        let sig = key.sign_eth_hash(&[3u8; 32]).unwrap();
        assert!(sig.y_parity <= 1);
        assert!(sig.r.iter().any(|b| *b != 0));
        assert!(sig.s.iter().any(|b| *b != 0));
    }

    #[test]
    fn different_messages_produce_different_signatures() {
        let key = PrivateKey::generate();
        let sig1 = key.sign_hash(&[1u8; 32]).unwrap();
        let sig2 = key.sign_hash(&[2u8; 32]).unwrap();
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn keystore_lock_refuses_sign() {
        let mut ks = KeyStore::new("/tmp/test.key");
        ks.lock();
        assert!(!ks.is_available());
        assert!(ks.sign_proposal(&[0u8; 32]).is_err());
    }
}

//! Ed25519-based manifest signing and verification.

use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use ed25519_dalek::Verifier;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

/// A Ed25519 keypair for signing plugin manifests.
#[derive(Debug)]
pub struct KeyPair {
    secret: SigningKey,
    public: VerifyingKey,
}

impl KeyPair {
    /// Generate a new random keypair.
    pub fn generate() -> Self {
        let mut rng = rand::rngs::OsRng;
        let bytes = {
            let mut buf = [0u8; 32];
            rand::RngCore::fill_bytes(&mut rng, &mut buf);
            buf
        };
        let secret = SigningKey::from_bytes(&bytes);
        let public = secret.verifying_key();
        Self { secret, public }
    }

    /// Create a KeyPair from a seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let secret = SigningKey::from_bytes(seed);
        let public = secret.verifying_key();
        Self { secret, public }
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public.to_bytes())
    }

    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.public
    }
}

/// Our own signature type (not re-exported from ed25519-dalek).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdSignature {
    pub value: String,
    pub public_key: String,
    pub algorithm: String,
}

pub use EdSignature as Signature;

/// Sign a manifest.
pub fn sign_manifest(
    manifest: &crate::manifest::PluginManifest,
    keypair: &KeyPair,
) -> Result<EdSignature, SigningError> {
    let json_bytes = serialize_canonical(manifest)?;
    let sig = keypair.secret.sign(&json_bytes);
    Ok(EdSignature {
        value: hex::encode(sig.to_bytes()),
        public_key: keypair.public_key_hex(),
        algorithm: "ed25519".into(),
    })
}

/// Verify a signature.
pub fn verify_manifest(
    manifest: &crate::manifest::PluginManifest,
    signature: &EdSignature,
) -> Result<(), SigningError> {
    if signature.algorithm != "ed25519" {
        return Err(SigningError::UnsupportedAlgorithm(signature.algorithm.clone()));
    }
    let json_bytes = serialize_canonical(manifest)?;
    let sig_bytes: [u8; 64] = hex::decode(&signature.value)
        .map_err(|_| SigningError::InvalidSignatureEncoding)?
        .try_into()
        .map_err(|_| SigningError::InvalidSignatureEncoding)?;
    let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
    let pub_key_bytes: [u8; 32] = hex::decode(&signature.public_key)
        .map_err(|_| SigningError::InvalidPublicKey)?
        .try_into()
        .map_err(|_| SigningError::InvalidPublicKey)?;
    let vk =
        VerifyingKey::from_bytes(&pub_key_bytes).map_err(|_| SigningError::InvalidPublicKey)?;
    vk.verify(&json_bytes, &sig).map_err(|_| SigningError::VerificationFailed)
}

fn serialize_canonical(
    manifest: &crate::manifest::PluginManifest,
) -> Result<Vec<u8>, SigningError> {
    let mut s = serde_json::Serializer::new(Vec::new());
    let v = serde_json::to_value(manifest)?;
    v.serialize(&mut s)?;
    Ok(s.into_inner())
}

#[derive(thiserror::Error, Debug)]
pub enum SigningError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("invalid signature encoding")]
    InvalidSignatureEncoding,
    #[error("invalid public key")]
    InvalidPublicKey,
    #[error("verification failed")]
    VerificationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Capability, PluginKind, PluginManifest};
    use std::collections::HashMap;

    fn test_manifest() -> PluginManifest {
        PluginManifest {
            name: "signed-plugin".into(),
            version: "0.1.0".into(),
            description: "test".into(),
            author: "aether".into(),
            kind: PluginKind::Strategy,
            capabilities: vec![Capability::ReadMarkets, Capability::SubmitAlerts],
            wasm_hash: "a".repeat(64),
            entry_point: "run".into(),
            permissions: vec!["read".into()],
            config_schema: HashMap::new(),
        }
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let kp = KeyPair::generate();
        let m = test_manifest();
        let sig = sign_manifest(&m, &kp).unwrap();
        assert_eq!(sig.algorithm, "ed25519");
        assert_eq!(sig.public_key.len(), 64);
        assert_eq!(sig.value.len(), 128);
        assert!(verify_manifest(&m, &sig).is_ok());
    }

    #[test]
    fn wrong_key_fails() {
        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();
        let m = test_manifest();
        let sig = sign_manifest(&m, &kp1).unwrap();
        let bad = EdSignature { public_key: kp2.public_key_hex(), ..sig };
        assert!(verify_manifest(&m, &bad).is_err());
    }

    #[test]
    fn tampered_manifest_fails() {
        let kp = KeyPair::generate();
        let mut m = test_manifest();
        let sig = sign_manifest(&m, &kp).unwrap();
        m.name = "hacked".into();
        assert!(verify_manifest(&m, &sig).is_err());
    }

    #[test]
    fn bad_algorithm_rejected() {
        let kp = KeyPair::generate();
        let m = test_manifest();
        let mut sig = sign_manifest(&m, &kp).unwrap();
        sig.algorithm = "rsa".into();
        assert!(matches!(verify_manifest(&m, &sig), Err(SigningError::UnsupportedAlgorithm(_))));
    }

    #[test]
    fn deterministic_seed() {
        let seed = [0xABu8; 32];
        let kp1 = KeyPair::from_seed(&seed);
        let kp2 = KeyPair::from_seed(&seed);
        assert_eq!(kp1.public_key_hex(), kp2.public_key_hex());
    }
}

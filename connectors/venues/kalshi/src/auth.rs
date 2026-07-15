//! Kalshi RSA request signing.
//!
//! Kalshi authenticates requests with RSA-PSS/SHA-256 signatures over
//! `timestamp + uppercase_method + path_without_query`.
//!
//! # Environment variables
//!
//! | Variable | Description |
//! |---|---|
//! | `AETHER_VENUE__KALSHI_KEY_ID` | The Kalshi API key ID (from member settings) |
//! | `AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH` | Path to PKCS#8 PEM private key file |

use ring::rand::SystemRandom;
use ring::signature::{RsaKeyPair, RSA_PSS_SHA256};
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during authentication setup or signing.
#[derive(Error, Debug)]
pub enum AuthError {
    /// Required environment variable is not set.
    #[error("missing environment variable: {0}")]
    MissingEnvVar(&'static str),

    /// Failed to read the private key file from disk.
    #[error("failed to read private key file at {path}: {source}")]
    KeyReadError {
        /// The path that was attempted.
        path: String,
        /// The underlying IO error.
        #[source]
        source: std::io::Error,
    },

    /// The PEM file could not be parsed.
    #[error("PEM parse error: {0}")]
    PemParse(String),

    /// The PEM file has an unexpected label (only "PRIVATE KEY" / PKCS#8 is supported).
    #[error("unexpected PEM label '{label}' -- expected 'PRIVATE KEY' (PKCS#8)")]
    UnexpectedLabel {
        /// The label found in the PEM file.
        label: String,
    },

    /// The DER bytes are not a valid PKCS#8 RSA private key.
    #[error("invalid RSA private key: {0}")]
    InvalidKey(String),

    /// Signing failed (should not happen with valid keys).
    #[error("signing failed: {0}")]
    SigningFailed(String),
}

/// Kalshi authentication handle.
///
/// Holds the key ID and an in-memory RSA key pair ready for signing.
/// Construct via [`KalshiAuth::from_env`].
pub struct KalshiAuth {
    key_id: String,
    key_pair: RsaKeyPair,
}

impl std::fmt::Debug for KalshiAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KalshiAuth").field("key_id", &self.key_id).finish_non_exhaustive()
    }
}

impl KalshiAuth {
    /// Load authentication from environment variables.
    ///
    /// Reads `AETHER_VENUE__KALSHI_KEY_ID` and
    /// `AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH`, then parses the PEM-encoded
    /// PKCS#8 RSA private key at the given path.
    pub fn from_env() -> Result<Self, AuthError> {
        let key_id = std::env::var("AETHER_VENUE__KALSHI_KEY_ID")
            .map_err(|_| AuthError::MissingEnvVar("AETHER_VENUE__KALSHI_KEY_ID"))?;

        let key_path = std::env::var("AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH")
            .map_err(|_| AuthError::MissingEnvVar("AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH"))?;

        let pem_bytes = fs::read(Path::new(&key_path))
            .map_err(|e| AuthError::KeyReadError { path: key_path.clone(), source: e })?;

        Self::from_pem_bytes(&key_id, &pem_bytes)
    }

    /// Parse a PEM-encoded PKCS#8 RSA private key and construct the auth handle.
    ///
    /// Available as a public constructor so tests can construct `KalshiAuth` without
    /// touching environment variables or the filesystem.
    pub fn from_pem_bytes(key_id: &str, pem_bytes: &[u8]) -> Result<Self, AuthError> {
        let parsed = pem::parse(pem_bytes).map_err(|e| AuthError::PemParse(e.to_string()))?;

        let label = parsed.tag();
        if label != "PRIVATE KEY" {
            return Err(AuthError::UnexpectedLabel { label: label.to_string() });
        }

        let key_pair = RsaKeyPair::from_pkcs8(parsed.contents())
            .map_err(|e| AuthError::InvalidKey(e.to_string()))?;

        Ok(Self { key_id: key_id.to_string(), key_pair })
    }

    /// Return the key ID used in the `KALSHI-ACCESS-KEY` header.
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Sign a request and return the Base64-encoded signature.
    ///
    /// The signature is computed over the concatenation
    /// `timestamp + uppercase_method + path_without_query`. Request bodies
    /// and query parameters are deliberately not part of Kalshi's signature.
    pub fn sign_request(
        &self,
        method: &str,
        path: &str,
        timestamp: &str,
    ) -> Result<String, AuthError> {
        let path_without_query = path.split('?').next().unwrap_or(path);
        let message = format!("{}{}{}", timestamp, method.to_ascii_uppercase(), path_without_query);
        let rng = SystemRandom::new();

        let mut sig_buf = vec![0u8; self.key_pair.public().modulus_len()];
        self.key_pair
            .sign(&RSA_PSS_SHA256, &rng, message.as_bytes(), &mut sig_buf)
            .map_err(|e| AuthError::SigningFailed(e.to_string()))?;

        use base64::Engine as _;
        Ok(base64::engine::general_purpose::STANDARD.encode(&sig_buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test PKCS#8 RSA private key (2048-bit, generated for testing only).
    const TEST_KEY_PEM: &str = concat!(
        "-----BEGIN ",
        r#"PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDJJgEmkCH8nR55
pqhp/MIFR4hIr/dvbhrY+Ja3VM+qnq9vUD0lvPkPSdvwMVT05n6YVtMMM3ionLcA
bjSX2qjMBQozVih7xZonMKCLryJehbZNLGzPZD4aOv2P8PtctY/pNisa7tG73OvC
OXdlefIz+jMoiHNVNzl/HoVH0HxR4YHASe4lDaPtbbAciw60mpC2G8XWGJFmZGYj
WYfSmZ5tt3nqyQSOZpgzD4TiVXMOGRtjJIk0FdHd1sgo/dDIn6uKoH9j4qV3Mfr8
Z1alWAmt+Pfkwkw6Tcx2Jwtvhh6WNaEXk5+9UyEw+D+U5DqdMOPuD7fFL/y9icDC
fJ+Y9zgfAgMBAAECggEABJkDybfdrwKAYdN3YgTPAoPiD5dGFpvzrSXxe/tKS+IY
rHivDR/GqZzMlC7sfDSQjDbf2BWNGn2KiU37kcUDurYax5Wek0WvAlpQMSEtre9s
fVMYoZzu9naGuTWO6U2VHoWIcrMmxB6GnQfnPMCO0rVTWgfUaww6Gje+YCfZz51H
iJNNrLS9qFiWwO/DbEIOIyKRmAwF+h62Tfc7UQG2HIkJtMagRvCos2/+/gcDJ183
Tnno5XisuJ1B3LVvzh1BNqZaWKiXJZmZA5vpz2cFlaKGFE/IVgzgvKxvDrEt2d7D
j5uYVUb+6oft7BIZem2jkQQLQKez1ZMRmNSXa83BAQKBgQD5O1dcPfvVSczN4/6X
NzrjgkLQ5nNP57PM+gS1LGIVXztFywEQjftTF0R9tKFFqi6rVq5VO5Zjg7P1BiGc
Rk7rZy8mQnZo54MT2JYTpVhX9gUYXwSEOnc9sFyx+ncBPmKkwTvSwZhVdkhCEggw
CZI3VZgpJB0damAWhajQOcOa3wKBgQDOnGNRTYkHiD5Cr76l+CpKONqKiNUaKiZx
ZehBsKMCAfv9z77i/H/Wsbgn/HxDinmIBskF71fAKUOOGcBusJ4cGJRh2B6vBy6g
hm+b+2nSawgWaF7+ttRfVzFGH+nETHClzRaHc3h0p2ccnSxwVu6nW1p1jyMx0c/q
gtynUKVKwQKBgDpVPEY3r7ilFE1gPpdP8vWK6G6ScYzTM08XeYCaCb7s0iessuwX
/ynceUhevZxbj57Eo/sI/lL+YWFI9RbpkdEhDnUK+0HkZdaAS+f/PCUiTOD+ZEU6
lewXWirB75aX7miXXZQfgbMHAzSLmeT8aH+RBhMjA7l9y02aLP/HdVPLAoGAJyR5
rG2ECGlHYlrpQ4hAes9Kl/RUayCRJ+qmlcthFoBJvUweXeJ4VbRVrz2mTSVu4NZo
PzeY6E7o/YLjchUD307IzcCkD4TM0JyniGWZJsQgRB6B4L/CfE2IiECDiSzyKncw
TXkS2QbeAg3E3YOasxobiSoVANs/CK7CHvCoYAECgYEA7+emQFZmbSrWlhn7xeEy
OMQVeC/F6xKe4lGiuXsnjKEO1K6bi3qvltRoUdhH7bnR+k55hbDZG1sRZpl+N5VV
L/pwyKxACFxRoBxJqeozXdOqWB/2nw+byZNtK1KfQLnAyGqADXPnXPBUxVFE+c/2
8jqtMyHz94du+Z7Y/kOyNns=
-----END PRIVATE KEY-----"#
    );

    #[test]
    fn from_pem_bytes_accepts_valid_key() {
        let auth = KalshiAuth::from_pem_bytes("test-key", TEST_KEY_PEM.as_bytes()).unwrap();
        assert_eq!(auth.key_id(), "test-key");
    }

    #[test]
    fn from_pem_bytes_rejects_bogus_bytes() {
        let result = KalshiAuth::from_pem_bytes("x", b"not a pem file");
        assert!(result.is_err());
    }

    #[test]
    fn from_pem_bytes_rejects_wrong_label() {
        // Wrap test key with a wrong label
        let body = TEST_KEY_PEM
            .replace(
                concat!("-----BEGIN ", "PRIVATE KEY-----"),
                concat!("-----BEGIN RSA ", "PRIVATE KEY-----"),
            )
            .replace(
                concat!("-----END ", "PRIVATE KEY-----"),
                concat!("-----END RSA ", "PRIVATE KEY-----"),
            );
        let result = KalshiAuth::from_pem_bytes("x", body.as_bytes());
        assert!(result.is_err());
        assert!(matches!(result, Err(AuthError::UnexpectedLabel { .. })));
    }

    #[test]
    fn sign_request_produces_valid_format() {
        let auth = KalshiAuth::from_pem_bytes("test-key-abc", TEST_KEY_PEM.as_bytes()).unwrap();
        let sig = auth.sign_request("GET", "/trade-api/v2/markets", "1712345678000").unwrap();
        // Signature should be non-empty Base64
        assert!(!sig.is_empty());
        // Base64 characters only
        assert!(sig.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='));
    }

    #[test]
    fn sign_request_different_timestamps_produce_different_signatures() {
        let auth = KalshiAuth::from_pem_bytes("k", TEST_KEY_PEM.as_bytes()).unwrap();
        let sig1 = auth.sign_request("GET", "/markets", "1000").unwrap();
        let sig2 = auth.sign_request("GET", "/markets", "2000").unwrap();
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn signing_ignores_query_string_and_normalizes_method_case() {
        let auth = KalshiAuth::from_pem_bytes("k", TEST_KEY_PEM.as_bytes()).unwrap();
        // RSA-PSS is randomized, so verify the signed message rather than comparing signatures.
        use base64::Engine as _;
        use ring::signature::{UnparsedPublicKey, RSA_PSS_2048_8192_SHA256};
        let sig = auth.sign_request("get", "/trade-api/v2/markets?limit=100", "1000").unwrap();
        let sig = base64::engine::general_purpose::STANDARD.decode(sig).unwrap();
        let public_key = auth.key_pair.public().as_ref();
        UnparsedPublicKey::new(&RSA_PSS_2048_8192_SHA256, public_key)
            .verify(b"1000GET/trade-api/v2/markets", &sig)
            .unwrap();
    }
}

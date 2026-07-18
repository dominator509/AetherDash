//! Operator TOTP verification inside the Guardian approval boundary.
//!
//! Secret references come from `users.totp_secret_ref`; secret bytes are read
//! only from systemd's credential directory and are zeroized immediately.

use aether_authz::verify_totp;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use zeroize::Zeroize;

#[derive(Debug, Error)]
pub enum TotpError {
    #[error("TOTP credential store is unavailable")]
    Unavailable,
    #[error("TOTP credential reference is invalid")]
    InvalidReference,
    #[error("TOTP credential is invalid")]
    InvalidCredential,
}

pub trait TotpAuthority: Send + Sync {
    fn verify(
        &self,
        secret_ref: &str,
        code: &str,
        now_unix_seconds: u64,
    ) -> Result<bool, TotpError>;
}

pub struct CredentialTotpAuthority {
    directory: PathBuf,
}

impl CredentialTotpAuthority {
    pub fn from_env() -> Result<Self, TotpError> {
        let directory = std::env::var("CREDENTIALS_DIRECTORY")
            .map(PathBuf::from)
            .map_err(|_| TotpError::Unavailable)?;
        Ok(Self { directory })
    }

    #[cfg(test)]
    fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn credential_path(&self, secret_ref: &str) -> Result<PathBuf, TotpError> {
        let path = Path::new(secret_ref);
        let mut components = path.components();
        let Some(Component::Normal(name)) = components.next() else {
            return Err(TotpError::InvalidReference);
        };
        if components.next().is_some() || name.is_empty() {
            return Err(TotpError::InvalidReference);
        }
        Ok(self.directory.join(name))
    }
}

impl TotpAuthority for CredentialTotpAuthority {
    fn verify(&self, secret_ref: &str, code: &str, now: u64) -> Result<bool, TotpError> {
        let path = self.credential_path(secret_ref)?;
        let mut credential = std::fs::read(path).map_err(|_| TotpError::Unavailable)?;
        while credential.last().is_some_and(u8::is_ascii_whitespace) {
            credential.pop();
        }
        let mut secret = decode_base32(&credential).unwrap_or_else(|| credential.clone());
        let result = verify_totp(&secret, code, now).map_err(|_| TotpError::InvalidCredential);
        secret.zeroize();
        credential.zeroize();
        result
    }
}

fn decode_base32(encoded: &[u8]) -> Option<Vec<u8>> {
    if encoded.is_empty() {
        return None;
    }
    let mut output = Vec::with_capacity(encoded.len() * 5 / 8);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in encoded.iter().copied().filter(|byte| *byte != b'=') {
        let normalized = byte.to_ascii_uppercase();
        let value = match normalized {
            b'A'..=b'Z' => normalized - b'A',
            b'2'..=b'7' => normalized - b'2' + 26,
            _ => return None,
        };
        accumulator = (accumulator << 5) | u32::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1_u32 << bits) - 1;
        }
    }
    (output.len() >= 16).then_some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_without_reading_outside_credentials() {
        let authority = CredentialTotpAuthority::new(std::env::temp_dir());
        assert!(matches!(
            authority.verify("../operator", "000000", 0),
            Err(TotpError::InvalidReference)
        ));
    }

    #[test]
    fn verifies_rfc_vector_from_credential_file() {
        let directory =
            std::env::temp_dir().join(format!("guardian-totp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("temporary credential directory");
        std::fs::write(directory.join("operator-totp"), b"12345678901234567890\n")
            .expect("temporary credential");
        let authority = CredentialTotpAuthority::new(directory.clone());
        assert!(authority.verify("operator-totp", "287082", 59).expect("verify"));
        std::fs::remove_dir_all(directory).expect("temporary credential cleanup");
    }

    #[test]
    fn decodes_standard_base32_totp_secrets() {
        assert_eq!(
            decode_base32(b"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"),
            Some(b"12345678901234567890".to_vec())
        );
    }
}

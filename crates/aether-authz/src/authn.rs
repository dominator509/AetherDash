use crate::UnixSeconds;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SESSION_IDLE_EXPIRY_SECS: u64 = 30 * 24 * 60 * 60;
const MAX_LOCKOUT_SECS: u64 = 15 * 60;

/// RFC 9106's second recommended profile for memory-constrained environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordPolicy {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self { memory_kib: 65_536, iterations: 3, parallelism: 4 }
    }
}

impl PasswordPolicy {
    fn engine(self) -> Result<Argon2<'static>, AuthnError> {
        let params = Params::new(self.memory_kib, self.iterations, self.parallelism, Some(32))
            .map_err(|_| AuthnError::InvalidPasswordPolicy)?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }

    pub fn hash(self, password: &[u8]) -> Result<String, AuthnError> {
        let salt = SaltString::generate(&mut OsRng);
        self.engine()?
            .hash_password(password, &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| AuthnError::PasswordHash)
    }

    pub fn verify(self, password: &[u8], encoded: &str) -> Result<bool, AuthnError> {
        let parsed = PasswordHash::new(encoded).map_err(|_| AuthnError::MalformedPasswordHash)?;
        Ok(self.engine()?.verify_password(password, &parsed).is_ok())
    }
}

#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthnError {
    #[error("invalid password hashing policy")]
    InvalidPasswordPolicy,
    #[error("password hashing failed")]
    PasswordHash,
    #[error("stored password hash is malformed")]
    MalformedPasswordHash,
    #[error("session is expired")]
    SessionExpired,
    #[error("session is revoked")]
    SessionRevoked,
    #[error("TOTP secret is invalid")]
    InvalidTotpSecret,
}

#[derive(Clone)]
pub struct SecretSessionToken([u8; 32]);

impl SecretSessionToken {
    #[must_use]
    pub fn expose_hex(&self) -> String {
        encode_hex(&self.0)
    }
}

impl std::fmt::Debug for SecretSessionToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretSessionToken([REDACTED])")
    }
}

#[derive(Debug, Clone)]
pub struct SessionTokenPair {
    pub token: SecretSessionToken,
    pub token_hash: String,
}

impl SessionTokenPair {
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let token = SecretSessionToken(bytes);
        let token_hash = hash_session_token(&token.expose_hex());
        Self { token, token_hash }
    }
}

#[must_use]
pub fn hash_session_token(token: &str) -> String {
    encode_hex(&Sha256::digest(token.as_bytes()))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub token_hash: String,
    pub device_label: String,
    pub absolute_expires_at: UnixSeconds,
    pub last_seen_at: UnixSeconds,
    pub revoked_at: Option<UnixSeconds>,
}

impl SessionRecord {
    pub fn validate_and_touch(
        &mut self,
        token: &str,
        now: UnixSeconds,
    ) -> Result<bool, AuthnError> {
        if self.revoked_at.is_some() {
            return Err(AuthnError::SessionRevoked);
        }
        if now >= self.absolute_expires_at
            || now.saturating_sub(self.last_seen_at) >= SESSION_IDLE_EXPIRY_SECS
        {
            return Err(AuthnError::SessionExpired);
        }
        if self.token_hash != hash_session_token(token) {
            return Ok(false);
        }
        self.last_seen_at = now;
        Ok(true)
    }

    pub fn revoke(&mut self, now: UnixSeconds) {
        self.revoked_at = Some(now);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LockoutState {
    pub failed_attempts: u32,
    pub locked_until: Option<UnixSeconds>,
}

impl LockoutState {
    pub fn record_failure(&mut self, now: UnixSeconds) -> u64 {
        self.failed_attempts = self.failed_attempts.saturating_add(1);
        let shift = self.failed_attempts.saturating_sub(1).min(20);
        let delay = 1_u64.checked_shl(shift).unwrap_or(MAX_LOCKOUT_SECS).min(MAX_LOCKOUT_SECS);
        self.locked_until = Some(now.saturating_add(delay));
        delay
    }

    #[must_use]
    pub fn is_locked(&self, now: UnixSeconds) -> bool {
        self.locked_until.is_some_and(|until| now < until)
    }

    pub fn record_success(&mut self) {
        *self = Self::default();
    }
}

/// RFC 6238 TOTP, HMAC-SHA1, six digits, 30-second step, ±1 validation window.
pub fn verify_totp(secret: &[u8], code: &str, now: UnixSeconds) -> Result<bool, AuthnError> {
    if secret.len() < 16 {
        return Err(AuthnError::InvalidTotpSecret);
    }
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(false);
    }
    let expected = code.as_bytes();
    let counter = now / 30;
    for candidate in [counter.saturating_sub(1), counter, counter.saturating_add(1)] {
        let generated = totp_at(secret, candidate)?;
        let mut difference = 0_u8;
        for (left, right) in generated.as_bytes().iter().zip(expected) {
            difference |= left ^ right;
        }
        if difference == 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn totp_at(secret: &[u8], counter: u64) -> Result<String, AuthnError> {
    let mut mac =
        Hmac::<Sha1>::new_from_slice(secret).map_err(|_| AuthnError::InvalidTotpSecret)?;
    mac.update(&counter.to_be_bytes());
    let output = mac.finalize().into_bytes();
    let offset = usize::from(output[19] & 0x0f);
    let binary = (u32::from(output[offset] & 0x7f) << 24)
        | (u32::from(output[offset + 1]) << 16)
        | (u32::from(output[offset + 2]) << 8)
        | u32::from(output[offset + 3]);
    Ok(format!("{:06}", binary % 1_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2id_profile_round_trip() {
        let policy = PasswordPolicy::default();
        let encoded = policy.hash(b"correct horse battery staple").expect("hash");
        assert!(encoded.starts_with("$argon2id$v=19$m=65536,t=3,p=4$"));
        assert!(policy.verify(b"correct horse battery staple", &encoded).expect("verify"));
        assert!(!policy.verify(b"wrong", &encoded).expect("verify"));
    }

    #[test]
    fn opaque_token_is_256_bits_and_debug_redacted() {
        let pair = SessionTokenPair::generate();
        assert_eq!(pair.token.expose_hex().len(), 64);
        assert_eq!(pair.token_hash, hash_session_token(&pair.token.expose_hex()));
        assert!(!format!("{:?}", pair.token).contains(&pair.token.expose_hex()));
    }

    #[test]
    fn revoked_and_idle_sessions_fail() {
        let token = "opaque";
        let mut session = SessionRecord {
            id: "s".into(),
            token_hash: hash_session_token(token),
            device_label: "desktop".into(),
            absolute_expires_at: SESSION_IDLE_EXPIRY_SECS * 2,
            last_seen_at: 0,
            revoked_at: None,
        };
        assert_eq!(
            session.validate_and_touch(token, SESSION_IDLE_EXPIRY_SECS),
            Err(AuthnError::SessionExpired)
        );
        session.last_seen_at = 10;
        session.revoke(11);
        assert_eq!(session.validate_and_touch(token, 12), Err(AuthnError::SessionRevoked));
    }

    #[test]
    fn lockout_is_exponential_and_capped() {
        let mut state = LockoutState::default();
        assert_eq!(state.record_failure(100), 1);
        assert!(state.is_locked(100));
        assert_eq!(state.record_failure(101), 2);
        for attempt in 0..20 {
            state.record_failure(200 + attempt);
        }
        assert_eq!(state.record_failure(300), MAX_LOCKOUT_SECS);
        state.record_success();
        assert!(!state.is_locked(300));
    }

    #[test]
    fn rfc6238_sha1_vector_and_adjacent_step() {
        let secret = b"12345678901234567890";
        // RFC 6238's 8-digit value at t=59 is 94287082, hence six-digit 287082.
        assert!(verify_totp(secret, "287082", 59).expect("totp"));
        assert!(verify_totp(secret, "287082", 89).expect("adjacent step"));
        assert!(!verify_totp(secret, "287082", 120).expect("stale"));
    }
}

use crate::{Action, UnixSeconds};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use thiserror::Error;

pub const STEP_UP_VALIDITY_SECS: u64 = 5 * 60;

pub trait TotpVerifier {
    fn verify(&self, actor_id: &str, code: &str, now: UnixSeconds) -> bool;
}

#[derive(Debug, Clone)]
struct Challenge {
    actor_id: String,
    action: Action,
    expires_at: UnixSeconds,
    consumed_at: Option<UnixSeconds>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StepUpError {
    #[error("step-up challenge was not found")]
    NotFound,
    #[error("step-up challenge does not match actor or action")]
    Mismatch,
    #[error("step-up challenge is stale")]
    Stale,
    #[error("step-up challenge was already consumed")]
    Consumed,
    #[error("TOTP verification failed")]
    InvalidTotp,
}

/// Issues opaque, hashed-at-rest challenges. All challenges are single-use;
/// this is stricter than SPEC-005's minimum (which mandates it for Guardian).
#[derive(Debug, Default)]
pub struct StepUpStore {
    challenges: HashMap<String, Challenge>,
}

impl StepUpStore {
    #[must_use]
    pub fn issue(&mut self, actor_id: &str, action: Action, now: UnixSeconds) -> String {
        let mut token = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut token);
        let plaintext = encode_hex(&token);
        self.challenges.insert(
            token_hash(&plaintext),
            Challenge {
                actor_id: actor_id.to_owned(),
                action,
                expires_at: now.saturating_add(STEP_UP_VALIDITY_SECS),
                consumed_at: None,
            },
        );
        plaintext
    }

    pub fn consume<V: TotpVerifier>(
        &mut self,
        token: &str,
        actor_id: &str,
        action: Action,
        code: &str,
        now: UnixSeconds,
        verifier: &V,
    ) -> Result<(), StepUpError> {
        let challenge = self.challenges.get_mut(&token_hash(token)).ok_or(StepUpError::NotFound)?;
        if challenge.actor_id != actor_id || challenge.action != action {
            return Err(StepUpError::Mismatch);
        }
        if challenge.consumed_at.is_some() {
            return Err(StepUpError::Consumed);
        }
        if now >= challenge.expires_at {
            return Err(StepUpError::Stale);
        }
        if !verifier.verify(actor_id, code, now) {
            return Err(StepUpError::InvalidTotp);
        }
        challenge.consumed_at = Some(now);
        Ok(())
    }
}

fn token_hash(token: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct AcceptCode;
    impl TotpVerifier for AcceptCode {
        fn verify(&self, _actor_id: &str, code: &str, _now: UnixSeconds) -> bool {
            code == "123456"
        }
    }

    #[test]
    fn step_up_is_five_minutes_and_single_use() {
        let mut store = StepUpStore::default();
        let token = store.issue("operator", Action::GuardianApproval, 100);
        assert_eq!(
            store
                .consume(&token, "operator", Action::GuardianApproval, "123456", 399, &AcceptCode,),
            Ok(())
        );
        assert_eq!(
            store
                .consume(&token, "operator", Action::GuardianApproval, "123456", 399, &AcceptCode,),
            Err(StepUpError::Consumed)
        );
    }

    #[test]
    fn stale_and_wrong_action_are_rejected() {
        let mut store = StepUpStore::default();
        let stale = store.issue("operator", Action::ActivateCaps, 100);
        assert_eq!(
            store.consume(&stale, "operator", Action::ActivateCaps, "123456", 400, &AcceptCode,),
            Err(StepUpError::Stale)
        );
        let mismatch = store.issue("operator", Action::ActivateCaps, 500);
        assert_eq!(
            store.consume(
                &mismatch,
                "operator",
                Action::GuardianApproval,
                "123456",
                501,
                &AcceptCode,
            ),
            Err(StepUpError::Mismatch)
        );
    }
}

//! WalletConnect session management.

use serde::{Deserialize, Serialize};

/// A WalletConnect v2 session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcSession {
    pub topic: String,
    pub chain_id: u64,
    pub accounts: Vec<String>,
    pub expiry: u64, // Unix seconds
    pub approved: bool,
}

impl WcSession {
    /// Create a pending session awaiting wallet approval.
    pub fn new(chain_id: u64, accounts: Vec<String>, topic: String) -> Self {
        Self {
            topic,
            chain_id,
            accounts,
            expiry: chrono::Utc::now().timestamp() as u64 + 3600,
            approved: false,
        }
    }

    /// Approve the session.
    pub fn approve(&mut self) {
        self.approved = true;
    }

    /// Check if the session is valid and approved.
    pub fn is_ready(&self) -> bool {
        self.approved && (chrono::Utc::now().timestamp() as u64) < self.expiry
    }

    /// Check if the session is still alive (may be unapproved).
    pub fn is_alive(&self) -> bool {
        (chrono::Utc::now().timestamp() as u64) < self.expiry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_is_unapproved() {
        let session = WcSession::new(1, vec!["0x1234".into()], "topic".into());
        assert!(!session.is_ready());
        assert!(session.is_alive());
    }

    #[test]
    fn approved_session_is_ready() {
        let mut session = WcSession::new(1, vec!["0x1234".into()], "topic".into());
        session.approve();
        assert!(session.is_ready());
    }

    #[test]
    fn expired_session_is_not_ready() {
        let mut session = WcSession::new(1, vec![], "topic".into());
        session.approve();
        session.expiry = 0; // Unix epoch
        assert!(!session.is_ready());
        assert!(!session.is_alive());
    }
}

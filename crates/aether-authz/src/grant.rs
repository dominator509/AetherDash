use crate::{ActorKind, Tier, UnixSeconds};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

pub const AGENT_GRANT_LIFETIME_SECS: u64 = 7 * 24 * 60 * 60;
pub const AUTOMATION_GRANT_LIFETIME_SECS: u64 = 30 * 24 * 60 * 60;

#[must_use]
pub const fn default_grant_lifetime_secs(kind: ActorKind) -> Option<u64> {
    match kind {
        ActorKind::Human => None,
        ActorKind::Agent => Some(AGENT_GRANT_LIFETIME_SECS),
        ActorKind::Automation => Some(AUTOMATION_GRANT_LIFETIME_SECS),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub id: String,
    pub actor_id: String,
    pub actor_kind: ActorKind,
    pub tier: Tier,
    /// Exact action/tool allowlist when `scope_restricted` is true.
    pub scopes: HashSet<String>,
    /// Distinguishes an omitted allowlist from an explicitly empty allowlist.
    pub scope_restricted: bool,
    pub expires_at: Option<UnixSeconds>,
    pub revoked_at: Option<UnixSeconds>,
}

impl Grant {
    #[must_use]
    pub fn is_active(&self, now: UnixSeconds) -> bool {
        self.revoked_at.is_none() && self.expires_at.map_or(true, |expiry| now < expiry)
    }

    #[must_use]
    pub fn permits_scope(&self, scope: &str) -> bool {
        !self.scope_restricted || self.scopes.contains(scope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GrantError {
    #[error("grant already exists")]
    Duplicate,
    #[error("grant not found")]
    NotFound,
}

/// Small authority store used by services/tests. It intentionally has no cache:
/// revocation is visible on the next request, comfortably inside the 5 s bound.
#[derive(Debug, Default)]
pub struct GrantStore {
    grants: HashMap<String, Grant>,
}

impl GrantStore {
    pub fn issue(&mut self, grant: Grant) -> Result<(), GrantError> {
        if self.grants.contains_key(&grant.id) {
            return Err(GrantError::Duplicate);
        }
        self.grants.insert(grant.id.clone(), grant);
        Ok(())
    }

    pub fn revoke(&mut self, id: &str, now: UnixSeconds) -> Result<(), GrantError> {
        let grant = self.grants.get_mut(id).ok_or(GrantError::NotFound)?;
        grant.revoked_at = Some(now);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Grant> {
        self.grants.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(kind: ActorKind, now: UnixSeconds) -> Grant {
        Grant {
            id: "grant-1".into(),
            actor_id: "actor".into(),
            actor_kind: kind,
            tier: Tier::DraftOnly,
            scopes: HashSet::new(),
            scope_restricted: false,
            expires_at: default_grant_lifetime_secs(kind).map(|lifetime| now + lifetime),
            revoked_at: None,
        }
    }

    #[test]
    fn non_human_default_expiries_match_spec() {
        assert_eq!(default_grant_lifetime_secs(ActorKind::Human), None);
        assert_eq!(default_grant_lifetime_secs(ActorKind::Agent), Some(7 * 86_400));
        assert_eq!(default_grant_lifetime_secs(ActorKind::Automation), Some(30 * 86_400));
    }

    #[test]
    fn revocation_is_visible_on_the_next_read() {
        let mut store = GrantStore::default();
        store.issue(grant(ActorKind::Agent, 100)).expect("issue");
        assert!(store.get("grant-1").expect("grant").is_active(101));
        store.revoke("grant-1", 102).expect("revoke");
        assert!(!store.get("grant-1").expect("grant").is_active(102));
    }
}

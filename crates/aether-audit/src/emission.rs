//! Audit emission catalog — documents every action class requiring audit events.
//! Used by the emission-coverage test to ensure no gaps.

use serde::{Deserialize, Serialize};

/// An action class that must emit audit events.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionClass {
    pub domain: String,
    pub action: String,
    pub actor_type: String,
}

/// The complete catalog of actions requiring audit events.
impl ActionClass {
    /// All required action classes per SPEC-010 and SPEC-005.
    pub fn catalog() -> Vec<Self> {
        vec![
            // Auth / permissions (EP-401)
            Self {
                domain: "auth".into(),
                action: "session.login".into(),
                actor_type: "human".into(),
            },
            Self {
                domain: "auth".into(),
                action: "session.logout".into(),
                actor_type: "human".into(),
            },
            Self {
                domain: "auth".into(),
                action: "grant.create".into(),
                actor_type: "admin".into(),
            },
            Self {
                domain: "auth".into(),
                action: "grant.revoke".into(),
                actor_type: "admin".into(),
            },
            Self {
                domain: "auth".into(),
                action: "caps.update".into(),
                actor_type: "admin".into(),
            },
            // Risk + orders (EP-305)
            Self {
                domain: "execution".into(),
                action: "risk.verdict".into(),
                actor_type: "system".into(),
            },
            Self {
                domain: "execution".into(),
                action: "order.submitted".into(),
                actor_type: "agent".into(),
            },
            Self {
                domain: "execution".into(),
                action: "order.filled".into(),
                actor_type: "system".into(),
            },
            Self {
                domain: "execution".into(),
                action: "order.cancelled".into(),
                actor_type: "agent".into(),
            },
            // Guardian (EP-306)
            Self {
                domain: "guardian".into(),
                action: "proposal.created".into(),
                actor_type: "agent".into(),
            },
            Self {
                domain: "guardian".into(),
                action: "proposal.approved".into(),
                actor_type: "human".into(),
            },
            Self {
                domain: "guardian".into(),
                action: "proposal.denied".into(),
                actor_type: "system".into(),
            },
            Self {
                domain: "guardian".into(),
                action: "tx.broadcast".into(),
                actor_type: "system".into(),
            },
            // Config / operations
            Self {
                domain: "operations".into(),
                action: "live_enabled.toggled".into(),
                actor_type: "admin".into(),
            },
            Self {
                domain: "operations".into(),
                action: "flag.changed".into(),
                actor_type: "admin".into(),
            },
            Self {
                domain: "operations".into(),
                action: "plugin.installed".into(),
                actor_type: "admin".into(),
            },
            // Scanner
            Self {
                domain: "scanner".into(),
                action: "opportunity.detected".into(),
                actor_type: "system".into(),
            },
            Self {
                domain: "scanner".into(),
                action: "opportunity.expired".into(),
                actor_type: "system".into(),
            },
        ]
    }

    /// Check that a set of emitted action classes covers the catalog.
    /// Returns any missing classes.
    pub fn check_coverage(emitted: &[Self]) -> Vec<&'static str> {
        let mut missing = Vec::new();
        for required in Self::catalog() {
            if !emitted.contains(&required) {
                missing.push(Box::leak(
                    format!("{}.{} ({})", required.domain, required.action, required.actor_type)
                        .into_boxed_str(),
                ) as &str);
            }
        }
        missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_non_empty() {
        assert!(!ActionClass::catalog().is_empty());
    }

    #[test]
    fn full_catalog_has_no_missing() {
        let all = ActionClass::catalog();
        let missing = ActionClass::check_coverage(&all);
        assert!(missing.is_empty(), "Missing: {:?}", missing);
    }

    #[test]
    fn empty_emissions_report_all_missing() {
        let missing = ActionClass::check_coverage(&[]);
        assert_eq!(missing.len(), ActionClass::catalog().len());
    }
}

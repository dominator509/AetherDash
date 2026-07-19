use aether_authz::{evaluate, Action, Actor, ActorKind, EvaluationContext, Verdict};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Metric,
    Attribution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImprovementEvidence {
    pub kind: EvidenceKind,
    pub source_id: String,
    pub value: Decimal,
    pub observed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImprovementProposal {
    pub id: String,
    pub summary: String,
    pub unified_diff: String,
    pub evidence: Vec<ImprovementEvidence>,
    pub digest: String,
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Proposed,
    ApplicationAuthorized,
    Rejected,
}

/// This receipt authorizes an external human-controlled apply operation; this
/// crate deliberately has no filesystem or process API capable of applying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanApplicationReceipt {
    proposal_id: String,
    proposal_digest: String,
    actor_id: String,
}

impl HumanApplicationReceipt {
    pub(crate) fn new(proposal_id: String, proposal_digest: String, actor_id: String) -> Self {
        Self { proposal_id, proposal_digest, actor_id }
    }

    #[must_use]
    pub fn proposal_id(&self) -> &str {
        &self.proposal_id
    }

    #[must_use]
    pub fn proposal_digest(&self) -> &str {
        &self.proposal_digest
    }

    #[must_use]
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }
}

#[derive(Debug, Default)]
pub struct ProposalStore {
    proposals: HashMap<String, ImprovementProposal>,
}

impl ProposalStore {
    pub fn propose(
        &mut self,
        id: &str,
        summary: &str,
        unified_diff: &str,
        evidence: Vec<ImprovementEvidence>,
    ) -> Result<&ImprovementProposal, ProposalError> {
        if id.is_empty() || self.proposals.contains_key(id) {
            return Err(ProposalError::InvalidIdentity);
        }
        if summary.trim().is_empty()
            || !unified_diff.starts_with("--- ")
            || !unified_diff.contains("\n+++ ")
        {
            return Err(ProposalError::InvalidDiff);
        }
        if evidence.is_empty()
            || evidence.iter().any(|item| item.source_id.trim().is_empty())
            || !evidence.iter().any(|item| item.kind == EvidenceKind::Metric)
        {
            return Err(ProposalError::MetricEvidenceRequired);
        }
        let digest = digest(summary, unified_diff, &evidence)?;
        self.proposals.insert(
            id.into(),
            ImprovementProposal {
                id: id.into(),
                summary: summary.into(),
                unified_diff: unified_diff.into(),
                evidence,
                digest,
                status: ProposalStatus::Proposed,
            },
        );
        self.proposals.get(id).ok_or(ProposalError::NotFound)
    }

    pub fn authorize_application(
        &mut self,
        id: &str,
        actor: &Actor,
        context: EvaluationContext<'_>,
    ) -> Result<HumanApplicationReceipt, ProposalError> {
        if actor.kind != ActorKind::Human
            || evaluate(actor, Action::ApplySelfImprovement, context).verdict != Verdict::Allow
        {
            return Err(ProposalError::HumanStepUpRequired);
        }
        let proposal = self.proposals.get_mut(id).ok_or(ProposalError::NotFound)?;
        if proposal.status != ProposalStatus::Proposed {
            return Err(ProposalError::InvalidTransition);
        }
        proposal.status = ProposalStatus::ApplicationAuthorized;
        Ok(HumanApplicationReceipt {
            proposal_id: proposal.id.clone(),
            proposal_digest: proposal.digest.clone(),
            actor_id: actor.id.clone(),
        })
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ImprovementProposal> {
        self.proposals.get(id)
    }
}

fn digest(
    summary: &str,
    unified_diff: &str,
    evidence: &[ImprovementEvidence],
) -> Result<String, ProposalError> {
    let bytes = serde_json::to_vec(&(summary, unified_diff, evidence))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    #[error("proposal identity is empty or duplicated")]
    InvalidIdentity,
    #[error("proposal must contain a human-reviewable unified diff")]
    InvalidDiff,
    #[error("at least one concrete metric citation is required")]
    MetricEvidenceRequired,
    #[error("proposal not found")]
    NotFound,
    #[error("application authorization requires a human and fresh step-up")]
    HumanStepUpRequired,
    #[error("invalid proposal lifecycle transition")]
    InvalidTransition,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_authz::{ActorKind, Grant, Tier};
    use std::collections::HashSet;

    fn evidence() -> Vec<ImprovementEvidence> {
        vec![ImprovementEvidence {
            kind: EvidenceKind::Metric,
            source_id: "aether_scan_cycle_ms:p95:2026-07-18".into(),
            value: Decimal::new(125, 0),
            observed_at: 100,
        }]
    }

    fn grant(kind: ActorKind) -> Grant {
        Grant {
            id: "proposal-grant".into(),
            actor_id: if kind == ActorKind::Human { "operator" } else { "agent" }.into(),
            actor_kind: kind,
            tier: Tier::BoundedAutopilot,
            scopes: HashSet::from([Action::ApplySelfImprovement.scope().into()]),
            scope_restricted: true,
            expires_at: None,
            revoked_at: None,
        }
    }

    fn proposal(store: &mut ProposalStore) {
        store
            .propose(
                "proposal-1",
                "Reduce scanner allocation churn",
                "--- a/scanner.rs\n+++ b/scanner.rs\n@@ -1 +1 @@\n-old\n+new",
                evidence(),
            )
            .expect("metric-cited proposal");
    }

    #[test]
    fn no_metric_means_no_proposal() {
        let mut store = ProposalStore::default();
        assert!(matches!(
            store.propose("proposal-1", "summary", "--- a\n+++ b", vec![]),
            Err(ProposalError::MetricEvidenceRequired)
        ));
    }

    #[test]
    fn only_human_step_up_can_authorize_application() {
        let mut store = ProposalStore::default();
        proposal(&mut store);
        let agent = Actor { id: "agent".into(), kind: ActorKind::Agent };
        let agent_grant = grant(ActorKind::Agent);
        let mut agent_context = EvaluationContext::new(100, Some(&agent_grant));
        agent_context.step_up_satisfied = true;
        assert!(matches!(
            store.authorize_application("proposal-1", &agent, agent_context),
            Err(ProposalError::HumanStepUpRequired)
        ));

        let human = Actor { id: "operator".into(), kind: ActorKind::Human };
        let human_grant = grant(ActorKind::Human);
        let mut context = EvaluationContext::new(100, Some(&human_grant));
        context.session_tier = Some(Tier::BoundedAutopilot);
        assert!(matches!(
            store.authorize_application("proposal-1", &human, context),
            Err(ProposalError::HumanStepUpRequired)
        ));
        context.step_up_satisfied = true;
        let receipt = store
            .authorize_application("proposal-1", &human, context)
            .expect("human authorization");
        assert_eq!(receipt.actor_id(), "operator");
        assert_eq!(
            store.get("proposal-1").expect("proposal").status,
            ProposalStatus::ApplicationAuthorized
        );
    }
}

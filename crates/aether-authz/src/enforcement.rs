use crate::{evaluate, Action, Actor, AuditRecord, AuditSink, AuditedDecision, EvaluationContext};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementPoint {
    Gateway,
    Router,
    Mcp,
    Guardian,
}

/// Evaluate and emit exactly one audit record. Audit failure is returned to the
/// caller, which must fail closed instead of performing the action unaudited.
pub fn enforce_at<S: AuditSink>(
    point: EnforcementPoint,
    actor: &Actor,
    action: Action,
    context: EvaluationContext<'_>,
    sink: &S,
) -> Result<AuditedDecision, S::Error> {
    let decision = evaluate(actor, action, context);
    let audit = AuditRecord {
        at: context.now,
        actor_id: actor.id.clone(),
        actor_kind: actor.kind,
        action,
        enforcement_point: point,
        grant_id: decision.grant_id.clone(),
        effective_tier: decision.effective_tier,
        verdict: decision.verdict,
        deciding_rule: decision.deciding_rule,
    };
    sink.emit(&audit)?;
    Ok(AuditedDecision { decision, audit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActorKind, Grant, MemoryAuditSink, Tier, Verdict};
    use std::collections::HashSet;

    fn actor() -> Actor {
        Actor { id: "operator".into(), kind: ActorKind::Human }
    }

    fn grant(tier: Tier) -> Grant {
        Grant {
            id: "grant-1".into(),
            actor_id: "operator".into(),
            actor_kind: ActorKind::Human,
            tier,
            scopes: HashSet::new(),
            scope_restricted: false,
            expires_at: None,
            revoked_at: None,
        }
    }

    #[test]
    fn every_enforcement_point_denies_independently_and_audits() {
        for point in [
            EnforcementPoint::Gateway,
            EnforcementPoint::Router,
            EnforcementPoint::Mcp,
            EnforcementPoint::Guardian,
        ] {
            let grant = grant(Tier::ReadOnly);
            let mut context = EvaluationContext::new(100, Some(&grant));
            context.session_tier = Some(Tier::YoloWithinHardCaps);
            let sink = MemoryAuditSink::default();
            let result = enforce_at(point, &actor(), Action::SubmitPaperOrder, context, &sink)
                .expect("memory sink must accept the record");
            assert_eq!(result.decision.verdict, Verdict::Deny);
            assert_eq!(sink.records().len(), 1);
            assert_eq!(sink.records()[0].enforcement_point, point);
        }
    }

    #[test]
    fn gateway_bypass_is_caught_by_router_recheck() {
        let grant = grant(Tier::ReadOnly);
        let mut context = EvaluationContext::new(100, Some(&grant));
        context.session_tier = Some(Tier::YoloWithinHardCaps);
        let sink = MemoryAuditSink::default();
        let router =
            enforce_at(EnforcementPoint::Router, &actor(), Action::SubmitLiveOrder, context, &sink)
                .expect("memory sink must accept the record");
        assert_eq!(router.decision.verdict, Verdict::Deny);
        assert_eq!(router.decision.deciding_rule, "tier.insufficient");
    }
}

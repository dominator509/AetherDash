use crate::{Action, Grant, Tier, UnixSeconds};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Human,
    Agent,
    Automation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub id: String,
    pub kind: ActorKind,
}

#[derive(Debug, Clone, Copy)]
pub struct EvaluationContext<'a> {
    pub now: UnixSeconds,
    pub grant: Option<&'a Grant>,
    /// Only human actors may use a session tier. It is ignored for agents/automations.
    pub session_tier: Option<Tier>,
    pub confirmed: bool,
    pub step_up_satisfied: bool,
    pub live_history_days: u16,
    pub wallet_above_threshold: bool,
    pub fresh_human_wallet_approval: bool,
}

impl<'a> EvaluationContext<'a> {
    #[must_use]
    pub const fn new(now: UnixSeconds, grant: Option<&'a Grant>) -> Self {
        Self {
            now,
            grant,
            session_tier: None,
            confirmed: false,
            step_up_satisfied: false,
            live_history_days: 0,
            wallet_above_threshold: false,
            fresh_human_wallet_approval: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Allow,
    Deny,
    ConfirmRequired,
    StepUpRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
    pub deciding_rule: &'static str,
    pub effective_tier: Option<Tier>,
    pub grant_id: Option<String>,
}

impl Decision {
    fn deny(rule: &'static str, grant: Option<&Grant>) -> Self {
        Self {
            verdict: Verdict::Deny,
            deciding_rule: rule,
            effective_tier: None,
            grant_id: grant.map(|value| value.id.clone()),
        }
    }
}

/// Evaluate one action. Hard-denies are deliberately checked before any grant
/// or tier arithmetic, so Tier 5 cannot bypass them.
#[must_use]
pub fn evaluate(actor: &Actor, action: Action, context: EvaluationContext<'_>) -> Decision {
    match action {
        Action::ReadSecretMaterial => {
            return Decision::deny("hard_deny.secret_material", context.grant)
        }
        Action::SetLiveEnabled => return Decision::deny("hard_deny.live_enabled", context.grant),
        Action::RaiseCapsProgrammatically => {
            return Decision::deny("hard_deny.programmatic_caps_raise", context.grant);
        }
        Action::DisableSafetyControl => {
            return Decision::deny("hard_deny.disable_safety_control", context.grant);
        }
        Action::WalletTransfer
            if context.wallet_above_threshold
                && !(actor.kind == ActorKind::Human
                    && context.fresh_human_wallet_approval
                    && context.step_up_satisfied) =>
        {
            return Decision::deny("hard_deny.wallet_threshold", context.grant);
        }
        _ => {}
    }

    let Some(grant) = context.grant else {
        return Decision::deny("grant.missing", None);
    };
    if grant.actor_id != actor.id || grant.actor_kind != actor.kind {
        return Decision::deny("grant.actor_mismatch", Some(grant));
    }
    if grant.revoked_at.is_some() {
        return Decision::deny("grant.revoked", Some(grant));
    }
    if grant.expires_at.is_some_and(|expiry| context.now >= expiry) {
        return Decision::deny("grant.tier_expired", Some(grant));
    }
    if !grant.permits_scope(action.scope()) {
        return Decision::deny("grant.scope_denied", Some(grant));
    }

    let effective_tier = match actor.kind {
        ActorKind::Human => {
            let Some(session_tier) = context.session_tier else {
                return Decision::deny("session.missing", Some(grant));
            };
            grant.tier.lower(session_tier)
        }
        // A supplied human session is intentionally ignored here.
        ActorKind::Agent | ActorKind::Automation => grant.tier,
    };

    if effective_tier < action.minimum_tier() {
        return Decision {
            verdict: Verdict::Deny,
            deciding_rule: "tier.insufficient",
            effective_tier: Some(effective_tier),
            grant_id: Some(grant.id.clone()),
        };
    }

    let requires_step_up = action.always_requires_step_up()
        || (action == Action::SubmitLiveOrder && context.live_history_days < 30);
    if requires_step_up && !context.step_up_satisfied {
        return Decision {
            verdict: Verdict::StepUpRequired,
            deciding_rule: "step_up.required",
            effective_tier: Some(effective_tier),
            grant_id: Some(grant.id.clone()),
        };
    }

    let requires_confirmation = (effective_tier == Tier::ConfirmEveryAction
        && action.is_mutating())
        || (effective_tier == Tier::BoundedAutopilot && action == Action::SubmitLiveOrder);
    if requires_confirmation && !context.confirmed {
        return Decision {
            verdict: Verdict::ConfirmRequired,
            deciding_rule: "confirmation.required",
            effective_tier: Some(effective_tier),
            grant_id: Some(grant.id.clone()),
        };
    }

    Decision {
        verdict: Verdict::Allow,
        deciding_rule: "tier.allowed",
        effective_tier: Some(effective_tier),
        grant_id: Some(grant.id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn human() -> Actor {
        Actor { id: "operator".into(), kind: ActorKind::Human }
    }

    fn grant(tier: Tier) -> Grant {
        Grant {
            id: "grant".into(),
            actor_id: "operator".into(),
            actor_kind: ActorKind::Human,
            tier,
            scopes: HashSet::new(),
            scope_restricted: false,
            expires_at: None,
            revoked_at: None,
        }
    }

    fn context(grant: &Grant, tier: Tier) -> EvaluationContext<'_> {
        let mut context = EvaluationContext::new(100, Some(grant));
        context.session_tier = Some(tier);
        context.confirmed = true;
        context.step_up_satisfied = true;
        context.live_history_days = 30;
        context
    }

    #[test]
    fn complete_tier_matrix_is_monotonic() {
        for action in Action::ALL_TIER_ACTIONS {
            for number in 1..=5 {
                let tier = Tier::try_from(number).expect("closed matrix tier");
                let grant = grant(tier);
                let decision = evaluate(&human(), action, context(&grant, tier));
                let expected =
                    if tier >= action.minimum_tier() { Verdict::Allow } else { Verdict::Deny };
                assert_eq!(decision.verdict, expected, "action={action:?}, tier={tier:?}");
            }
        }
    }

    #[test]
    fn tier_three_requires_confirmation_for_every_mutation() {
        let grant = grant(Tier::ConfirmEveryAction);
        let mut context = context(&grant, Tier::ConfirmEveryAction);
        context.confirmed = false;
        assert_eq!(
            evaluate(&human(), Action::SubmitPaperOrder, context).verdict,
            Verdict::ConfirmRequired
        );
    }

    #[test]
    fn agent_never_inherits_human_session_tier() {
        let actor = Actor { id: "agent".into(), kind: ActorKind::Agent };
        let grant = Grant {
            id: "agent-grant".into(),
            actor_id: "agent".into(),
            actor_kind: ActorKind::Agent,
            tier: Tier::ReadOnly,
            scopes: HashSet::new(),
            scope_restricted: false,
            expires_at: None,
            revoked_at: None,
        };
        let mut context = EvaluationContext::new(100, Some(&grant));
        context.session_tier = Some(Tier::YoloWithinHardCaps);
        assert_eq!(evaluate(&actor, Action::SubmitPaperOrder, context).verdict, Verdict::Deny);
    }

    #[test]
    fn expired_revoked_and_out_of_scope_grants_deny_mid_session() {
        let mut expired = grant(Tier::YoloWithinHardCaps);
        expired.expires_at = Some(100);
        assert_eq!(
            evaluate(&human(), Action::Query, context(&expired, Tier::YoloWithinHardCaps))
                .deciding_rule,
            "grant.tier_expired"
        );
        let mut revoked = grant(Tier::YoloWithinHardCaps);
        revoked.revoked_at = Some(99);
        assert_eq!(
            evaluate(&human(), Action::Query, context(&revoked, Tier::YoloWithinHardCaps))
                .deciding_rule,
            "grant.revoked"
        );
        let mut scoped = grant(Tier::YoloWithinHardCaps);
        scoped.scope_restricted = true;
        scoped.scopes.insert("data.query".into());
        assert_eq!(
            evaluate(&human(), Action::Metrics, context(&scoped, Tier::YoloWithinHardCaps))
                .deciding_rule,
            "grant.scope_denied"
        );
        scoped.scopes.clear();
        assert_eq!(
            evaluate(&human(), Action::Query, context(&scoped, Tier::YoloWithinHardCaps))
                .deciding_rule,
            "grant.scope_denied",
            "an explicitly empty allowlist must deny all scopes"
        );
    }

    #[test]
    fn hard_denies_three_through_six_hold_at_tier_five() {
        let grant = grant(Tier::YoloWithinHardCaps);
        let context = context(&grant, Tier::YoloWithinHardCaps);
        for action in [
            Action::ReadSecretMaterial,
            Action::SetLiveEnabled,
            Action::RaiseCapsProgrammatically,
            Action::DisableSafetyControl,
        ] {
            assert_eq!(evaluate(&human(), action, context).verdict, Verdict::Deny, "{action:?}");
        }
        let mut wallet = context;
        wallet.wallet_above_threshold = true;
        wallet.fresh_human_wallet_approval = false;
        assert_eq!(evaluate(&human(), Action::WalletTransfer, wallet).verdict, Verdict::Deny);
    }

    #[test]
    fn fresh_step_up_is_required_for_irreversible_actions() {
        let grant = grant(Tier::YoloWithinHardCaps);
        let mut context = context(&grant, Tier::YoloWithinHardCaps);
        context.step_up_satisfied = false;
        assert_eq!(
            evaluate(&human(), Action::ActivateCaps, context).verdict,
            Verdict::StepUpRequired
        );
        context.live_history_days = 29;
        assert_eq!(
            evaluate(&human(), Action::SubmitLiveOrder, context).verdict,
            Verdict::StepUpRequired
        );
    }
}

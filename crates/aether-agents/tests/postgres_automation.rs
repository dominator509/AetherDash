#![allow(clippy::expect_used)]

use aether_agents::persistence::{PersistenceError, PgAgentRepository};
use aether_agents::proposal::{EvidenceKind, ImprovementEvidence, ProposalStatus, ProposalStore};
use aether_agents::scheduler::{AutomationBudget, CalendarMinute, CronExpression};
use aether_authz::{Action, Actor, ActorKind, EvaluationContext, Grant, Tier};
use rust_decimal::Decimal;
use std::collections::HashSet;

#[tokio::test]
#[ignore = "requires migrated Postgres; run through scripts/test-integration.sh"]
async fn schedule_budget_and_revocation_survive_repository_restart() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = sqlx::PgPool::connect(&url).await.expect("connect");
    sqlx::query("DELETE FROM agent_automations WHERE id = 'ep406-durable-fixture'")
        .execute(&pool)
        .await
        .expect("clear fixture");
    let owner = Actor { id: "operator".into(), kind: ActorKind::Human };
    let owner_grant = Grant {
        id: "owner-grant".into(),
        actor_id: owner.id.clone(),
        actor_kind: ActorKind::Human,
        tier: Tier::BoundedAutopilot,
        scopes: HashSet::from([Action::ScheduleAutomation.scope().into()]),
        scope_restricted: true,
        expires_at: None,
        revoked_at: None,
    };
    let mut owner_context = EvaluationContext::new(1, Some(&owner_grant));
    owner_context.session_tier = Some(Tier::BoundedAutopilot);
    owner_context.confirmed = true;
    let automation_grant = Grant {
        id: "automation-grant".into(),
        actor_id: "ep406-automation".into(),
        actor_kind: ActorKind::Automation,
        tier: Tier::ReadOnly,
        scopes: HashSet::from([Action::Query.scope().into()]),
        scope_restricted: true,
        expires_at: Some(10_000),
        revoked_at: None,
    };
    PgAgentRepository::new(pool.clone())
        .schedule(
            "ep406-durable-fixture",
            &owner,
            owner_context,
            &automation_grant,
            Action::Query,
            CronExpression::parse("0 12 * * *").expect("cron"),
            AutomationBudget { max_runs: 1, max_cost_minor: 5, cost_per_run_minor: 5 },
        )
        .await
        .expect("schedule");

    let restarted = PgAgentRepository::new(pool.clone());
    let slot = CalendarMinute {
        unix_minute: 100,
        minute: 0,
        hour: 12,
        day_of_month: 1,
        month: 1,
        day_of_week: 1,
    };
    let dispatch = restarted
        .claim_due("ep406-durable-fixture", slot, &automation_grant)
        .await
        .expect("claim")
        .expect("due");
    assert_eq!(dispatch.run_number, 1);
    let mut next = slot;
    next.unix_minute += 1;
    assert!(matches!(
        restarted.claim_due("ep406-durable-fixture", next, &automation_grant).await,
        Err(PersistenceError::BudgetExceeded)
    ));
    assert!(restarted.revoke("ep406-durable-fixture").await.expect("revoke"));
    assert!(restarted
        .claim_due("ep406-durable-fixture", next, &automation_grant)
        .await
        .expect("revoked")
        .is_none());
}

#[tokio::test]
#[ignore = "requires migrated Postgres; run through scripts/test-integration.sh"]
async fn metric_cited_proposal_requires_durable_human_step_up_authorization() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = sqlx::PgPool::connect(&url).await.expect("connect");
    sqlx::query("DELETE FROM improvement_proposals WHERE id = 'ep406-proposal-fixture'")
        .execute(&pool)
        .await
        .expect("clear fixture");
    let mut memory = ProposalStore::default();
    let proposal = memory
        .propose(
            "ep406-proposal-fixture",
            "Reduce scanner p95",
            "--- a/scanner.rs\n+++ b/scanner.rs\n@@ -1 +1 @@\n-old\n+new",
            vec![ImprovementEvidence {
                kind: EvidenceKind::Metric,
                source_id: "aether_scan_cycle_ms:p95:2026-07-18".into(),
                value: Decimal::new(125, 0),
                observed_at: 100,
            }],
        )
        .expect("proposal")
        .clone();
    let repo = PgAgentRepository::new(pool);
    repo.insert_proposal(&proposal).await.expect("persist inert proposal");

    let human = Actor { id: "operator".into(), kind: ActorKind::Human };
    let grant = Grant {
        id: "proposal-grant".into(),
        actor_id: human.id.clone(),
        actor_kind: ActorKind::Human,
        tier: Tier::BoundedAutopilot,
        scopes: HashSet::from([Action::ApplySelfImprovement.scope().into()]),
        scope_restricted: true,
        expires_at: None,
        revoked_at: None,
    };
    let mut context = EvaluationContext::new(100, Some(&grant));
    context.session_tier = Some(Tier::BoundedAutopilot);
    assert!(matches!(
        repo.authorize_proposal_application("ep406-proposal-fixture", &human, context).await,
        Err(PersistenceError::HumanStepUpRequired)
    ));
    context.step_up_satisfied = true;
    let receipt = repo
        .authorize_proposal_application("ep406-proposal-fixture", &human, context)
        .await
        .expect("human authorization");
    assert_eq!(receipt.proposal_digest(), proposal.digest);
    assert_eq!(
        repo.proposal_status("ep406-proposal-fixture").await.expect("status"),
        Some(ProposalStatus::ApplicationAuthorized)
    );
}

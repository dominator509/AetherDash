use crate::proposal::{HumanApplicationReceipt, ImprovementProposal, ProposalStatus};
use crate::scheduler::{AutomationBudget, CalendarMinute, CronExpression};
use aether_authz::{evaluate, Action, Actor, ActorKind, EvaluationContext, Grant, Verdict};
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct PgAgentRepository {
    pool: PgPool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutomationDispatch {
    pub action: Action,
    pub run_number: u64,
    pub total_cost_minor: u64,
}

impl PgAgentRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn schedule(
        &self,
        id: &str,
        owner: &Actor,
        owner_context: EvaluationContext<'_>,
        automation_grant: &Grant,
        action: Action,
        cron: CronExpression,
        budget: AutomationBudget,
    ) -> Result<(), PersistenceError> {
        if id.is_empty()
            || automation_grant.actor_kind != ActorKind::Automation
            || budget.max_runs == 0
            || budget.cost_per_run_minor > budget.max_cost_minor
            || evaluate(owner, Action::ScheduleAutomation, owner_context).verdict != Verdict::Allow
        {
            return Err(PersistenceError::ScheduleDenied);
        }
        let automation_actor =
            Actor { id: automation_grant.actor_id.clone(), kind: ActorKind::Automation };
        if evaluate(
            &automation_actor,
            action,
            EvaluationContext::new(owner_context.now, Some(automation_grant)),
        )
        .verdict
            != Verdict::Allow
        {
            return Err(PersistenceError::ExecutionDenied);
        }
        sqlx::query(
            "INSERT INTO agent_automations \
             (id, actor_id, grant_id, action, cron, max_runs, max_cost_minor, cost_per_run_minor) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(&automation_grant.actor_id)
        .bind(&automation_grant.id)
        .bind(sqlx::types::Json(action))
        .bind(sqlx::types::Json(cron))
        .bind(to_i64(budget.max_runs)?)
        .bind(to_i64(budget.max_cost_minor)?)
        .bind(to_i64(budget.cost_per_run_minor)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn claim_due(
        &self,
        id: &str,
        slot: CalendarMinute,
        current_grant: &Grant,
    ) -> Result<Option<AutomationDispatch>, PersistenceError> {
        type Row = (
            String,
            String,
            sqlx::types::Json<Action>,
            sqlx::types::Json<CronExpression>,
            i64,
            i64,
            i64,
            i64,
            i64,
            Option<i64>,
            String,
        );
        let mut tx = self.pool.begin().await?;
        let row: Row = sqlx::query_as(
            "SELECT actor_id, grant_id, action, cron, max_runs, max_cost_minor, \
                    cost_per_run_minor, runs, cost_minor, last_slot, status \
             FROM agent_automations WHERE id = $1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound)?;
        if row.10 != "active" || row.9 == Some(to_i64(slot.unix_minute)?) || !row.3 .0.matches(slot)
        {
            tx.rollback().await?;
            return Ok(None);
        }
        if row.1 != current_grant.id {
            tx.rollback().await?;
            return Err(PersistenceError::GrantMismatch);
        }
        let actor = Actor { id: row.0, kind: ActorKind::Automation };
        let context =
            EvaluationContext::new(slot.unix_minute.saturating_mul(60), Some(current_grant));
        if evaluate(&actor, row.2 .0, context).verdict != Verdict::Allow {
            tx.rollback().await?;
            return Err(PersistenceError::ExecutionDenied);
        }
        let runs = row.7.checked_add(1).ok_or(PersistenceError::BudgetExceeded)?;
        let cost = row.8.checked_add(row.6).ok_or(PersistenceError::BudgetExceeded)?;
        if runs > row.4 || cost > row.5 {
            tx.rollback().await?;
            return Err(PersistenceError::BudgetExceeded);
        }
        sqlx::query(
            "UPDATE agent_automations SET runs = $2, cost_minor = $3, last_slot = $4, \
             updated_ts = now() WHERE id = $1",
        )
        .bind(id)
        .bind(runs)
        .bind(cost)
        .bind(to_i64(slot.unix_minute)?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(AutomationDispatch {
            action: row.2 .0,
            run_number: u64::try_from(runs).map_err(|_| PersistenceError::Range)?,
            total_cost_minor: u64::try_from(cost).map_err(|_| PersistenceError::Range)?,
        }))
    }

    pub async fn pause(&self, id: &str) -> Result<bool, sqlx::Error> {
        transition(&self.pool, id, "active", "paused").await
    }

    pub async fn revoke(&self, id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE agent_automations SET status = 'revoked', updated_ts = now() \
             WHERE id = $1 AND status <> 'revoked'",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn insert_proposal(&self, proposal: &ImprovementProposal) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO improvement_proposals \
             (id, summary, unified_diff, evidence, digest, status) \
             VALUES ($1, $2, $3, $4, $5, 'proposed')",
        )
        .bind(&proposal.id)
        .bind(&proposal.summary)
        .bind(&proposal.unified_diff)
        .bind(sqlx::types::Json(&proposal.evidence))
        .bind(&proposal.digest)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn authorize_proposal_application(
        &self,
        id: &str,
        actor: &Actor,
        context: EvaluationContext<'_>,
    ) -> Result<HumanApplicationReceipt, PersistenceError> {
        if actor.kind != ActorKind::Human
            || evaluate(actor, Action::ApplySelfImprovement, context).verdict != Verdict::Allow
        {
            return Err(PersistenceError::HumanStepUpRequired);
        }
        let mut tx = self.pool.begin().await?;
        let digest = sqlx::query_scalar::<_, String>(
            "SELECT digest FROM improvement_proposals \
             WHERE id = $1 AND status = 'proposed' FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::ProposalNotFoundOrFinal)?;
        let result = sqlx::query(
            "UPDATE improvement_proposals SET status = 'application_authorized', \
             authorized_by = $2, authorized_ts = now(), updated_ts = now() \
             WHERE id = $1 AND status = 'proposed'",
        )
        .bind(id)
        .bind(&actor.id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            tx.rollback().await?;
            return Err(PersistenceError::ProposalNotFoundOrFinal);
        }
        tx.commit().await?;
        Ok(HumanApplicationReceipt::new(id.into(), digest, actor.id.clone()))
    }

    pub async fn proposal_status(&self, id: &str) -> Result<Option<ProposalStatus>, sqlx::Error> {
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM improvement_proposals WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(status.and_then(|value| match value.as_str() {
            "proposed" => Some(ProposalStatus::Proposed),
            "application_authorized" => Some(ProposalStatus::ApplicationAuthorized),
            "rejected" => Some(ProposalStatus::Rejected),
            _ => None,
        }))
    }
}

async fn transition(pool: &PgPool, id: &str, from: &str, to: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE agent_automations SET status = $3, updated_ts = now() \
         WHERE id = $1 AND status = $2",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

fn to_i64(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| PersistenceError::Range)
}

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("automation schedule denied")]
    ScheduleDenied,
    #[error("automation not found")]
    NotFound,
    #[error("current grant does not match the scheduled automation")]
    GrantMismatch,
    #[error("current grant denied automation execution")]
    ExecutionDenied,
    #[error("automation budget exceeded")]
    BudgetExceeded,
    #[error("numeric value exceeds durable range")]
    Range,
    #[error("proposal application requires a human and fresh step-up")]
    HumanStepUpRequired,
    #[error("proposal is missing or already final")]
    ProposalNotFoundOrFinal,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

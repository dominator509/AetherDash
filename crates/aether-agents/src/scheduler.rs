use aether_authz::{evaluate, Action, Actor, ActorKind, EvaluationContext, Grant, Verdict};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarMinute {
    pub unix_minute: u64,
    pub minute: u8,
    pub hour: u8,
    pub day_of_month: u8,
    pub month: u8,
    pub day_of_week: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum CronField {
    Any,
    Exact(u8),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CronExpression {
    minute: CronField,
    hour: CronField,
    day_of_month: CronField,
    month: CronField,
    day_of_week: CronField,
}

impl CronExpression {
    pub fn parse(value: &str) -> Result<Self, SchedulerError> {
        let fields: Vec<_> = value.split_ascii_whitespace().collect();
        if fields.len() != 5 {
            return Err(SchedulerError::InvalidCron);
        }
        Ok(Self {
            minute: parse_field(fields[0], 0, 59)?,
            hour: parse_field(fields[1], 0, 23)?,
            day_of_month: parse_field(fields[2], 1, 31)?,
            month: parse_field(fields[3], 1, 12)?,
            day_of_week: parse_field(fields[4], 0, 6)?,
        })
    }

    pub(crate) fn matches(self, value: CalendarMinute) -> bool {
        field_matches(self.minute, value.minute)
            && field_matches(self.hour, value.hour)
            && field_matches(self.day_of_month, value.day_of_month)
            && field_matches(self.month, value.month)
            && field_matches(self.day_of_week, value.day_of_week)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutomationBudget {
    pub max_runs: u64,
    pub max_cost_minor: u64,
    pub cost_per_run_minor: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationStatus {
    Active,
    Paused,
    Revoked,
}

#[derive(Debug, Clone)]
struct ScheduledAutomation {
    id: String,
    actor: Actor,
    grant_id: String,
    action: Action,
    cron: CronExpression,
    budget: AutomationBudget,
    runs: u64,
    cost_minor: u64,
    last_slot: Option<u64>,
    status: AutomationStatus,
}

#[derive(Debug, Default)]
pub struct AutomationScheduler {
    tasks: HashMap<String, ScheduledAutomation>,
}

impl AutomationScheduler {
    pub fn schedule(
        &mut self,
        id: &str,
        owner: &Actor,
        owner_context: EvaluationContext<'_>,
        automation_grant: &Grant,
        action: Action,
        cron: CronExpression,
        budget: AutomationBudget,
    ) -> Result<(), SchedulerError> {
        if id.is_empty()
            || self.tasks.contains_key(id)
            || budget.max_runs == 0
            || budget.cost_per_run_minor > budget.max_cost_minor
        {
            return Err(SchedulerError::InvalidSchedule);
        }
        if evaluate(owner, Action::ScheduleAutomation, owner_context).verdict != Verdict::Allow {
            return Err(SchedulerError::ScheduleDenied);
        }
        if automation_grant.actor_kind != ActorKind::Automation {
            return Err(SchedulerError::AutomationGrantRequired);
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
            return Err(SchedulerError::ExecutionDenied);
        }
        self.tasks.insert(
            id.into(),
            ScheduledAutomation {
                id: id.into(),
                actor: Actor { id: automation_grant.actor_id.clone(), kind: ActorKind::Automation },
                grant_id: automation_grant.id.clone(),
                action,
                cron,
                budget,
                runs: 0,
                cost_minor: 0,
                last_slot: None,
                status: AutomationStatus::Active,
            },
        );
        Ok(())
    }

    pub fn run_due<T>(
        &mut self,
        id: &str,
        slot: CalendarMinute,
        current_grant: &Grant,
        run: impl FnOnce() -> T,
    ) -> Result<Option<T>, SchedulerError> {
        let task = self.tasks.get_mut(id).ok_or(SchedulerError::NotFound)?;
        if task.status != AutomationStatus::Active
            || task.last_slot == Some(slot.unix_minute)
            || !task.cron.matches(slot)
        {
            return Ok(None);
        }
        if current_grant.id != task.grant_id {
            return Err(SchedulerError::GrantMismatch);
        }
        let context =
            EvaluationContext::new(slot.unix_minute.saturating_mul(60), Some(current_grant));
        if evaluate(&task.actor, task.action, context).verdict != Verdict::Allow {
            return Err(SchedulerError::ExecutionDenied);
        }
        let runs = task.runs.checked_add(1).ok_or(SchedulerError::BudgetExceeded)?;
        let cost = task
            .cost_minor
            .checked_add(task.budget.cost_per_run_minor)
            .ok_or(SchedulerError::BudgetExceeded)?;
        if runs > task.budget.max_runs || cost > task.budget.max_cost_minor {
            return Err(SchedulerError::BudgetExceeded);
        }
        // Attempts consume budget before dispatch, including failed downstream work.
        task.runs = runs;
        task.cost_minor = cost;
        task.last_slot = Some(slot.unix_minute);
        Ok(Some(run()))
    }

    pub fn pause(&mut self, id: &str) -> Result<(), SchedulerError> {
        let task = self.tasks.get_mut(id).ok_or(SchedulerError::NotFound)?;
        if task.status == AutomationStatus::Revoked {
            return Err(SchedulerError::InvalidTransition);
        }
        task.status = AutomationStatus::Paused;
        Ok(())
    }

    pub fn revoke(&mut self, id: &str) -> Result<(), SchedulerError> {
        self.tasks.get_mut(id).ok_or(SchedulerError::NotFound)?.status = AutomationStatus::Revoked;
        Ok(())
    }

    #[must_use]
    pub fn status(&self, id: &str) -> Option<AutomationStatus> {
        self.tasks.get(id).map(|task| {
            debug_assert_eq!(task.id, id);
            task.status
        })
    }
}

fn parse_field(value: &str, min: u8, max: u8) -> Result<CronField, SchedulerError> {
    if value == "*" {
        return Ok(CronField::Any);
    }
    let value: u8 = value.parse().map_err(|_| SchedulerError::InvalidCron)?;
    if !(min..=max).contains(&value) {
        return Err(SchedulerError::InvalidCron);
    }
    Ok(CronField::Exact(value))
}

const fn field_matches(field: CronField, value: u8) -> bool {
    matches!(field, CronField::Any) || matches!(field, CronField::Exact(exact) if exact == value)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SchedulerError {
    #[error("cron must be five fields containing '*' or exact bounded integers")]
    InvalidCron,
    #[error("automation schedule is invalid or duplicated")]
    InvalidSchedule,
    #[error("scheduling was denied by canonical authorization")]
    ScheduleDenied,
    #[error("task requires its own automation grant")]
    AutomationGrantRequired,
    #[error("automation not found")]
    NotFound,
    #[error("current grant does not match the scheduled automation")]
    GrantMismatch,
    #[error("automation execution denied by current grant")]
    ExecutionDenied,
    #[error("automation run or cost budget exceeded")]
    BudgetExceeded,
    #[error("invalid automation lifecycle transition")]
    InvalidTransition,
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_authz::Tier;
    use std::collections::HashSet;

    fn grant(id: &str, actor: &str, kind: ActorKind, tier: Tier, scope: &str) -> Grant {
        Grant {
            id: id.into(),
            actor_id: actor.into(),
            actor_kind: kind,
            tier,
            scopes: HashSet::from([scope.into()]),
            scope_restricted: true,
            expires_at: Some(10_000),
            revoked_at: None,
        }
    }

    fn slot(minute: u64) -> CalendarMinute {
        CalendarMinute {
            unix_minute: minute,
            minute: 0,
            hour: 12,
            day_of_month: 1,
            month: 1,
            day_of_week: 1,
        }
    }

    fn scheduled() -> (AutomationScheduler, Grant) {
        let owner = Actor { id: "operator".into(), kind: ActorKind::Human };
        let owner_grant = grant(
            "owner-grant",
            "operator",
            ActorKind::Human,
            Tier::BoundedAutopilot,
            Action::ScheduleAutomation.scope(),
        );
        let mut owner_context = EvaluationContext::new(1, Some(&owner_grant));
        owner_context.session_tier = Some(Tier::BoundedAutopilot);
        owner_context.confirmed = true;
        let automation_grant = grant(
            "automation-grant",
            "daily-reader",
            ActorKind::Automation,
            Tier::ReadOnly,
            Action::Query.scope(),
        );
        let mut scheduler = AutomationScheduler::default();
        scheduler
            .schedule(
                "daily-reader",
                &owner,
                owner_context,
                &automation_grant,
                Action::Query,
                CronExpression::parse("0 12 * * *").expect("cron"),
                AutomationBudget { max_runs: 1, max_cost_minor: 5, cost_per_run_minor: 5 },
            )
            .expect("schedule");
        (scheduler, automation_grant)
    }

    #[test]
    fn cron_run_is_deduplicated_and_budget_pre_authorized() {
        let (mut scheduler, grant) = scheduled();
        assert_eq!(scheduler.run_due("daily-reader", slot(100), &grant, || 7), Ok(Some(7)));
        assert_eq!(scheduler.run_due("daily-reader", slot(100), &grant, || 8), Ok(None));
        assert_eq!(
            scheduler.run_due("daily-reader", slot(101), &grant, || 9),
            Err(SchedulerError::BudgetExceeded)
        );
    }

    #[test]
    fn grant_revocation_and_scheduler_revocation_are_immediate() {
        let (mut scheduler, mut grant) = scheduled();
        grant.revoked_at = Some(2);
        assert_eq!(
            scheduler.run_due("daily-reader", slot(100), &grant, || ()),
            Err(SchedulerError::ExecutionDenied)
        );
        scheduler.revoke("daily-reader").expect("revoke task");
        grant.revoked_at = None;
        assert_eq!(scheduler.run_due("daily-reader", slot(101), &grant, || 1), Ok(None));
    }

    #[test]
    fn scope_and_cron_are_fail_closed() {
        assert_eq!(CronExpression::parse("*/5 * * * *"), Err(SchedulerError::InvalidCron));
        let (mut scheduler, mut grant) = scheduled();
        grant.scopes.clear();
        assert_eq!(
            scheduler.run_due("daily-reader", slot(100), &grant, || ()),
            Err(SchedulerError::ExecutionDenied)
        );
    }
}

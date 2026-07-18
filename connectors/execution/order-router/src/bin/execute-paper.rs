//! Authoritative one-shot paper execution transport.
//!
//! The server plane supplies a fresh database snapshot. This binary repeats
//! canonical authorization and risk checks in the router, persists only the
//! router-produced fills, and drains the durable fill outbox before reporting
//! completion. Input and output are JSON over stdio; no shell is involved.

use std::collections::HashSet;
use std::io::{self, Read};
use std::sync::Arc;

use aether_authz::{Actor, ActorKind, EvaluationContext, Grant, Tier};
use aether_bus::producer::KafkaProducer;
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::market::Market;
use aether_core::order::{CapsSnapshot, OrderIntent};
use aether_core::quote::OrderBook;
use aether_core::time::UtcTime;
use aether_order_router::router::{
    MemoryExecutionAuditSink, OrderRouter, RouterAuthContext, RouterConfig,
};
use aether_paper_ledger::ledger::{LedgerOrder, OrderStatus, PaperLedger};
use aether_paper_ledger::persistence::PersistOutcome;
use aether_paper_ledger::{PaperExecutionService, PostgresLedgerStore};
use aether_risk_engine::engine::{Balances, PositionOutcome, RiskContext, VenueHealthStatus};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Request {
    actor: ActorWire,
    approval_id: Option<Ulid>,
    intent: OrderIntent,
    book: OrderBook,
    risk: RiskWire,
    opportunity_id: Option<Ulid>,
}

#[derive(Debug, Deserialize)]
struct ActorWire {
    id: String,
    kind: ActorKind,
}

#[derive(Debug, Deserialize)]
struct BalanceWire {
    venue: VenueId,
    #[serde(with = "aether_core::decimal::decimal_string")]
    free: Decimal,
    #[serde(with = "aether_core::decimal::decimal_string")]
    locked: Decimal,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct PositionWire {
    market: MarketKey,
    outcome: PositionOutcomeWire,
    #[serde(with = "aether_core::decimal::decimal_string")]
    size: Decimal,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PositionOutcomeWire {
    Yes,
    No,
}

#[derive(Debug, Deserialize)]
struct VenueHealthWire {
    venue: VenueId,
    status: String,
    breaker_open: bool,
}

#[derive(Debug, Deserialize)]
struct JurisdictionWire {
    venue: VenueId,
    eligible: bool,
}

#[derive(Debug, Deserialize)]
struct RiskWire {
    evaluated_at: UtcTime,
    markets: Vec<Market>,
    balances: Vec<BalanceWire>,
    #[serde(default)]
    positions: Vec<PositionWire>,
    venue_health: Vec<VenueHealthWire>,
    active_caps: CapsSnapshot,
    caps_by_version: Vec<CapsSnapshot>,
    #[serde(with = "aether_core::decimal::decimal_string")]
    daily_notional: Decimal,
    jurisdiction_eligible: Vec<JurisdictionWire>,
    live_enabled: bool,
}

#[derive(Debug, Serialize)]
struct Response {
    status: &'static str,
    order_id: String,
    fill_count: usize,
    replayed: bool,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("paper execution rejected: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let request: Request = serde_json::from_str(&raw)?;
    if !request.intent.paper {
        return Err("authoritative paper entrypoint refuses live intents".into());
    }

    let database_url = std::env::var("DATABASE_URL")?;
    let pool = sqlx::PgPool::connect(&database_url).await?;
    let actor = Actor { id: request.actor.id, kind: request.actor.kind };
    let grant = load_current_grant(&pool, &actor).await?;
    let confirmed = if grant.tier >= Tier::YoloWithinHardCaps {
        true
    } else {
        validate_approval(&pool, request.approval_id, &actor.id, request.opportunity_id).await?
    };
    let mut evaluation =
        EvaluationContext::new(unix_seconds(request.risk.evaluated_at)?, Some(&grant));
    evaluation.session_tier = (actor.kind == ActorKind::Human).then_some(grant.tier);
    evaluation.confirmed = confirmed;

    let risk = into_risk_context(request.risk);
    let audit = Arc::new(MemoryExecutionAuditSink::default());
    let router = OrderRouter::new(RouterConfig::default(), audit);
    let result = router.submit(
        request.intent.clone(),
        &request.book,
        &risk,
        &RouterAuthContext { actor: &actor, evaluation },
    )?;

    let store = PostgresLedgerStore::new(pool);
    let producer = KafkaProducer::from_env()?;
    let service = PaperExecutionService::new(PaperLedger::new(), producer);
    if let Some(existing) = store.existing_execution(&request.intent).await? {
        service.flush_persisted_outbox(&store).await?;
        let response = Response {
            status: "completed",
            order_id: request.intent.id.to_string(),
            fill_count: existing.len(),
            replayed: true,
        };
        serde_json::to_writer(io::stdout().lock(), &response)?;
        return Ok(());
    }

    let now = UtcTime::now();
    let total_filled: Decimal = result.fills.iter().map(|fill| fill.size).sum();
    let status = if total_filled >= request.intent.size {
        OrderStatus::Filled
    } else {
        OrderStatus::PartiallyFilled
    };
    let order = LedgerOrder {
        order_id: request.intent.id,
        intent: request.intent.clone(),
        status,
        created_ts: now,
        updated_ts: now,
    };
    let persisted = store.persist_execution(&order, &result.fills).await?;
    let authoritative_fills = if matches!(persisted, PersistOutcome::AlreadyExists) {
        store
            .existing_execution(&request.intent)
            .await?
            .ok_or("durable idempotency winner has no fills")?
    } else {
        result.fills
    };
    service.flush_persisted_outbox(&store).await?;
    let response = Response {
        status: "completed",
        order_id: request.intent.id.to_string(),
        fill_count: authoritative_fills.len(),
        replayed: result.replayed || matches!(persisted, PersistOutcome::AlreadyExists),
    };
    serde_json::to_writer(io::stdout().lock(), &response)?;
    Ok(())
}

async fn load_current_grant(
    pool: &sqlx::PgPool,
    actor: &Actor,
) -> Result<Grant, Box<dyn std::error::Error + Send + Sync>> {
    let kind = match actor.kind {
        ActorKind::Human => "human",
        ActorKind::Agent => "agent",
        ActorKind::Automation => "automation",
    };
    let row: Option<(String, i32, serde_json::Value, Option<i64>)> = sqlx::query_as(
        "SELECT id, tier, scopes, extract(epoch from expires_ts)::bigint \
         FROM permission_grants WHERE actor_id=$1 AND actor_kind=$2 \
         AND revoked_ts IS NULL AND (expires_ts IS NULL OR expires_ts > now()) \
         ORDER BY tier DESC, expires_ts DESC NULLS FIRST, id ASC LIMIT 1",
    )
    .bind(&actor.id)
    .bind(kind)
    .fetch_optional(pool)
    .await?;
    let (id, tier, raw_scopes, expires_at) = row.ok_or("current grant is unavailable")?;
    let (scopes, scope_restricted) = parse_scopes(&raw_scopes)?;
    Ok(Grant {
        id,
        actor_id: actor.id.clone(),
        actor_kind: actor.kind,
        tier: Tier::try_from(u8::try_from(tier)?)?,
        scopes,
        scope_restricted,
        expires_at: expires_at.map(u64::try_from).transpose()?,
        revoked_at: None,
    })
}

fn parse_scopes(
    raw: &serde_json::Value,
) -> Result<(HashSet<String>, bool), Box<dyn std::error::Error + Send + Sync>> {
    if raw.is_null() || raw.as_object().is_some_and(serde_json::Map::is_empty) {
        return Ok((HashSet::new(), false));
    }
    let values = raw
        .as_array()
        .or_else(|| raw.get("allowed").and_then(serde_json::Value::as_array))
        .ok_or("grant scopes are malformed")?;
    let scopes = values
        .iter()
        .map(|value| value.as_str().map(str::to_owned).ok_or("grant scope is not a string"))
        .collect::<Result<HashSet<_>, _>>()?;
    Ok((scopes, true))
}

async fn validate_approval(
    pool: &sqlx::PgPool,
    approval_id: Option<Ulid>,
    actor_id: &str,
    opportunity_id: Option<Ulid>,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let approval_id = approval_id.ok_or("current tier requires a consumed approval")?;
    let opportunity_id = opportunity_id.ok_or("approval target is unavailable")?;
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM approval_references \
         WHERE id=$1 AND actor_id=$2 AND target_id=$3 AND action='execute_paper' \
         AND status='approved' AND consumed_ts IS NOT NULL AND expires_ts > now())",
    )
    .bind(approval_id.to_string())
    .bind(actor_id)
    .bind(opportunity_id.to_string())
    .fetch_one(pool)
    .await?;
    if !valid {
        return Err("approval is not current and action-bound".into());
    }
    Ok(true)
}

fn unix_seconds(value: UtcTime) -> Result<u64, &'static str> {
    let millis = value.unix_millis();
    if millis < 0 {
        return Err("evaluation time predates unix epoch");
    }
    Ok((millis as u64) / 1_000)
}

fn into_risk_context(wire: RiskWire) -> RiskContext {
    let markets = wire.markets.into_iter().map(|market| (market.key.clone(), market)).collect();
    let balances = wire
        .balances
        .into_iter()
        .map(|item| {
            (item.venue, Balances { free: item.free, locked: item.locked, currency: item.currency })
        })
        .collect();
    let positions = wire
        .positions
        .into_iter()
        .map(|item| {
            let outcome = match item.outcome {
                PositionOutcomeWire::Yes => PositionOutcome::Yes,
                PositionOutcomeWire::No => PositionOutcome::No,
            };
            ((item.market, outcome), item.size)
        })
        .collect();
    let venue_health = wire
        .venue_health
        .into_iter()
        .map(|item| {
            (item.venue, VenueHealthStatus { status: item.status, breaker_open: item.breaker_open })
        })
        .collect();
    let caps_by_version =
        wire.caps_by_version.into_iter().map(|caps| (caps.version, caps)).collect();
    let jurisdiction_eligible =
        wire.jurisdiction_eligible.into_iter().map(|item| (item.venue, item.eligible)).collect();
    RiskContext {
        evaluated_at: wire.evaluated_at,
        markets,
        balances,
        positions,
        venue_health,
        active_caps: wire.active_caps,
        caps_by_version,
        daily_notional: wire.daily_notional,
        jurisdiction_eligible,
        live_enabled: wire.live_enabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scope_object_is_unrestricted() {
        let (scopes, restricted) = parse_scopes(&serde_json::json!({})).expect("valid scopes");
        assert!(scopes.is_empty());
        assert!(!restricted);
    }

    #[test]
    fn explicit_scope_allowlist_is_restricted() {
        let (scopes, restricted) =
            parse_scopes(&serde_json::json!({"allowed": ["orders.submit_paper"]}))
                .expect("valid scopes");
        assert!(restricted);
        assert!(scopes.contains("orders.submit_paper"));
    }

    #[test]
    fn malformed_scope_document_fails_closed() {
        assert!(parse_scopes(&serde_json::json!({"allowed": "orders.submit_paper"})).is_err());
    }
}

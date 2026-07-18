//! Durable SPEC-012 scanner lifecycle persistence.

use aether_bus::envelope::Envelope;
use aether_bus::producer::{MessageProducer, ProducerError};
use aether_bus::topics::Topic;
use aether_core::ids::Ulid;
use aether_core::opportunity::{Opportunity, OpportunityKind};
use aether_core::time::UtcTime;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};

#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("opportunity requires an expiry timestamp")]
    MissingExpiry,
    #[error("open opportunity for dedupe key disappeared")]
    MissingOpenChain,
    #[error("bus publish error: {0}")]
    Publish(#[from] ProducerError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistDisposition {
    Created(Ulid),
    Updated(Ulid),
    Existing(Ulid),
}

impl PersistDisposition {
    pub fn opportunity_id(self) -> Ulid {
        match self {
            Self::Created(id) | Self::Updated(id) | Self::Existing(id) => id,
        }
    }

    pub fn should_publish(self) -> bool {
        matches!(self, Self::Created(_))
    }
}

#[derive(Clone)]
pub struct LifecycleStore {
    pool: PgPool,
}

impl LifecycleStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Insert a detected opportunity and immediately append the scanner-owned
    /// `detected -> scored` transition. An existing open chain is updated in
    /// place before surfacing, never duplicated.
    pub async fn persist_scored(
        &self,
        opportunity: &Opportunity,
    ) -> Result<PersistDisposition, LifecycleError> {
        let expires = opportunity.expires_ts.ok_or(LifecycleError::MissingExpiry)?;
        let dedupe_key = dedupe_key(opportunity);
        let kind = kind_name(opportunity.kind);
        let legs = serde_json::to_value(&opportunity.legs)?;
        let edge = serde_json::to_value(&opportunity.edge)?;
        let explain_ref = serde_json::to_value(&opportunity.explain_ref)?;
        let mut tx = self.pool.begin().await?;

        let inserted: Option<String> = sqlx::query_scalar(
            "INSERT INTO opportunities \
             (id,kind,legs,gross_edge,edge,confidence,detected_ts,expires_ts,explain_ref,trace_id,state,dedupe_key) \
             VALUES ($1,$2,$3,$4,$5,$6,$7::timestamptz,$8::timestamptz,$9,$10,'detected',$11) \
             ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL AND state <> 'closed' \
             DO NOTHING RETURNING id",
        )
        .bind(opportunity.id.to_string())
        .bind(kind)
        .bind(&legs)
        .bind(opportunity.gross_edge)
        .bind(&edge)
        .bind(opportunity.confidence.value())
        .bind(opportunity.detected_ts.to_string())
        .bind(expires.to_string())
        .bind(&explain_ref)
        .bind(opportunity.trace_id.to_string())
        .bind(&dedupe_key)
        .fetch_optional(&mut *tx)
        .await?;

        if inserted.is_some() {
            append_transition_tx(
                &mut tx,
                opportunity.id,
                "detected",
                "scored",
                "scanner",
                json!({"dedupe_key": dedupe_key}),
            )
            .await?;
            sqlx::query(
                "INSERT INTO opportunity_detection_outbox (event_id,opportunity_id,payload) \
                 VALUES ($1,$2,$3)",
            )
            .bind(Ulid::new().to_string())
            .bind(opportunity.id.to_string())
            .bind(serde_json::to_value(opportunity)?)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(PersistDisposition::Created(opportunity.id));
        }

        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT id,state FROM opportunities \
             WHERE dedupe_key=$1 AND state <> 'closed' FOR UPDATE",
        )
        .bind(&dedupe_key)
        .fetch_optional(&mut *tx)
        .await?;
        let (id, state) = existing.ok_or(LifecycleError::MissingOpenChain)?;
        let id = Ulid::from_string(&id).map_err(|_| LifecycleError::MissingOpenChain)?;
        let disposition = if matches!(state.as_str(), "detected" | "scored") {
            sqlx::query(
                "UPDATE opportunities SET legs=$2,gross_edge=$3,edge=$4,confidence=$5,\
                 expires_ts=$6::timestamptz,explain_ref=$7,trace_id=$8,updated_ts=now() WHERE id=$1",
            )
            .bind(id.to_string())
            .bind(legs)
            .bind(opportunity.gross_edge)
            .bind(edge)
            .bind(opportunity.confidence.value())
            .bind(expires.to_string())
            .bind(explain_ref)
            .bind(opportunity.trace_id.to_string())
            .execute(&mut *tx)
            .await?;
            if state == "detected" {
                append_transition_tx(
                    &mut tx,
                    id,
                    "detected",
                    "scored",
                    "scanner",
                    json!({"deduplicated": true}),
                )
                .await?;
            }
            PersistDisposition::Updated(id)
        } else {
            PersistDisposition::Existing(id)
        };
        tx.commit().await?;
        Ok(disposition)
    }

    /// Expire every overdue scanner-owned chain and close it with explicit
    /// zero realized attribution. The injected timestamp makes sweeps replayable.
    pub async fn expire_due(&self, now: UtcTime) -> Result<Vec<Ulid>, LifecycleError> {
        let mut tx = self.pool.begin().await?;
        let due: Vec<(String, String, rust_decimal::Decimal)> = sqlx::query_as(
            "SELECT id,state,(edge->>'net_edge')::numeric FROM opportunities \
             WHERE state IN ('detected','scored') AND expires_ts <= $1::timestamptz \
             ORDER BY id FOR UPDATE",
        )
        .bind(now.to_string())
        .fetch_all(&mut *tx)
        .await?;
        let mut expired = Vec::with_capacity(due.len());
        for (id, state, predicted_net_edge) in due {
            let parsed = Ulid::from_string(&id).map_err(|_| LifecycleError::MissingOpenChain)?;
            append_transition_tx(
                &mut tx,
                parsed,
                &state,
                "expired",
                "scanner",
                json!({"reason": "ttl_expired"}),
            )
            .await?;
            sqlx::query(
                "INSERT INTO attribution \
                 (id,opportunity_id,predicted_net_edge,realized_pnl,outcome,closed_ts,reason_ignored,detail) \
                 VALUES ($1,$2,$3,0,'expired_pre_surface',$4::timestamptz,'ttl_expired',$5) \
                 ON CONFLICT (opportunity_id) DO NOTHING",
            )
            .bind(Ulid::new().to_string())
            .bind(&id)
            .bind(predicted_net_edge)
            .bind(now.to_string())
            .bind(json!({"reason": "ttl_expired", "realized": "0"}))
            .execute(&mut *tx)
            .await?;
            append_transition_tx(
                &mut tx,
                parsed,
                "expired",
                "closed",
                "attribution",
                json!({"reason": "ttl_expired"}),
            )
            .await?;
            expired.push(parsed);
        }
        tx.commit().await?;
        Ok(expired)
    }

    pub async fn open_count(&self) -> Result<i64, LifecycleError> {
        Ok(sqlx::query_scalar("SELECT count(*) FROM opportunities WHERE state <> 'closed'")
            .fetch_one(&self.pool)
            .await?)
    }

    /// Publish pending detections at least once. A crash after Kafka accepts a
    /// message but before `published_ts` may repeat it; the stable opportunity
    /// id and producer key make that duplicate explicit and deduplicable.
    pub async fn flush_detection_outbox<P: MessageProducer>(
        &self,
        producer: &P,
        limit: i64,
    ) -> Result<usize, LifecycleError> {
        let rows: Vec<(String, String, serde_json::Value)> = sqlx::query_as(
            "SELECT event_id,opportunity_id,payload FROM opportunity_detection_outbox \
             WHERE published_ts IS NULL ORDER BY created_ts,event_id LIMIT $1",
        )
        .bind(limit.clamp(1, 1_000))
        .fetch_all(&self.pool)
        .await?;
        let mut published = 0;
        for (event_id, opportunity_id, payload) in rows {
            let opportunity: Opportunity = serde_json::from_value(payload)?;
            let mut envelope = Envelope::new("opportunity", opportunity.clone());
            envelope.trace_id = opportunity.trace_id.to_string();
            envelope.ts = opportunity.detected_ts.to_string();
            producer.send(Topic::OPPS_DETECTED, envelope, Some(&opportunity_id)).await?;
            let changed = sqlx::query(
                "UPDATE opportunity_detection_outbox SET published_ts=now() \
                 WHERE event_id=$1 AND published_ts IS NULL",
            )
            .bind(event_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
            published += usize::try_from(changed).unwrap_or_default();
        }
        Ok(published)
    }
}

async fn append_transition_tx(
    tx: &mut Transaction<'_, Postgres>,
    opportunity_id: Ulid,
    from: &str,
    to: &str,
    actor: &str,
    detail: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO opportunity_events \
         (id,opportunity_id,from_state,to_state,actor,detail) VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(Ulid::new().to_string())
    .bind(opportunity_id.to_string())
    .bind(from)
    .bind(to)
    .bind(actor)
    .bind(detail)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn kind_name(kind: OpportunityKind) -> &'static str {
    match kind {
        OpportunityKind::Arbitrage => "arbitrage",
        OpportunityKind::Value => "value",
        OpportunityKind::Catalyst => "catalyst",
        OpportunityKind::Hedge => "hedge",
    }
}

pub fn dedupe_key(opportunity: &Opportunity) -> String {
    let mut legs: Vec<&str> = opportunity.legs.iter().map(|leg| leg.market.as_str()).collect();
    legs.sort_unstable();
    format!("{}|{}", kind_name(opportunity.kind), legs.join("|"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::decimal::Confidence;
    use aether_core::ids::{MarketKey, VenueId};
    use aether_core::opportunity::{BrainRef, EdgeCosts, EdgeDecomposition, OpportunityLeg};
    use aether_core::order::Side;
    use rust_decimal::Decimal;

    fn opportunity(legs: Vec<OpportunityLeg>) -> Opportunity {
        Opportunity {
            id: Ulid::new(),
            kind: OpportunityKind::Arbitrage,
            legs,
            gross_edge: Decimal::ONE,
            edge: EdgeDecomposition::compute(Decimal::ONE, EdgeCosts::zero()),
            confidence: Confidence::new(Decimal::ONE).unwrap(),
            detected_ts: UtcTime::from_unix_millis(1_000).unwrap(),
            expires_ts: UtcTime::from_unix_millis(31_000).ok(),
            explain_ref: BrainRef { object_id: Ulid::new(), provenance_hash: "test".into() },
            trace_id: Ulid::new(),
        }
    }

    #[test]
    fn key_is_independent_of_leg_order() {
        let a = MarketKey::new(&VenueId::new("kalshi").unwrap(), "a").unwrap();
        let b = MarketKey::new(&VenueId::new("polymarket").unwrap(), "b").unwrap();
        let leg =
            |market, side| OpportunityLeg { market, side, target_price: None, size_hint: None };
        let first = opportunity(vec![leg(a.clone(), Side::Buy), leg(b.clone(), Side::Sell)]);
        let second = opportunity(vec![leg(b, Side::Buy), leg(a, Side::Sell)]);
        assert_eq!(dedupe_key(&first), dedupe_key(&second));
    }
}

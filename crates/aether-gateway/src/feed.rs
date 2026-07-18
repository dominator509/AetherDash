//! Durable opportunity-to-WebSocket feed surfacing.

use crate::frames::ServerFrame;
use aether_core::opportunity::Opportunity;
use aether_core::Ulid;
use sqlx::PgPool;
use tokio::sync::broadcast;

#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("feed frame serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("opportunity {0} does not exist")]
    MissingOpportunity(String),
}

/// Advance a scored opportunity through the canonical lifecycle and publish a
/// feed frame. Redelivery of an already-surfaced event is idempotent at the DB
/// boundary and may rebroadcast the same stable opportunity id.
pub async fn surface_opportunity(
    pool: &PgPool,
    tx: &broadcast::Sender<String>,
    opportunity: &Opportunity,
) -> Result<bool, FeedError> {
    let mut db = pool.begin().await?;
    let state: Option<String> =
        sqlx::query_scalar("SELECT state FROM opportunities WHERE id=$1 FOR UPDATE")
            .bind(opportunity.id.to_string())
            .fetch_optional(&mut *db)
            .await?;
    let state = state.ok_or_else(|| FeedError::MissingOpportunity(opportunity.id.to_string()))?;
    if state == "scored" {
        sqlx::query(
            "INSERT INTO opportunity_events \
             (id,opportunity_id,from_state,to_state,actor,detail) \
             VALUES ($1,$2,'scored','surfaced','gateway',$3)",
        )
        .bind(Ulid::new().to_string())
        .bind(opportunity.id.to_string())
        .bind(serde_json::json!({"delivery": "websocket_feed"}))
        .execute(&mut *db)
        .await?;
    } else if state != "surfaced" {
        // A stale at-least-once delivery must not resurrect accepted, ignored,
        // expired, executed, or closed chains.
        db.rollback().await?;
        return Ok(false);
    }

    let frame = serde_json::to_string(&ServerFrame::FeedItem {
        id: Some(opportunity.id.to_string()),
        trace_id: Some(opportunity.trace_id.to_string()),
        body: serde_json::to_value(opportunity)?,
    })?;
    db.commit().await?;
    // No active receiver is not an error: the surfaced row remains the durable
    // feed authority and can be loaded by a later client session.
    let _ = tx.send(frame);
    Ok(true)
}

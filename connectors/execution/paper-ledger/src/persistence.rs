//! Transactional Postgres persistence for paper executions and attribution.

use aether_core::ids::{MarketKey, Money, Ulid};
use aether_core::json::JsonObject;
use aether_core::order::{Fill, OrderIntent, Side};
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;

use crate::ledger::{LedgerOrder, OrderStatus};
use crate::positions::{Position, PositionTracker};

type DurableIntentIdentity =
    (String, String, String, Option<Decimal>, Decimal, String, String, bool);
type DurableFillRow =
    (String, String, Decimal, Decimal, Decimal, String, serde_json::Value, String, bool);

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("invalid persistence input: {0}")]
    Invalid(String),
    #[error("opportunity {id} must be in {expected} state, found {actual}")]
    InvalidOpportunityState { id: Ulid, expected: &'static str, actual: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistOutcome {
    Inserted,
    AlreadyExists,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DurableFillEvent {
    pub event_id: Ulid,
    pub fill: Fill,
}

/// Recoverable predicted-vs-realized components stored in attribution.detail.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionComponents {
    pub predicted_fees: Decimal,
    pub realized_fees: Decimal,
    pub predicted_slippage: Decimal,
    pub realized_slippage: Decimal,
    pub predicted_funding: Option<Decimal>,
    pub realized_funding: Option<Decimal>,
}

impl AttributionComponents {
    pub fn divergence_detail(&self) -> serde_json::Value {
        json!({
            "fees": {
                "predicted": self.predicted_fees.to_string(),
                "realized": self.realized_fees.to_string(),
                "divergence": (self.realized_fees - self.predicted_fees).to_string()
            },
            "slippage": {
                "predicted": self.predicted_slippage.to_string(),
                "realized": self.realized_slippage.to_string(),
                "divergence": (self.realized_slippage - self.predicted_slippage).to_string()
            },
            "funding": match (self.predicted_funding, self.realized_funding) {
                (Some(predicted), Some(realized)) => json!({
                    "predicted": predicted.to_string(),
                    "realized": realized.to_string(),
                    "divergence": (realized - predicted).to_string()
                }),
                _ => json!({"detail": "not_recoverable"}),
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct AttributionClose {
    pub opportunity_id: Ulid,
    pub predicted_net_edge: Decimal,
    pub realized_pnl: Decimal,
    pub outcome: String,
    pub components: AttributionComponents,
}

#[derive(Debug, Clone)]
pub struct PostgresLedgerStore {
    pool: PgPool,
}

impl PostgresLedgerStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str) -> Result<Self, PersistenceError> {
        Ok(Self::new(PgPool::connect(database_url).await?))
    }

    /// Atomically write the intent, order, fills, and paper positions.
    /// The order primary key is the idempotency gate across process restarts.
    pub async fn persist_execution(
        &self,
        order: &LedgerOrder,
        fills: &[Fill],
    ) -> Result<PersistOutcome, PersistenceError> {
        if !order.intent.paper || fills.iter().any(|fill| !fill.paper) {
            return Err(PersistenceError::Invalid(
                "paper store refuses live order or fill rows".to_owned(),
            ));
        }
        if fills.is_empty() {
            return Err(PersistenceError::Invalid("execution has no fills".to_owned()));
        }
        if fills.iter().any(|fill| fill.order_id != order.order_id) {
            return Err(PersistenceError::Invalid("fill order_id mismatch".to_owned()));
        }

        let mut tx = self.pool.begin().await?;
        insert_intent(&mut tx, order).await?;

        let total_size: Decimal = fills.iter().map(|fill| fill.size).sum();
        let total_notional: Decimal = fills.iter().map(|fill| fill.price * fill.size).sum();
        let average_price = total_notional / total_size;
        let total_fee: Decimal = fills.iter().map(|fill| fill.fee.amount).sum();
        let fee_currency = &fills[0].fee.currency;
        if fills.iter().any(|fill| fill.fee.currency != *fee_currency) {
            return Err(PersistenceError::Invalid("mixed fee currencies".to_owned()));
        }

        let inserted = sqlx::query(
            "INSERT INTO orders (order_id,intent_id,market,side,price,size,fee_amount,fee_currency,venue_ref,ts,state,paper) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::timestamptz,$11,true) \
             ON CONFLICT (order_id) DO NOTHING",
        )
        .bind(order.order_id.to_string())
        .bind(order.intent.id.to_string())
        .bind(order.intent.market.as_str())
        .bind(side_name(order.intent.side))
        .bind(average_price)
        .bind(total_size)
        .bind(total_fee)
        .bind(fee_currency)
        .bind(serde_json::to_value(&fills[0].venue_ref).map_err(invalid_json)?)
        .bind(order.updated_ts.to_string())
        .bind(status_name(order.status))
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if inserted == 0 {
            let existing: Option<(String, String, String, bool)> =
                sqlx::query_as("SELECT intent_id,market,side,paper FROM orders WHERE order_id=$1")
                    .bind(order.order_id.to_string())
                    .fetch_optional(&mut *tx)
                    .await?;
            let expected = (
                order.intent.id.to_string(),
                order.intent.market.as_str().to_owned(),
                side_name(order.intent.side).to_owned(),
                true,
            );
            if existing.as_ref() != Some(&expected) {
                return Err(PersistenceError::Invalid(format!(
                    "order id collision with different durable payload: {}",
                    order.order_id
                )));
            }
            tx.rollback().await?;
            return Ok(PersistOutcome::AlreadyExists);
        }

        for fill in fills {
            let fill_id = Ulid::new();
            sqlx::query(
                "INSERT INTO fills (id,order_id,market,side,price,size,fee_amount,fee_currency,venue_ref,ts,paper) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::timestamptz,true)",
            )
            .bind(fill_id.to_string())
            .bind(fill.order_id.to_string())
            .bind(fill.market.as_str())
            .bind(side_name(fill.side))
            .bind(fill.price)
            .bind(fill.size)
            .bind(fill.fee.amount)
            .bind(&fill.fee.currency)
            .bind(serde_json::to_value(&fill.venue_ref).map_err(invalid_json)?)
            .bind(fill.ts.to_string())
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO paper_fill_outbox (fill_id,order_id,market_key,payload) \
                 VALUES ($1,$2,$3,$4)",
            )
            .bind(fill_id.to_string())
            .bind(fill.order_id.to_string())
            .bind(fill.market.as_str())
            .bind(serde_json::to_value(fill).map_err(invalid_json)?)
            .execute(&mut *tx)
            .await?;
        }

        let market = &order.intent.market;
        let existing_position: Option<(Decimal, Decimal, Decimal)> = sqlx::query_as(
            "SELECT side_exposure,avg_price,realized_pnl_amount FROM positions \
             WHERE market_key=$1 AND paper=true FOR UPDATE",
        )
        .bind(market.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        let mut durable_positions = PositionTracker::new();
        if let Some((net_size, avg_entry_price, realized_pnl)) = existing_position {
            durable_positions.restore(Position {
                market: market.clone(),
                net_size,
                avg_entry_price,
                realized_pnl,
                fill_count: 0,
                fees_paid: Decimal::ZERO,
            });
        }
        for fill in fills {
            durable_positions
                .apply_fill(fill)
                .map_err(|error| PersistenceError::Invalid(error.to_string()))?;
        }
        let position = durable_positions
            .get(market)
            .ok_or_else(|| PersistenceError::Invalid("durable position missing".to_owned()))?;
        let cached_mark: Option<Decimal> =
            sqlx::query_scalar("SELECT mid FROM quotes_latest WHERE market_key=$1")
                .bind(market.as_str())
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
        let mark = cached_mark
            .or_else(|| fills.last().map(|fill| fill.price))
            .unwrap_or(position.avg_entry_price);
        let unrealized = (mark - position.avg_entry_price) * position.net_size;
        sqlx::query(
                "INSERT INTO positions (market_key,paper,side_exposure,avg_price,size,realized_pnl_amount,realized_pnl_currency,unrealized_pnl_amount,unrealized_pnl_currency,ts) \
                 VALUES ($1,true,$2,$3,$4,$5,$6,$7,$6,$8::timestamptz) \
                 ON CONFLICT (market_key,paper) DO UPDATE SET \
                   side_exposure=EXCLUDED.side_exposure,avg_price=EXCLUDED.avg_price,size=EXCLUDED.size, \
                   realized_pnl_amount=EXCLUDED.realized_pnl_amount,realized_pnl_currency=EXCLUDED.realized_pnl_currency, \
                   unrealized_pnl_amount=EXCLUDED.unrealized_pnl_amount,unrealized_pnl_currency=EXCLUDED.unrealized_pnl_currency, \
                   ts=EXCLUDED.ts,updated_ts=now()",
            )
            .bind(market.as_str())
            .bind(position.net_size)
            .bind(position.avg_entry_price)
            .bind(position.net_size.abs())
            .bind(position.realized_pnl)
            .bind(fee_currency)
            .bind(unrealized)
            .bind(order.updated_ts.to_string())
            .execute(&mut *tx)
            .await?;

        sqlx::query("UPDATE order_intents SET state='routed',updated_ts=now() WHERE id=$1")
            .bind(order.intent.id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(PersistOutcome::Inserted)
    }

    /// Persist quote-driven unrealized P&L for the paper position only.
    pub async fn update_mark(
        &self,
        market: &MarketKey,
        mark: Decimal,
    ) -> Result<(), PersistenceError> {
        if mark <= Decimal::ZERO {
            return Err(PersistenceError::Invalid("mark must be positive".to_owned()));
        }
        sqlx::query(
            "UPDATE positions SET unrealized_pnl_amount=($2-avg_price)*side_exposure, \
             ts=now(),updated_ts=now() WHERE market_key=$1 AND paper=true",
        )
        .bind(market.as_str())
        .bind(mark)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Return durable fills for an idempotent retry, while rejecting an ID
    /// collision whose immutable intent fields differ.
    pub async fn existing_execution(
        &self,
        intent: &OrderIntent,
    ) -> Result<Option<Vec<Fill>>, PersistenceError> {
        let existing: Option<DurableIntentIdentity> = sqlx::query_as(
            "SELECT market,side,order_type,limit_price,size,size_unit,tif,paper \
                 FROM order_intents WHERE id=$1",
        )
        .bind(intent.id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        let expected = (
            intent.market.as_str().to_owned(),
            side_name(intent.side).to_owned(),
            format!("{:?}", intent.order_type).to_lowercase(),
            intent.limit_price,
            intent.size,
            format!("{:?}", intent.size_unit).to_lowercase(),
            format!("{:?}", intent.tif).to_lowercase(),
            true,
        );
        if existing != expected || !intent.paper {
            return Err(PersistenceError::Invalid(format!(
                "intent id collision with different durable payload: {}",
                intent.id
            )));
        }

        let rows: Vec<DurableFillRow> = sqlx::query_as(
            "SELECT market,side,price,size,fee_amount,fee_currency,venue_ref, \
                        to_char(ts AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'),paper \
                 FROM fills WHERE order_id=$1 ORDER BY created_ts,id",
        )
        .bind(intent.id.to_string())
        .fetch_all(&self.pool)
        .await?;
        if rows.is_empty() {
            return Err(PersistenceError::Invalid(format!(
                "durable order {} has no fills",
                intent.id
            )));
        }
        rows.into_iter()
            .map(|(market, side, price, size, fee_amount, fee_currency, venue_ref, ts, paper)| {
                Ok(Fill {
                    order_id: intent.id,
                    market: MarketKey::from_string(market)
                        .map_err(|error| PersistenceError::Invalid(error.to_string()))?,
                    side: parse_side(&side)?,
                    price,
                    size,
                    fee: Money::new(fee_amount, fee_currency),
                    venue_ref: JsonObject::new(venue_ref)
                        .map_err(|error| PersistenceError::Invalid(error.to_string()))?,
                    ts: serde_json::from_value::<UtcTime>(serde_json::Value::String(ts))
                        .map_err(invalid_json)?,
                    paper,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    /// Load unpublished fills from the transactional outbox.
    pub async fn pending_fill_events(&self) -> Result<Vec<DurableFillEvent>, PersistenceError> {
        let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
            "SELECT fill_id,payload FROM paper_fill_outbox \
             WHERE published_ts IS NULL ORDER BY created_ts,fill_id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(id, payload)| {
                Ok(DurableFillEvent {
                    event_id: Ulid::from_string(&id)
                        .map_err(|error| PersistenceError::Invalid(error.to_string()))?,
                    fill: serde_json::from_value(payload).map_err(invalid_json)?,
                })
            })
            .collect()
    }

    pub async fn mark_fill_published(&self, event_id: Ulid) -> Result<(), PersistenceError> {
        let updated = sqlx::query(
            "UPDATE paper_fill_outbox SET published_ts=now() \
             WHERE fill_id=$1 AND published_ts IS NULL",
        )
        .bind(event_id.to_string())
        .execute(&self.pool)
        .await?
        .rows_affected();
        if updated > 1 {
            return Err(PersistenceError::Invalid(format!(
                "outbox event {} updated more than once",
                event_id
            )));
        }
        Ok(())
    }

    /// Transition an accepted opportunity to executed when its fill is durable.
    pub async fn record_opportunity_execution(
        &self,
        opportunity_id: Ulid,
        order_id: Ulid,
        requested_size: Decimal,
        fills: &[Fill],
    ) -> Result<(), PersistenceError> {
        let mut tx = self.pool.begin().await?;
        let state = opportunity_state(&mut tx, opportunity_id).await?;
        if state == "executed" {
            tx.commit().await?;
            return Ok(());
        }
        if state != "accepted" {
            return Err(PersistenceError::InvalidOpportunityState {
                id: opportunity_id,
                expected: "accepted or executed",
                actual: state,
            });
        }
        let filled_size: Decimal = fills.iter().map(|fill| fill.size).sum();
        let detail = json!({
            "order_id": order_id.to_string(),
            "fill_count": fills.len(),
            "filled_size": filled_size.to_string(),
            "partial": filled_size < requested_size
        });
        transition_opportunity(&mut tx, opportunity_id, "accepted", "executed", detail).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Write attribution and close the lifecycle in one transaction.
    pub async fn close_opportunity(
        &self,
        close: &AttributionClose,
    ) -> Result<(), PersistenceError> {
        let mut tx = self.pool.begin().await?;
        let state = opportunity_state(&mut tx, close.opportunity_id).await?;
        if state != "executed" && state != "closed" {
            return Err(PersistenceError::InvalidOpportunityState {
                id: close.opportunity_id,
                expected: "executed or closed",
                actual: state,
            });
        }
        let detail = close.components.divergence_detail();
        sqlx::query(
            "INSERT INTO attribution (id,opportunity_id,predicted_net_edge,realized_pnl,outcome,closed_ts,detail) \
             VALUES ($1,$2,$3,$4,$5,now(),$6) \
             ON CONFLICT (opportunity_id) DO UPDATE SET predicted_net_edge=EXCLUDED.predicted_net_edge, \
               realized_pnl=EXCLUDED.realized_pnl,outcome=EXCLUDED.outcome,closed_ts=EXCLUDED.closed_ts,detail=EXCLUDED.detail,updated_ts=now()",
        )
        .bind(Ulid::new().to_string())
        .bind(close.opportunity_id.to_string())
        .bind(close.predicted_net_edge)
        .bind(close.realized_pnl)
        .bind(&close.outcome)
        .bind(detail.clone())
        .execute(&mut *tx)
        .await?;
        if state == "executed" {
            transition_opportunity(&mut tx, close.opportunity_id, "executed", "closed", detail)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn insert_intent(
    tx: &mut Transaction<'_, Postgres>,
    order: &LedgerOrder,
) -> Result<(), PersistenceError> {
    let intent = &order.intent;
    sqlx::query(
        "INSERT INTO order_intents (id,market,side,order_type,limit_price,size,size_unit,tif,paper,origin,quote_snapshot,caps_version,state,created_ts,updated_ts) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,true,$9,$10,$11,'pending',$12::timestamptz,$12::timestamptz) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(intent.id.to_string())
    .bind(intent.market.as_str())
    .bind(side_name(intent.side))
    .bind(format!("{:?}", intent.order_type).to_lowercase())
    .bind(intent.limit_price)
    .bind(intent.size)
    .bind(format!("{:?}", intent.size_unit).to_lowercase())
    .bind(format!("{:?}", intent.tif).to_lowercase())
    .bind(serde_json::to_value(&intent.origin).map_err(invalid_json)?)
    .bind(serde_json::to_value(&intent.quote_snapshot).map_err(invalid_json)?)
    .bind(intent.caps_version.to_string())
    .bind(intent.created_ts.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn opportunity_state(
    tx: &mut Transaction<'_, Postgres>,
    id: Ulid,
) -> Result<String, PersistenceError> {
    let actual: Option<String> =
        sqlx::query_scalar("SELECT state FROM opportunities WHERE id=$1 FOR UPDATE")
            .bind(id.to_string())
            .fetch_optional(&mut **tx)
            .await?;
    actual.ok_or(PersistenceError::InvalidOpportunityState {
        id,
        expected: "existing opportunity",
        actual: "missing".to_owned(),
    })
}

async fn transition_opportunity(
    tx: &mut Transaction<'_, Postgres>,
    id: Ulid,
    from: &'static str,
    to: &'static str,
    detail: serde_json::Value,
) -> Result<(), PersistenceError> {
    sqlx::query("UPDATE opportunities SET state=$2,updated_ts=now() WHERE id=$1")
        .bind(id.to_string())
        .bind(to)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO opportunity_events (id,opportunity_id,from_state,to_state,actor,detail) \
         VALUES ($1,$2,$3,$4,'paper-ledger',$5)",
    )
    .bind(Ulid::new().to_string())
    .bind(id.to_string())
    .bind(from)
    .bind(to)
    .bind(detail)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Buy => "buy",
        Side::Sell => "sell",
        Side::BuyNo => "buy_no",
        Side::SellNo => "sell_no",
    }
}

fn parse_side(side: &str) -> Result<Side, PersistenceError> {
    match side {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        "buy_no" => Ok(Side::BuyNo),
        "sell_no" => Ok(Side::SellNo),
        value => Err(PersistenceError::Invalid(format!("unknown durable side: {value}"))),
    }
}

fn status_name(status: OrderStatus) -> &'static str {
    match status {
        OrderStatus::Accepted => "open",
        OrderStatus::PartiallyFilled => "partial",
        OrderStatus::Filled => "filled",
        OrderStatus::Cancelled => "cancelled",
        OrderStatus::Rejected => "rejected",
    }
}

fn invalid_json(error: serde_json::Error) -> PersistenceError {
    PersistenceError::Invalid(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribution_divergence_is_componentwise() {
        let detail = AttributionComponents {
            predicted_fees: Decimal::new(10, 2),
            realized_fees: Decimal::new(12, 2),
            predicted_slippage: Decimal::new(20, 2),
            realized_slippage: Decimal::new(15, 2),
            predicted_funding: None,
            realized_funding: None,
        }
        .divergence_detail();

        assert_eq!(detail["fees"]["divergence"], "0.02");
        assert_eq!(detail["slippage"]["divergence"], "-0.05");
        assert_eq!(detail["funding"]["detail"], "not_recoverable");
    }
}

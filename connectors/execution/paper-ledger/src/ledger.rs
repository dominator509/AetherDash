//! Paper ledger — accepts paper intents and fills via the shared fill model.

use aether_core::ids::Ulid;
use aether_core::order::{Fill, OrderIntent};
use aether_core::quote::{OrderBook, Quote};
use aether_fillmodel::config::FillConfig;
use aether_fillmodel::walk::{walk_book, FillError};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use crate::pnl::{PnLCalculator, PnLError};
use crate::positions::{PositionError, PositionTracker};

/// Errors from the paper ledger.
#[derive(Error, Debug)]
pub enum LedgerError {
    /// The fill model failed to produce fills.
    #[error("fill model error: {0}")]
    FillModel(#[from] FillError),

    /// The intent was rejected (not paper, or duplicate).
    #[error("ledger rejected intent: {0}")]
    Rejected(String),

    /// Position accounting failed.
    #[error(transparent)]
    Position(#[from] PositionError),

    /// P&L accounting failed.
    #[error(transparent)]
    PnL(#[from] PnLError),
}

/// A recorded order in the ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerOrder {
    pub order_id: Ulid,
    pub intent: OrderIntent,
    pub status: OrderStatus,
    pub created_ts: aether_core::time::UtcTime,
    pub updated_ts: aether_core::time::UtcTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Accepted,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// Result of executing an intent. Replayed idempotent submissions return the
/// original fills without mutating state or re-emitting events.
#[derive(Debug, Clone, PartialEq)]
pub struct Submission {
    pub fills: Vec<Fill>,
    pub replayed: bool,
}

/// The paper ledger.
///
/// Accepts paper intents, fills them, and maintains a working in-process view.
/// [`PostgresLedgerStore`](crate::persistence::PostgresLedgerStore) is the
/// relational source of truth; the service publishes through its durable
/// Postgres outbox after the execution transaction commits.
#[derive(Debug, Clone, Default)]
pub struct PaperLedger {
    orders: HashMap<Ulid, LedgerOrder>,
    fills: Vec<Fill>,
    positions: PositionTracker,
    pnl: PnLCalculator,
    fill_config: FillConfig,
}

impl PaperLedger {
    /// Create a new paper ledger with default fill config.
    pub fn new() -> Self {
        Self { fill_config: FillConfig::default(), ..Default::default() }
    }

    /// Create a new ledger with the given fill config.
    pub fn with_config(fill_config: FillConfig) -> Self {
        Self { fill_config, ..Default::default() }
    }

    /// Submit a paper order intent for execution.
    ///
    /// # Requirements
    /// - intent.paper MUST be true
    /// - intent.id MUST be unique (idempotency check)
    /// - A current OrderBook must be provided for fill calculation
    pub fn submit(
        &mut self,
        intent: OrderIntent,
        book: &OrderBook,
    ) -> Result<Vec<Fill>, LedgerError> {
        self.execute(intent, book).map(|submission| submission.fills)
    }

    /// Execute an intent and report whether this was an idempotent replay.
    pub fn execute(
        &mut self,
        intent: OrderIntent,
        book: &OrderBook,
    ) -> Result<Submission, LedgerError> {
        if !intent.paper {
            return Err(LedgerError::Rejected(
                "only paper intents accepted by paper ledger".into(),
            ));
        }

        if let Some(existing) = self.orders.get(&intent.id) {
            if existing.intent != intent {
                return Err(LedgerError::Rejected(format!(
                    "intent id collision with different payload: {}",
                    intent.id
                )));
            }
            let fills =
                self.fills.iter().filter(|fill| fill.order_id == intent.id).cloned().collect();
            return Ok(Submission { fills, replayed: true });
        }

        // Calculate first. Failed fills must not leave ghost Accepted orders
        // that permanently block a corrected retry.
        let fills = walk_book(book, &intent, &self.fill_config)?;

        // Apply all accounting to clones first. Arithmetic failure leaves the
        // ledger unchanged and safe to retry.
        let mut positions = self.positions.clone();
        for fill in &fills {
            positions.apply_fill(fill)?;
        }
        let mut pnl = self.pnl.clone();
        pnl.record_fills(&fills)?;
        let total_filled = fills.iter().try_fold(Decimal::ZERO, |total, fill| {
            total
                .checked_add(fill.size)
                .ok_or_else(|| LedgerError::Rejected("filled size overflow".to_owned()))
        })?;

        // Record the successfully executable order.
        let now = aether_core::time::UtcTime::now();
        let order = LedgerOrder {
            order_id: intent.id,
            intent: intent.clone(),
            status: OrderStatus::Accepted,
            created_ts: now,
            updated_ts: now,
        };
        self.orders.insert(intent.id, order);

        // Update order status based on fill outcome
        let status = if total_filled >= intent.size {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };
        if let Some(order) = self.orders.get_mut(&intent.id) {
            order.status = status;
            order.updated_ts = now;
        }

        self.positions = positions;
        self.pnl = pnl;
        self.fills.extend(fills.clone());

        Ok(Submission { fills, replayed: false })
    }

    /// Cancel an open order.
    pub fn cancel(&mut self, order_id: &Ulid) -> Result<(), LedgerError> {
        let order = self
            .orders
            .get_mut(order_id)
            .ok_or_else(|| LedgerError::Rejected(format!("order not found: {}", order_id)))?;
        match order.status {
            OrderStatus::Accepted | OrderStatus::PartiallyFilled => {
                order.status = OrderStatus::Cancelled;
                order.updated_ts = aether_core::time::UtcTime::now();
                Ok(())
            }
            _ => Err(LedgerError::Rejected(format!(
                "cannot cancel order in status {:?}",
                order.status
            ))),
        }
    }

    /// Get all recorded orders.
    pub fn orders(&self) -> &HashMap<Ulid, LedgerOrder> {
        &self.orders
    }

    /// Get all recorded fills.
    pub fn fills(&self) -> &[Fill] {
        &self.fills
    }

    /// Get current positions.
    pub fn positions(&self) -> &PositionTracker {
        &self.positions
    }

    /// Get P&L calculator.
    pub fn pnl(&self) -> &PnLCalculator {
        &self.pnl
    }

    /// Update unrealized P&L from a quote midpoint.
    pub fn update_quote(&mut self, quote: &Quote) -> Result<(), LedgerError> {
        let Some(mid) = quote.mid else {
            return Err(LedgerError::Rejected("quote has no midpoint".to_owned()));
        };
        if mid <= Decimal::ZERO {
            return Err(LedgerError::Rejected("quote midpoint must be positive".to_owned()));
        }
        self.pnl.update_mark(&quote.market, mid);
        Ok(())
    }
}

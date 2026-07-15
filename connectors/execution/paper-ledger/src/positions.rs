//! Paper position tracker.

use aether_core::ids::MarketKey;
use aether_core::order::Fill;
use aether_core::order::Side;
use rust_decimal::prelude::Signed;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PositionError {
    #[error("position arithmetic overflow while computing {0}")]
    Arithmetic(&'static str),
    #[error("fill size must be positive")]
    InvalidSize,
}

/// A tracked position for a single market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub market: MarketKey,
    /// Positive = long, negative = short.
    pub net_size: Decimal,
    /// Volume-weighted average entry price.
    pub avg_entry_price: Decimal,
    /// Total realized P&L for closed portions.
    pub realized_pnl: Decimal,
    /// Number of fills contributing to this position.
    pub fill_count: u64,
    /// Execution fees paid by this position, in the fill fee currency.
    pub fees_paid: Decimal,
}

/// Tracks paper positions across markets.
#[derive(Debug, Clone, Default)]
pub struct PositionTracker {
    positions: HashMap<MarketKey, Position>,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self { positions: HashMap::new() }
    }

    /// Apply a fill to the position tracker.
    pub fn apply_fill(&mut self, fill: &Fill) -> Result<(), PositionError> {
        if fill.size <= Decimal::ZERO {
            return Err(PositionError::InvalidSize);
        }
        // Compute against a clone and commit only after every checked
        // operation succeeds; an overflow must not partially mutate state.
        let mut next = self.positions.get(&fill.market).cloned().unwrap_or_else(|| Position {
            market: fill.market.clone(),
            net_size: Decimal::ZERO,
            avg_entry_price: Decimal::ZERO,
            realized_pnl: Decimal::ZERO,
            fill_count: 0,
            fees_paid: Decimal::ZERO,
        });

        let old_size = next.net_size;
        let size_delta = match fill.side {
            Side::Buy | Side::BuyNo => fill.size,
            Side::Sell | Side::SellNo => -fill.size,
        };
        let new_size =
            old_size.checked_add(size_delta).ok_or(PositionError::Arithmetic("net size"))?;
        next.fees_paid = next
            .fees_paid
            .checked_add(fill.fee.amount)
            .ok_or(PositionError::Arithmetic("fees paid"))?;
        next.realized_pnl = next
            .realized_pnl
            .checked_sub(fill.fee.amount)
            .ok_or(PositionError::Arithmetic("fee-adjusted realized P&L"))?;

        if old_size == Decimal::ZERO {
            next.net_size = new_size;
            next.avg_entry_price = fill.price;
        } else if old_size.signum() == size_delta.signum() {
            // Increasing an existing exposure: average absolute notionals.
            let old_abs = old_size.abs();
            let delta_abs = size_delta.abs();
            next.net_size = new_size;
            let old_notional = next
                .avg_entry_price
                .checked_mul(old_abs)
                .ok_or(PositionError::Arithmetic("old notional"))?;
            let fill_notional = fill
                .price
                .checked_mul(delta_abs)
                .ok_or(PositionError::Arithmetic("fill notional"))?;
            let total_notional = old_notional
                .checked_add(fill_notional)
                .ok_or(PositionError::Arithmetic("total notional"))?;
            let total_size =
                old_abs.checked_add(delta_abs).ok_or(PositionError::Arithmetic("total size"))?;
            next.avg_entry_price = total_notional
                .checked_div(total_size)
                .ok_or(PositionError::Arithmetic("average entry price"))?;
        } else {
            // Reducing, closing, or flipping exposure. Realize only the
            // overlapping quantity and never re-average the surviving leg.
            let closed_size = old_size.abs().min(size_delta.abs());
            let realized = fill
                .price
                .checked_sub(next.avg_entry_price)
                .and_then(|value| value.checked_mul(closed_size))
                .and_then(|value| value.checked_mul(old_size.signum()))
                .ok_or(PositionError::Arithmetic("realized P&L"))?;
            next.realized_pnl = next
                .realized_pnl
                .checked_add(realized)
                .ok_or(PositionError::Arithmetic("cumulative realized P&L"))?;
            next.net_size = new_size;
            if new_size == Decimal::ZERO {
                next.avg_entry_price = Decimal::ZERO;
            } else if new_size.signum() != old_size.signum() {
                next.avg_entry_price = fill.price;
            }
        }

        next.fill_count =
            next.fill_count.checked_add(1).ok_or(PositionError::Arithmetic("fill count"))?;
        self.positions.insert(fill.market.clone(), next);
        Ok(())
    }

    /// Seed a durable position during crash recovery.
    pub fn restore(&mut self, position: Position) {
        self.positions.insert(position.market.clone(), position);
    }

    pub fn get(&self, market: &MarketKey) -> Option<&Position> {
        self.positions.get(market)
    }

    pub fn all(&self) -> &HashMap<MarketKey, Position> {
        &self.positions
    }
}

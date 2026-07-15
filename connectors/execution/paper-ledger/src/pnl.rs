//! P&L calculator for paper positions.

use aether_core::ids::MarketKey;
use aether_core::order::Fill;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PnLError {
    #[error("P&L arithmetic overflow while computing {0}")]
    Arithmetic(&'static str),
}

/// A P&L summary for reporting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLSummary {
    /// Total realized P&L across all closed positions.
    pub total_realized: Decimal,
    /// Total unrealized P&L based on current mark prices.
    pub total_unrealized: Decimal,
    /// Fees already included in realized P&L, exposed for attribution.
    pub total_fees: Decimal,
    /// Per-market P&L breakdown.
    pub per_market: Vec<MarketPnL>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketPnL {
    pub market: MarketKey,
    pub realized: Decimal,
    pub unrealized: Decimal,
    pub net_position: Decimal,
    pub avg_entry: Decimal,
}

/// Tracks P&L for paper positions.
#[derive(Debug, Clone, Default)]
pub struct PnLCalculator {
    total_fills: u64,
    total_fees: Decimal,
    current_mark_prices: HashMap<MarketKey, Decimal>,
}

impl PnLCalculator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record fills for P&L tracking.
    pub fn record_fills(&mut self, fills: &[Fill]) -> Result<(), PnLError> {
        let mut next = self.clone();
        for fill in fills {
            next.total_fills =
                next.total_fills.checked_add(1).ok_or(PnLError::Arithmetic("fill count"))?;
            next.total_fees = next
                .total_fees
                .checked_add(fill.fee.amount)
                .ok_or(PnLError::Arithmetic("total fees"))?;
            // Update mark price from fill (last trade)
            next.current_mark_prices.insert(fill.market.clone(), fill.price);
        }
        *self = next;
        Ok(())
    }

    /// Update the mark price for a market (e.g., from a new quote).
    pub fn update_mark(&mut self, market: &MarketKey, price: Decimal) {
        self.current_mark_prices.insert(market.clone(), price);
    }

    /// Compute P&L summary from positions and current mark prices.
    pub fn compute_summary(
        &self,
        positions: &super::positions::PositionTracker,
    ) -> Result<PnLSummary, PnLError> {
        let mut total_realized = Decimal::ZERO;
        let mut total_unrealized = Decimal::ZERO;
        let mut per_market = Vec::new();

        let mut ordered_positions: Vec<_> = positions.all().iter().collect();
        ordered_positions.sort_by(|(left, _), (right, _)| left.as_str().cmp(right.as_str()));

        for (market, pos) in ordered_positions {
            let unrealized = if pos.net_size != Decimal::ZERO {
                if let Some(mark) = self.current_mark_prices.get(market) {
                    mark.checked_sub(pos.avg_entry_price)
                        .and_then(|difference| difference.checked_mul(pos.net_size))
                        .ok_or(PnLError::Arithmetic("unrealized P&L"))?
                } else {
                    Decimal::ZERO
                }
            } else {
                Decimal::ZERO
            };

            total_realized = total_realized
                .checked_add(pos.realized_pnl)
                .ok_or(PnLError::Arithmetic("total realized P&L"))?;
            total_unrealized = total_unrealized
                .checked_add(unrealized)
                .ok_or(PnLError::Arithmetic("total unrealized P&L"))?;

            per_market.push(MarketPnL {
                market: market.clone(),
                realized: pos.realized_pnl,
                unrealized,
                net_position: pos.net_size,
                avg_entry: pos.avg_entry_price,
            });
        }

        Ok(PnLSummary { total_realized, total_unrealized, total_fees: self.total_fees, per_market })
    }

    pub fn total_fills(&self) -> u64 {
        self.total_fills
    }

    pub fn total_fees(&self) -> Decimal {
        self.total_fees
    }
}

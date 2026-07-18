//! Sensitivity analysis: how edge changes with size and staleness.

use aether_core::decimal::decimal_string;
use aether_core::order::Side;
use aether_core::quote::OrderBook;
use aether_decompose::decompose::{decompose, DecompositionContext};
use aether_decompose::fees::FeeCatalog;
use aether_fillmodel::config::FillConfig;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::simulator::{walk_leg, SimulationError};

/// One row of the sensitivity table.
#[derive(Debug, Clone, Serialize)]
pub struct SensitivityRow {
    #[serde(with = "decimal_string")]
    pub size_multiplier: Decimal,
    pub staleness_ms: i64,
    #[serde(with = "decimal_string")]
    pub net_edge: Decimal,
    #[serde(with = "decimal_string")]
    pub slippage_est: Decimal,
}

/// A table of net-edge values at varying size and staleness combinations.
#[derive(Debug, Clone, Serialize)]
pub struct SensitivityTable {
    pub rows: Vec<SensitivityRow>,
}

impl SensitivityTable {
    /// Compute a 4x4 sensitivity grid.
    ///
    /// Tests four size multipliers (0.5x, 1x, 1.5x, 2x) against four
    /// staleness levels (0ms, 2000ms, 5000ms, 10000ms).
    ///
    /// Each size uses the same fill-model walk as the base simulation and
    /// paper ledger, so depth exhaustion is visible in the size axis.
    #[allow(clippy::too_many_arguments)]
    pub fn compute(
        buy_book: &OrderBook,
        sell_book: &OrderBook,
        fill_config: &FillConfig,
        buy_price: Decimal,
        sell_price: Decimal,
        price_kind: &str,
        base_notional: Decimal,
        fee_catalog: &FeeCatalog,
        buy_venue: &str,
        sell_venue: &str,
        is_cross_chain: bool,
        bridge_fee_bps: Decimal,
        mismatch_discount: Decimal,
        funding_rate: Decimal,
        hold_hours: Decimal,
        tick_stale_ms: i64,
        confidence: Decimal,
        confidence_k: Decimal,
    ) -> Result<Self, SimulationError> {
        let mut rows = Vec::new();

        for size_mul in [
            Decimal::new(5, 1),  // 0.5
            Decimal::ONE,        // 1.0
            Decimal::new(15, 1), // 1.5
            Decimal::new(2, 0),  // 2.0
        ] {
            let size = base_notional * size_mul;
            let fee_amount =
                fee_catalog.estimate_pair(buy_venue, sell_venue, buy_price, sell_price, size)?;
            let (_, buy_slippage) = walk_leg(buy_book, Side::Buy, size, fill_config)?;
            let (_, sell_slippage) = walk_leg(sell_book, Side::Sell, size, fill_config)?;
            let buy_depth: Decimal = buy_book.asks().iter().map(|level| level.size).sum();
            let sell_depth: Decimal = sell_book.bids().iter().map(|level| level.size).sum();
            for staleness_ms in [0i64, 2000i64, 5000i64, 10000i64] {
                let ctx = DecompositionContext {
                    buy_price,
                    sell_price,
                    price_kind: price_kind.to_owned(),
                    notional: size,
                    fee_amount: Some(fee_amount),
                    is_cross_chain,
                    bridge_fee_bps,
                    mismatch_discount,
                    funding_rate,
                    hold_hours,
                    max_quote_age_ms: staleness_ms,
                    tick_stale_ms,
                    confidence,
                    confidence_k,
                    avg_depth: (buy_depth + sell_depth) / Decimal::new(2, 0),
                    ..Default::default()
                };
                let dec = decompose(&ctx).with_slippage(buy_slippage + sell_slippage);

                rows.push(SensitivityRow {
                    size_multiplier: size_mul,
                    staleness_ms,
                    net_edge: dec.net_edge,
                    slippage_est: dec.slippage_est,
                });
            }
        }

        Ok(Self { rows })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::SimulationConfig;
    use aether_core::ids::{MarketKey, VenueId};
    use aether_core::quote::BookLevel;
    use aether_core::time::UtcTime;
    use rust_decimal::Decimal;

    fn book() -> OrderBook {
        OrderBook::new(
            MarketKey::new(&VenueId::new("test").unwrap(), "SENS").unwrap(),
            vec![BookLevel { price: Decimal::new(99, 0), size: Decimal::new(100, 0) }],
            vec![BookLevel { price: Decimal::new(101, 0), size: Decimal::new(100, 0) }],
            1,
            UtcTime::from_unix_millis(1_000).unwrap(),
            None,
        )
        .unwrap()
    }

    #[test]
    fn sensitivity_table_4x4() {
        let config = SimulationConfig::default();
        let buy_book = book();
        let sell_book = book();
        let table = SensitivityTable::compute(
            &buy_book,
            &sell_book,
            &config.fill_config,
            Decimal::new(100, 0),
            Decimal::new(101, 0),
            "currency",
            Decimal::new(100, 0),
            &FeeCatalog::load_embedded().unwrap(),
            "hyperliquid",
            "hyperliquid",
            false,
            config.default_bridge_bps,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            5000,
            Decimal::ONE,
            config.confidence_k,
        )
        .unwrap();
        assert_eq!(table.rows.len(), 16);
        let row = &table.rows[4]; // index 4 = size 1.0, staleness 0
        assert_eq!(row.size_multiplier, Decimal::ONE);
        assert_eq!(row.staleness_ms, 0);
    }

    #[test]
    fn sensitivity_net_edge_decreases_with_staleness() {
        let config = SimulationConfig::default();
        let buy_book = book();
        let sell_book = book();
        let table = SensitivityTable::compute(
            &buy_book,
            &sell_book,
            &config.fill_config,
            Decimal::new(100, 0),
            Decimal::new(110, 0),
            "currency",
            Decimal::new(1000, 0),
            &FeeCatalog::load_embedded().unwrap(),
            "hyperliquid",
            "hyperliquid",
            false,
            config.default_bridge_bps,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            5000,
            Decimal::new(9, 1), // 0.9 confidence → penalty for very stale
            config.confidence_k,
        )
        .unwrap();

        // net_edge should be >= for fresh quotes vs stale ones
        let fresh =
            table.rows.iter().find(|r| r.staleness_ms == 0 && r.size_multiplier == Decimal::ONE);
        let stale = table
            .rows
            .iter()
            .find(|r| r.staleness_ms == 10000 && r.size_multiplier == Decimal::ONE);
        if let (Some(f), Some(s)) = (fresh, stale) {
            assert!(f.net_edge >= s.net_edge, "net_edge should not increase with staleness");
        }
    }

    #[test]
    fn sensitivity_size_axis_includes_depth_exhaustion_slippage() {
        let config = SimulationConfig::default();
        let buy_book = book();
        let sell_book = book();
        let table = SensitivityTable::compute(
            &buy_book,
            &sell_book,
            &config.fill_config,
            Decimal::new(100, 0),
            Decimal::new(110, 0),
            "currency",
            Decimal::new(100, 0),
            &FeeCatalog::load_embedded().unwrap(),
            "hyperliquid",
            "hyperliquid",
            false,
            config.default_bridge_bps,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            5000,
            Decimal::ONE,
            config.confidence_k,
        )
        .unwrap();
        let small = table
            .rows
            .iter()
            .find(|row| row.size_multiplier == Decimal::new(5, 1) && row.staleness_ms == 0)
            .unwrap();
        let large = table
            .rows
            .iter()
            .find(|row| row.size_multiplier == Decimal::new(2, 0) && row.staleness_ms == 0)
            .unwrap();
        assert_eq!(small.slippage_est, Decimal::ZERO);
        assert!(large.slippage_est > Decimal::ZERO);
        assert!(large.net_edge < small.net_edge);
    }
}

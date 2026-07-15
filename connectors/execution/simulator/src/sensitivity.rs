//! Sensitivity analysis: how edge changes with size and staleness.

use aether_decompose::decompose::{decompose, DecompositionContext, EdgeDecomposition};
use rust_decimal::Decimal;

/// One row of the sensitivity table.
#[derive(Debug, Clone)]
pub struct SensitivityRow {
    pub size_multiplier: Decimal,
    pub staleness_ms: i64,
    pub net_edge: Decimal,
}

/// A table of net-edge values at varying size and staleness combinations.
#[derive(Debug, Clone)]
pub struct SensitivityTable {
    pub rows: Vec<SensitivityRow>,
}

impl SensitivityTable {
    /// Compute a 4x4 sensitivity grid.
    ///
    /// Tests four size multipliers (0.5x, 1x, 1.5x, 2x) against four
    /// staleness levels (0ms, 2000ms, 5000ms, 10000ms).
    ///
    /// Each cell is computed by running a fresh decomposition with the
    /// varied notional and staleness.  The `_base` decomposition is
    /// provided as a reference point but the table recomputes each cell
    /// independently.
    ///
    /// NOTE: This function does NOT walk order books (no fills).  The
    /// net-edge values reflect the base decomposition's cost components
    /// at the varied sizes, without book-walk slippage.  Use
    /// `Simulator::simulate` with actual books for that.
    #[allow(clippy::too_many_arguments)]
    pub fn compute(
        _base: &EdgeDecomposition,
        buy_price: Decimal,
        sell_price: Decimal,
        price_kind: &str,
        base_notional: Decimal,
        fee_bps: Decimal,
        is_cross_chain: bool,
        bridge_fee_bps: Decimal,
        mismatch_discount: Decimal,
        funding_rate: Decimal,
        hold_hours: Decimal,
        tick_stale_ms: i64,
        confidence: Decimal,
        confidence_k: Decimal,
    ) -> Self {
        let mut rows = Vec::new();

        for size_mul in [
            Decimal::new(5, 1),  // 0.5
            Decimal::ONE,         // 1.0
            Decimal::new(15, 1),  // 1.5
            Decimal::new(2, 0),   // 2.0
        ] {
            for staleness_ms in [0i64, 2000i64, 5000i64, 10000i64] {
                let ctx = DecompositionContext {
                    buy_price,
                    sell_price,
                    price_kind: price_kind.to_owned(),
                    notional: base_notional * size_mul,
                    fee_bps,
                    is_cross_chain,
                    bridge_fee_bps,
                    mismatch_discount,
                    funding_rate,
                    hold_hours,
                    max_quote_age_ms: staleness_ms,
                    tick_stale_ms,
                    confidence,
                    confidence_k,
                    ..Default::default()
                };
                let dec = decompose(&ctx);

                rows.push(SensitivityRow {
                    size_multiplier: size_mul,
                    staleness_ms,
                    net_edge: dec.net_edge,
                });
            }
        }

        Self { rows }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::SimulationConfig;
    use aether_decompose::decompose::DecompositionContext;
    use rust_decimal::Decimal;

    #[test]
    fn sensitivity_table_4x4() {
        let config = SimulationConfig::default();
        let base = decompose(&DecompositionContext::default());
        let table = SensitivityTable::compute(
            &base,
            Decimal::new(100, 0),
            Decimal::new(101, 0),
            "probability",
            Decimal::new(100, 0),
            config.default_fee_bps,
            false,
            config.default_bridge_bps,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            5000,
            Decimal::ONE,
            config.confidence_k,
        );
        assert_eq!(table.rows.len(), 16);
        let row = &table.rows[4]; // index 4 = size 1.0, staleness 0
        assert_eq!(row.size_multiplier, Decimal::ONE);
        assert_eq!(row.staleness_ms, 0);
    }

    #[test]
    fn sensitivity_net_edge_decreases_with_staleness() {
        let config = SimulationConfig::default();
        let base = decompose(&DecompositionContext::default());
        let table = SensitivityTable::compute(
            &base,
            Decimal::new(100, 0),
            Decimal::new(110, 0),
            "probability",
            Decimal::new(1000, 0),
            config.default_fee_bps,
            false,
            config.default_bridge_bps,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            5000,
            Decimal::new(9, 1), // 0.9 confidence → penalty for very stale
            config.confidence_k,
        );

        // net_edge should be >= for fresh quotes vs stale ones
        let fresh = table.rows.iter().find(|r| r.staleness_ms == 0 && r.size_multiplier == Decimal::ONE);
        let stale = table.rows.iter().find(|r| r.staleness_ms == 10000 && r.size_multiplier == Decimal::ONE);
        if let (Some(f), Some(s)) = (fresh, stale) {
            assert!(f.net_edge >= s.net_edge,
                "net_edge should not increase with staleness");
        }
    }
}

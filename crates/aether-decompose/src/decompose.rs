use rust_decimal::Decimal;

use crate::components::*;
use crate::mismatch::MismatchConfig;

/// Full 11-component edge decomposition per SPEC-012.
#[derive(Debug, Clone)]
pub struct EdgeDecomposition {
    pub gross_spread: Decimal,
    pub fees: Decimal,
    pub slippage_est: Decimal,
    pub funding_cost: Decimal,
    pub gas_cost: Decimal,
    pub bridge_cost: Decimal,
    pub settlement_mismatch_discount: Decimal,
    pub liquidity_haircut: Decimal,
    pub staleness_penalty: Decimal,
    pub confidence_penalty: Decimal,
    pub net_edge: Decimal,
}

/// Context for computing a decomposition.
pub struct DecompositionContext {
    pub buy_price: Decimal,
    pub sell_price: Decimal,
    pub price_kind: String,
    pub notional: Decimal,
    pub fee_bps: Decimal,
    pub is_cross_chain: bool,
    pub bridge_fee_bps: Decimal,
    pub mismatch_discount: Decimal,
    pub funding_rate: Decimal,
    pub hold_hours: Decimal,
    pub max_quote_age_ms: i64,
    pub tick_stale_ms: i64,
    pub confidence: Decimal,
    pub confidence_k: Decimal,
    pub gas_units: u64,
    pub gas_price_gwei: u64,
}

impl Default for DecompositionContext {
    fn default() -> Self {
        Self {
            buy_price: Decimal::ZERO,
            sell_price: Decimal::ZERO,
            price_kind: "probability".into(),
            notional: Decimal::new(100, 0),
            fee_bps: Decimal::new(1, 3), // 0.001 = 10bps
            is_cross_chain: false,
            bridge_fee_bps: Decimal::new(5, 3), // 0.5%
            mismatch_discount: Decimal::ZERO,
            funding_rate: Decimal::ZERO,
            hold_hours: Decimal::new(24, 0),
            max_quote_age_ms: 0,
            tick_stale_ms: 5000,
            confidence: Decimal::ONE,
            confidence_k: Decimal::new(1, 0),
            gas_units: 21000,
            gas_price_gwei: 20,
        }
    }
}

impl DecompositionContext {
    /// Apply venue-pair mismatch discount from config.
    pub fn apply_mismatch_config(&mut self, _config: &MismatchConfig) {
        // In V1, reads mismatch discount directly from context.
        // Future: look up venue pair in config.pair_discounts, fall back to default.
    }
}

/// Compute full decomposition. All 11 components, explicit zeros enforced.
pub fn decompose(ctx: &DecompositionContext) -> EdgeDecomposition {
    let gross = gross_spread(ctx.buy_price, ctx.sell_price, &ctx.price_kind);
    let fee = fees(ctx.notional, ctx.fee_bps);
    let slippage = Decimal::ZERO; // Requires order book -- set by caller
    let funding = funding_cost(ctx.funding_rate, ctx.hold_hours);
    let gas = gas_cost(ctx.gas_units, ctx.gas_price_gwei);
    let bridge = bridge_cost(ctx.bridge_fee_bps, ctx.notional, ctx.is_cross_chain);
    let mismatch = settlement_mismatch_discount(gross, ctx.mismatch_discount);
    let liquidity = Decimal::ZERO; // Requires size/depth -- set by caller
    let staleness = staleness_penalty(ctx.max_quote_age_ms, ctx.tick_stale_ms);
    let confidence = confidence_penalty(gross, ctx.confidence, ctx.confidence_k);
    let costs = vec![
        fee,
        slippage,
        funding,
        gas,
        bridge,
        mismatch,
        liquidity,
        staleness,
        confidence,
    ];
    let net = net_edge(gross, &costs);
    EdgeDecomposition {
        gross_spread: gross,
        fees: fee,
        slippage_est: slippage,
        funding_cost: funding,
        gas_cost: gas,
        bridge_cost: bridge,
        settlement_mismatch_discount: mismatch,
        liquidity_haircut: liquidity,
        staleness_penalty: staleness,
        confidence_penalty: confidence,
        net_edge: net,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompose_default_all_zero() {
        let mut ctx = DecompositionContext::default();
        // Zero out notional and fee to ensure all costs are zero
        ctx.notional = Decimal::ZERO;
        ctx.fee_bps = Decimal::ZERO;
        ctx.gas_units = 0;
        ctx.gas_price_gwei = 0;
        let ed = decompose(&ctx);
        assert_eq!(ed.gross_spread, Decimal::ZERO);
        assert_eq!(ed.fees, Decimal::ZERO);
        assert_eq!(ed.net_edge, Decimal::ZERO);
        // All components are explicitly zero
        assert_eq!(ed.slippage_est, Decimal::ZERO);
        assert_eq!(ed.funding_cost, Decimal::ZERO);
        assert_eq!(ed.gas_cost, Decimal::ZERO);
        assert_eq!(ed.bridge_cost, Decimal::ZERO);
        assert_eq!(ed.settlement_mismatch_discount, Decimal::ZERO);
        assert_eq!(ed.liquidity_haircut, Decimal::ZERO);
        assert_eq!(ed.staleness_penalty, Decimal::ZERO);
        assert_eq!(ed.confidence_penalty, Decimal::ZERO);
    }

    #[test]
    fn decompose_simple_probability_arb() {
        // buy 0.65, sell 0.70 => gross = 0.05.
        // Costs: fees 0.20 + gas 0.00042 = 0.20042 exceed gross => net clamped to 0.
        let mut ctx = DecompositionContext::default();
        ctx.buy_price = Decimal::new(65, 2);
        ctx.sell_price = Decimal::new(70, 2);
        ctx.price_kind = "probability".into();
        ctx.notional = Decimal::new(100, 0);
        ctx.fee_bps = Decimal::new(1, 3);

        let ed = decompose(&ctx);
        assert_eq!(ed.gross_spread, Decimal::new(5, 2));
        assert_eq!(ed.fees, Decimal::new(2, 1)); // 100 * 0.001 * 2 = 0.20
        // Costs exceed gross, so net_edge is clamped to zero
        assert_eq!(ed.net_edge, Decimal::ZERO);
    }
}

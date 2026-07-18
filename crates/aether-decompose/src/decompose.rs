use aether_core::decimal::decimal_string;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::components::*;
use crate::mismatch::MismatchConfig;

/// Full 11-component edge decomposition per SPEC-012.
#[derive(Debug, Clone, Serialize)]
pub struct EdgeDecomposition {
    #[serde(with = "decimal_string")]
    pub gross_spread: Decimal,
    #[serde(with = "decimal_string")]
    pub fees: Decimal,
    #[serde(with = "decimal_string")]
    pub slippage_est: Decimal,
    #[serde(with = "decimal_string")]
    pub funding_cost: Decimal,
    #[serde(with = "decimal_string")]
    pub gas_cost: Decimal,
    #[serde(with = "decimal_string")]
    pub bridge_cost: Decimal,
    #[serde(with = "decimal_string")]
    pub settlement_mismatch_discount: Decimal,
    #[serde(with = "decimal_string")]
    pub liquidity_haircut: Decimal,
    #[serde(with = "decimal_string")]
    pub staleness_penalty: Decimal,
    #[serde(with = "decimal_string")]
    pub confidence_penalty: Decimal,
    #[serde(with = "decimal_string")]
    pub net_edge: Decimal,
}

impl EdgeDecomposition {
    /// Replace the request-time book-walk estimate and re-establish the sum law.
    pub fn with_slippage(mut self, slippage_est: Decimal) -> Self {
        self.slippage_est = slippage_est.max(Decimal::ZERO);
        self.net_edge = net_edge(
            self.gross_spread,
            &[
                self.fees,
                self.slippage_est,
                self.funding_cost,
                self.gas_cost,
                self.bridge_cost,
                self.settlement_mismatch_discount,
                self.liquidity_haircut,
                self.staleness_penalty,
                self.confidence_penalty,
            ],
        );
        self
    }
}

/// Context for computing a decomposition.
#[derive(Debug, Clone)]
pub struct DecompositionContext {
    pub buy_price: Decimal,
    pub sell_price: Decimal,
    pub price_kind: String,
    pub notional: Decimal,
    pub fee_bps: Decimal,
    /// Precomputed venue-specific fee amount. When present, this is already
    /// expressed in the same units as the opportunity P&L.
    pub fee_amount: Option<Decimal>,
    pub is_cross_chain: bool,
    pub bridge_fee_bps: Decimal,
    pub mismatch_discount: Decimal,
    pub funding_rate: Decimal,
    pub hold_hours: Decimal,
    pub max_quote_age_ms: i64,
    pub tick_stale_ms: i64,
    pub confidence: Decimal,
    pub confidence_k: Decimal,
    /// Whether this opportunity has an on-chain leg and therefore incurs gas.
    pub requires_gas: bool,
    pub gas_units: u64,
    pub gas_price_gwei: u64,
    /// Average visible depth used for the configured liquidity haircut.
    pub avg_depth: Decimal,
}

impl Default for DecompositionContext {
    fn default() -> Self {
        Self {
            buy_price: Decimal::ZERO,
            sell_price: Decimal::ZERO,
            price_kind: "probability".into(),
            notional: Decimal::new(100, 0),
            fee_bps: Decimal::new(1, 3), // 0.001 = 10bps
            fee_amount: None,
            is_cross_chain: false,
            bridge_fee_bps: Decimal::new(5, 3), // 0.5%
            mismatch_discount: Decimal::ZERO,
            funding_rate: Decimal::ZERO,
            hold_hours: Decimal::new(24, 0),
            max_quote_age_ms: 0,
            tick_stale_ms: 5000,
            confidence: Decimal::ONE,
            confidence_k: Decimal::new(1, 0),
            requires_gas: false,
            gas_units: 21000,
            gas_price_gwei: 20,
            avg_depth: Decimal::ZERO,
        }
    }
}

impl DecompositionContext {
    /// Apply venue-pair mismatch discount from config.
    pub fn apply_mismatch_config(&mut self, config: &MismatchConfig, venue_a: &str, venue_b: &str) {
        self.mismatch_discount = config.discount_for(venue_a, venue_b);
    }
}

/// Compute full decomposition. All 11 components, explicit zeros enforced.
pub fn decompose(ctx: &DecompositionContext) -> EdgeDecomposition {
    let gross = gross_spread(ctx.buy_price, ctx.sell_price, &ctx.price_kind);
    let fee = ctx.fee_amount.unwrap_or_else(|| fees(ctx.notional, ctx.fee_bps));
    let slippage = Decimal::ZERO; // Requires order book -- set by caller
    let funding = funding_cost(ctx.funding_rate, ctx.hold_hours);
    let gas =
        if ctx.requires_gas { gas_cost(ctx.gas_units, ctx.gas_price_gwei) } else { Decimal::ZERO };
    let bridge = bridge_cost(ctx.bridge_fee_bps, ctx.notional, ctx.is_cross_chain);
    let mismatch = settlement_mismatch_discount(gross, ctx.mismatch_discount);
    let liquidity = liquidity_haircut(ctx.notional, ctx.avg_depth);
    let staleness = staleness_penalty(ctx.max_quote_age_ms, ctx.tick_stale_ms);
    let confidence = confidence_penalty(gross, ctx.confidence, ctx.confidence_k);
    let costs =
        vec![fee, slippage, funding, gas, bridge, mismatch, liquidity, staleness, confidence];
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
        let ctx = DecompositionContext {
            notional: Decimal::ZERO,
            fee_bps: Decimal::ZERO,
            gas_units: 0,
            gas_price_gwei: 0,
            ..Default::default()
        };
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
        // Costs: fees 0.20 exceed gross, so the honest edge is negative.
        let ctx = DecompositionContext {
            buy_price: Decimal::new(65, 2),
            sell_price: Decimal::new(70, 2),
            price_kind: "probability".into(),
            notional: Decimal::new(100, 0),
            fee_bps: Decimal::new(1, 3),
            ..Default::default()
        };

        let ed = decompose(&ctx);
        assert_eq!(ed.gross_spread, Decimal::new(5, 2));
        assert_eq!(ed.fees, Decimal::new(2, 1)); // 100 * 0.001 * 2 = 0.20
        assert_eq!(ed.net_edge, Decimal::new(-15, 2));
    }
}

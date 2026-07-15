//! Golden tests for the 11-component edge decomposition engine (SPEC-012).
//!
//! Every test below is hand-computed.  Modifying a golden value requires
//! re-verification against a known-correct reference.

use aether_decompose::components::*;
use aether_decompose::decompose::{decompose, DecompositionContext};
use aether_decompose::mismatch::MismatchConfig;
use rust_decimal::Decimal;

// ---------------------------------------------------------------------------
// Component-level golden tests
// ---------------------------------------------------------------------------

/// Golden 1: All-zero context produces all zeros.
/// Every component must be present and explicitly zero, never defaulted.
#[test]
fn golden_all_zero() {
    let mut ctx = DecompositionContext::default();
    ctx.notional = Decimal::ZERO;
    ctx.fee_bps = Decimal::ZERO;
    ctx.gas_units = 0;
    ctx.gas_price_gwei = 0;
    let ed = decompose(&ctx);
    assert_eq!(ed.gross_spread, Decimal::ZERO, "gross_spread must be zero");
    assert_eq!(ed.fees, Decimal::ZERO, "fees must be zero");
    assert_eq!(ed.slippage_est, Decimal::ZERO, "slippage_est must be zero");
    assert_eq!(ed.funding_cost, Decimal::ZERO, "funding_cost must be zero");
    assert_eq!(ed.gas_cost, Decimal::ZERO, "gas_cost must be zero");
    assert_eq!(ed.bridge_cost, Decimal::ZERO, "bridge_cost must be zero");
    assert_eq!(
        ed.settlement_mismatch_discount, Decimal::ZERO,
        "settlement_mismatch_discount must be zero",
    );
    assert_eq!(ed.liquidity_haircut, Decimal::ZERO, "liquidity_haircut must be zero");
    assert_eq!(ed.staleness_penalty, Decimal::ZERO, "staleness_penalty must be zero");
    assert_eq!(ed.confidence_penalty, Decimal::ZERO, "confidence_penalty must be zero");
    assert_eq!(ed.net_edge, Decimal::ZERO, "net_edge must be zero");
}

/// Golden 2: Simple probability arb.
/// buy=0.65, sell=0.70  =>  gross_spread = 0.05.
#[test]
fn golden_probability_arb_simple() {
    let mut ctx = DecompositionContext::default();
    ctx.buy_price = Decimal::new(65, 2);
    ctx.sell_price = Decimal::new(70, 2);
    ctx.price_kind = "probability".into();
    ctx.notional = Decimal::new(100, 0);
    ctx.fee_bps = Decimal::new(1, 3); // 10bps
    ctx.confidence = Decimal::ONE;
    let ed = decompose(&ctx);
    assert_eq!(ed.gross_spread, Decimal::new(5, 2), "gross spread 0.05");
    assert_eq!(ed.fees, Decimal::new(2, 1), "fees = 100 * 0.001 * 2 = 0.20");
    assert_eq!(ed.gas_cost, Decimal::new(42, 5), "gas_cost = 0.00042");
    assert_eq!(ed.confidence_penalty, Decimal::ZERO, "full confidence -> zero");
    // net = 0.05 - 0.20 - 0.00042 = -0.15042, clamped to 0
    assert_eq!(ed.net_edge, Decimal::ZERO, "net_edge clamped to zero");
}

/// Golden 3: Currency arb.
/// buy=$99, sell=$101  =>  mid=$100, gross = |101-99|/100 = 0.02.
#[test]
fn golden_currency_arb() {
    let mut ctx = DecompositionContext::default();
    ctx.buy_price = Decimal::new(99, 0);
    ctx.sell_price = Decimal::new(101, 0);
    ctx.price_kind = "currency".into();
    ctx.fee_bps = Decimal::ZERO; // isolate gross spread check
    ctx.confidence = Decimal::ONE;
    ctx.gas_units = 0;
    ctx.gas_price_gwei = 0;
    let ed = decompose(&ctx);
    assert_eq!(ed.gross_spread, Decimal::new(2, 2), "gross = (101-99)/100 = 0.02");
    assert_eq!(ed.net_edge, Decimal::new(2, 2), "net = gross when all costs zero");
}

/// Golden 4: Sum-law verification with three components active.
/// Only fees, gas, and confidence_penalty are non-zero.
#[test]
fn golden_sum_law_three_components() {
    let mut ctx = DecompositionContext::default();
    ctx.buy_price = Decimal::new(100, 0);
    ctx.sell_price = Decimal::new(110, 0);
    ctx.price_kind = "currency".into();
    ctx.notional = Decimal::new(1000, 0);
    ctx.fee_bps = Decimal::new(1, 3); // 10bps
    ctx.confidence = Decimal::new(9, 1); // 0.9
    ctx.confidence_k = Decimal::ONE;
    ctx.gas_units = 21000;
    ctx.gas_price_gwei = 50;
    // gross = (110-100)/105 = 0.095238...
    let gross = gross_spread(Decimal::new(100, 0), Decimal::new(110, 0), "currency");
    let fee = fees(Decimal::new(1000, 0), Decimal::new(1, 3)); // 2.0
    let gas = gas_cost(21000, 50); // 0.00105
    let conf = confidence_penalty(gross, Decimal::new(9, 1), Decimal::ONE); // (1-0.9)*gross

    let ed = decompose(&ctx);
    assert_eq!(ed.gross_spread, gross);
    let costs = vec![fee, gas, conf];
    let total_costs: Decimal = costs.iter().sum();
    let expected_net = (gross - total_costs).max(Decimal::ZERO);
    assert_eq!(ed.net_edge, expected_net, "sum law: net = max(0, gross - costs)");
    assert_eq!(ed.fees, fee);
    assert_eq!(ed.gas_cost, gas);
    assert_eq!(ed.confidence_penalty, conf);
}

/// Golden 5: Fees golden value.
/// $1000 notional at 10bps per leg => $2.00 total.
#[test]
fn golden_fees() {
    assert_eq!(
        fees(Decimal::new(1000, 0), Decimal::new(1, 3)),
        Decimal::new(200, 2),
    );
}

/// Golden 6: Gross spread for probability returns zero when sell < buy.
/// Probability gross is max(0, sell - buy).
#[test]
fn golden_probability_no_arb() {
    let g = gross_spread(Decimal::new(70, 2), Decimal::new(65, 2), "probability");
    assert_eq!(g, Decimal::ZERO, "no arb when buy > sell");
}

/// Golden 7: Confidence penalty at 80% confidence.
/// penalty = (1 - 0.8) * gross * k = 0.2 * 100 * 1 = 20.
#[test]
fn golden_confidence_penalty() {
    let p = confidence_penalty(Decimal::new(100, 0), Decimal::new(8, 1), Decimal::ONE);
    assert_eq!(p, Decimal::new(20, 0));
}

/// Golden 8: Staleness penalty at 2x stale limit.
/// max_quote_age=10000ms, tick_stale=5000ms.
/// excess = (10000-5000)/5000 = 1.0, penalty = min(1.0, 1.0) * 0.01 = 0.01.
#[test]
fn golden_staleness_penalty() {
    let p = staleness_penalty(10000, 5000);
    assert_eq!(p, Decimal::new(1, 2));
}

/// Golden 9: Staleness penalty capped at 1%.
/// max_quote_age=60000ms, tick_stale=5000ms;
/// excess = 11.0, capped to 1.0, penalty = 0.01.
#[test]
fn golden_staleness_penalty_capped() {
    let p = staleness_penalty(60000, 5000);
    assert_eq!(p, Decimal::new(1, 2));
}

/// Golden 10: Liquidity haircut above depth.
/// size=300, depth=100, ratio=3, excess=2, haircut=2*0.005=0.01.
#[test]
fn golden_liquidity_haircut() {
    let h = liquidity_haircut(Decimal::new(300, 0), Decimal::new(100, 0));
    assert_eq!(h, Decimal::new(1, 2));
}

/// Golden 11: Bridge cost for cross-chain with 0.5% fee.
/// notional=1000, fee=0.5% => 5.0.
#[test]
fn golden_bridge_cost() {
    let c = bridge_cost(Decimal::new(5, 3), Decimal::new(1000, 0), true);
    assert_eq!(c, Decimal::new(5, 0));
}

/// Golden 12: Full decomposition with known values.
/// Reproduces a realistic scenario with all components contributing.
#[test]
fn golden_full_decomposition_realistic() {
    let mut ctx = DecompositionContext::default();
    ctx.buy_price = Decimal::new(40, 2);    // 0.40
    ctx.sell_price = Decimal::new(55, 2);   // 0.55
    ctx.price_kind = "probability".into();
    ctx.notional = Decimal::new(10000, 0);  // $10k
    ctx.fee_bps = Decimal::new(1, 3);        // 10bps
    ctx.is_cross_chain = true;
    ctx.bridge_fee_bps = Decimal::new(5, 3); // 0.5%
    ctx.mismatch_discount = Decimal::new(1, 2); // 1%
    ctx.funding_rate = Decimal::new(5, 3);   // 0.5% annual
    ctx.hold_hours = Decimal::new(12, 0);
    ctx.max_quote_age_ms = 10000;
    ctx.tick_stale_ms = 5000;
    ctx.confidence = Decimal::new(85, 2);    // 0.85
    ctx.confidence_k = Decimal::new(1, 0);
    ctx.gas_units = 50000;
    ctx.gas_price_gwei = 30;

    let ed = decompose(&ctx);

    // gross = 0.55 - 0.40 = 0.15
    assert_eq!(ed.gross_spread, Decimal::new(15, 2));

    // fees = 10000 * 0.001 * 2 = 20
    assert_eq!(ed.fees, Decimal::new(20, 0));

    // gas = 50000 * 30 / 1e9 = 0.0015
    assert_eq!(ed.gas_cost, Decimal::new(15, 4));

    // bridge = 10000 * 0.005 = 50
    assert_eq!(ed.bridge_cost, Decimal::new(50, 0));

    // settlement = 0.15 * 0.01 = 0.0015
    assert_eq!(ed.settlement_mismatch_discount, Decimal::new(15, 4));

    // funding = 0.005 * 12 / 24 = 0.0025
    assert_eq!(ed.funding_cost, Decimal::new(25, 4));

    // staleness = (10000-5000)/5000 = 1.0, capped at 1.0 => 1% = 0.01
    assert_eq!(ed.staleness_penalty, Decimal::new(1, 2));

    // confidence = (1-0.85) * 0.15 * 1 = 0.0225
    assert_eq!(ed.confidence_penalty, Decimal::new(225, 4));

    // slippage_est = 0, liquidity_haircut = 0 (no book/size provided)
    assert_eq!(ed.slippage_est, Decimal::ZERO);
    assert_eq!(ed.liquidity_haircut, Decimal::ZERO);

    // net = max(0, gross - sum(costs))
    let costs = vec![
        ed.fees,
        ed.slippage_est,
        ed.funding_cost,
        ed.gas_cost,
        ed.bridge_cost,
        ed.settlement_mismatch_discount,
        ed.liquidity_haircut,
        ed.staleness_penalty,
        ed.confidence_penalty,
    ];
    let total_costs: Decimal = costs.iter().sum();
    let expected_net = (ed.gross_spread - total_costs).max(Decimal::ZERO);
    assert_eq!(
        ed.net_edge, expected_net,
        "sum law holds: gross={}, costs={}, net={}",
        ed.gross_spread, total_costs, ed.net_edge,
    );
}

/// Golden 13: MismatchConfig default and lookup.
#[test]
fn golden_mismatch_config() {
    use std::collections::HashMap;
    let mut discounts = HashMap::new();
    discounts.insert("kalshi:polymarket".into(), Decimal::new(1, 2));
    let cfg = MismatchConfig::new(discounts, Decimal::new(5, 3));
    assert_eq!(cfg.discount_for("kalshi", "polymarket"), Decimal::new(1, 2));
    assert_eq!(cfg.discount_for("polymarket", "kalshi"), Decimal::new(1, 2), "reverse lookup");
    assert_eq!(cfg.discount_for("kalshi", "hyperliquid"), Decimal::new(5, 3), "fallback default");
}

/// Golden 14: Edge clamping when costs exceed gross.
#[test]
fn golden_net_edge_clamping() {
    // gross=1, costs=10 => net clamped to 0
    assert_eq!(net_edge(Decimal::new(1, 0), &[Decimal::new(10, 0)]), Decimal::ZERO);
    // gross=10, costs=10 => net clamped to 0 (gross - costs = 0)
    assert_eq!(net_edge(Decimal::new(10, 0), &[Decimal::new(10, 0)]), Decimal::ZERO);
    // gross=10, costs=9 => net = 1
    assert_eq!(net_edge(Decimal::new(10, 0), &[Decimal::new(9, 0)]), Decimal::new(1, 0));
}

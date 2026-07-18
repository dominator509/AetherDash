//! Individual edge-decomposition components.
//! Each is a pure function returning Decimal. Explicit zeros are REQUIRED.
//! SPEC-012 sum law: net_edge = gross_spread - sum(all others)

use aether_core::ids::Ulid;
use aether_core::order::{OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce};
use aether_core::quote::{OrderBook, Quote, QuoteSource};
use aether_fillmodel::config::FillConfig;
use aether_fillmodel::walk::{walk_book, FillError};
use rust_decimal::Decimal;

/// 1. gross_spread: leg-weighted price gap in common space.
///
/// For binary contracts: |price_a - price_b|. For currency: |mid_a - mid_b| / mid_a.
pub fn gross_spread(
    buy_price: Decimal,
    sell_price: Decimal,
    kind: &str, // "probability" or "currency"
) -> Decimal {
    if buy_price <= Decimal::ZERO || sell_price <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    match kind {
        "probability" => (sell_price - buy_price).max(Decimal::ZERO),
        _ => {
            let mid = (buy_price + sell_price) / Decimal::new(2, 0);
            if mid <= Decimal::ZERO {
                Decimal::ZERO
            } else {
                ((sell_price - buy_price) / mid).abs()
            }
        }
    }
}

/// 2. fees: venue fee schedules. Default 10bps per leg (0.001 x notional each).
pub fn fees(notional: Decimal, fee_bps: Decimal) -> Decimal {
    notional.max(Decimal::ZERO) * fee_bps.clamp(Decimal::ZERO, Decimal::ONE) * Decimal::new(2, 0)
    // both legs
}

/// 3. slippage_est: book-walk cost to fill intended size.
pub fn slippage_est(book: &OrderBook, size: Decimal, side: Side) -> Result<Decimal, FillError> {
    let origin = Origin::new(OriginKind::Automation, 3, Ulid::new())
        .map_err(|error| FillError::InvalidInput(error.to_string()))?;
    let quote_snapshot = Quote {
        market: book.market.clone(),
        bid: book.bids().first().map(|l| l.price),
        ask: book.asks().first().map(|l| l.price),
        mid: None,
        last: None,
        bid_size: book.bids().first().map(|l| l.size),
        ask_size: book.asks().first().map(|l| l.size),
        ts: book.ts,
        source: QuoteSource::Snapshot,
        seq: book.seq,
    };
    let intent = OrderIntent {
        id: Ulid::new(),
        market: book.market.clone(),
        side,
        order_type: OrderType::Market,
        limit_price: None,
        size,
        size_unit: SizeUnit::Contracts,
        tif: TimeInForce::Ioc,
        paper: true,
        origin,
        quote_snapshot,
        caps_version: Ulid::new(),
        created_ts: book.ts,
    };
    let fills = walk_book(book, &intent, &FillConfig::default())?;
    let total_notional: Decimal = fills.iter().map(|f| f.price * f.size).sum();
    let total_size: Decimal = fills.iter().map(|f| f.size).sum();
    if total_size <= Decimal::ZERO {
        return Err(FillError::InvalidInput("fill size must be positive".to_owned()));
    }
    let avg_price = total_notional / total_size;
    let best_price = match side {
        Side::Buy | Side::BuyNo => book.asks().first().map(|level| level.price),
        Side::Sell | Side::SellNo => book.bids().first().map(|level| level.price),
    }
    .ok_or_else(|| FillError::NoLiquidity { market: book.market.as_str().to_owned(), side })?;
    Ok(((avg_price - best_price) / best_price).abs())
}

/// 4. funding_cost: for perps only. funding_rate x expected hold duration.
pub fn funding_cost(funding_rate: Decimal, hold_hours: Decimal) -> Decimal {
    funding_rate * hold_hours / Decimal::new(24, 0) // annualized -> hourly prorated
}

/// 5. gas_cost: simulated gas x gas price. V1 simplified.
pub fn gas_cost(gas_units: u64, gas_price_gwei: u64) -> Decimal {
    Decimal::from(gas_units) * Decimal::from(gas_price_gwei) / Decimal::new(1_000_000_000, 0)
}

/// 6. bridge_cost: cross-chain bridge fee + time-value penalty.
pub fn bridge_cost(bridge_fee_bps: Decimal, notional: Decimal, is_cross_chain: bool) -> Decimal {
    if is_cross_chain {
        notional * bridge_fee_bps
    } else {
        Decimal::ZERO
    }
}

/// 7. settlement_mismatch_discount: configured discount for venue pair oracle mismatch.
pub fn settlement_mismatch_discount(gross: Decimal, discount_factor: Decimal) -> Decimal {
    gross * discount_factor
}

/// 8. liquidity_haircut: post-entry exit risk, fn of size vs depth.
pub fn liquidity_haircut(size: Decimal, avg_depth: Decimal) -> Decimal {
    if avg_depth <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    let ratio = size / avg_depth;
    if ratio <= Decimal::ONE {
        Decimal::ZERO
    } else {
        (ratio - Decimal::ONE) * Decimal::new(5, 3) // 0.5% per unit above depth
    }
}

/// 9. staleness_penalty: monotone in max quote age, zero below tick_stale_ms.
pub fn staleness_penalty(max_quote_age_ms: i64, tick_stale_ms: i64) -> Decimal {
    if tick_stale_ms <= 0 || max_quote_age_ms <= tick_stale_ms {
        Decimal::ZERO
    } else {
        let excess = Decimal::from(max_quote_age_ms - tick_stale_ms) / Decimal::from(tick_stale_ms);
        excess.min(Decimal::ONE) * Decimal::new(1, 2) // 1% per multiple of stale_ms up to 1%
    }
}

/// 10. confidence_penalty: (1-confidence) x gross_spread x k.
pub fn confidence_penalty(gross_spread: Decimal, confidence: Decimal, k: Decimal) -> Decimal {
    let bounded_confidence = confidence.clamp(Decimal::ZERO, Decimal::ONE);
    (Decimal::ONE - bounded_confidence) * gross_spread.max(Decimal::ZERO) * k.max(Decimal::ZERO)
}

/// 11. net_edge: gross_spread minus sum of all components.
pub fn net_edge(gross: Decimal, costs: &[Decimal]) -> Decimal {
    let total_costs: Decimal = costs.iter().sum();
    gross - total_costs
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::ids::{MarketKey, VenueId};
    use aether_core::quote::BookLevel;
    use aether_core::time::UtcTime;

    fn test_market() -> MarketKey {
        MarketKey::new(&VenueId::new("test").unwrap(), "TEST").unwrap()
    }

    fn test_book() -> OrderBook {
        OrderBook::new(
            test_market(),
            vec![
                BookLevel { price: Decimal::new(9980, 2), size: Decimal::new(1000, 0) },
                BookLevel { price: Decimal::new(9970, 2), size: Decimal::new(500, 0) },
            ],
            vec![
                BookLevel { price: Decimal::new(10020, 2), size: Decimal::new(1000, 0) },
                BookLevel { price: Decimal::new(10050, 2), size: Decimal::new(500, 0) },
            ],
            2,
            UtcTime::now(),
            None,
        )
        .expect("valid test book")
    }

    #[test]
    fn gross_spread_probability_simple() {
        // buy 0.65, sell 0.70 => gross = 0.05
        let g = gross_spread(Decimal::new(65, 2), Decimal::new(70, 2), "probability");
        assert_eq!(g, Decimal::new(5, 2));
    }

    #[test]
    fn gross_spread_currency() {
        let g = gross_spread(Decimal::new(99, 0), Decimal::new(101, 0), "currency");
        // mid = 100, |101-99|/100 = 0.02
        assert_eq!(g, Decimal::new(2, 2));
    }

    #[test]
    fn gross_spread_zero_on_invalid_input() {
        assert_eq!(gross_spread(Decimal::ZERO, Decimal::new(70, 2), "probability"), Decimal::ZERO);
        assert_eq!(gross_spread(Decimal::new(65, 2), Decimal::ZERO, "probability"), Decimal::ZERO);
    }

    #[test]
    fn fees_correct() {
        // $1000 notional x 10bps x 2 legs = $2
        assert_eq!(fees(Decimal::new(1000, 0), Decimal::new(1, 3)), Decimal::new(2, 0));
    }

    #[test]
    fn fees_zero_notional() {
        assert_eq!(fees(Decimal::ZERO, Decimal::new(1, 3)), Decimal::ZERO);
    }

    #[test]
    fn slippage_est_returns_decimal() {
        let s = slippage_est(&test_book(), Decimal::new(100, 0), Side::Buy).unwrap();
        assert!(s >= Decimal::ZERO);
    }

    #[test]
    fn slippage_est_rejects_empty_execution_side() {
        let book = OrderBook::new(test_market(), vec![], vec![], 0, UtcTime::now(), None).unwrap();
        assert!(matches!(
            slippage_est(&book, Decimal::ONE, Side::Buy),
            Err(FillError::NoLiquidity { .. })
        ));
    }

    #[test]
    fn funding_cost_hourly() {
        // 10% annual rate = 0.10, 24 hour hold => 0.10 * 24/24 = 0.10
        let r = funding_cost(Decimal::new(10, 2), Decimal::new(24, 0));
        assert_eq!(r, Decimal::new(10, 2));
    }

    #[test]
    fn funding_cost_zero_rate() {
        assert_eq!(funding_cost(Decimal::ZERO, Decimal::new(24, 0)), Decimal::ZERO);
    }

    #[test]
    fn gas_cost_simple() {
        // 21000 units x 20 gwei / 1e9 = 0.00042
        assert_eq!(gas_cost(21000, 20), Decimal::new(42, 5));
    }

    #[test]
    fn bridge_cost_only_cross_chain() {
        assert_eq!(bridge_cost(Decimal::new(5, 3), Decimal::new(1000, 0), false), Decimal::ZERO);
        // 0.5% of 1000 = 5
        assert_eq!(
            bridge_cost(Decimal::new(5, 3), Decimal::new(1000, 0), true),
            Decimal::new(5, 0)
        );
    }

    #[test]
    fn settlement_mismatch_discount_10pct() {
        let gross = Decimal::new(100, 0);
        let discount = settlement_mismatch_discount(gross, Decimal::new(10, 2));
        assert_eq!(discount, Decimal::new(10, 0));
    }

    #[test]
    fn liquidity_haircut_no_haircut_below_depth() {
        assert_eq!(liquidity_haircut(Decimal::new(50, 0), Decimal::new(100, 0)), Decimal::ZERO);
    }

    #[test]
    fn liquidity_haircut_above_depth() {
        // ratio=200/100=2, excess=1, 1 * 0.005 = 0.005
        let h = liquidity_haircut(Decimal::new(200, 0), Decimal::new(100, 0));
        assert_eq!(h, Decimal::new(5, 3));
    }

    #[test]
    fn liquidity_haircut_zero_depth() {
        assert_eq!(liquidity_haircut(Decimal::new(100, 0), Decimal::ZERO), Decimal::ZERO);
    }

    #[test]
    fn staleness_penalty_fresh() {
        assert_eq!(staleness_penalty(1000, 5000), Decimal::ZERO);
    }

    #[test]
    fn staleness_penalty_stale() {
        // 10s stale vs 5s limit: excess = (10000-5000)/5000 = 1.0
        // penalty = min(1.0, 1.0) * 0.01 = 0.01
        let p = staleness_penalty(10000, 5000);
        assert_eq!(p, Decimal::new(1, 2));
    }

    #[test]
    fn staleness_penalty_capped() {
        // 50s stale vs 5s limit: excess = 9.0 capped to 1.0, penalty = 0.01
        let p = staleness_penalty(50000, 5000);
        assert_eq!(p, Decimal::new(1, 2));
    }

    #[test]
    fn confidence_penalty_80pct() {
        // (1-0.8) * 100 * 1 = 20
        let p = confidence_penalty(Decimal::new(100, 0), Decimal::new(8, 1), Decimal::ONE);
        assert_eq!(p, Decimal::new(20, 0));
    }

    #[test]
    fn confidence_penalty_full_confidence() {
        assert_eq!(
            confidence_penalty(Decimal::new(100, 0), Decimal::ONE, Decimal::ONE),
            Decimal::ZERO
        );
    }

    #[test]
    fn net_edge_simple() {
        let gross = Decimal::new(100, 0);
        let costs = vec![Decimal::new(10, 0), Decimal::new(5, 0), Decimal::new(2, 0)];
        assert_eq!(net_edge(gross, &costs), Decimal::new(83, 0));
    }

    #[test]
    fn net_edge_preserves_negative_edge_for_sum_law() {
        let gross = Decimal::new(10, 0);
        let costs = vec![Decimal::new(10, 0), Decimal::new(10, 0)];
        assert_eq!(net_edge(gross, &costs), Decimal::new(-10, 0));
    }

    #[test]
    fn net_edge_empty_costs() {
        assert_eq!(net_edge(Decimal::new(100, 0), &[]), Decimal::new(100, 0));
    }
}

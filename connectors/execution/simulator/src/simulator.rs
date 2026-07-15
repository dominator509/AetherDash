//! Request-time decomposition + fill walk simulator.

use aether_core::ids::Ulid;
use aether_core::order::{
    OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce,
};
use aether_core::quote::{OrderBook, Quote, QuoteSource};
use aether_core::time::UtcTime;
use aether_core::Fill;
use aether_decompose::components::net_edge;
use aether_decompose::decompose::{decompose, DecompositionContext, EdgeDecomposition};
use aether_decompose::mismatch::MismatchConfig;
use aether_fillmodel::config::FillConfig;
use aether_fillmodel::walk::walk_book;
use rust_decimal::Decimal;
use thiserror::Error;

use crate::sensitivity::SensitivityTable;

/// Simulator configuration.
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub default_fee_bps: Decimal,
    pub default_bridge_bps: Decimal,
    pub confidence_k: Decimal,
    pub fill_config: FillConfig,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            default_fee_bps: Decimal::new(1, 3),   // 10 bps
            default_bridge_bps: Decimal::new(5, 3), // 0.5%
            confidence_k: Decimal::new(1, 0),
            fill_config: FillConfig::default(),
        }
    }
}

/// Simulation inputs.
#[derive(Debug, Clone)]
pub struct SimulationInput {
    pub buy_price: Decimal,
    pub sell_price: Decimal,
    pub price_kind: String,
    pub notional: Decimal,
    pub buy_book: Option<OrderBook>,
    pub sell_book: Option<OrderBook>,
    pub funding_rate: Decimal,
    pub hold_hours: Decimal,
    pub max_quote_age_ms: i64,
    pub tick_stale_ms: i64,
    pub confidence: Decimal,
    pub is_cross_chain: bool,
    pub mismatch_discount: Decimal,
}

/// Full simulation result.
#[derive(Debug, Clone)]
pub struct Simulation {
    pub decomposition: EdgeDecomposition,
    pub buy_fills: Vec<Fill>,
    pub sell_fills: Vec<Fill>,
    pub sensitivity: SensitivityTable,
}

/// Errors from the simulator.
#[derive(Error, Debug)]
pub enum SimulationError {
    /// The input data was insufficient to produce a result.
    #[error("insufficient data: {0}")]
    InsufficientData(String),
}

/// The request-time simulator.
///
/// Combines the decomposition engine with the shared fill walk to produce
/// a complete net-edge picture including sensitivity analysis.
pub struct Simulator {
    config: SimulationConfig,
    #[allow(dead_code)]
    mismatch: MismatchConfig,
}

impl Simulator {
    pub fn new(config: SimulationConfig) -> Self {
        Self {
            config,
            mismatch: MismatchConfig::default(),
        }
    }

    /// Run a full simulation including decomposition, optional book walk,
    /// and sensitivity table.
    pub fn simulate(
        &self,
        input: &SimulationInput,
    ) -> Result<Simulation, SimulationError> {
        // ---- 1. Compute base decomposition ----
        let ctx = DecompositionContext {
            buy_price: input.buy_price,
            sell_price: input.sell_price,
            price_kind: input.price_kind.clone(),
            notional: input.notional,
            fee_bps: self.config.default_fee_bps,
            is_cross_chain: input.is_cross_chain,
            bridge_fee_bps: self.config.default_bridge_bps,
            mismatch_discount: input.mismatch_discount,
            funding_rate: input.funding_rate,
            hold_hours: input.hold_hours,
            max_quote_age_ms: input.max_quote_age_ms,
            tick_stale_ms: input.tick_stale_ms,
            confidence: input.confidence,
            confidence_k: self.config.confidence_k,
            // gas_units and gas_price_gwei use Default
            ..Default::default()
        };
        let mut decomp = decompose(&ctx);

        // ---- 2. Book walk for fills ----
        let buy_fills = if let Some(ref book) = input.buy_book {
            match walk_book(
                book,
                &build_intent(book, Side::Buy, input.notional),
                &self.config.fill_config,
            ) {
                Ok(fills) => {
                    decomp.slippage_est =
                        compute_slippage(&fills, input.notional, Side::Buy, book);
                    fills
                }
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        let sell_fills = if let Some(ref book) = input.sell_book {
            match walk_book(
                book,
                &build_intent(book, Side::Sell, input.notional),
                &self.config.fill_config,
            ) {
                Ok(fills) => {
                    let sell_slip =
                        compute_slippage(&fills, input.notional, Side::Sell, book);
                    decomp.slippage_est = decomp.slippage_est.max(sell_slip);
                    fills
                }
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        // ---- 3. Recompute net-edge after actual slippage ----
        let all_costs = vec![
            decomp.fees,
            decomp.slippage_est,
            decomp.funding_cost,
            decomp.gas_cost,
            decomp.bridge_cost,
            decomp.settlement_mismatch_discount,
            decomp.liquidity_haircut,
            decomp.staleness_penalty,
            decomp.confidence_penalty,
        ];
        decomp.net_edge = net_edge(decomp.gross_spread, &all_costs);

        // ---- 4. Sensitivity ----
        let sensitivity = SensitivityTable::compute(
            &decomp,
            input.buy_price,
            input.sell_price,
            &input.price_kind,
            input.notional,
            self.config.default_fee_bps,
            input.is_cross_chain,
            self.config.default_bridge_bps,
            input.mismatch_discount,
            input.funding_rate,
            input.hold_hours,
            input.tick_stale_ms,
            input.confidence,
            self.config.confidence_k,
        );

        Ok(Simulation {
            decomposition: decomp,
            buy_fills,
            sell_fills,
            sensitivity,
        })
    }
}

// -- Private helpers ---------------------------------------------------------

/// Build an `OrderIntent` from a book snapshot, side, and size.
///
/// The intent is for a paper market order with immediate-or-cancel
/// time-in-force, matching the conventions used by the paper ledger.
fn build_intent(book: &OrderBook, side: Side, size: Decimal) -> OrderIntent {
    let origin =
        Origin::new(OriginKind::Automation, 3, Ulid::new()).expect("valid origin with tier 3");

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

    OrderIntent {
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
        created_ts: UtcTime::now(),
    }
}

/// Compute the fractional slippage (`|avg - best| / best`) from a set of fills.
///
/// Returns a Decimal fraction (e.g. 0.01 for 1% slippage) compatible with
/// the cost units used by the decomposition engine.
fn compute_slippage(fills: &[Fill], _size: Decimal, side: Side, book: &OrderBook) -> Decimal {
    if fills.is_empty() {
        return Decimal::ZERO;
    }

    let total_notional: Decimal = fills.iter().map(|f| f.price * f.size).sum();
    let total_size: Decimal = fills.iter().map(|f| f.size).sum();

    if total_size <= Decimal::ZERO {
        return Decimal::ZERO;
    }

    let avg_price = total_notional / total_size;
    let best_price = match side {
        Side::Buy | Side::BuyNo => book
            .asks()
            .first()
            .map(|l| l.price)
            .unwrap_or(Decimal::ZERO),
        Side::Sell | Side::SellNo => book
            .bids()
            .first()
            .map(|l| l.price)
            .unwrap_or(Decimal::ZERO),
    };

    if best_price > Decimal::ZERO {
        ((avg_price - best_price) / best_price).abs()
    } else {
        Decimal::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::ids::{MarketKey, VenueId};
    use aether_core::quote::BookLevel;

    fn test_market() -> MarketKey {
        MarketKey::new(&VenueId::new("test").unwrap(), "TST").unwrap()
    }

    fn test_book() -> OrderBook {
        let ts = UtcTime::from_unix_millis(1752152096000).unwrap();
        OrderBook::new(
            test_market(),
            vec![
                BookLevel {
                    price: Decimal::new(9980, 2),
                    size: Decimal::new(1000, 0),
                },
                BookLevel {
                    price: Decimal::new(9970, 2),
                    size: Decimal::new(500, 0),
                },
            ],
            vec![
                BookLevel {
                    price: Decimal::new(10020, 2),
                    size: Decimal::new(1000, 0),
                },
                BookLevel {
                    price: Decimal::new(10050, 2),
                    size: Decimal::new(500, 0),
                },
            ],
            2,
            ts,
            None,
        )
        .expect("valid test book")
    }

    #[test]
    fn build_intent_creates_market_order() {
        let book = test_book();
        let intent = build_intent(&book, Side::Buy, Decimal::new(100, 0));
        assert_eq!(intent.side, Side::Buy);
        assert_eq!(intent.size, Decimal::new(100, 0));
        assert_eq!(intent.order_type, OrderType::Market);
        assert!(intent.paper);
        assert_eq!(intent.tif, TimeInForce::Ioc);
        assert_eq!(intent.market, test_market());
    }

    #[test]
    fn compute_slippage_empty_fills() {
        let book = test_book();
        let s = compute_slippage(&[], Decimal::new(100, 0), Side::Buy, &book);
        assert_eq!(s, Decimal::ZERO);
    }

    #[test]
    fn simulate_decomposes_without_books() {
        let sim = Simulator::new(SimulationConfig::default());
        let input = SimulationInput {
            buy_price: Decimal::new(65, 2),
            sell_price: Decimal::new(70, 2),
            price_kind: "probability".into(),
            notional: Decimal::new(100, 0),
            buy_book: None,
            sell_book: None,
            funding_rate: Decimal::ZERO,
            hold_hours: Decimal::ZERO,
            max_quote_age_ms: 0,
            tick_stale_ms: 5000,
            confidence: Decimal::ONE,
            is_cross_chain: false,
            mismatch_discount: Decimal::ZERO,
        };
        let result = sim.simulate(&input).expect("simulation without books should succeed");
        assert_eq!(result.decomposition.gross_spread, Decimal::new(5, 2));
        assert!(result.buy_fills.is_empty());
        assert!(result.sell_fills.is_empty());
        assert_eq!(result.sensitivity.rows.len(), 16); // 4 sizes x 4 staleness
    }

    #[test]
    fn simulate_with_buy_book_produces_fills() {
        let sim = Simulator::new(SimulationConfig::default());
        let book = test_book();
        let input = SimulationInput {
            buy_price: Decimal::new(100, 0),
            sell_price: Decimal::new(101, 0),
            price_kind: "probability".into(),
            notional: Decimal::new(100, 0),
            buy_book: Some(book),
            sell_book: None,
            funding_rate: Decimal::ZERO,
            hold_hours: Decimal::ZERO,
            max_quote_age_ms: 0,
            tick_stale_ms: 5000,
            confidence: Decimal::ONE,
            is_cross_chain: false,
            mismatch_discount: Decimal::ZERO,
        };
        let result = sim.simulate(&input).expect("simulation with buy book should succeed");
        assert!(!result.buy_fills.is_empty(), "should have buy fills");
        assert!(result.sell_fills.is_empty());
        // First fill should be at best ask = 100.20
        assert_eq!(result.buy_fills[0].price, Decimal::new(10020, 2));
        assert_eq!(result.buy_fills[0].size, Decimal::new(100, 0));
    }
}

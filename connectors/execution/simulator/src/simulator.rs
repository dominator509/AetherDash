//! Request-time decomposition + fill walk simulator.

use aether_core::decimal::decimal_string;
use aether_core::ids::Ulid;
use aether_core::order::{OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce};
use aether_core::quote::{OrderBook, Quote, QuoteSource};
use aether_core::time::UtcTime;
use aether_core::Fill;
use aether_decompose::decompose::{decompose, DecompositionContext, EdgeDecomposition};
use aether_decompose::fees::{FeeCatalog, FeeError};
use aether_decompose::mismatch::MismatchConfig;
use aether_fillmodel::config::FillConfig;
use aether_fillmodel::walk::walk_book;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
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
            default_fee_bps: Decimal::new(1, 3),    // 10 bps
            default_bridge_bps: Decimal::new(5, 3), // 0.5%
            confidence_k: Decimal::new(1, 0),
            fill_config: FillConfig::default(),
        }
    }
}

/// Simulation inputs.
#[derive(Debug, Clone, Deserialize)]
pub struct SimulationInput {
    #[serde(with = "decimal_string")]
    pub buy_price: Decimal,
    #[serde(with = "decimal_string")]
    pub sell_price: Decimal,
    pub price_kind: String,
    #[serde(with = "decimal_string")]
    pub notional: Decimal,
    pub buy_book: Option<OrderBook>,
    pub sell_book: Option<OrderBook>,
    #[serde(with = "decimal_string")]
    pub funding_rate: Decimal,
    #[serde(with = "decimal_string")]
    pub hold_hours: Decimal,
    pub max_quote_age_ms: i64,
    pub tick_stale_ms: i64,
    #[serde(with = "decimal_string")]
    pub confidence: Decimal,
    pub is_cross_chain: bool,
    pub buy_venue: String,
    pub sell_venue: String,
}

/// Full simulation result.
#[derive(Debug, Clone, Serialize)]
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
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("fill-model error: {0}")]
    Fill(String),
    #[error("fee configuration error: {0}")]
    Fee(#[from] FeeError),
}

/// The request-time simulator.
///
/// Combines the decomposition engine with the shared fill walk to produce
/// a complete net-edge picture including sensitivity analysis.
pub struct Simulator {
    config: SimulationConfig,
    mismatch: MismatchConfig,
    fees: FeeCatalog,
}

impl Simulator {
    pub fn new(config: SimulationConfig) -> Result<Self, SimulationError> {
        Ok(Self {
            config,
            mismatch: MismatchConfig::load_embedded()
                .map_err(|error| SimulationError::Configuration(error.to_string()))?,
            fees: FeeCatalog::load_embedded()?,
        })
    }

    /// Run a full simulation including decomposition, optional book walk,
    /// and sensitivity table.
    pub fn simulate(&self, input: &SimulationInput) -> Result<Simulation, SimulationError> {
        validate_input(input)?;
        // ---- 1. Compute base decomposition ----
        let fee_amount = self.fees.estimate_pair(
            &input.buy_venue,
            &input.sell_venue,
            input.buy_price,
            input.sell_price,
            input.notional,
        )?;
        let ctx = DecompositionContext {
            buy_price: input.buy_price,
            sell_price: input.sell_price,
            price_kind: input.price_kind.clone(),
            notional: input.notional,
            fee_amount: Some(fee_amount),
            is_cross_chain: input.is_cross_chain,
            bridge_fee_bps: self.config.default_bridge_bps,
            mismatch_discount: self.mismatch.discount_for(&input.buy_venue, &input.sell_venue),
            funding_rate: input.funding_rate,
            hold_hours: input.hold_hours,
            max_quote_age_ms: input.max_quote_age_ms,
            tick_stale_ms: input.tick_stale_ms,
            confidence: input.confidence,
            confidence_k: self.config.confidence_k,
            // gas_units and gas_price_gwei use Default
            ..Default::default()
        };
        let decomp = decompose(&ctx);

        // ---- 2. Book walk for fills ----
        let buy_book = input
            .buy_book
            .as_ref()
            .ok_or_else(|| SimulationError::InsufficientData("buy_book is required".into()))?;
        let sell_book = input
            .sell_book
            .as_ref()
            .ok_or_else(|| SimulationError::InsufficientData("sell_book is required".into()))?;
        let (buy_fills, buy_slippage) =
            walk_leg(buy_book, Side::Buy, input.notional, &self.config.fill_config)?;
        let (sell_fills, sell_slippage) =
            walk_leg(sell_book, Side::Sell, input.notional, &self.config.fill_config)?;

        // ---- 3. Recompute net-edge after actual slippage ----
        let decomp = decomp.with_slippage(buy_slippage + sell_slippage);

        // ---- 4. Sensitivity ----
        let sensitivity = SensitivityTable::compute(
            buy_book,
            sell_book,
            &self.config.fill_config,
            input.buy_price,
            input.sell_price,
            &input.price_kind,
            input.notional,
            &self.fees,
            &input.buy_venue,
            &input.sell_venue,
            input.is_cross_chain,
            self.config.default_bridge_bps,
            self.mismatch.discount_for(&input.buy_venue, &input.sell_venue),
            input.funding_rate,
            input.hold_hours,
            input.tick_stale_ms,
            input.confidence,
            self.config.confidence_k,
        )?;

        Ok(Simulation { decomposition: decomp, buy_fills, sell_fills, sensitivity })
    }
}

// -- Private helpers ---------------------------------------------------------

/// Run one simulator/paper-ledger-compatible fill leg and return its
/// fractional slippage. Scanner candidates use this same seam.
pub fn walk_leg(
    book: &OrderBook,
    side: Side,
    size: Decimal,
    config: &FillConfig,
) -> Result<(Vec<Fill>, Decimal), SimulationError> {
    let intent = build_intent(book, side, size)?;
    let fills = walk_book(book, &intent, config)
        .map_err(|error| SimulationError::Fill(error.to_string()))?;
    let slippage = compute_slippage(&fills, side, book);
    Ok((fills, slippage))
}

fn validate_input(input: &SimulationInput) -> Result<(), SimulationError> {
    if input.buy_price <= Decimal::ZERO || input.sell_price <= Decimal::ZERO {
        return Err(SimulationError::InsufficientData(
            "buy_price and sell_price must be positive".into(),
        ));
    }
    if input.notional <= Decimal::ZERO {
        return Err(SimulationError::InsufficientData("notional must be positive".into()));
    }
    if !matches!(input.price_kind.as_str(), "probability" | "currency") {
        return Err(SimulationError::InsufficientData(
            "price_kind must be probability or currency".into(),
        ));
    }
    if input.confidence < Decimal::ZERO || input.confidence > Decimal::ONE {
        return Err(SimulationError::InsufficientData(
            "confidence must be between zero and one".into(),
        ));
    }
    if input.max_quote_age_ms < 0 || input.tick_stale_ms <= 0 || input.hold_hours < Decimal::ZERO {
        return Err(SimulationError::InsufficientData(
            "quote ages, stale threshold, and hold duration are invalid".into(),
        ));
    }
    let (Some(buy_book), Some(sell_book)) = (&input.buy_book, &input.sell_book) else {
        return Err(SimulationError::InsufficientData(
            "buy_book and sell_book are required for fill-model simulation".into(),
        ));
    };
    for (book, venue) in [(buy_book, &input.buy_venue), (sell_book, &input.sell_venue)] {
        if venue.is_empty() || !book.market.as_str().starts_with(&format!("mkt:{venue}:")) {
            return Err(SimulationError::InsufficientData(
                "book market does not match its declared venue".into(),
            ));
        }
    }
    Ok(())
}

/// Build an `OrderIntent` from a book snapshot, side, and size.
///
/// The intent is for a paper market order with immediate-or-cancel
/// time-in-force, matching the conventions used by the paper ledger.
fn build_intent(
    book: &OrderBook,
    side: Side,
    size: Decimal,
) -> Result<OrderIntent, SimulationError> {
    let origin = Origin::new(OriginKind::Automation, 3, Ulid::new())
        .map_err(|error| SimulationError::Configuration(error.to_string()))?;

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

    Ok(OrderIntent {
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
    })
}

/// Compute the fractional slippage (`|avg - best| / best`) from a set of fills.
fn compute_slippage(fills: &[Fill], side: Side, book: &OrderBook) -> Decimal {
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
        Side::Buy | Side::BuyNo => book.asks().first().map(|l| l.price).unwrap_or(Decimal::ZERO),
        Side::Sell | Side::SellNo => book.bids().first().map(|l| l.price).unwrap_or(Decimal::ZERO),
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
        MarketKey::new(&VenueId::new("hyperliquid").unwrap(), "TST").unwrap()
    }

    fn test_book() -> OrderBook {
        let ts = UtcTime::from_unix_millis(1752152096000).unwrap();
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
            ts,
            None,
        )
        .expect("valid test book")
    }

    #[test]
    fn build_intent_creates_market_order() {
        let book = test_book();
        let intent = build_intent(&book, Side::Buy, Decimal::new(100, 0)).unwrap();
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
        let s = compute_slippage(&[], Side::Buy, &book);
        assert_eq!(s, Decimal::ZERO);
    }

    #[test]
    fn simulate_rejects_missing_books() {
        let sim = Simulator::new(SimulationConfig::default()).unwrap();
        let input = SimulationInput {
            buy_price: Decimal::new(65, 2),
            sell_price: Decimal::new(70, 2),
            price_kind: "currency".into(),
            notional: Decimal::new(100, 0),
            buy_book: None,
            sell_book: None,
            funding_rate: Decimal::ZERO,
            hold_hours: Decimal::ZERO,
            max_quote_age_ms: 0,
            tick_stale_ms: 5000,
            confidence: Decimal::ONE,
            is_cross_chain: false,
            buy_venue: "hyperliquid".into(),
            sell_venue: "hyperliquid".into(),
        };
        assert!(matches!(sim.simulate(&input), Err(SimulationError::InsufficientData(_))));
    }

    #[test]
    fn simulate_with_buy_book_produces_fills() {
        let sim = Simulator::new(SimulationConfig::default()).unwrap();
        let book = test_book();
        let input = SimulationInput {
            buy_price: Decimal::new(100, 0),
            sell_price: Decimal::new(101, 0),
            price_kind: "currency".into(),
            notional: Decimal::new(100, 0),
            buy_book: Some(book.clone()),
            sell_book: Some(book),
            funding_rate: Decimal::ZERO,
            hold_hours: Decimal::ZERO,
            max_quote_age_ms: 0,
            tick_stale_ms: 5000,
            confidence: Decimal::ONE,
            is_cross_chain: false,
            buy_venue: "hyperliquid".into(),
            sell_venue: "hyperliquid".into(),
        };
        let result = sim.simulate(&input).expect("simulation with buy book should succeed");
        assert!(!result.buy_fills.is_empty(), "should have buy fills");
        assert!(!result.sell_fills.is_empty(), "should have sell fills");
        // First fill should be at best ask = 100.20
        assert_eq!(result.buy_fills[0].price, Decimal::new(10020, 2));
        assert_eq!(result.buy_fills[0].size, Decimal::new(100, 0));
    }
}

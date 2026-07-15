//! Book-walk fill algorithm.
//!
//! Walks the order book from best level inward, filling against
//! available liquidity until the intent size is satisfied.

use crate::config::{Aggressiveness, FillConfig};
use aether_core::ids::Money;
use aether_core::json::JsonObject;
use aether_core::order::{Fill, OrderIntent, OrderType, Side};
use aether_core::quote::{BookLevel, OrderBook};
use rust_decimal::Decimal;
use thiserror::Error;

/// Errors from the fill model.
#[derive(Error, Debug)]
pub enum FillError {
    /// The order book has no liquidity on the requested side.
    #[error("no liquidity on side {side:?} for market {market}")]
    NoLiquidity { market: String, side: Side },

    /// The intent size could not be fully filled even with depth exhaustion.
    #[error("insufficient depth: filled {filled} of {requested} for market {market}")]
    InsufficientDepth { market: String, filled: Decimal, requested: Decimal },

    /// The limit price was violated by the fill price.
    #[error("limit price {limit} violated by fill at {fill_price}")]
    LimitViolation { limit: Decimal, fill_price: Decimal },

    /// The book and intent refer to different markets.
    #[error("book market {book_market} does not match intent market {intent_market}")]
    MarketMismatch { book_market: String, intent_market: String },

    /// Input or configuration is invalid.
    #[error("invalid fill input: {0}")]
    InvalidInput(String),

    /// Fixed-point arithmetic exceeded Decimal's representable range.
    #[error("fill arithmetic overflow while computing {0}")]
    ArithmeticOverflow(&'static str),
}

/// Walk the order book to fill an order intent.
///
/// # Algorithm
///
/// 1. Determine the relevant side of the book (bids for Buy, asks for Sell)
/// 2. Sort levels best-first (bids descending, asks ascending)
/// 3. Walk levels, consuming size at each level's price
/// 4. If size remains after visible levels, apply depth-exhaustion extrapolation
/// 5. For limit orders, check limit price against each fill price
/// 6. Return one or more Fill structs
///
/// # Determinism guarantee
///
/// This function is pure: the fill timestamp is the supplied book snapshot's
/// timestamp, so identical inputs produce byte-for-byte identical fills.
pub fn walk_book(
    book: &OrderBook,
    intent: &OrderIntent,
    config: &FillConfig,
) -> Result<Vec<Fill>, FillError> {
    config.validate().map_err(|message| FillError::InvalidInput(message.to_owned()))?;
    if book.market != intent.market {
        return Err(FillError::MarketMismatch {
            book_market: book.market.as_str().to_owned(),
            intent_market: intent.market.as_str().to_owned(),
        });
    }
    if intent.size <= Decimal::ZERO {
        return Err(FillError::InvalidInput("intent size must be positive".to_owned()));
    }

    let levels: &[BookLevel] = match intent.side {
        Side::Buy | Side::BuyNo => book.asks(),
        Side::Sell | Side::SellNo => book.bids(),
    };

    if levels.is_empty() {
        return Err(FillError::NoLiquidity {
            market: intent.market.as_str().to_string(),
            side: intent.side,
        });
    }

    let mut fills = Vec::new();
    let mut remaining = intent.size;
    let mut first_limit_violation = None;

    for level in levels {
        if remaining <= Decimal::ZERO {
            break;
        }
        if config.aggressiveness == Aggressiveness::PassiveAtTouch && !fills.is_empty() {
            break;
        }

        if level.price <= Decimal::ZERO || level.size <= Decimal::ZERO {
            return Err(FillError::InvalidInput(
                "book levels must have positive price and size".to_owned(),
            ));
        }

        // Limit orders stop at the first non-marketable level. Previously the
        // model discarded valid earlier fills by returning an error here.
        if intent.order_type == OrderType::Limit {
            let Some(limit) = intent.limit_price else {
                return Err(FillError::InvalidInput("limit order requires limit_price".to_owned()));
            };
            let violates = match intent.side {
                Side::Buy => level.price > limit,
                Side::Sell => level.price < limit,
                Side::BuyNo => level.price > limit,
                Side::SellNo => level.price < limit,
            };
            if violates {
                first_limit_violation = Some((limit, level.price));
                break;
            }
        }

        let fill_size = remaining.min(level.size);
        if fill_size <= Decimal::ZERO {
            continue;
        }

        let fill_price = level.price;
        let notional = fill_price
            .checked_mul(fill_size)
            .ok_or(FillError::ArithmeticOverflow("visible fill notional"))?;
        let fee_amount = config
            .fee_rate
            .checked_mul(notional)
            .ok_or(FillError::ArithmeticOverflow("visible fill fee"))?;

        let venue_json = serde_json::json!({
            "market": book.market.as_str(),
            "book_ts": book.ts.to_string(),
        });

        fills.push(Fill {
            order_id: intent.id,
            market: intent.market.clone(),
            side: intent.side,
            price: fill_price,
            size: fill_size,
            fee: Money::new(fee_amount, &config.fee_currency),
            venue_ref: JsonObject::new(venue_json).unwrap_or_default(),
            ts: book.ts,
            paper: intent.paper,
        });

        remaining = remaining
            .checked_sub(fill_size)
            .ok_or(FillError::ArithmeticOverflow("remaining size"))?;
    }

    // Depth exhaustion: extrapolate remaining size at worst visible level
    if remaining > Decimal::ZERO
        && first_limit_violation.is_none()
        && config.aggressiveness == Aggressiveness::CrossToDepth
    {
        let worst_price = levels.last().map(|level| level.price);

        if let Some(base_price) = worst_price {
            // Pessimism must move buys upward and sells downward. Multiplying
            // both sides made exhausted sell fills artificially profitable.
            let exhaustion_price = match intent.side {
                Side::Buy | Side::BuyNo => base_price
                    .checked_mul(config.depth_exhaustion_multiplier)
                    .ok_or(FillError::ArithmeticOverflow("buy exhaustion price"))?,
                Side::Sell | Side::SellNo => base_price
                    .checked_div(config.depth_exhaustion_multiplier)
                    .ok_or(FillError::ArithmeticOverflow("sell exhaustion price"))?,
            };

            // Check limit against exhaustion price
            if intent.order_type == OrderType::Limit {
                let limit = intent.limit_price.ok_or_else(|| {
                    FillError::InvalidInput("limit order requires limit_price".to_owned())
                })?;
                let violates = match intent.side {
                    Side::Buy | Side::BuyNo => exhaustion_price > limit,
                    Side::Sell | Side::SellNo => exhaustion_price < limit,
                };
                if violates {
                    first_limit_violation = Some((limit, exhaustion_price));
                }
            }

            if first_limit_violation.is_none() {
                let notional = exhaustion_price
                    .checked_mul(remaining)
                    .ok_or(FillError::ArithmeticOverflow("exhausted fill notional"))?;
                let fee_amount = config
                    .fee_rate
                    .checked_mul(notional)
                    .ok_or(FillError::ArithmeticOverflow("exhausted fill fee"))?;
                let venue_json = serde_json::json!({
                    "market": book.market.as_str(),
                    "book_ts": book.ts.to_string(),
                    "depth_exhausted": true,
                    "exhaustion_multiplier": config.depth_exhaustion_multiplier.to_string(),
                });

                fills.push(Fill {
                    order_id: intent.id,
                    market: intent.market.clone(),
                    side: intent.side,
                    price: exhaustion_price,
                    size: remaining,
                    fee: Money::new(fee_amount, &config.fee_currency),
                    venue_ref: JsonObject::new(venue_json).unwrap_or_default(),
                    ts: book.ts,
                    paper: intent.paper,
                });
            }
        }
    }

    if fills.is_empty() {
        if let Some((limit, fill_price)) = first_limit_violation {
            return Err(FillError::LimitViolation { limit, fill_price });
        }
        return Err(FillError::InsufficientDepth {
            market: intent.market.as_str().to_owned(),
            filled: Decimal::ZERO,
            requested: intent.size,
        });
    }

    Ok(fills)
}

/// Contract-name alias used by both EP-304 and the EP-307 simulator.
pub fn walk(
    book: &OrderBook,
    intent: &OrderIntent,
    config: &FillConfig,
) -> Result<Vec<Fill>, FillError> {
    walk_book(book, intent, config)
}

//! Quote and OrderBook types. SPEC-001 market data.

use crate::decimal::decimal_string;
use crate::ids::MarketKey;
use crate::time::UtcTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Source of a quote: stream (real-time), poll (periodic), or snapshot (on-demand).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteSource {
    Stream,
    Poll,
    Snapshot,
}

/// A market quote — bid/ask/mid/last with sizes and timestamp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub market: MarketKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid_size: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask_size: Option<Decimal>,
    pub ts: UtcTime,
    pub source: QuoteSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

/// A single level in an order book: price + size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookLevel {
    #[serde(with = "decimal_string")]
    pub price: Decimal,
    #[serde(with = "decimal_string")]
    pub size: Decimal,
}

/// An order book snapshot. Bids descending, asks ascending — enforced by constructor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderBook {
    pub market: MarketKey,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
    pub depth: usize,
    pub ts: UtcTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

/// Validation errors for OrderBook construction.
#[derive(Debug, thiserror::Error)]
pub enum OrderBookError {
    #[error("bids must be in descending order (by price)")]
    BidsNotDescending,
    #[error("asks must be in ascending order (by price)")]
    AsksNotAscending,
}

impl OrderBook {
    /// Create an OrderBook, enforcing bid/ask ordering invariants.
    /// Bids MUST be descending (highest price first).
    /// Asks MUST be ascending (lowest price first).
    pub fn new(
        market: MarketKey,
        bids: Vec<BookLevel>,
        asks: Vec<BookLevel>,
        depth: usize,
        ts: UtcTime,
        seq: Option<u64>,
    ) -> Result<Self, OrderBookError> {
        // Verify bids are strictly descending
        for w in bids.windows(2) {
            if w[0].price <= w[1].price {
                return Err(OrderBookError::BidsNotDescending);
            }
        }
        // Verify asks are strictly ascending
        for w in asks.windows(2) {
            if w[0].price >= w[1].price {
                return Err(OrderBookError::AsksNotAscending);
            }
        }
        Ok(Self { market, bids, asks, depth, ts, seq })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::VenueId;
    use rust_decimal::Decimal;

    fn mk_key() -> MarketKey {
        MarketKey::new(&VenueId::new("kalshi"), "TEST-1")
    }

    fn ts() -> UtcTime {
        UtcTime::from_unix_millis(1752152096789).unwrap()
    }

    #[test]
    fn order_book_valid_ordering() {
        let bids = vec![
            BookLevel { price: Decimal::new(995, 2), size: Decimal::new(10, 0) },
            BookLevel { price: Decimal::new(990, 2), size: Decimal::new(5, 0) },
        ];
        let asks = vec![
            BookLevel { price: Decimal::new(1000, 2), size: Decimal::new(10, 0) },
            BookLevel { price: Decimal::new(1005, 2), size: Decimal::new(5, 0) },
        ];
        let ob = OrderBook::new(mk_key(), bids, asks, 2, ts(), None);
        assert!(ob.is_ok());
    }

    #[test]
    fn order_book_rejects_non_descending_bids() {
        let bids = vec![
            BookLevel { price: Decimal::new(990, 2), size: Decimal::new(10, 0) },
            BookLevel { price: Decimal::new(995, 2), size: Decimal::new(5, 0) }, // higher after lower
        ];
        let ob = OrderBook::new(mk_key(), bids, vec![], 1, ts(), None);
        assert!(matches!(ob, Err(OrderBookError::BidsNotDescending)));
    }

    #[test]
    fn order_book_rejects_non_ascending_asks() {
        let asks = vec![
            BookLevel { price: Decimal::new(1005, 2), size: Decimal::new(10, 0) },
            BookLevel { price: Decimal::new(1000, 2), size: Decimal::new(5, 0) }, // lower after higher
        ];
        let ob = OrderBook::new(mk_key(), vec![], asks, 1, ts(), None);
        assert!(matches!(ob, Err(OrderBookError::AsksNotAscending)));
    }

    #[test]
    fn quote_serde_round_trip() {
        let q = Quote {
            market: mk_key(),
            bid: Some(Decimal::new(99, 2)),
            ask: Some(Decimal::new(101, 2)),
            mid: Some(Decimal::new(100, 2)),
            last: None,
            bid_size: Some(Decimal::new(100, 0)),
            ask_size: Some(Decimal::new(200, 0)),
            ts: ts(),
            source: QuoteSource::Stream,
            seq: Some(42),
        };
        let json = serde_json::to_string(&q).unwrap();
        let q2: Quote = serde_json::from_str(&json).unwrap();
        assert_eq!(q, q2);
    }
}

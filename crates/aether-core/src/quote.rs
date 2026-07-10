//! Quote and OrderBook types. SPEC-001 market data.

use crate::decimal::{decimal_option_string, decimal_string};
use crate::ids::MarketKey;
use crate::time::UtcTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteSource {
    Stream,
    Poll,
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub market: MarketKey,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "decimal_option_string")]
    pub bid: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "decimal_option_string")]
    pub ask: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "decimal_option_string")]
    pub mid: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "decimal_option_string")]
    pub last: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "decimal_option_string")]
    pub bid_size: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "decimal_option_string")]
    pub ask_size: Option<Decimal>,
    pub ts: UtcTime,
    pub source: QuoteSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

// ── BookLevel ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookLevel {
    #[serde(with = "decimal_string")]
    pub price: Decimal,
    #[serde(with = "decimal_string")]
    pub size: Decimal,
}

// ── OrderBook ──────────────────────────────────────────────────────

/// An order book snapshot. Bids descending, asks ascending — enforced on deserialize.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderBook {
    pub market: MarketKey,
    bids: Vec<BookLevel>,
    asks: Vec<BookLevel>,
    pub depth: usize,
    pub ts: UtcTime,
    pub seq: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum OrderBookError {
    #[error("bids must be in descending order (by price)")]
    BidsNotDescending,
    #[error("asks must be in ascending order (by price)")]
    AsksNotAscending,
}

impl OrderBook {
    pub fn new(
        market: MarketKey,
        bids: Vec<BookLevel>,
        asks: Vec<BookLevel>,
        depth: usize,
        ts: UtcTime,
        seq: Option<u64>,
    ) -> Result<Self, OrderBookError> {
        for w in bids.windows(2) {
            if w[0].price <= w[1].price {
                return Err(OrderBookError::BidsNotDescending);
            }
        }
        for w in asks.windows(2) {
            if w[0].price >= w[1].price {
                return Err(OrderBookError::AsksNotAscending);
            }
        }
        Ok(Self { market, bids, asks, depth, ts, seq })
    }

    pub fn bids(&self) -> &[BookLevel] {
        &self.bids
    }

    pub fn asks(&self) -> &[BookLevel] {
        &self.asks
    }
}

// Custom serialize/deserialize through validated constructor

#[derive(Serialize, Deserialize)]
struct OrderBookWire {
    market: MarketKey,
    bids: Vec<BookLevel>,
    asks: Vec<BookLevel>,
    depth: usize,
    ts: UtcTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    seq: Option<u64>,
}

impl Serialize for OrderBook {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = OrderBookWire {
            market: self.market.clone(),
            bids: self.bids.clone(),
            asks: self.asks.clone(),
            depth: self.depth,
            ts: self.ts,
            seq: self.seq,
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OrderBook {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = OrderBookWire::deserialize(deserializer)?;
        OrderBook::new(wire.market, wire.bids, wire.asks, wire.depth, wire.ts, wire.seq)
            .map_err(serde::de::Error::custom)
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
        assert!(OrderBook::new(mk_key(), bids, asks, 2, ts(), None).is_ok());
    }

    #[test]
    fn order_book_rejects_non_descending_bids() {
        let bids = vec![
            BookLevel { price: Decimal::new(990, 2), size: Decimal::new(10, 0) },
            BookLevel { price: Decimal::new(995, 2), size: Decimal::new(5, 0) },
        ];
        assert!(matches!(
            OrderBook::new(mk_key(), bids, vec![], 1, ts(), None),
            Err(OrderBookError::BidsNotDescending)
        ));
    }

    #[test]
    fn order_book_rejects_non_ascending_asks() {
        let asks = vec![
            BookLevel { price: Decimal::new(1005, 2), size: Decimal::new(10, 0) },
            BookLevel { price: Decimal::new(1000, 2), size: Decimal::new(5, 0) },
        ];
        assert!(matches!(
            OrderBook::new(mk_key(), vec![], asks, 1, ts(), None),
            Err(OrderBookError::AsksNotAscending)
        ));
    }

    #[test]
    fn order_book_deserialize_rejects_bad_ordering() {
        let json = r#"{"market":"mkt:kalshi:TEST-1","bids":[{"price":"9.90","size":"10"},{"price":"9.95","size":"5"}],"asks":[],"depth":1,"ts":"2026-07-10T12:34:56.789Z"}"#;
        let result: Result<OrderBook, _> = serde_json::from_str(json);
        assert!(result.is_err(), "deserializing non-descending bids should fail");
    }

    #[test]
    fn order_book_deserialize_rejects_bad_asks() {
        let json = r#"{"market":"mkt:kalshi:TEST-1","bids":[],"asks":[{"price":"10.05","size":"10"},{"price":"10.00","size":"5"}],"depth":1,"ts":"2026-07-10T12:34:56.789Z"}"#;
        let result: Result<OrderBook, _> = serde_json::from_str(json);
        assert!(result.is_err(), "deserializing non-ascending asks should fail");
    }

    #[test]
    fn quote_rejects_numeric_decimal() {
        let json = r#"{"market":"mkt:kalshi:TEST-1","bid":0.65,"ts":"2026-07-10T12:34:56.789Z","source":"stream"}"#;
        let result: Result<Quote, _> = serde_json::from_str(json);
        assert!(result.is_err(), "numeric bid should be rejected");
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

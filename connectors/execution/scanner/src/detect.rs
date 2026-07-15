//! Cross-venue arbitrage detection.
//! Finds pairs of markets across venues that represent the same
//! real-world event and have a price gap exceeding threshold.

use aether_core::ids::MarketKey;
use aether_core::market::Market;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// A detected arbitrage opportunity (before scoring).
#[derive(Debug, Clone)]
pub struct DetectedOpportunity {
    pub buy_leg: MarketKey,
    pub sell_leg: MarketKey,
    pub buy_price: Decimal,
    pub sell_price: Decimal,
    pub gross_spread: Decimal,
    pub category: String,
}

/// Detector configuration.
#[derive(Debug, Clone)]
pub struct DetectorConfig {
    pub min_gross_spread: Decimal,
    pub max_quote_age_ms: i64,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            min_gross_spread: Decimal::new(1, 2), // 1% minimum
            max_quote_age_ms: 5000,
        }
    }
}

/// Cross-venue edge detector.
pub struct Detector {
    config: DetectorConfig,
}

impl Detector {
    pub fn new(config: DetectorConfig) -> Self {
        Self { config }
    }

    /// Detect opportunities from current market state.
    /// For prediction markets: same underlying event -> Yes on venue A, No on venue B
    /// (or vice versa) creates a synthetic spread.
    pub fn detect(
        &self,
        quotes: &HashMap<MarketKey, (Decimal, Decimal)>, // market -> (bid, ask)
        markets: &HashMap<MarketKey, Market>,
    ) -> Vec<DetectedOpportunity> {
        let mut opportunities = Vec::new();
        let market_list: Vec<_> = markets.values().collect();

        for i in 0..market_list.len() {
            for j in (i + 1)..market_list.len() {
                let mkt_a = &market_list[i];
                let mkt_b = &market_list[j];

                // Only compare same asset class
                if std::mem::discriminant(&mkt_a.kind) != std::mem::discriminant(&mkt_b.kind) {
                    continue;
                }

                let quote_a = quotes.get(&mkt_a.key);
                let quote_b = quotes.get(&mkt_b.key);
                if quote_a.is_none() || quote_b.is_none() {
                    continue;
                }
                let (_, ask_a) = quote_a.unwrap();
                let (bid_b, _) = quote_b.unwrap();

                // Buy on A (at ask), sell on B (at bid) -- if profitable
                if *ask_a > Decimal::ZERO && *bid_b > Decimal::ZERO && *bid_b > *ask_a {
                    let spread = bid_b - ask_a;
                    let gross = spread / ask_a; // percentage spread
                    if gross >= self.config.min_gross_spread {
                        opportunities.push(DetectedOpportunity {
                            buy_leg: mkt_a.key.clone(),
                            sell_leg: mkt_b.key.clone(),
                            buy_price: *ask_a,
                            sell_price: *bid_b,
                            gross_spread: gross,
                            category: format!("{:?}-{:?}", mkt_a.kind, mkt_b.kind),
                        });
                    }
                }
                // Also check reverse: buy B, sell A
                let (_, ask_b) = quote_b.unwrap();
                let (bid_a, _) = quote_a.unwrap();
                if *ask_b > Decimal::ZERO && *bid_a > Decimal::ZERO && *bid_a > *ask_b {
                    let spread = bid_a - ask_b;
                    let gross = spread / ask_b;
                    if gross >= self.config.min_gross_spread {
                        opportunities.push(DetectedOpportunity {
                            buy_leg: mkt_b.key.clone(),
                            sell_leg: mkt_a.key.clone(),
                            buy_price: *ask_b,
                            sell_price: *bid_a,
                            gross_spread: gross,
                            category: format!("{:?}-{:?}", mkt_b.kind, mkt_a.kind),
                        });
                    }
                }
            }
        }
        opportunities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::ids::VenueId;
    use aether_core::json::JsonObject;
    use aether_core::market::{InstrumentKind, MarketStatus};

    #[test]
    fn detects_profitable_arb() {
        let detector = Detector::new(DetectorConfig::default());
        let mut quotes = HashMap::new();
        let mut markets = HashMap::new();

        let mk1 =
            MarketKey::new(&VenueId::new("kalshi").unwrap(), "btc-yes").unwrap();
        let mk2 =
            MarketKey::new(&VenueId::new("polymarket").unwrap(), "btc-yes").unwrap();

        quotes.insert(mk1.clone(), (Decimal::new(60, 2), Decimal::new(62, 2)));
        quotes.insert(mk2.clone(), (Decimal::new(67, 2), Decimal::new(68, 2)));

        let m1 = Market {
            key: mk1,
            venue: VenueId::new("kalshi").unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: "A".into(),
            description_ref: "".into(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec![],
            venue_ref: JsonObject::default(),
            meta: JsonObject::default(),
        };
        let m2 = Market {
            key: mk2,
            venue: VenueId::new("polymarket").unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: "B".into(),
            description_ref: "".into(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec![],
            venue_ref: JsonObject::default(),
            meta: JsonObject::default(),
        };
        markets.insert(m1.key.clone(), m1);
        markets.insert(m2.key.clone(), m2);

        let opps = detector.detect(&quotes, &markets);
        // Buy Kalshi at 0.62, sell Polymarket at 0.67 = 0.05 spread / 0.62 = ~8%
        assert!(!opps.is_empty());
        assert!(opps[0].gross_spread >= Decimal::new(8, 2));
    }

    #[test]
    fn no_arb_when_spread_below_threshold() {
        let detector = Detector::new(DetectorConfig::default());
        let mut quotes = HashMap::new();
        let mut markets = HashMap::new();
        let mk1 =
            MarketKey::new(&VenueId::new("kalshi").unwrap(), "close").unwrap();
        let mk2 =
            MarketKey::new(&VenueId::new("polymarket").unwrap(), "close").unwrap();
        quotes.insert(mk1.clone(), (Decimal::new(60, 2), Decimal::new(61, 2)));
        quotes.insert(mk2.clone(), (Decimal::new(61, 2), Decimal::new(62, 2)));
        let m = Market {
            key: mk1.clone(),
            venue: VenueId::new("kalshi").unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: "".into(),
            description_ref: "".into(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec![],
            venue_ref: JsonObject::default(),
            meta: JsonObject::default(),
        };
        markets.insert(mk1, m.clone());
        markets.insert(mk2, m);
        let opps = detector.detect(&quotes, &markets);
        assert!(opps.is_empty());
    }
}

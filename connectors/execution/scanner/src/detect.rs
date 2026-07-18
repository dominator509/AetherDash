//! Cross-venue arbitrage detection.
//! Finds pairs of markets across venues that represent the same
//! real-world event and have a price gap exceeding threshold.

use aether_core::ids::MarketKey;
use aether_core::market::Market;
use aether_core::market::MarketStatus;
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::collections::HashSet;

/// A top-of-book snapshot with the receive time required for the scanner's
/// cheap freshness gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarketQuote {
    pub bid: Decimal,
    pub ask: Decimal,
    pub ts: UtcTime,
}

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
        quotes: &HashMap<MarketKey, MarketQuote>,
        markets: &HashMap<MarketKey, Market>,
        now: UtcTime,
    ) -> Vec<DetectedOpportunity> {
        self.detect_bounded(quotes, markets, now, usize::MAX).0
    }

    /// Detect opportunities while evaluating at most `max_pairs` market pairs.
    /// Returns the opportunities and the exact number of pairs evaluated.
    pub fn detect_bounded(
        &self,
        quotes: &HashMap<MarketKey, MarketQuote>,
        markets: &HashMap<MarketKey, Market>,
        now: UtcTime,
        max_pairs: usize,
    ) -> (Vec<DetectedOpportunity>, usize) {
        self.detect_bounded_inner(quotes, markets, now, max_pairs, None)
    }

    /// Re-evaluate only pairs touched by changed market-data keys.
    pub fn detect_changed_bounded(
        &self,
        quotes: &HashMap<MarketKey, MarketQuote>,
        markets: &HashMap<MarketKey, Market>,
        changed: &HashSet<MarketKey>,
        now: UtcTime,
        max_pairs: usize,
    ) -> (Vec<DetectedOpportunity>, usize) {
        self.detect_bounded_inner(quotes, markets, now, max_pairs, Some(changed))
    }

    fn detect_bounded_inner(
        &self,
        quotes: &HashMap<MarketKey, MarketQuote>,
        markets: &HashMap<MarketKey, Market>,
        now: UtcTime,
        max_pairs: usize,
        changed: Option<&HashSet<MarketKey>>,
    ) -> (Vec<DetectedOpportunity>, usize) {
        let mut opportunities = Vec::new();
        let mut market_list: Vec<_> = markets.values().collect();
        market_list.sort_by(|left, right| left.key.as_str().cmp(right.key.as_str()));
        let mut pairs_evaluated = 0;

        'pairs: for i in 0..market_list.len() {
            for j in (i + 1)..market_list.len() {
                let mkt_a = &market_list[i];
                let mkt_b = &market_list[j];

                if changed
                    .is_some_and(|keys| !keys.contains(&mkt_a.key) && !keys.contains(&mkt_b.key))
                {
                    continue;
                }
                if pairs_evaluated >= max_pairs {
                    break 'pairs;
                }
                pairs_evaluated += 1;

                // Cheap filters first: an arbitrage needs distinct venues,
                // live markets, the same instrument kind, and a defensible
                // canonical event match.
                if mkt_a.venue == mkt_b.venue
                    || mkt_a.status != MarketStatus::Open
                    || mkt_b.status != MarketStatus::Open
                    || mkt_a.kind != mkt_b.kind
                    || canonical_event_key(mkt_a) != canonical_event_key(mkt_b)
                {
                    continue;
                }

                let quote_a = quotes.get(&mkt_a.key);
                let quote_b = quotes.get(&mkt_b.key);
                if quote_a.is_none() || quote_b.is_none() {
                    continue;
                }
                let (Some(quote_a), Some(quote_b)) = (quote_a, quote_b) else {
                    continue;
                };
                if quote_is_stale(quote_a, now, self.config.max_quote_age_ms)
                    || quote_is_stale(quote_b, now, self.config.max_quote_age_ms)
                {
                    continue;
                }
                let ask_a = quote_a.ask;
                let bid_b = quote_b.bid;

                // Buy on A (at ask), sell on B (at bid) -- if profitable
                if ask_a > Decimal::ZERO && bid_b > Decimal::ZERO && bid_b > ask_a {
                    let spread = bid_b - ask_a;
                    let gross = spread / ask_a; // percentage spread
                    if gross >= self.config.min_gross_spread {
                        opportunities.push(DetectedOpportunity {
                            buy_leg: mkt_a.key.clone(),
                            sell_leg: mkt_b.key.clone(),
                            buy_price: ask_a,
                            sell_price: bid_b,
                            gross_spread: gross,
                            category: canonical_event_key(mkt_a),
                        });
                    }
                }
                // Also check reverse: buy B, sell A
                let ask_b = quote_b.ask;
                let bid_a = quote_a.bid;
                if ask_b > Decimal::ZERO && bid_a > Decimal::ZERO && bid_a > ask_b {
                    let spread = bid_a - ask_b;
                    let gross = spread / ask_b;
                    if gross >= self.config.min_gross_spread {
                        opportunities.push(DetectedOpportunity {
                            buy_leg: mkt_b.key.clone(),
                            sell_leg: mkt_a.key.clone(),
                            buy_price: ask_b,
                            sell_price: bid_a,
                            gross_spread: gross,
                            category: canonical_event_key(mkt_b),
                        });
                    }
                }
            }
        }
        (opportunities, pairs_evaluated)
    }
}

fn quote_is_stale(quote: &MarketQuote, now: UtcTime, max_age_ms: i64) -> bool {
    max_age_ms < 0 || now.unix_millis().saturating_sub(quote.ts.unix_millis()) > max_age_ms
}

fn canonical_event_key(market: &Market) -> String {
    for source in [&market.meta, &market.venue_ref] {
        if let Some(value) =
            source.as_value().get("canonical_event_id").and_then(serde_json::Value::as_str)
        {
            let normalized = value.trim().to_ascii_lowercase();
            if !normalized.is_empty() {
                return format!("id:{normalized}");
            }
        }
    }

    let normalized_title: String = market
        .title
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    format!("title:{normalized_title}")
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

        let mk1 = MarketKey::new(&VenueId::new("kalshi").unwrap(), "btc-yes").unwrap();
        let mk2 = MarketKey::new(&VenueId::new("polymarket").unwrap(), "btc-yes").unwrap();

        let now = UtcTime::from_unix_millis(1_000_000).unwrap();
        quotes.insert(
            mk1.clone(),
            MarketQuote { bid: Decimal::new(60, 2), ask: Decimal::new(62, 2), ts: now },
        );
        quotes.insert(
            mk2.clone(),
            MarketQuote { bid: Decimal::new(67, 2), ask: Decimal::new(68, 2), ts: now },
        );

        let m1 = Market {
            key: mk1,
            venue: VenueId::new("kalshi").unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: "BTC above 100k".into(),
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
            title: "BTC above $100k?".into(),
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

        let opps = detector.detect(&quotes, &markets, now);
        // Buy Kalshi at 0.62, sell Polymarket at 0.67 = 0.05 spread / 0.62 = ~8%
        assert!(!opps.is_empty());
        assert!(opps[0].gross_spread >= Decimal::new(8, 2));
    }

    #[test]
    fn no_arb_when_spread_below_threshold() {
        let detector = Detector::new(DetectorConfig::default());
        let mut quotes = HashMap::new();
        let mut markets = HashMap::new();
        let mk1 = MarketKey::new(&VenueId::new("kalshi").unwrap(), "close").unwrap();
        let mk2 = MarketKey::new(&VenueId::new("polymarket").unwrap(), "close").unwrap();
        let now = UtcTime::from_unix_millis(1_000_000).unwrap();
        quotes.insert(
            mk1.clone(),
            MarketQuote { bid: Decimal::new(60, 2), ask: Decimal::new(61, 2), ts: now },
        );
        quotes.insert(
            mk2.clone(),
            MarketQuote { bid: Decimal::new(61, 2), ask: Decimal::new(62, 2), ts: now },
        );
        let m1 = Market {
            key: mk1.clone(),
            venue: VenueId::new("kalshi").unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: "same event".into(),
            description_ref: "".into(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec![],
            venue_ref: JsonObject::default(),
            meta: JsonObject::default(),
        };
        let m2 =
            Market { key: mk2.clone(), venue: VenueId::new("polymarket").unwrap(), ..m1.clone() };
        markets.insert(mk1, m1);
        markets.insert(mk2, m2);
        let opps = detector.detect(&quotes, &markets, now);
        assert!(opps.is_empty());
    }

    #[test]
    fn rejects_different_events_and_stale_quotes() {
        let now = UtcTime::from_unix_millis(10_000).unwrap();
        let stale = UtcTime::from_unix_millis(1_000).unwrap();
        let mk1 = MarketKey::new(&VenueId::new("kalshi").unwrap(), "a").unwrap();
        let mk2 = MarketKey::new(&VenueId::new("polymarket").unwrap(), "b").unwrap();
        let make_market = |key: MarketKey, venue: &str, title: &str| Market {
            key,
            venue: VenueId::new(venue).unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: title.into(),
            description_ref: String::new(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec![],
            venue_ref: JsonObject::default(),
            meta: JsonObject::default(),
        };
        let mut markets = HashMap::new();
        markets.insert(mk1.clone(), make_market(mk1.clone(), "kalshi", "event a"));
        markets.insert(mk2.clone(), make_market(mk2.clone(), "polymarket", "event b"));
        let mut quotes = HashMap::new();
        quotes.insert(
            mk1.clone(),
            MarketQuote { bid: Decimal::new(50, 2), ask: Decimal::new(51, 2), ts: now },
        );
        quotes.insert(
            mk2.clone(),
            MarketQuote { bid: Decimal::new(70, 2), ask: Decimal::new(71, 2), ts: now },
        );
        let detector = Detector::new(DetectorConfig::default());
        assert!(detector.detect(&quotes, &markets, now).is_empty());

        markets.get_mut(&mk2).unwrap().title = "event a".into();
        quotes.get_mut(&mk2).unwrap().ts = stale;
        assert!(detector.detect(&quotes, &markets, now).is_empty());
    }
}

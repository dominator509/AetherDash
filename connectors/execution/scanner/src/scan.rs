//! Scanner types and scan orchestration.
//!
use crate::{
    CadenceController, Detector, DetectorConfig, EvidenceSnapshot, LifecycleError, LifecycleStore,
    MarketQuote, PersistDisposition, Scorer,
};
use aether_bus::envelope::Envelope;
use aether_bus::producer::{MessageProducer, ProducerError};
use aether_bus::topics::Topic;
use aether_core::decimal::Confidence;
use aether_core::ids::{MarketKey, Ulid};
use aether_core::market::Market;
use aether_core::opportunity::{
    BrainRef, EdgeCosts, EdgeDecomposition, Opportunity, OpportunityKind, OpportunityLeg,
};
use aether_core::order::Side;
use aether_core::quote::OrderBook;
use aether_core::time::UtcTime;
use aether_decompose::decompose::{decompose, DecompositionContext};
use aether_decompose::fees::{FeeCatalog, FeeError};
use aether_decompose::mismatch::{MismatchConfig, MismatchLoadError};
use aether_fillmodel::config::FillConfig;
use aether_observe::metrics::MetricRegistry;
use aether_simulator::walk_leg;
use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

/// Configuration for the opportunity scanner.
pub struct ScanConfig {
    /// Maximum number of venue pairs to evaluate per cycle.
    pub max_pairs_per_cycle: usize,
    /// Minimum net edge (as a fraction) to emit an opportunity.
    pub min_net_edge: Decimal,
    /// Base decomposition context applied to all scans.
    pub base_context: DecompositionContext,
    pub detector: DetectorConfig,
    pub target_cycle_ms: u64,
    pub fill_config: FillConfig,
}

impl Default for ScanConfig {
    fn default() -> Self {
        // Scanner edges are expressed per normalized unit. Request-time
        // simulation scales this same math to the requested size.
        let base_context = DecompositionContext { notional: Decimal::ONE, ..Default::default() };
        Self {
            max_pairs_per_cycle: 100,
            min_net_edge: Decimal::new(1, 3), // 0.001 = 10bps minimum
            base_context,
            detector: DetectorConfig::default(),
            target_cycle_ms: 500,
            fill_config: FillConfig::default(),
        }
    }
}

impl std::fmt::Debug for ScanConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanConfig")
            .field("max_pairs_per_cycle", &self.max_pairs_per_cycle)
            .field("min_net_edge", &self.min_net_edge)
            .field("base_context", &format_args!("DecompositionContext(...)"))
            .finish()
    }
}

/// Outcome of a single scan cycle.
#[derive(Debug, Clone)]
pub struct ScanOutcome {
    /// Number of venue pairs evaluated.
    pub pairs_evaluated: usize,
    /// Number of opportunities found above the minimum edge.
    pub opportunities: Vec<Opportunity>,
    pub cycle_ms: u64,
    pub shed_level: usize,
    /// Candidates rejected because a current book was missing or unfillable.
    pub fill_failures: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurablePublishReport {
    pub created: usize,
    pub updated: usize,
    pub existing: usize,
    pub published: usize,
}

/// Errors from the scanner.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("no quote data available for market pair")]
    NoQuoteData,
    #[error("decomposition error: {0}")]
    Decompose(String),
    #[error("internal scanner error: {0}")]
    Internal(String),
    #[error("mismatch configuration error: {0}")]
    Mismatch(#[from] MismatchLoadError),
    #[error("invalid confidence produced by scorer: {0}")]
    InvalidConfidence(String),
    #[error("fee configuration error: {0}")]
    Fee(#[from] FeeError),
}

/// The opportunity scanner.
///
pub struct Scanner {
    config: ScanConfig,
    detector: Detector,
    scorer: Scorer,
    mismatch: MismatchConfig,
    fees: FeeCatalog,
    cadence: CadenceController,
    metrics: Option<Arc<MetricRegistry>>,
    fill_failures_total: u64,
}

impl Scanner {
    /// Create a new scanner with the given configuration.
    pub fn new(config: ScanConfig) -> Result<Self, ScanError> {
        let detector = Detector::new(config.detector.clone());
        let cadence = CadenceController::new(config.target_cycle_ms);
        Ok(Self {
            config,
            detector,
            scorer: Scorer::new(),
            mismatch: MismatchConfig::load_embedded()?,
            fees: FeeCatalog::load_embedded()?,
            cadence,
            metrics: None,
            fill_failures_total: 0,
        })
    }

    pub fn with_metrics(
        config: ScanConfig,
        metrics: Arc<MetricRegistry>,
    ) -> Result<Self, ScanError> {
        let mut scanner = Self::new(config)?;
        scanner.metrics = Some(metrics);
        Ok(scanner)
    }

    /// Access the scanner configuration.
    pub fn config(&self) -> &ScanConfig {
        &self.config
    }

    pub fn cadence_mut(&mut self) -> &mut CadenceController {
        &mut self.cadence
    }

    /// Run one deterministic scan cycle over a point-in-time market snapshot.
    pub fn scan_once(
        &mut self,
        quotes: &HashMap<MarketKey, MarketQuote>,
        books: &HashMap<MarketKey, OrderBook>,
        markets: &HashMap<MarketKey, Market>,
        evidence: &HashMap<String, EvidenceSnapshot>,
        now: UtcTime,
        explain_ref: &BrainRef,
        trace_id: Ulid,
    ) -> Result<ScanOutcome, ScanError> {
        self.scan_impl(quotes, books, markets, evidence, None, now, explain_ref, trace_id)
    }

    /// Incremental production cycle: only pairs touched by changed market
    /// data are reconsidered, while scoring and decomposition remain identical.
    #[allow(clippy::too_many_arguments)]
    pub fn scan_incremental(
        &mut self,
        quotes: &HashMap<MarketKey, MarketQuote>,
        books: &HashMap<MarketKey, OrderBook>,
        markets: &HashMap<MarketKey, Market>,
        evidence: &HashMap<String, EvidenceSnapshot>,
        changed: &HashSet<MarketKey>,
        now: UtcTime,
        explain_ref: &BrainRef,
        trace_id: Ulid,
    ) -> Result<ScanOutcome, ScanError> {
        self.scan_impl(quotes, books, markets, evidence, Some(changed), now, explain_ref, trace_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_impl(
        &mut self,
        quotes: &HashMap<MarketKey, MarketQuote>,
        books: &HashMap<MarketKey, OrderBook>,
        markets: &HashMap<MarketKey, Market>,
        evidence: &HashMap<String, EvidenceSnapshot>,
        changed: Option<&HashSet<MarketKey>>,
        now: UtcTime,
        explain_ref: &BrainRef,
        trace_id: Ulid,
    ) -> Result<ScanOutcome, ScanError> {
        let started = Instant::now();
        let shed_level = self.cadence.begin_cycle();
        let (detected, pairs_evaluated) = match changed {
            Some(keys) => self.detector.detect_changed_bounded(
                quotes,
                markets,
                keys,
                now,
                self.config.max_pairs_per_cycle,
            ),
            None => {
                self.detector.detect_bounded(quotes, markets, now, self.config.max_pairs_per_cycle)
            }
        };
        let mut opportunities = Vec::new();
        let mut fill_failures = 0;
        let candidate_limit = match shed_level {
            0 => detected.len(),
            1 => detected.len().saturating_mul(3).div_ceil(4),
            2 => detected.len().div_ceil(2),
            3 => detected.len().div_ceil(4),
            _ => detected.len().min(1),
        };

        for candidate in detected.into_iter().take(candidate_limit) {
            let quote_age_ms = [&candidate.buy_leg, &candidate.sell_leg]
                .iter()
                .filter_map(|key| quotes.get(*key))
                .map(|quote| now.unix_millis().saturating_sub(quote.ts.unix_millis()))
                .max()
                .unwrap_or_default();
            let scored = self.scorer.score(
                candidate.gross_spread,
                quote_age_ms,
                self.config.detector.max_quote_age_ms,
                2,
                evidence.get(&candidate.category).map(|snapshot| snapshot.signal),
            );
            if scored.confidence < self.scorer.min_confidence {
                continue;
            }

            let buy_market = markets
                .get(&candidate.buy_leg)
                .ok_or_else(|| ScanError::Internal("detected buy market disappeared".into()))?;
            let sell_market = markets
                .get(&candidate.sell_leg)
                .ok_or_else(|| ScanError::Internal("detected sell market disappeared".into()))?;
            if !self.fees.is_executable(buy_market.venue.as_str())?
                || !self.fees.is_executable(sell_market.venue.as_str())?
            {
                continue;
            }
            let (Some(buy_book), Some(sell_book)) =
                (books.get(&candidate.buy_leg), books.get(&candidate.sell_leg))
            else {
                fill_failures += 1;
                continue;
            };
            let Ok((_, buy_slippage)) = walk_leg(
                buy_book,
                Side::Buy,
                self.config.base_context.notional,
                &self.config.fill_config,
            ) else {
                fill_failures += 1;
                continue;
            };
            let Ok((_, sell_slippage)) = walk_leg(
                sell_book,
                Side::Sell,
                self.config.base_context.notional,
                &self.config.fill_config,
            ) else {
                fill_failures += 1;
                continue;
            };
            let mut context = self.config.base_context.clone();
            context.buy_price = candidate.buy_price;
            context.sell_price = candidate.sell_price;
            context.max_quote_age_ms = quote_age_ms;
            context.confidence = scored.confidence;
            context.fee_amount = Some(self.fees.estimate_pair(
                buy_market.venue.as_str(),
                sell_market.venue.as_str(),
                candidate.buy_price,
                candidate.sell_price,
                context.notional,
            )?);
            let buy_depth: Decimal = buy_book.asks().iter().map(|level| level.size).sum();
            let sell_depth: Decimal = sell_book.bids().iter().map(|level| level.size).sum();
            context.avg_depth = (buy_depth + sell_depth) / Decimal::new(2, 0);
            context.apply_mismatch_config(
                &self.mismatch,
                buy_market.venue.as_str(),
                sell_market.venue.as_str(),
            );
            let computed = decompose(&context).with_slippage(buy_slippage + sell_slippage);
            if computed.net_edge < self.config.min_net_edge {
                continue;
            }
            let edge = EdgeDecomposition::compute(
                computed.gross_spread,
                EdgeCosts {
                    fees: computed.fees,
                    slippage_est: computed.slippage_est,
                    funding_cost: computed.funding_cost,
                    gas_cost: computed.gas_cost,
                    bridge_cost: computed.bridge_cost,
                    settlement_mismatch_discount: computed.settlement_mismatch_discount,
                    liquidity_haircut: computed.liquidity_haircut,
                    staleness_penalty: computed.staleness_penalty,
                    confidence_penalty: computed.confidence_penalty,
                },
            );
            let confidence = Confidence::new(scored.confidence)
                .map_err(|error| ScanError::InvalidConfidence(error.to_string()))?;
            let expires_ms = now.unix_millis().saturating_add(30_000);
            let expires_ts = UtcTime::from_unix_millis(expires_ms)
                .map_err(|error| ScanError::Internal(error.to_string()))?;
            opportunities.push(Opportunity {
                id: deterministic_opportunity_id(&candidate, now),
                kind: OpportunityKind::Arbitrage,
                legs: vec![
                    OpportunityLeg {
                        market: candidate.buy_leg,
                        side: Side::Buy,
                        target_price: Some(candidate.buy_price),
                        size_hint: Some(context.notional),
                    },
                    OpportunityLeg {
                        market: candidate.sell_leg,
                        side: Side::Sell,
                        target_price: Some(candidate.sell_price),
                        size_hint: Some(context.notional),
                    },
                ],
                gross_edge: computed.gross_spread,
                edge,
                confidence,
                detected_ts: now,
                expires_ts: Some(expires_ts),
                explain_ref: evidence
                    .get(&candidate.category)
                    .map_or_else(|| explain_ref.clone(), |snapshot| snapshot.explain_ref.clone()),
                trace_id,
            });
        }

        self.cadence.end_cycle(started.elapsed());
        self.fill_failures_total = self
            .fill_failures_total
            .saturating_add(u64::try_from(fill_failures).unwrap_or(u64::MAX));
        if let Some(metrics) = &self.metrics {
            let cycle_ms = u32::try_from(self.cadence.cycle_metric()).unwrap_or(u32::MAX);
            metrics.observe_histogram(
                crate::SCAN_CYCLE_METRIC,
                "Scanner cycle duration",
                f64::from(cycle_ms),
                &[100.0, 250.0, 500.0, 1_000.0],
                HashMap::new(),
            );
            metrics.set_counter(
                "aether_scan_shed_total",
                "Scanner cycles with cost-aware shedding",
                self.cadence.cycles_shed,
                HashMap::new(),
            );
            metrics.set_counter(
                "aether_scan_fill_failures_total",
                "Scanner candidates rejected by missing or unfillable books",
                self.fill_failures_total,
                HashMap::new(),
            );
        }
        Ok(ScanOutcome {
            pairs_evaluated,
            opportunities,
            cycle_ms: self.cadence.cycle_metric(),
            shed_level,
            fill_failures,
        })
    }
}

fn deterministic_opportunity_id(
    candidate: &crate::DetectedOpportunity,
    detected_ts: UtcTime,
) -> Ulid {
    let mut first = std::collections::hash_map::DefaultHasher::new();
    candidate.buy_leg.hash(&mut first);
    candidate.sell_leg.hash(&mut first);
    candidate.category.hash(&mut first);
    let mut second = std::collections::hash_map::DefaultHasher::new();
    candidate.sell_leg.hash(&mut second);
    candidate.buy_leg.hash(&mut second);
    candidate.gross_spread.to_string().hash(&mut second);
    let randomness = ((first.finish() as u128) << 16) | (second.finish() as u128 & 0xffff);
    Ulid(ulid::Ulid::from_parts(detected_ts.unix_millis().max(0) as u64, randomness))
}

impl ScanOutcome {
    /// Publish newly detected opportunities to the canonical bus topic.
    /// Dedupe updates are intentionally not republished as fresh detections.
    pub async fn publish<P: MessageProducer>(&self, producer: &P) -> Result<(), ProducerError> {
        for opportunity in &self.opportunities {
            let mut envelope = Envelope::new("opportunity", opportunity.clone());
            envelope.trace_id = opportunity.trace_id.to_string();
            envelope.ts = opportunity.detected_ts.to_string();
            producer
                .send(Topic::OPPS_DETECTED, envelope, Some(&opportunity.id.to_string()))
                .await?;
        }
        Ok(())
    }

    /// Production path: make scored lifecycle state and its detection outbox
    /// durable before publishing. Open-chain duplicates update rather than emit.
    pub async fn persist_and_publish<P: MessageProducer>(
        &self,
        store: &LifecycleStore,
        producer: &P,
    ) -> Result<DurablePublishReport, LifecycleError> {
        let mut report = DurablePublishReport { created: 0, updated: 0, existing: 0, published: 0 };
        for opportunity in &self.opportunities {
            match store.persist_scored(opportunity).await? {
                PersistDisposition::Created(_) => report.created += 1,
                PersistDisposition::Updated(_) => report.updated += 1,
                PersistDisposition::Existing(_) => report.existing += 1,
            }
        }
        report.published = store.flush_detection_outbox(producer, 1_000).await?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_bus::producer::StubProducer;
    use aether_core::ids::VenueId;
    use aether_core::json::JsonObject;
    use aether_core::market::{InstrumentKind, MarketStatus};
    use aether_core::quote::BookLevel;

    #[test]
    fn scan_config_default() {
        let cfg = ScanConfig::default();
        assert_eq!(cfg.max_pairs_per_cycle, 100);
        assert_eq!(cfg.min_net_edge, Decimal::new(1, 3));
    }

    #[test]
    fn scanner_scan_once_returns_zeros() {
        let scanner = Scanner::new(ScanConfig::default());
        assert!(scanner.is_ok());
    }

    fn market(key: MarketKey, venue: &str) -> Market {
        Market {
            key,
            venue: VenueId::new(venue).unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: "BTC above 100k".into(),
            description_ref: String::new(),
            status: MarketStatus::Open,
            close_ts: None,
            resolve_ts: None,
            outcome: None,
            jurisdiction_flags: vec![],
            venue_ref: JsonObject::default(),
            meta: JsonObject::new(serde_json::json!({"canonical_event_id": "btc-100k"})).unwrap(),
        }
    }

    fn book(market: MarketKey, bid: Decimal, ask: Decimal, now: UtcTime) -> OrderBook {
        OrderBook::new(
            market,
            vec![BookLevel { price: bid, size: Decimal::new(10, 0) }],
            vec![BookLevel { price: ask, size: Decimal::new(10, 0) }],
            1,
            now,
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn scan_builds_and_publishes_canonical_opportunity() {
        let now = UtcTime::from_unix_millis(1_000_000).unwrap();
        let buy = MarketKey::new(&VenueId::new("kalshi").unwrap(), "btc").unwrap();
        let sell = MarketKey::new(&VenueId::new("polymarket").unwrap(), "btc").unwrap();
        let mut markets = HashMap::new();
        markets.insert(buy.clone(), market(buy.clone(), "kalshi"));
        markets.insert(sell.clone(), market(sell.clone(), "polymarket"));
        let mut quotes = HashMap::new();
        quotes.insert(
            buy.clone(),
            MarketQuote { bid: Decimal::new(60, 2), ask: Decimal::new(62, 2), ts: now },
        );
        quotes.insert(
            sell.clone(),
            MarketQuote { bid: Decimal::new(67, 2), ask: Decimal::new(68, 2), ts: now },
        );
        let books = HashMap::from([
            (buy.clone(), book(buy, Decimal::new(60, 2), Decimal::new(62, 2), now)),
            (sell.clone(), book(sell, Decimal::new(67, 2), Decimal::new(68, 2), now)),
        ]);
        let explain_ref = BrainRef { object_id: Ulid::new(), provenance_hash: "fixture".into() };
        let trace_id = Ulid::new();
        let metrics = Arc::new(MetricRegistry::new());
        let mut scanner =
            Scanner::with_metrics(ScanConfig::default(), Arc::clone(&metrics)).unwrap();
        let evidence = HashMap::from([(
            "id:btc-100k".to_owned(),
            EvidenceSnapshot {
                signal: crate::EvidenceSignal { relevance: Decimal::ONE, trust: Decimal::ONE },
                explain_ref: explain_ref.clone(),
            },
        )]);
        let outcome = scanner
            .scan_once(&quotes, &books, &markets, &evidence, now, &explain_ref, trace_id)
            .unwrap();
        assert_eq!(outcome.pairs_evaluated, 1);
        assert_eq!(outcome.opportunities.len(), 1);
        assert!(outcome.opportunities[0].edge.validate().is_ok());
        assert!(outcome.opportunities[0].edge.net_edge() > Decimal::ZERO);
        let exported_metrics = metrics.export_prometheus();
        assert!(exported_metrics.contains("aether_scan_cycle_ms"));
        assert!(exported_metrics.contains("aether_scan_shed_total"));
        assert!(exported_metrics.contains("aether_scan_cycle_ms_bucket"));

        let producer = StubProducer::new();
        outcome.publish(&producer).await.unwrap();
        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, Topic::OPPS_DETECTED);
        assert!(sent[0].1.contains("aether.opportunity.v1"));
        drop(sent);

        let expected = serde_json::to_string(&outcome.opportunities).unwrap();
        for _ in 0..2 {
            let mut replay_scanner = Scanner::new(ScanConfig::default()).unwrap();
            let replay = replay_scanner
                .scan_once(&quotes, &books, &markets, &evidence, now, &explain_ref, trace_id)
                .unwrap();
            assert_eq!(serde_json::to_string(&replay.opportunities).unwrap(), expected);
        }
    }

    #[test]
    fn phase_one_fixture_p95_stays_within_cadence_without_shedding() {
        let now = UtcTime::from_unix_millis(1_000_000).unwrap();
        let venue_names = ["kalshi", "polymarket", "hyperliquid", "alpaca", "openbb"];
        let mut markets = HashMap::new();
        let mut quotes = HashMap::new();
        let mut books = HashMap::new();
        let fallback = BrainRef { object_id: Ulid::new(), provenance_hash: "benchmark".into() };
        let mut evidence = HashMap::new();
        for event in 0..10 {
            let event_key = format!("event-{event}");
            evidence.insert(
                format!("id:{event_key}"),
                EvidenceSnapshot {
                    signal: crate::EvidenceSignal { relevance: Decimal::ONE, trust: Decimal::ONE },
                    explain_ref: fallback.clone(),
                },
            );
            for (venue_index, venue_name) in venue_names.iter().enumerate() {
                let venue = VenueId::new(*venue_name).unwrap();
                let key = MarketKey::new(&venue, &format!("{event}-{venue_index}")).unwrap();
                let ask = Decimal::new(60 + i64::try_from(venue_index).unwrap(), 2);
                let bid = ask - Decimal::new(1, 2);
                markets.insert(
                    key.clone(),
                    Market {
                        key: key.clone(),
                        venue,
                        kind: InstrumentKind::BinaryContract,
                        title: event_key.clone(),
                        description_ref: String::new(),
                        status: MarketStatus::Open,
                        close_ts: None,
                        resolve_ts: None,
                        outcome: None,
                        jurisdiction_flags: vec![],
                        venue_ref: JsonObject::default(),
                        meta: JsonObject::new(serde_json::json!({"canonical_event_id": event_key}))
                            .unwrap(),
                    },
                );
                quotes.insert(key.clone(), MarketQuote { bid, ask, ts: now });
                books.insert(key.clone(), book(key, bid, ask, now));
            }
        }
        let config = ScanConfig { max_pairs_per_cycle: 5_000, ..ScanConfig::default() };
        let mut scanner = Scanner::new(config).unwrap();
        let mut durations = Vec::new();
        for _ in 0..50 {
            let outcome = scanner
                .scan_once(&quotes, &books, &markets, &evidence, now, &fallback, Ulid::new())
                .unwrap();
            durations.push(outcome.cycle_ms);
        }
        durations.sort_unstable();
        let p95 = durations[47];
        assert!(p95 <= 500, "Phase-1 scanner p95 {p95} ms exceeded 500 ms");
        assert_eq!(scanner.cadence_mut().cycles_shed, 0);
        scanner.cadence_mut().end_cycle(std::time::Duration::from_millis(501));
        let shed = scanner
            .scan_once(&quotes, &books, &markets, &evidence, now, &fallback, Ulid::new())
            .unwrap();
        assert_eq!(shed.shed_level, 4);
        assert!(shed.opportunities.len() <= 1);
        assert_eq!(scanner.cadence_mut().cycles_shed, 1);
    }
}

//! Incremental runtime seam for canonical market-data bus envelopes.

use crate::{
    DurablePublishReport, EvidenceSnapshot, LifecycleError, LifecycleStore, MarketQuote, ScanError,
    ScanOutcome, Scanner,
};
use aether_bus::envelope::Envelope;
use aether_bus::producer::MessageProducer;
use aether_core::ids::{MarketKey, Ulid};
use aether_core::market::Market;
use aether_core::opportunity::BrainRef;
use aether_core::quote::{OrderBook, Quote};
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("scanner error: {0}")]
    Scan(#[from] ScanError),
    #[error("lifecycle error: {0}")]
    Lifecycle(#[from] LifecycleError),
    #[error("market-data envelope has an invalid trace id")]
    InvalidTrace,
    #[error("quote is missing a positive executable bid or ask")]
    IncompleteQuote,
}

#[derive(Debug, Clone)]
pub struct RuntimeCycleReport {
    pub scan: Option<ScanOutcome>,
    pub durable: DurablePublishReport,
    pub expired: Vec<Ulid>,
    pub open_chains: i64,
}

pub struct ScannerRuntime {
    scanner: Scanner,
    quotes: HashMap<MarketKey, MarketQuote>,
    books: HashMap<MarketKey, OrderBook>,
    markets: HashMap<MarketKey, Market>,
    evidence: HashMap<String, EvidenceSnapshot>,
    fallback_explain_ref: BrainRef,
    dirty: HashSet<MarketKey>,
    trace_ids: HashMap<MarketKey, Ulid>,
}

impl ScannerRuntime {
    pub fn new(scanner: Scanner, fallback_explain_ref: BrainRef) -> Self {
        Self {
            scanner,
            quotes: HashMap::new(),
            books: HashMap::new(),
            markets: HashMap::new(),
            evidence: HashMap::new(),
            fallback_explain_ref,
            dirty: HashSet::new(),
            trace_ids: HashMap::new(),
        }
    }

    pub fn replace_markets(&mut self, markets: impl IntoIterator<Item = Market>) {
        self.markets = markets.into_iter().map(|market| (market.key.clone(), market)).collect();
        self.dirty.extend(self.markets.keys().cloned());
    }

    pub fn replace_evidence(&mut self, evidence: HashMap<String, EvidenceSnapshot>) {
        self.evidence = evidence;
        self.dirty.extend(self.markets.keys().cloned());
    }

    pub fn apply_quote_envelope(&mut self, envelope: Envelope<Quote>) -> Result<(), RuntimeError> {
        let trace_id =
            Ulid::from_string(&envelope.trace_id).map_err(|_| RuntimeError::InvalidTrace)?;
        let quote = envelope.payload;
        let (Some(bid), Some(ask)) = (quote.bid, quote.ask) else {
            self.quotes.remove(&quote.market);
            self.dirty.insert(quote.market);
            return Err(RuntimeError::IncompleteQuote);
        };
        if bid <= Decimal::ZERO || ask <= Decimal::ZERO || bid > ask {
            self.quotes.remove(&quote.market);
            self.dirty.insert(quote.market);
            return Err(RuntimeError::IncompleteQuote);
        }
        self.trace_ids.insert(quote.market.clone(), trace_id);
        self.dirty.insert(quote.market.clone());
        self.quotes.insert(quote.market, MarketQuote { bid, ask, ts: quote.ts });
        Ok(())
    }

    pub fn apply_book_envelope(
        &mut self,
        envelope: Envelope<OrderBook>,
    ) -> Result<(), RuntimeError> {
        let trace_id =
            Ulid::from_string(&envelope.trace_id).map_err(|_| RuntimeError::InvalidTrace)?;
        let book = envelope.payload;
        self.trace_ids.insert(book.market.clone(), trace_id);
        self.dirty.insert(book.market.clone());
        self.books.insert(book.market.clone(), book);
        Ok(())
    }

    pub async fn run_due<P: MessageProducer>(
        &mut self,
        now: UtcTime,
        store: &LifecycleStore,
        producer: &P,
    ) -> Result<RuntimeCycleReport, RuntimeError> {
        let changed = self.dirty.clone();
        let scan = if changed.is_empty() {
            None
        } else {
            let trace_id = changed
                .iter()
                .filter_map(|key| self.trace_ids.get(key).copied().map(|trace| (key, trace)))
                .min_by(|left, right| left.0.as_str().cmp(right.0.as_str()))
                .map_or(self.fallback_explain_ref.object_id, |(_, trace)| trace);
            Some(self.scanner.scan_incremental(
                &self.quotes,
                &self.books,
                &self.markets,
                &self.evidence,
                &changed,
                now,
                &self.fallback_explain_ref,
                trace_id,
            )?)
        };
        let durable = if let Some(outcome) = &scan {
            outcome.persist_and_publish(store, producer).await?
        } else {
            DurablePublishReport {
                created: 0,
                updated: 0,
                existing: 0,
                published: store.flush_detection_outbox(producer, 1_000).await?,
            }
        };
        for key in &changed {
            self.dirty.remove(key);
        }
        let expired = store.expire_due(now).await?;
        let open_chains = store.open_count().await?;
        Ok(RuntimeCycleReport { scan, durable, expired, open_chains })
    }
}

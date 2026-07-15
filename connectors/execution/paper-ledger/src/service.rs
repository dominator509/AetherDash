//! Bus-facing paper execution service with a retryable fill outbox.

use std::collections::VecDeque;

use aether_bus::envelope::Envelope;
use aether_bus::producer::{MessageProducer, ProducerError};
use aether_bus::topics::Topic;
use aether_core::order::{Fill, OrderIntent};
use aether_core::quote::OrderBook;
use thiserror::Error;

use crate::ledger::{LedgerError, PaperLedger, Submission};
use crate::persistence::{PersistOutcome, PersistenceError, PostgresLedgerStore};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Publish(#[from] ProducerError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

/// Executes paper intents and emits canonical fills on `orders.fills`.
///
/// New fills enter an outbox before publication. A transient publisher error
/// leaves the failed fill and all later fills queued, so `flush_outbox()` can
/// retry without re-running execution or duplicating position changes.
pub struct PaperExecutionService<P> {
    ledger: PaperLedger,
    producer: P,
    outbox: VecDeque<Fill>,
}

impl<P: MessageProducer> PaperExecutionService<P> {
    pub fn new(ledger: PaperLedger, producer: P) -> Self {
        Self { ledger, producer, outbox: VecDeque::new() }
    }

    pub async fn submit(
        &mut self,
        intent: OrderIntent,
        book: &OrderBook,
    ) -> Result<Submission, ServiceError> {
        let submission = self.ledger.execute(intent, book)?;
        if !submission.replayed {
            self.outbox.extend(submission.fills.iter().cloned());
        }
        self.flush_outbox().await?;
        Ok(submission)
    }

    /// Production path: make relational state durable before publishing fills.
    /// An optional opportunity id is transitioned to `executed` only after the
    /// order/fill/position transaction succeeds.
    pub async fn submit_persisted(
        &mut self,
        store: &PostgresLedgerStore,
        intent: OrderIntent,
        book: &OrderBook,
        opportunity_id: Option<aether_core::ids::Ulid>,
    ) -> Result<Submission, ServiceError> {
        if let Some(fills) = store.existing_execution(&intent).await? {
            if let Some(opportunity_id) = opportunity_id {
                store
                    .record_opportunity_execution(opportunity_id, intent.id, intent.size, &fills)
                    .await?;
            }
            self.flush_persisted_outbox(store).await?;
            return Ok(Submission { fills, replayed: true });
        }
        let ledger_before_execution = self.ledger.clone();
        let submission = self.ledger.execute(intent.clone(), book)?;
        let order = self.ledger.orders().get(&intent.id).ok_or_else(|| {
            LedgerError::Rejected("executed order missing from ledger".to_owned())
        })?;
        let outcome = store.persist_execution(order, &submission.fills).await?;

        if outcome == PersistOutcome::AlreadyExists {
            // Another process won the durable idempotency race after our
            // preflight read. Discard speculative in-memory accounting and
            // return the authoritative stored fills.
            self.ledger = ledger_before_execution;
            let fills = store.existing_execution(&intent).await?.ok_or_else(|| {
                LedgerError::Rejected("durable idempotency winner has no execution".to_owned())
            })?;
            if let Some(opportunity_id) = opportunity_id {
                store
                    .record_opportunity_execution(opportunity_id, intent.id, intent.size, &fills)
                    .await?;
            }
            self.flush_persisted_outbox(store).await?;
            return Ok(Submission { fills, replayed: true });
        }

        if let Some(opportunity_id) = opportunity_id {
            store
                .record_opportunity_execution(
                    opportunity_id,
                    intent.id,
                    intent.size,
                    &submission.fills,
                )
                .await?;
        }
        self.flush_persisted_outbox(store).await?;
        Ok(Submission { fills: submission.fills, replayed: false })
    }

    /// Drain the crash-safe Postgres outbox. Safe to call at startup and
    /// after any previous publisher failure.
    pub async fn flush_persisted_outbox(
        &self,
        store: &PostgresLedgerStore,
    ) -> Result<(), ServiceError> {
        for event in store.pending_fill_events().await? {
            let key = event.fill.market.as_str().to_owned();
            let mut envelope = Envelope::new("fill", event.fill);
            envelope.trace_id = event.event_id.to_string();
            self.producer.send(Topic::ORDERS_FILLS, envelope, Some(&key)).await?;
            store.mark_fill_published(event.event_id).await?;
        }
        Ok(())
    }

    pub async fn flush_outbox(&mut self) -> Result<(), ProducerError> {
        while let Some(fill) = self.outbox.front() {
            let key = fill.market.as_str().to_owned();
            self.producer
                .send(Topic::ORDERS_FILLS, Envelope::new("fill", fill.clone()), Some(&key))
                .await?;
            self.outbox.pop_front();
        }
        Ok(())
    }

    pub fn pending_fills(&self) -> usize {
        self.outbox.len()
    }

    pub fn ledger(&self) -> &PaperLedger {
        &self.ledger
    }

    pub fn producer(&self) -> &P {
        &self.producer
    }
}

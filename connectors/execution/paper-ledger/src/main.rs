//! Bus-driven paper ledger process.

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use aether_bus::consumer::{KafkaConsumer, MessageConsumer};
use aether_bus::producer::{BreakerProducer, KafkaProducer, MessageProducer};
use aether_bus::quarantine::QuarantineStorage;
use aether_bus::topics::{ConsumerGroup, Topic};
use aether_core::ids::MarketKey;
use aether_core::order::OrderIntent;
use aether_core::quote::OrderBook;
use aether_paper_ledger::{PaperExecutionService, PaperLedger, PostgresLedgerStore};

type BoxError = Box<dyn Error + Send + Sync>;
type ProductionConsumer = KafkaConsumer<BreakerProducer<KafkaProducer>>;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")?;
    let store = PostgresLedgerStore::connect(&database_url).await?;
    let book_topics = configured_book_topics()?;
    let cache = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
    let book_consumer = production_consumer("paper-ledger-books")?;
    let intent_consumer = production_consumer("paper-ledger-intents")?;
    let fill_producer = KafkaProducer::from_env()?;

    tracing::info!(?book_topics, "paper ledger started");
    tokio::try_join!(
        run_books(book_consumer, book_topics, cache.clone(), store.clone()),
        run_intents(intent_consumer, fill_producer, cache, store),
    )?;
    Ok(())
}

fn configured_book_topics() -> Result<Vec<String>, BoxError> {
    let raw = std::env::var("AETHER_PAPER_BOOK_TOPICS")?;
    let topics: Vec<_> = raw
        .split(',')
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .map(str::to_owned)
        .collect();
    if topics.is_empty() || topics.iter().any(|topic| !topic.starts_with("md.books.")) {
        return Err(
            "AETHER_PAPER_BOOK_TOPICS must list comma-separated md.books.<venue> topics".into()
        );
    }
    Ok(topics)
}

fn production_consumer(service_name: &str) -> Result<ProductionConsumer, BoxError> {
    let bootstrap =
        std::env::var("AETHER_KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".to_owned());
    let group = ConsumerGroup::for_service(service_name);
    let quarantine_producer = KafkaProducer::from_env()?;
    let quarantine_storage = Arc::new(QuarantineStorage::new_from_env()?);
    Ok(KafkaConsumer::new(&bootstrap, &group, quarantine_producer, quarantine_storage)?)
}

async fn run_books(
    consumer: ProductionConsumer,
    topics: Vec<String>,
    cache: Arc<tokio::sync::RwLock<HashMap<MarketKey, OrderBook>>>,
    store: PostgresLedgerStore,
) -> Result<(), BoxError> {
    let topic_refs: Vec<_> = topics.iter().map(String::as_str).collect();
    loop {
        let envelopes = consumer.consume::<OrderBook>(&topic_refs).await?;
        for envelope in envelopes {
            let book = envelope.payload;
            if let (Some(bid), Some(ask)) = (book.bids().first(), book.asks().first()) {
                store
                    .update_mark(&book.market, (bid.price + ask.price) / rust_decimal::Decimal::TWO)
                    .await?;
            }
            cache.write().await.insert(book.market.clone(), book);
        }
        consumer.ack()?;
        consumer.commit_sync()?;
    }
}

async fn run_intents<P: MessageProducer>(
    consumer: ProductionConsumer,
    producer: P,
    cache: Arc<tokio::sync::RwLock<HashMap<MarketKey, OrderBook>>>,
    store: PostgresLedgerStore,
) -> Result<(), BoxError> {
    let mut service = PaperExecutionService::new(PaperLedger::new(), producer);
    service.flush_persisted_outbox(&store).await?;
    loop {
        let envelopes = consumer.consume::<OrderIntent>(&[Topic::ORDERS_INTENTS]).await?;
        for envelope in envelopes {
            let intent = envelope.payload;
            let book = loop {
                if let Some(book) = cache.read().await.get(&intent.market).cloned() {
                    break book;
                }
                tracing::warn!(market = %intent.market, "paper intent waiting for current book");
                tokio::time::sleep(Duration::from_millis(250)).await;
            };
            service.submit_persisted(&store, intent, &book, None).await?;
        }
        consumer.ack()?;
        consumer.commit_sync()?;
    }
}

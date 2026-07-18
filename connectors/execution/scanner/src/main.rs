//! Production bus-driven EP-307 scanner.

use aether_bus::consumer::{KafkaConsumer, MessageConsumer};
use aether_bus::producer::{BreakerProducer, KafkaProducer};
use aether_bus::quarantine::QuarantineStorage;
use aether_bus::topics::ConsumerGroup;
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::json::JsonObject;
use aether_core::market::Market;
use aether_core::opportunity::BrainRef;
use aether_core::quote::{OrderBook, Quote};
use aether_core::time::UtcTime;
use aether_observe::metrics::MetricRegistry;
use aether_scanner::{LifecycleStore, ScanConfig, Scanner, ScannerRuntime};
use axum::{extract::State, routing::get, Router};
use sqlx::PgPool;
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

type BoxError = Box<dyn Error + Send + Sync>;
type ProductionConsumer = KafkaConsumer<BreakerProducer<KafkaProducer>>;
type SharedRuntime = Arc<Mutex<ScannerRuntime>>;
type MarketRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Vec<String>,
    serde_json::Value,
    serde_json::Value,
);

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let pool = PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let store = LifecycleStore::new(pool.clone());
    let metrics = Arc::new(MetricRegistry::new());
    metrics.register_standard_metrics();
    let scanner = Scanner::with_metrics(ScanConfig::default(), Arc::clone(&metrics))?;
    let fallback =
        BrainRef { object_id: Ulid::new(), provenance_hash: "scanner:no-evidence".into() };
    let mut runtime = ScannerRuntime::new(scanner, fallback);
    runtime.replace_markets(load_open_markets(&pool).await?);
    let runtime = Arc::new(Mutex::new(runtime));
    let quote_topics = configured_topics("AETHER_SCANNER_TICK_TOPICS", "md.ticks.")?;
    let book_topics = configured_topics("AETHER_SCANNER_BOOK_TOPICS", "md.books.")?;
    let producer = KafkaProducer::from_env()?;

    tracing::info!(?quote_topics, ?book_topics, "scanner started");
    tokio::try_join!(
        consume_quotes(production_consumer("scanner-quotes")?, quote_topics, runtime.clone()),
        consume_books(production_consumer("scanner-books")?, book_topics, runtime.clone()),
        run_cycles(runtime.clone(), store, producer, Arc::clone(&metrics)),
        refresh_markets(runtime, pool),
        serve_metrics(metrics),
    )?;
    Ok(())
}

fn configured_topics(name: &str, prefix: &str) -> Result<Vec<String>, BoxError> {
    let raw = std::env::var(name)?;
    let topics: Vec<_> = raw
        .split(',')
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .map(str::to_owned)
        .collect();
    if topics.is_empty() || topics.iter().any(|topic| !topic.starts_with(prefix)) {
        return Err(format!("{name} must list comma-separated {prefix}<venue> topics").into());
    }
    Ok(topics)
}

fn production_consumer(service: &str) -> Result<ProductionConsumer, BoxError> {
    let bootstrap =
        std::env::var("AETHER_KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".into());
    let group = ConsumerGroup::for_service(service);
    let quarantine_producer = KafkaProducer::from_env()?;
    let quarantine_storage = Arc::new(QuarantineStorage::new_from_env()?);
    Ok(KafkaConsumer::new(&bootstrap, &group, quarantine_producer, quarantine_storage)?)
}

async fn consume_quotes(
    consumer: ProductionConsumer,
    topics: Vec<String>,
    runtime: SharedRuntime,
) -> Result<(), BoxError> {
    let refs: Vec<_> = topics.iter().map(String::as_str).collect();
    loop {
        for envelope in consumer.consume::<Quote>(&refs).await? {
            if let Err(error) = runtime.lock().await.apply_quote_envelope(envelope) {
                tracing::warn!(%error, "quote rejected by scanner runtime");
            }
        }
        consumer.ack()?;
        consumer.commit_sync()?;
    }
}

async fn consume_books(
    consumer: ProductionConsumer,
    topics: Vec<String>,
    runtime: SharedRuntime,
) -> Result<(), BoxError> {
    let refs: Vec<_> = topics.iter().map(String::as_str).collect();
    loop {
        for envelope in consumer.consume::<OrderBook>(&refs).await? {
            runtime.lock().await.apply_book_envelope(envelope)?;
        }
        consumer.ack()?;
        consumer.commit_sync()?;
    }
}

async fn run_cycles(
    runtime: SharedRuntime,
    store: LifecycleStore,
    producer: BreakerProducer<KafkaProducer>,
    metrics: Arc<MetricRegistry>,
) -> Result<(), BoxError> {
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let report = runtime.lock().await.run_due(UtcTime::now(), &store, &producer).await?;
        metrics.set_gauge(
            "aether_opportunity_lifecycle_open",
            "Open opportunity lifecycle count",
            report.open_chains as f64,
            HashMap::new(),
        );
    }
}

async fn refresh_markets(runtime: SharedRuntime, pool: PgPool) -> Result<(), BoxError> {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.tick().await;
    loop {
        interval.tick().await;
        runtime.lock().await.replace_markets(load_open_markets(&pool).await?);
    }
}

async fn serve_metrics(metrics: Arc<MetricRegistry>) -> Result<(), BoxError> {
    async fn endpoint(State(metrics): State<Arc<MetricRegistry>>) -> String {
        metrics.export_prometheus()
    }
    let bind =
        std::env::var("AETHER_SCANNER_METRICS_BIND").unwrap_or_else(|_| "127.0.0.1:9107".into());
    let address: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, Router::new().route("/metrics", get(endpoint)).with_state(metrics))
        .await?;
    Ok(())
}

async fn load_open_markets(pool: &PgPool) -> Result<Vec<Market>, BoxError> {
    let rows: Vec<MarketRow> = sqlx::query_as(
        "SELECT key,venue,kind,title,description_ref,status,\
         (extract(epoch FROM close_ts)*1000)::bigint,\
         (extract(epoch FROM resolve_ts)*1000)::bigint,outcome,jurisdiction_flags,venue_ref,meta \
         FROM markets WHERE status='open' ORDER BY key",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(market_from_row).collect()
}

fn market_from_row(row: MarketRow) -> Result<Market, BoxError> {
    let (
        key,
        venue,
        kind,
        title,
        description_ref,
        status,
        close_ms,
        resolve_ms,
        outcome,
        jurisdiction_flags,
        venue_ref,
        meta,
    ) = row;
    let venue = VenueId::new(&venue)?;
    let external = key
        .strip_prefix(&format!("mkt:{}:", venue.as_str()))
        .ok_or("market key does not match venue")?;
    Ok(Market {
        key: MarketKey::new(&venue, external)?,
        venue,
        kind: serde_json::from_value(serde_json::Value::String(kind))?,
        title,
        description_ref,
        status: serde_json::from_value(serde_json::Value::String(status))?,
        close_ts: close_ms.map(UtcTime::from_unix_millis).transpose()?,
        resolve_ts: resolve_ms.map(UtcTime::from_unix_millis).transpose()?,
        outcome,
        jurisdiction_flags,
        venue_ref: JsonObject::new(venue_ref)?,
        meta: JsonObject::new(meta)?,
    })
}

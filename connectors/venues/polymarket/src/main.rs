//! AETHER Terminal -- Polymarket venue adapter binary (read-only).
//!
//! Runs a Tonic gRPC server implementing `aether.venue.v1.VenueAdapter` with:
//!
//! - `list_markets` / `get_market`  (Gamma REST API + Polygon RPC)
//! - `stream_ticks` / `stream_book` (CLOB WebSocket, no auth)
//! - `submit_order` / `cancel_order` (capability_missing -- read-only)
//! - `get_balances`                  (capability_missing -- read-only)
//! - `health`                        (implemented)
//! - Plain HTTP `/healthz` and `/readyz` endpoints (for k8s probes)
//!
//! # Usage
//!
//! ```text
//! AETHER_VENUE__POLYMARKET_GAMMA_URL=https://gamma-api.polymarket.com \
//! cargo run -p aether-venue-polymarket
//! ```

mod auth;
mod client;
mod clob;
mod health;
mod normalize;
mod replay;
mod rpc;
mod stream;

use aether_bus::envelope::Envelope;
use aether_bus::producer::{BreakerProducer, KafkaProducer, MessageProducer, ProducerError};
use aether_bus::quarantine::{Quarantine, QuarantineStorage};
use aether_core::market::{Market, MarketStatus};
use aether_core::quote::Quote;
use aether_core::time::UtcTime;
use aether_proto::aether::core::v1::{
    self as core_proto, InstrumentKind, MarketStatus as ProtoMarketStatus,
};
use aether_proto::aether::venue::v1::venue_adapter_server::{VenueAdapter, VenueAdapterServer};
use aether_proto::aether::venue::v1::{
    self as venue_proto, Balances, CancelOrderRequest, CancelOrderResponse, GetBalancesRequest,
    HealthRequest, ListMarketsRequest, OrderAck, StreamBookRequest, StreamTicksRequest,
    VenueHealth,
};
use futures::Stream;
use prost_types::Timestamp;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};

// ---------------------------------------------------------------------------
// Type aliases for streaming responses
// ---------------------------------------------------------------------------

/// Boxed, pinned, Send stream of gRPC results.
type GrpcStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

// ---------------------------------------------------------------------------
// Venue adapter service
// ---------------------------------------------------------------------------

/// gRPC service implementation for Polymarket.
///
/// This adapter is **read-only** by design.  Order RPCs return
/// `capability_missing` — a US-jurisdiction blocking decision recorded
/// in EP-302.
pub struct PolymarketVenueAdapter {
    client: client::GammaClient,
    clob: Arc<clob::ClobClient>,
    rpc: Arc<rpc::RpcClient>,
    rpc_degraded: Arc<AtomicBool>,
    stream: Arc<stream::PolymarketStream>,
    last_tick_ms: Arc<AtomicI64>,
    quarantine_producer: Arc<BreakerProducer<KafkaProducer>>,
    quarantine_storage: Arc<QuarantineStorage>,
}

impl std::fmt::Debug for PolymarketVenueAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("PolymarketVenueAdapter").finish_non_exhaustive()
    }
}

impl PolymarketVenueAdapter {
    /// Create a new adapter with the given Gamma REST client and stream handle.
    pub fn new(
        client: client::GammaClient,
        clob: clob::ClobClient,
        rpc: rpc::RpcClient,
        stream: stream::PolymarketStream,
        quarantine_producer: BreakerProducer<KafkaProducer>,
        quarantine_storage: QuarantineStorage,
    ) -> Self {
        Self {
            client,
            clob: Arc::new(clob),
            rpc: Arc::new(rpc),
            rpc_degraded: Arc::new(AtomicBool::new(false)),
            stream: Arc::new(stream),
            last_tick_ms: Arc::new(AtomicI64::new(0)),
            quarantine_producer: Arc::new(quarantine_producer),
            quarantine_storage: Arc::new(quarantine_storage),
        }
    }
}

fn capability_missing_status() -> Status {
    Status::failed_precondition(
        "capability_missing: Polymarket pack is read-only; order execution is blocked for US jurisdictions",
    )
}

#[derive(Clone)]
struct GrpcProducer {
    tx: mpsc::Sender<serde_json::Value>,
    last_tick_ms: Arc<AtomicI64>,
}

impl MessageProducer for GrpcProducer {
    async fn send<T: serde::Serialize + Send + Sync>(
        &self,
        topic: &str,
        envelope: Envelope<T>,
        _key: Option<&str>,
    ) -> Result<(), ProducerError> {
        if topic == "md.ticks.polymarket" {
            self.last_tick_ms.store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
        }
        let value = serde_json::to_value(envelope.payload)
            .map_err(|error| ProducerError::Send(error.to_string()))?;
        self.tx.send(value).await.map_err(|_| ProducerError::Send("gRPC stream closed".to_string()))
    }
}

#[tonic::async_trait]
impl VenueAdapter for PolymarketVenueAdapter {
    // -- Streaming response types (all use GrpcStream) -- //
    type ListMarketsStream = GrpcStream<core_proto::Market>;
    type StreamTicksStream = GrpcStream<core_proto::Quote>;
    type StreamBookStream = GrpcStream<core_proto::OrderBook>;

    // ---- M2: real implementation ---- //

    async fn list_markets(
        &self,
        _request: Request<ListMarketsRequest>,
    ) -> Result<Response<Self::ListMarketsStream>, Status> {
        let mut raw_markets = Vec::new();
        let limit = 100u32;
        let mut offset = 0u32;
        loop {
            let page = match self.client.get_markets(limit, offset).await {
                Ok(page) => page,
                Err(error) => {
                    if let Some(raw) = error.raw_payload() {
                        Quarantine::publish(
                            self.quarantine_producer.as_ref(),
                            self.quarantine_storage.as_ref(),
                            "polymarket",
                            &error.to_string(),
                            raw,
                        )
                        .await
                        .map_err(|quarantine_error| {
                            Status::internal(format!(
                                "market response was malformed and quarantine failed: {quarantine_error}"
                            ))
                        })?;
                    }
                    return Err(Status::unavailable(format!("failed to fetch markets: {error}")));
                }
            };
            let count = page.len() as u32;
            raw_markets.extend(page);
            if count < limit {
                break;
            }
            offset += count;
        }

        let (tx, rx) = mpsc::channel(16);
        let quarantine_producer = Arc::clone(&self.quarantine_producer);
        let quarantine_storage = Arc::clone(&self.quarantine_storage);

        tokio::spawn(async move {
            for raw in raw_markets {
                let raw_bytes = serde_json::to_vec(&raw).unwrap_or_default();
                match normalize::normalize_markets(raw) {
                    Ok(domain_markets) => {
                        for domain_market in domain_markets {
                            if let Some(proto) = domain_market_to_proto(domain_market) {
                                let _ = tx.send(Ok(proto)).await;
                            }
                        }
                    }
                    Err(e) => {
                        if let Err(error) = Quarantine::publish(
                            quarantine_producer.as_ref(),
                            quarantine_storage.as_ref(),
                            "polymarket",
                            &e.to_string(),
                            &raw_bytes,
                        )
                        .await
                        {
                            let _ = tx
                                .send(Err(Status::internal(format!(
                                    "failed to quarantine malformed market: {error}"
                                ))))
                                .await;
                            break;
                        }
                        tracing::warn!(error = %e, "skipping malformed market");
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn get_market(
        &self,
        request: Request<venue_proto::GetMarketRequest>,
    ) -> Result<Response<core_proto::Market>, Status> {
        let req = request.into_inner();
        let key = req.key.ok_or_else(|| Status::invalid_argument("market key is required"))?;

        let token_id = parse_polymarket_market_key(&key.value).map_err(Status::invalid_argument)?;

        let raw = match self.client.get_market_by_token_id(token_id).await {
            Ok(Some(market)) => market,
            Ok(None) => {
                return Err(Status::not_found(format!(
                    "market with token_id '{token_id}' not found"
                )))
            }
            Err(error) => {
                if let Some(raw) = error.raw_payload() {
                    Quarantine::publish(
                        self.quarantine_producer.as_ref(),
                        self.quarantine_storage.as_ref(),
                        "polymarket",
                        &error.to_string(),
                        raw,
                    )
                    .await
                    .map_err(|quarantine_error| {
                        Status::internal(format!(
                            "market response was malformed and quarantine failed: {quarantine_error}"
                        ))
                    })?;
                }
                return Err(Status::unavailable(format!("failed to fetch market: {error}")));
            }
        };

        let outcomes: Vec<String> = serde_json::from_str(&raw.outcomes).map_err(|error| {
            Status::internal(format!("market outcomes were malformed: {error}"))
        })?;

        let raw_bytes = serde_json::to_vec(&raw).unwrap_or_default();
        let domains = match normalize::normalize_markets(raw) {
            Ok(domains) => domains,
            Err(error) => {
                Quarantine::publish(
                    self.quarantine_producer.as_ref(),
                    self.quarantine_storage.as_ref(),
                    "polymarket",
                    &error.to_string(),
                    &raw_bytes,
                )
                .await
                .map_err(|quarantine_error| {
                    Status::internal(format!(
                        "normalization failed and quarantine was unavailable: {quarantine_error}"
                    ))
                })?;
                return Err(Status::internal("market normalization failed; payload quarantined"));
            }
        };

        // Find the specific outcome matching this token_id.
        let mut domain = domains
            .into_iter()
            .find(|market| market.key.as_str() == format!("mkt:polymarket:{token_id}"))
            .ok_or_else(|| {
                Status::internal("normalized market set did not contain the requested token")
            })?;

        // Check on-chain resolution via Polygon RPC.
        if let Some(condition_id) =
            domain.meta.as_value().get("condition_id").and_then(|v| v.as_str())
        {
            match self.rpc.check_resolution(condition_id, outcomes.len()).await {
                Ok(rpc::ResolutionStatus::Resolved { outcome_index }) => {
                    self.rpc_degraded.store(false, Ordering::Relaxed);
                    domain.status = aether_core::market::MarketStatus::Resolved;
                    domain.outcome =
                        outcome_index.and_then(|index| outcomes.get(index as usize).cloned());
                }
                Ok(_) => self.rpc_degraded.store(false, Ordering::Relaxed),
                Err(error) => {
                    self.rpc_degraded.store(true, Ordering::Relaxed);
                    tracing::warn!(%error, %condition_id, "Polygon resolution read degraded");
                }
            }
        }

        let proto = domain_market_to_proto(domain)
            .ok_or_else(|| Status::internal("market conversion failed"))?;

        Ok(Response::new(proto))
    }

    // ---- M4: stub implementations ---- //

    async fn stream_ticks(
        &self,
        request: Request<StreamTicksRequest>,
    ) -> Result<Response<Self::StreamTicksStream>, Status> {
        let mut token_ids = Vec::new();
        for key in request.into_inner().keys {
            let token_id = match parse_polymarket_market_key(&key.value) {
                Ok(token_id) => token_id,
                Err(message) => return Err(Status::invalid_argument(message)),
            };
            token_ids.push(token_id.to_string());
        }
        if token_ids.is_empty() {
            return Err(Status::invalid_argument("at least one market key is required"));
        }

        let (value_tx, mut value_rx) = mpsc::channel(64);
        let (grpc_tx, grpc_rx) = mpsc::channel(64);
        let stream = Arc::clone(&self.stream);
        let last_tick_ms = Arc::clone(&self.last_tick_ms);
        let quarantine_producer = Arc::clone(&self.quarantine_producer);
        let quarantine_storage = Arc::clone(&self.quarantine_storage);
        tokio::spawn(async move {
            let producer = GrpcProducer { tx: value_tx, last_tick_ms };
            if let Err(error) = stream
                .stream_ticks_to_bus(
                    &token_ids,
                    &producer,
                    quarantine_producer.as_ref(),
                    quarantine_storage.as_ref(),
                )
                .await
            {
                tracing::warn!(%error, "Polymarket tick stream ended");
            }
        });
        tokio::spawn(async move {
            while let Some(value) = value_rx.recv().await {
                let result =
                    serde_json::from_value::<Quote>(value).map(quote_to_proto).map_err(|error| {
                        Status::internal(format!("invalid normalized quote: {error}"))
                    });
                if grpc_tx.send(result).await.is_err() {
                    break;
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(grpc_rx))))
    }

    async fn stream_book(
        &self,
        request: Request<StreamBookRequest>,
    ) -> Result<Response<Self::StreamBookStream>, Status> {
        let request = request.into_inner();
        let key = request.key.ok_or_else(|| Status::invalid_argument("market key is required"))?;
        let token_id =
            parse_polymarket_market_key(&key.value).map_err(Status::invalid_argument)?.to_string();
        let (value_tx, mut value_rx) = mpsc::channel(64);
        let (grpc_tx, grpc_rx) = mpsc::channel(64);
        let stream = Arc::clone(&self.stream);
        let clob = Arc::clone(&self.clob);
        let last_tick_ms = Arc::clone(&self.last_tick_ms);
        let quarantine_producer = Arc::clone(&self.quarantine_producer);
        let quarantine_storage = Arc::clone(&self.quarantine_storage);
        tokio::spawn(async move {
            let producer = GrpcProducer { tx: value_tx, last_tick_ms };
            match clob.get_order_book(&token_id).await {
                Ok(snapshot) => {
                    let raw = serde_json::to_vec(&snapshot).unwrap_or_default();
                    match normalize::normalize_book(snapshot) {
                        Ok(book) => {
                            if let Err(error) = producer
                                .send(
                                    "md.books.polymarket",
                                    Envelope::new("order_book", &book),
                                    Some(book.market.as_str()),
                                )
                                .await
                            {
                                tracing::warn!(%error, "initial CLOB snapshot publish failed");
                            }
                        }
                        Err(error) => {
                            if let Err(quarantine_error) = Quarantine::publish(
                                quarantine_producer.as_ref(),
                                quarantine_storage.as_ref(),
                                "polymarket",
                                &error.to_string(),
                                &raw,
                            )
                            .await
                            {
                                tracing::warn!(%quarantine_error, "initial CLOB normalization quarantine failed");
                            }
                            tracing::warn!(%error, "initial CLOB snapshot normalization failed");
                        }
                    }
                }
                Err(error) => {
                    if let Some(raw) = error.raw_payload() {
                        if let Err(quarantine_error) = Quarantine::publish(
                            quarantine_producer.as_ref(),
                            quarantine_storage.as_ref(),
                            "polymarket",
                            &error.to_string(),
                            raw,
                        )
                        .await
                        {
                            tracing::warn!(%quarantine_error, "initial CLOB quarantine failed");
                        }
                    }
                    tracing::warn!(%error, "initial CLOB snapshot unavailable");
                }
            }
            if let Err(error) = stream
                .stream_books_to_bus(
                    std::slice::from_ref(&token_id),
                    &producer,
                    quarantine_producer.as_ref(),
                    quarantine_storage.as_ref(),
                )
                .await
            {
                tracing::warn!(%error, "Polymarket book stream ended");
            }
        });
        tokio::spawn(async move {
            while let Some(value) = value_rx.recv().await {
                let result = serde_json::from_value::<aether_core::quote::OrderBook>(value)
                    .map(order_book_to_proto)
                    .map_err(|error| Status::internal(format!("invalid normalized book: {error}")));
                if grpc_tx.send(result).await.is_err() {
                    break;
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(grpc_rx))))
    }

    // ---- M5: order implementations (read-only -- capability_missing) ---- //

    async fn submit_order(
        &self,
        _request: Request<core_proto::Order>,
    ) -> Result<Response<OrderAck>, Status> {
        Err(capability_missing_status())
    }

    async fn cancel_order(
        &self,
        _request: Request<CancelOrderRequest>,
    ) -> Result<Response<CancelOrderResponse>, Status> {
        Err(capability_missing_status())
    }

    async fn get_balances(
        &self,
        _request: Request<GetBalancesRequest>,
    ) -> Result<Response<Balances>, Status> {
        Err(capability_missing_status())
    }

    // ---- M6: health implementation ---- //

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<VenueHealth>, Status> {
        let mut h = health::check_health(&self.client).await;
        if h.status != "down" {
            let last_tick = self.last_tick_ms.load(Ordering::Relaxed);
            apply_tick_health(&mut h, last_tick, chrono::Utc::now().timestamp_millis());
            h.rate_remaining = h.rate_remaining.min(self.clob.rate_remaining().await);
            if self.rpc_degraded.load(Ordering::Relaxed) {
                h.status = "degraded".into();
            }
        }
        Ok(Response::new(h))
    }
}

fn apply_tick_health(health: &mut VenueHealth, last_tick_ms: i64, now_ms: i64) {
    if last_tick_ms <= 0 {
        health.status = "degraded".into();
        health.lag_ms = u64::MAX;
        return;
    }
    health.lag_ms = now_ms.saturating_sub(last_tick_ms) as u64;
    health.status = if health.lag_ms <= 2_000 { "ok" } else { "degraded" }.into();
}

// ---------------------------------------------------------------------------
// Domain -> Proto conversion
// ---------------------------------------------------------------------------

/// Convert a canonical `aether_core::Market` to its proto representation.
fn domain_market_to_proto(m: Market) -> Option<core_proto::Market> {
    let venue_str = m.venue.as_str().to_string();
    let (venue_id, market_key) = (
        core_proto::VenueId { value: venue_str },
        core_proto::MarketKey { value: m.key.as_str().to_string() },
    );

    let kind = match m.kind {
        aether_core::market::InstrumentKind::BinaryContract => {
            InstrumentKind::BinaryContract as i32
        }
        aether_core::market::InstrumentKind::CategoricalContract => {
            InstrumentKind::CategoricalContract as i32
        }
        _ => InstrumentKind::Unspecified as i32,
    };

    let status = match m.status {
        MarketStatus::Open => ProtoMarketStatus::Open as i32,
        MarketStatus::Halted => ProtoMarketStatus::Halted as i32,
        MarketStatus::Closed => ProtoMarketStatus::Closed as i32,
        MarketStatus::Resolved => ProtoMarketStatus::Resolved as i32,
    };

    let close_ts = m.close_ts.map(utc_time_to_proto);
    let resolve_ts = m.resolve_ts.map(utc_time_to_proto);

    let venue_ref = serde_json::to_string(m.venue_ref.as_value()).unwrap_or_default();
    let meta = serde_json::to_string(m.meta.as_value()).unwrap_or_default();

    Some(core_proto::Market {
        key: Some(market_key),
        venue: Some(venue_id),
        kind,
        title: m.title,
        description_ref: m.description_ref,
        status,
        close_ts,
        resolve_ts,
        outcome: m.outcome,
        jurisdiction_flags: m.jurisdiction_flags,
        venue_ref,
        meta,
    })
}

/// Convert `UtcTime` to `prost_types::Timestamp`.
fn utc_time_to_proto(t: UtcTime) -> Timestamp {
    let millis = t.unix_millis();
    Timestamp { seconds: millis / 1000, nanos: ((millis % 1000) * 1_000_000) as i32 }
}

fn parse_polymarket_market_key(value: &str) -> Result<&str, &'static str> {
    value
        .strip_prefix("mkt:polymarket:")
        .filter(|token_id| !token_id.is_empty())
        .ok_or("market key must be mkt:polymarket:{token_id}")
}

fn quote_to_proto(quote: Quote) -> core_proto::Quote {
    core_proto::Quote {
        market: Some(core_proto::MarketKey { value: quote.market.as_str().to_string() }),
        bid: quote.bid.map(|value| value.to_string()),
        ask: quote.ask.map(|value| value.to_string()),
        mid: quote.mid.map(|value| value.to_string()),
        last: quote.last.map(|value| value.to_string()),
        bid_size: quote.bid_size.map(|value| value.to_string()),
        ask_size: quote.ask_size.map(|value| value.to_string()),
        ts: Some(utc_time_to_proto(quote.ts)),
        source: core_proto::QuoteSource::Stream as i32,
        seq: quote.seq,
    }
}

fn order_book_to_proto(book: aether_core::quote::OrderBook) -> core_proto::OrderBook {
    let to_level = |level: &aether_core::quote::BookLevel| core_proto::BookLevel {
        price: level.price.to_string(),
        size: level.size.to_string(),
    };
    core_proto::OrderBook {
        market: Some(core_proto::MarketKey { value: book.market.as_str().to_string() }),
        bids: book.bids().iter().map(to_level).collect(),
        asks: book.asks().iter().map(to_level).collect(),
        depth: u32::try_from(book.depth).unwrap_or(u32::MAX),
        ts: Some(utc_time_to_proto(book.ts)),
        seq: book.seq,
    }
}

// ---------------------------------------------------------------------------
// HTTP health endpoints (plain TCP, no framework)
// ---------------------------------------------------------------------------

/// Bind a minimal TCP listener for health probes and the feed-lag metric.
async fn serve_http_health(
    port: u16,
    last_tick_ms: Arc<AtomicI64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TokioTcpListener::bind(&addr).await?;
    tracing::info!("health HTTP server listening on {addr}");

    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                tracing::warn!(error = %e, "accept failed");
                continue;
            }
        };

        let last_tick_ms = Arc::clone(&last_tick_ms);
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            if n == 0 {
                return;
            }

            let request = String::from_utf8_lossy(&buf[..n]);

            let (status_line, content_type, body) = health_http_response(
                &request,
                last_tick_ms.load(Ordering::Relaxed),
                chrono::Utc::now().timestamp_millis(),
            );

            let response = format!(
                "{status_line}content-length: {}\r\ncontent-type: {content_type}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );

            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
            tracing::debug!(peer = %peer, path = %request.lines().next().unwrap_or(""), "health request");
        });
    }
}

fn health_http_response(
    request: &str,
    last_tick_ms: i64,
    now_ms: i64,
) -> (&'static str, &'static str, String) {
    if request.starts_with("GET /healthz") || request.starts_with("GET /readyz") {
        return ("HTTP/1.1 200 OK\r\n", "text/plain", "OK".to_string());
    }
    if request.starts_with("GET /metrics") {
        let lag = if last_tick_ms <= 0 {
            "NaN".to_string()
        } else {
            now_ms.saturating_sub(last_tick_ms).to_string()
        };
        let body = format!(
            "# HELP aether_feed_lag_ms Age of the most recent normalized venue tick in milliseconds.\n\
# TYPE aether_feed_lag_ms gauge\n\
aether_feed_lag_ms{{venue=\"polymarket\"}} {lag}\n"
        );
        return ("HTTP/1.1 200 OK\r\n", "text/plain; version=0.0.4", body);
    }
    ("HTTP/1.1 404 Not Found\r\n", "text/plain", "Not Found".to_string())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,aether_venue_polymarket=debug".into()),
        )
        .init();

    // -- Auth & client setup -- //
    let _auth = auth::PolymarketAuth::from_env()?;
    let client = client::GammaClient::from_env();
    let clob_client = clob::ClobClient::from_env();
    let rpc_client = rpc::RpcClient::from_env();

    // -- Stream setup -- //
    let polymarket_stream = stream::PolymarketStream::from_env();
    let quarantine_producer = KafkaProducer::from_env()?;
    let quarantine_storage = QuarantineStorage::new_from_env()?;

    // -- gRPC adapter -- //
    let adapter = PolymarketVenueAdapter::new(
        client,
        clob_client,
        rpc_client,
        polymarket_stream,
        quarantine_producer,
        quarantine_storage,
    );
    let health_clock = Arc::clone(&adapter.last_tick_ms);
    let grpc_addr = std::env::var("AETHER_VENUE__POLYMARKET_GRPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50055".to_string())
        .parse()?;
    let health_http_port: u16 = std::env::var("AETHER_VENUE__POLYMARKET_HEALTH_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8085);

    tracing::info!("starting gRPC server on {grpc_addr}");
    tracing::info!("starting HTTP health server on port {health_http_port}");

    let (mut health_reporter, grpc_health) = tonic_health::server::health_reporter();
    health_reporter.set_serving::<VenueAdapterServer<PolymarketVenueAdapter>>().await;

    // Run both servers concurrently
    let grpc = async move {
        Server::builder()
            .add_service(grpc_health)
            .add_service(VenueAdapterServer::new(adapter))
            .serve(grpc_addr)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    };

    let health = serve_http_health(health_http_port, health_clock);

    tokio::try_join!(grpc, health)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_capability_gate_is_explicit() {
        let status = capability_missing_status();
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert!(status.message().starts_with("capability_missing:"));
    }

    #[test]
    fn health_lag_is_last_tick_age_not_request_latency() {
        let mut health = VenueHealth { status: "ok".into(), lag_ms: 1, rate_remaining: 73 };
        apply_tick_health(&mut health, 10_000, 11_250);
        assert_eq!(health.status, "ok");
        assert_eq!(health.lag_ms, 1_250);
        assert_eq!(health.rate_remaining, 73);

        apply_tick_health(&mut health, 10_000, 13_001);
        assert_eq!(health.status, "degraded");
        assert_eq!(health.lag_ms, 3_001);
    }

    #[test]
    fn health_without_a_tick_is_explicitly_unknown() {
        let mut health = VenueHealth { status: "ok".into(), lag_ms: 0, rate_remaining: 100 };
        apply_tick_health(&mut health, 0, 11_250);
        assert_eq!(health.status, "degraded");
        assert_eq!(health.lag_ms, u64::MAX);
    }

    #[test]
    fn metrics_exposes_feed_lag_from_the_same_tick_clock() {
        let (status, content_type, body) =
            health_http_response("GET /metrics HTTP/1.1\r\n", 10_000, 11_250);
        assert_eq!(status, "HTTP/1.1 200 OK\r\n");
        assert_eq!(content_type, "text/plain; version=0.0.4");
        assert!(body.contains("aether_feed_lag_ms{venue=\"polymarket\"} 1250"));

        let (_, _, unknown) = health_http_response("GET /metrics HTTP/1.1\r\n", 0, 11_250);
        assert!(unknown.contains("aether_feed_lag_ms{venue=\"polymarket\"} NaN"));
    }

    #[test]
    fn parse_polymarket_market_key_valid() {
        let result = parse_polymarket_market_key("mkt:polymarket:0xabc123def456");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "0xabc123def456");
    }

    #[test]
    fn parse_polymarket_market_key_missing_token_id_is_error() {
        assert!(parse_polymarket_market_key("mkt:polymarket:").is_err());
    }

    #[test]
    fn parse_polymarket_market_key_wrong_prefix_is_error() {
        assert!(parse_polymarket_market_key("mkt:kalshi:some_ticker").is_err());
    }

    #[test]
    fn parse_polymarket_market_key_garbage_is_error() {
        assert!(parse_polymarket_market_key("not-even-close").is_err());
    }
}

//! AETHER Terminal -- Hyperliquid venue adapter binary.
//!
//! Runs a Tonic gRPC server implementing `aether.venue.v1.VenueAdapter` with:
//!
//! - `list_markets` / `get_market` (REST meta endpoint)
//! - `stream_ticks` / `stream_book` (polling-based streams)
//! - `submit_order` / `cancel_order` / `get_balances` (capability_missing)
//! - `health` (implemented)
//! - Plain HTTP `/healthz` and `/readyz` endpoints (for k8s probes)
//!
//! # Usage
//!
//! ```text
//! cargo run -p aether-venue-hyperliquid
//! ```

mod auth;
mod client;
mod health;
mod normalize;
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
    Balances, CancelOrderRequest, CancelOrderResponse, GetBalancesRequest, HealthRequest,
    ListMarketsRequest, OrderAck, StreamBookRequest, StreamTicksRequest, VenueHealth,
};
use futures::Stream;
use prost_types::Timestamp;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};

// ---------------------------------------------------------------------------
// Type aliases for streaming responses
// ---------------------------------------------------------------------------

type GrpcStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

// ---------------------------------------------------------------------------
// Venue adapter service
// ---------------------------------------------------------------------------

/// gRPC service implementation for Hyperliquid.
pub struct HlVenueAdapter {
    client: client::HlClient,
    stream: Arc<stream::HlStream>,
    last_tick_ms: Arc<AtomicI64>,
    quarantine_producer: Arc<BreakerProducer<KafkaProducer>>,
    quarantine_storage: Arc<QuarantineStorage>,
}

impl std::fmt::Debug for HlVenueAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("HlVenueAdapter").finish_non_exhaustive()
    }
}

impl HlVenueAdapter {
    pub fn new(
        client: client::HlClient,
        stream: stream::HlStream,
        quarantine_producer: BreakerProducer<KafkaProducer>,
        quarantine_storage: QuarantineStorage,
    ) -> Self {
        Self {
            client,
            stream: Arc::new(stream),
            last_tick_ms: Arc::new(AtomicI64::new(0)),
            quarantine_producer: Arc::new(quarantine_producer),
            quarantine_storage: Arc::new(quarantine_storage),
        }
    }
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
        if topic == "md.ticks.hyperliquid" {
            self.last_tick_ms.store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
        }
        let value = serde_json::to_value(envelope.payload)
            .map_err(|error| ProducerError::Send(error.to_string()))?;
        self.tx.send(value).await.map_err(|_| ProducerError::Send("gRPC stream closed".to_string()))
    }
}

#[tonic::async_trait]
impl VenueAdapter for HlVenueAdapter {
    type ListMarketsStream = GrpcStream<core_proto::Market>;
    type StreamTicksStream = GrpcStream<core_proto::Quote>;
    type StreamBookStream = GrpcStream<core_proto::OrderBook>;

    // ---- Market listing ---- //

    async fn list_markets(
        &self,
        _request: Request<ListMarketsRequest>,
    ) -> Result<Response<Self::ListMarketsStream>, Status> {
        let meta_ctx =
            self.client.get_meta_and_asset_ctxs().await.map_err(|error| {
                Status::unavailable(format!("failed to fetch markets: {error}"))
            })?;
        let spot_ctx = self.client.get_spot_meta_and_asset_ctxs().await.map_err(|error| {
            Status::unavailable(format!("failed to fetch spot markets: {error}"))
        })?;

        let (tx, rx) = mpsc::channel(16);
        let quarantine_producer = Arc::clone(&self.quarantine_producer);
        let quarantine_storage = Arc::clone(&self.quarantine_storage);

        tokio::spawn(async move {
            let assets = meta_ctx.universe;
            let ctxs = meta_ctx.asset_ctxs;

            for (i, raw) in assets.into_iter().enumerate() {
                let raw_bytes = serde_json::to_vec(&raw).unwrap_or_default();
                let ctx = ctxs.get(i);
                match normalize::normalize_market(raw, ctx) {
                    Ok(domain_market) => {
                        if let Some(proto) = domain_market_to_proto(domain_market) {
                            let _ = tx.send(Ok(proto)).await;
                        }
                    }
                    Err(e) => {
                        if let Err(error) = Quarantine::publish(
                            quarantine_producer.as_ref(),
                            quarantine_storage.as_ref(),
                            "hyperliquid",
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

            for (index, raw) in spot_ctx.universe.into_iter().enumerate() {
                let raw_bytes = serde_json::to_vec(&raw).unwrap_or_default();
                match normalize::normalize_spot_market(
                    raw,
                    &spot_ctx.tokens,
                    spot_ctx.asset_ctxs.get(index),
                ) {
                    Ok(market) => {
                        if let Some(proto) = domain_market_to_proto(market) {
                            let _ = tx.send(Ok(proto)).await;
                        }
                    }
                    Err(error) => {
                        if let Err(quarantine_error) = Quarantine::publish(
                            quarantine_producer.as_ref(),
                            quarantine_storage.as_ref(),
                            "hyperliquid",
                            &error.to_string(),
                            &raw_bytes,
                        )
                        .await
                        {
                            let _ = tx
                                .send(Err(Status::internal(format!(
                                "failed to quarantine malformed spot market: {quarantine_error}"
                            ))))
                                .await;
                            break;
                        }
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn get_market(
        &self,
        request: Request<aether_proto::aether::venue::v1::GetMarketRequest>,
    ) -> Result<Response<core_proto::Market>, Status> {
        let req = request.into_inner();
        let key = req.key.ok_or_else(|| Status::invalid_argument("market key is required"))?;

        let coin = key
            .value
            .strip_prefix("mkt:hyperliquid:")
            .filter(|coin| !coin.is_empty() && !coin.contains(':'))
            .ok_or_else(|| Status::invalid_argument("market key must be mkt:hyperliquid:{coin}"))?;

        if let Some(index) = coin.strip_prefix('@').and_then(|value| value.parse::<u32>().ok()) {
            let mut spot = self.client.get_spot_meta_and_asset_ctxs().await.map_err(|error| {
                Status::unavailable(format!("failed to fetch spot market: {error}"))
            })?;
            let position = spot
                .universe
                .iter()
                .position(|pair| pair.index == index)
                .ok_or_else(|| Status::not_found(format!("spot coin '@{index}' not found")))?;
            let raw = spot.universe.remove(position);
            let raw_bytes = serde_json::to_vec(&raw).unwrap_or_default();
            let market = match normalize::normalize_spot_market(
                raw,
                &spot.tokens,
                spot.asset_ctxs.get(position),
            ) {
                Ok(market) => market,
                Err(error) => {
                    Quarantine::publish(
                        self.quarantine_producer.as_ref(),
                        self.quarantine_storage.as_ref(),
                        "hyperliquid",
                        &error.to_string(),
                        &raw_bytes,
                    )
                    .await
                    .map_err(|quarantine_error| {
                        Status::internal(format!(
                        "normalization failed and quarantine was unavailable: {quarantine_error}"
                    ))
                    })?;
                    return Err(Status::internal(
                        "spot market normalization failed; payload quarantined",
                    ));
                }
            };
            return domain_market_to_proto(market)
                .map(Response::new)
                .ok_or_else(|| Status::internal("market conversion failed"));
        }

        let meta_ctx = self
            .client
            .get_meta_and_asset_ctxs()
            .await
            .map_err(|error| Status::unavailable(format!("failed to fetch market: {error}")))?;

        let mut meta_ctx = meta_ctx;
        let upper = coin.to_uppercase();
        let pos = meta_ctx.universe.iter().position(|a| a.name == upper);

        match pos {
            Some(idx) => {
                let raw = meta_ctx.universe.remove(idx);
                let ctx = meta_ctx.asset_ctxs.get(idx);
                let raw_bytes = serde_json::to_vec(&raw).unwrap_or_default();
                let domain = match normalize::normalize_market(raw, ctx) {
                    Ok(domain) => domain,
                    Err(error) => {
                        Quarantine::publish(
                            self.quarantine_producer.as_ref(),
                            self.quarantine_storage.as_ref(),
                            "hyperliquid",
                            &error.to_string(),
                            &raw_bytes,
                        )
                        .await
                        .map_err(|quarantine_error| {
                            Status::internal(format!(
                                "normalization failed and quarantine was unavailable: {quarantine_error}"
                            ))
                        })?;
                        return Err(Status::internal(
                            "market normalization failed; payload quarantined",
                        ));
                    }
                };

                let proto = domain_market_to_proto(domain)
                    .ok_or_else(|| Status::internal("market conversion failed"))?;

                Ok(Response::new(proto))
            }
            None => Err(Status::not_found(format!("coin '{coin}' not found"))),
        }
    }

    // ---- Streaming ---- //

    async fn stream_ticks(
        &self,
        request: Request<StreamTicksRequest>,
    ) -> Result<Response<Self::StreamTicksStream>, Status> {
        let mut tickers = Vec::new();
        for key in request.into_inner().keys {
            let ticker = match parse_hyperliquid_market_key(&key.value) {
                Ok(ticker) => ticker,
                Err(message) => return Err(Status::invalid_argument(message)),
            };
            tickers.push(ticker.to_string());
        }
        if tickers.is_empty() {
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
                .stream_mids_to_bus(
                    &tickers,
                    &producer,
                    quarantine_producer.as_ref(),
                    quarantine_storage.as_ref(),
                )
                .await
            {
                tracing::warn!(%error, "Hyperliquid tick stream ended");
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
        let ticker =
            parse_hyperliquid_market_key(&key.value).map_err(Status::invalid_argument)?.to_string();
        let (value_tx, mut value_rx) = mpsc::channel(64);
        let (grpc_tx, grpc_rx) = mpsc::channel(64);
        let stream = Arc::clone(&self.stream);
        let last_tick_ms = Arc::clone(&self.last_tick_ms);
        let quarantine_producer = Arc::clone(&self.quarantine_producer);
        let quarantine_storage = Arc::clone(&self.quarantine_storage);
        tokio::spawn(async move {
            let producer = GrpcProducer { tx: value_tx, last_tick_ms };
            if let Err(error) = stream
                .stream_books_to_bus(
                    &[ticker],
                    &producer,
                    quarantine_producer.as_ref(),
                    quarantine_storage.as_ref(),
                )
                .await
            {
                tracing::warn!(%error, "Hyperliquid book stream ended");
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

    // ---- Capability missing ---- //

    async fn submit_order(
        &self,
        _request: Request<core_proto::Order>,
    ) -> Result<Response<OrderAck>, Status> {
        Err(capability_missing("orders"))
    }

    async fn cancel_order(
        &self,
        _request: Request<CancelOrderRequest>,
    ) -> Result<Response<CancelOrderResponse>, Status> {
        Err(capability_missing("orders"))
    }

    async fn get_balances(
        &self,
        _request: Request<GetBalancesRequest>,
    ) -> Result<Response<Balances>, Status> {
        Err(capability_missing("balances"))
    }

    // ---- Health ---- //

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<VenueHealth>, Status> {
        let mut h = health::check_health(&self.client).await;
        if h.status != "down" {
            let last_tick = self.last_tick_ms.load(Ordering::Relaxed);
            apply_tick_health(&mut h, last_tick, chrono::Utc::now().timestamp_millis());
        }
        Ok(Response::new(h))
    }
}

fn capability_missing(capability: &str) -> Status {
    Status::failed_precondition(format!(
        "capability_missing: Hyperliquid pack does not declare '{capability}'"
    ))
}

fn apply_tick_health(health: &mut VenueHealth, last_tick_ms: i64, now_ms: i64) {
    if last_tick_ms <= 0 {
        health.status = "degraded".into();
        health.lag_ms = u64::MAX;
        return;
    }
    health.lag_ms = now_ms.saturating_sub(last_tick_ms) as u64;
    health.status = if health.lag_ms <= 5_000 { "ok" } else { "degraded" }.into();
}

// ---------------------------------------------------------------------------
// Domain -> Proto conversion
// ---------------------------------------------------------------------------

fn domain_market_to_proto(m: Market) -> Option<core_proto::Market> {
    let venue_str = m.venue.as_str().to_string();
    let (venue_id, market_key) = (
        core_proto::VenueId { value: venue_str },
        core_proto::MarketKey { value: m.key.as_str().to_string() },
    );

    let kind = match m.kind {
        aether_core::market::InstrumentKind::Perp => InstrumentKind::Perp as i32,
        aether_core::market::InstrumentKind::Spot => InstrumentKind::Spot as i32,
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

fn utc_time_to_proto(t: UtcTime) -> Timestamp {
    let millis = t.unix_millis();
    Timestamp { seconds: millis / 1000, nanos: ((millis % 1000) * 1_000_000) as i32 }
}

fn parse_hyperliquid_market_key(value: &str) -> Result<&str, &'static str> {
    value
        .strip_prefix("mkt:hyperliquid:")
        .filter(|ticker| !ticker.is_empty() && !ticker.contains(':'))
        .ok_or("market key must be mkt:hyperliquid:{coin}")
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
        source: core_proto::QuoteSource::Poll as i32,
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
// HTTP health endpoints
// ---------------------------------------------------------------------------

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
aether_feed_lag_ms{{venue=\"hyperliquid\"}} {lag}\n"
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
                .unwrap_or_else(|_| "info,aether_venue_hyperliquid=debug".into()),
        )
        .init();

    // -- Client setup (no auth needed) -- //
    let client = client::HlClient::from_env();
    let hl_stream = stream::HlStream::new(client::HlClient::from_env());
    let quarantine_producer = KafkaProducer::from_env()?;
    let quarantine_storage = QuarantineStorage::new_from_env()?;

    // -- gRPC adapter -- //
    let adapter = HlVenueAdapter::new(client, hl_stream, quarantine_producer, quarantine_storage);
    let health_clock = Arc::clone(&adapter.last_tick_ms);
    let grpc_addr = std::env::var("AETHER_VENUE__HYPERLIQUID_GRPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50056".to_string())
        .parse()?;
    let health_http_port: u16 = std::env::var("AETHER_VENUE__HYPERLIQUID_HEALTH_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8086);

    tracing::info!("starting gRPC server on {grpc_addr}");
    tracing::info!("starting HTTP health server on port {health_http_port}");

    let (mut grpc_health_reporter, grpc_health_service) = tonic_health::server::health_reporter();
    grpc_health_reporter.set_serving::<VenueAdapterServer<HlVenueAdapter>>().await;

    let grpc = async move {
        Server::builder()
            .add_service(grpc_health_service)
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
    fn health_lag_is_last_tick_age_not_request_latency() {
        let mut health = VenueHealth { status: "ok".into(), lag_ms: 1, rate_remaining: 120 };
        apply_tick_health(&mut health, 10_000, 11_250);
        assert_eq!(health.status, "ok");
        assert_eq!(health.lag_ms, 1_250);
        assert_eq!(health.rate_remaining, 120);

        apply_tick_health(&mut health, 10_000, 16_001);
        assert_eq!(health.status, "degraded");
        assert_eq!(health.lag_ms, 6_001);
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
        assert!(body.contains("aether_feed_lag_ms{venue=\"hyperliquid\"} 1250"));

        let (_, _, unknown) = health_http_response("GET /metrics HTTP/1.1\r\n", 0, 11_250);
        assert!(unknown.contains("aether_feed_lag_ms{venue=\"hyperliquid\"} NaN"));
    }
}

//! AETHER Terminal -- Kalshi venue adapter binary.
//!
//! Runs a Tonic gRPC server implementing `aether.venue.v1.VenueAdapter` with:
//!
//! - `list_markets` / `get_market`  (M2 -- real Kalshi REST calls)
//! - `stream_ticks` / `stream_book` (authenticated WebSocket streams)
//! - `submit_order` / `cancel_order` (sandbox-only V2 event orders)
//! - `get_balances`                  (sandbox portfolio balance)
//! - `health`                        (implemented)
//! - Plain HTTP `/healthz` and `/readyz` endpoints (for k8s probes)
//!
//! # Usage
//!
//! ```text
//! AETHER_VENUE__KALSHI_KEY_ID=k1_xxx                         \
//! AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH=/path/to/key.pem     \
//! AETHER_VENUE__KALSHI_BASE_URL=https://external-api.demo.kalshi.co \
//! cargo run -p aether-venue-kalshi
//! ```

mod auth;
mod client;
mod health;
mod normalize;
mod orders;
mod stream;

use aether_bus::envelope::Envelope;
use aether_bus::producer::{BreakerProducer, KafkaProducer, MessageProducer, ProducerError};
use aether_bus::quarantine::{Quarantine, QuarantineStorage};
use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::market::{Market, MarketStatus};
use aether_core::order::{OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce};
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

/// Boxed, pinned, Send stream of gRPC results.
type GrpcStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

// ---------------------------------------------------------------------------
// Venue adapter service
// ---------------------------------------------------------------------------

/// gRPC service implementation for Kalshi.
pub struct KalshiVenueAdapter {
    client: client::KalshiClient,
    orders: orders::KalshiOrders,
    stream: Arc<stream::KalshiStream>,
    last_tick_ms: Arc<AtomicI64>,
    quarantine_producer: Arc<BreakerProducer<KafkaProducer>>,
    quarantine_storage: Arc<QuarantineStorage>,
}

impl std::fmt::Debug for KalshiVenueAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("KalshiVenueAdapter").finish_non_exhaustive()
    }
}

impl KalshiVenueAdapter {
    /// Create a new adapter with the given Kalshi REST client and orders handle.
    pub fn new(
        client: client::KalshiClient,
        orders: orders::KalshiOrders,
        stream: stream::KalshiStream,
        quarantine_producer: BreakerProducer<KafkaProducer>,
        quarantine_storage: QuarantineStorage,
    ) -> Self {
        Self {
            client,
            orders,
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
        if topic == "md.ticks.kalshi" {
            self.last_tick_ms.store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
        }
        let value = serde_json::to_value(envelope.payload)
            .map_err(|error| ProducerError::Send(error.to_string()))?;
        self.tx.send(value).await.map_err(|_| ProducerError::Send("gRPC stream closed".to_string()))
    }
}

#[tonic::async_trait]
impl VenueAdapter for KalshiVenueAdapter {
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
        let mut cursor = None;
        loop {
            let page = match self.client.get_markets(100, cursor.as_deref()).await {
                Ok(page) => page,
                Err(error) => {
                    if let Some(raw) = error.raw_payload() {
                        Quarantine::publish(
                            self.quarantine_producer.as_ref(),
                            self.quarantine_storage.as_ref(),
                            "kalshi",
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
            raw_markets.extend(page.markets);
            match page.cursor.filter(|value| !value.is_empty()) {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }

        let (tx, rx) = mpsc::channel(16);
        let quarantine_producer = Arc::clone(&self.quarantine_producer);
        let quarantine_storage = Arc::clone(&self.quarantine_storage);

        tokio::spawn(async move {
            for raw in raw_markets {
                let raw_bytes = serde_json::to_vec(&raw).unwrap_or_default();
                match normalize::normalize_market(raw) {
                    Ok(domain_market) => {
                        if let Some(proto) = domain_market_to_proto(domain_market) {
                            let _ = tx.send(Ok(proto)).await;
                        }
                    }
                    Err(e) => {
                        if let Err(error) = Quarantine::publish(
                            quarantine_producer.as_ref(),
                            quarantine_storage.as_ref(),
                            "kalshi",
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
        request: Request<aether_proto::aether::venue::v1::GetMarketRequest>,
    ) -> Result<Response<core_proto::Market>, Status> {
        let req = request.into_inner();
        let key = req.key.ok_or_else(|| Status::invalid_argument("market key is required"))?;

        let ticker = key
            .value
            .strip_prefix("mkt:kalshi:")
            .filter(|ticker| !ticker.is_empty() && !ticker.contains(':'))
            .ok_or_else(|| Status::invalid_argument("market key must be mkt:kalshi:{ticker}"))?;

        let raw = match self.client.get_market(ticker).await {
            Ok(raw) => raw,
            Err(error) => {
                if let Some(raw) = error.raw_payload() {
                    Quarantine::publish(
                        self.quarantine_producer.as_ref(),
                        self.quarantine_storage.as_ref(),
                        "kalshi",
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

        let raw_bytes = serde_json::to_vec(&raw).unwrap_or_default();
        let domain = match normalize::normalize_market(raw) {
            Ok(domain) => domain,
            Err(error) => {
                Quarantine::publish(
                    self.quarantine_producer.as_ref(),
                    self.quarantine_storage.as_ref(),
                    "kalshi",
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

        let proto = domain_market_to_proto(domain)
            .ok_or_else(|| Status::internal("market conversion failed"))?;

        Ok(Response::new(proto))
    }

    // ---- M4: stub implementations ---- //

    async fn stream_ticks(
        &self,
        request: Request<StreamTicksRequest>,
    ) -> Result<Response<Self::StreamTicksStream>, Status> {
        let mut tickers = Vec::new();
        for key in request.into_inner().keys {
            let ticker = match parse_kalshi_market_key(&key.value) {
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
                .stream_ticks_to_bus(
                    &tickers,
                    &producer,
                    quarantine_producer.as_ref(),
                    quarantine_storage.as_ref(),
                )
                .await
            {
                tracing::warn!(%error, "Kalshi tick stream ended");
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
            parse_kalshi_market_key(&key.value).map_err(Status::invalid_argument)?.to_string();
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
                tracing::warn!(%error, "Kalshi book stream ended");
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

    // ---- M5: order implementations ---- //

    async fn submit_order(
        &self,
        request: Request<core_proto::Order>,
    ) -> Result<Response<OrderAck>, Status> {
        let proto = request.into_inner();

        let intent = proto_order_to_intent(&proto)
            .map_err(|e| Status::invalid_argument(format!("invalid order: {e}")))?;

        let ack = self
            .orders
            .submit_order(&intent)
            .await
            .map_err(|e| Status::internal(format!("submit_order failed: {e}")))?;

        Ok(Response::new(OrderAck { venue_ref: ack.venue_ref, status: ack.status }))
    }

    async fn cancel_order(
        &self,
        request: Request<CancelOrderRequest>,
    ) -> Result<Response<CancelOrderResponse>, Status> {
        let req = request.into_inner();

        self.orders
            .cancel_order(&req.venue_ref)
            .await
            .map_err(|e| Status::internal(format!("cancel_order failed: {e}")))?;

        Ok(Response::new(CancelOrderResponse { cancelled: true }))
    }

    async fn get_balances(
        &self,
        _request: Request<GetBalancesRequest>,
    ) -> Result<Response<Balances>, Status> {
        let domain = self
            .orders
            .get_balances()
            .await
            .map_err(|e| Status::internal(format!("get_balances failed: {e}")))?;

        let balances = domain
            .balances
            .into_iter()
            .map(|b| aether_proto::aether::venue::v1::Balance {
                asset: b.asset,
                free: b.free,
                locked: b.locked,
            })
            .collect();

        Ok(Response::new(Balances { balances }))
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

fn parse_kalshi_market_key(value: &str) -> Result<&str, &'static str> {
    value
        .strip_prefix("mkt:kalshi:")
        .filter(|ticker| !ticker.is_empty() && !ticker.contains(':'))
        .ok_or("market key must be mkt:kalshi:{ticker}")
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
// Proto -> Domain conversion (M5 orders)
// ---------------------------------------------------------------------------

/// Convert a proto `Order` to a domain `OrderIntent`.
///
/// Because the proto `Order` message has fewer fields than `OrderIntent`,
/// some fields are filled with sensible defaults:
///
/// | Field | Source |
/// |---|---|
/// | `id` / `client_order_id` | `proto.order_id` (same ULID) |
/// | `market` | `proto.market` |
/// | `side` | `proto.side` (mapped via enum ordinal) |
/// | `order_type` | `Limit` (Kalshi only supports limit) |
/// | `limit_price` | `proto.price` (decimal string) |
/// | `size` | `proto.size` (decimal string) |
/// | `size_unit` | `Contracts` |
/// | `tif` | `Day` |
/// | `paper` | `proto.paper` |
/// | `origin` | `Agent` tier 3, anonymous |
/// | `quote_snapshot` | empty default |
/// | `caps_version` | fresh `Ulid` |
/// | `created_ts` | `now()` |
fn proto_order_to_intent(proto: &core_proto::Order) -> Result<OrderIntent, String> {
    let ulid = proto
        .order_id
        .as_ref()
        .ok_or_else(|| "order_id is required for idempotency".to_string())
        .and_then(|u| Ulid::from_string(&u.value).map_err(|e| e.to_string()))?;

    let market = proto
        .market
        .as_ref()
        .map(|m| {
            let ticker = m
                .value
                .strip_prefix("mkt:kalshi:")
                .filter(|ticker| !ticker.is_empty() && !ticker.contains(':'))
                .ok_or_else(|| "market key must be mkt:kalshi:{ticker}".to_string())?;
            MarketKey::new(&VenueId::new("kalshi").map_err(|e| e.to_string())?, ticker)
                .map_err(|e| e.to_string())
        })
        .unwrap_or_else(|| Err("market key is required".to_string()))?;

    let side = match proto.side {
        1 => Side::Buy,
        2 => Side::Sell,
        3 => Side::BuyNo,
        4 => Side::SellNo,
        v => return Err(format!("unknown side value: {v}")),
    };

    let limit_price = if proto.price.is_empty() {
        None
    } else {
        Some(
            proto
                .price
                .parse::<rust_decimal::Decimal>()
                .map_err(|e| format!("invalid price '{price}': {e}", price = proto.price))?,
        )
    };

    let size = proto
        .size
        .parse::<rust_decimal::Decimal>()
        .map_err(|e| format!("invalid size '{size}': {e}", size = proto.size))?;

    Ok(OrderIntent {
        id: ulid,
        market: market.clone(),
        side,
        order_type: OrderType::Limit,
        limit_price,
        size,
        size_unit: SizeUnit::Contracts,
        tif: TimeInForce::Day,
        paper: proto.paper,
        origin: Origin::new(OriginKind::Agent, 3, Ulid::new())
            .map_err(|e| format!("invalid origin: {e}"))?,
        quote_snapshot: Quote {
            market: market.clone(),
            bid: None,
            ask: None,
            mid: None,
            last: None,
            bid_size: None,
            ask_size: None,
            ts: UtcTime::now(),
            source: aether_core::quote::QuoteSource::Poll,
            seq: None,
        },
        caps_version: Ulid::new(),
        created_ts: UtcTime::now(),
    })
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
aether_feed_lag_ms{{venue=\"kalshi\"}} {lag}\n"
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
                .unwrap_or_else(|_| "info,aether_venue_kalshi=debug".into()),
        )
        .init();

    // -- Auth & client setup -- //
    let auth = auth::KalshiAuth::from_env()?;
    let client = client::KalshiClient::from_env(auth);

    // -- Orders client (separate KalshiClient; reqwest::Client is pooled) -- //
    let orders_auth = auth::KalshiAuth::from_env()?;
    let orders_client = client::KalshiClient::from_env(orders_auth);
    let orders = orders::KalshiOrders::new(orders_client);

    let stream_auth = auth::KalshiAuth::from_env()?;
    let kalshi_stream = stream::KalshiStream::from_env(stream_auth);
    let quarantine_producer = KafkaProducer::from_env()?;
    let quarantine_storage = QuarantineStorage::new_from_env()?;

    // -- gRPC adapter -- //
    let adapter = KalshiVenueAdapter::new(
        client,
        orders,
        kalshi_stream,
        quarantine_producer,
        quarantine_storage,
    );
    let health_clock = Arc::clone(&adapter.last_tick_ms);
    let grpc_addr = std::env::var("AETHER_VENUE__KALSHI_GRPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50054".to_string())
        .parse()?;
    let health_http_port: u16 = std::env::var("AETHER_VENUE__KALSHI_HEALTH_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8084);

    tracing::info!("starting gRPC server on {grpc_addr}");
    tracing::info!("starting HTTP health server on port {health_http_port}");

    // Run both servers concurrently
    let grpc = async move {
        Server::builder()
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
        assert!(body.contains("aether_feed_lag_ms{venue=\"kalshi\"} 1250"));

        let (_, _, unknown) = health_http_response("GET /metrics HTTP/1.1\r\n", 0, 11_250);
        assert!(unknown.contains("aether_feed_lag_ms{venue=\"kalshi\"} NaN"));
    }
}

//! AETHER Gateway binary — WebSocket server entry point.
//!
//! All core logic lives in the library crate; this binary
//! only initializes the DB pool and starts the server.

// Binary entry point: startup failures should crash immediately.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;

use aether_bus::consumer::{KafkaConsumer, MessageConsumer};
use aether_bus::producer::{BreakerProducer, KafkaProducer};
use aether_bus::topics::Topic;
use aether_core::opportunity::Opportunity;
use aether_gateway::{auth, AppState};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Initialize Postgres connection pool
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aether:aether@localhost:5432/aether".into());
    let pool = auth::init_db_pool(&database_url);
    let state = AppState::new(pool);

    if std::env::var("AETHER_GATEWAY_BUS_ENABLED").as_deref() == Ok("1") {
        let feed_state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = consume_opportunities(feed_state).await {
                tracing::error!(%error, "gateway opportunity consumer stopped");
            }
        });
    }

    let app = aether_gateway::build_router(state);

    let bind = std::env::var("AETHER_GATEWAY__BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let addr: SocketAddr =
        bind.parse().unwrap_or_else(|_| panic!("invalid AETHER_GATEWAY__BIND value: {bind}"));
    tracing::info!("Gateway listening on {addr}");
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to bind TCP listener on {addr}: {e}");
            panic!("TCP bind failed: {e}");
        }
    };
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("axum serve exited with error: {e}");
    }
}

type ProductionConsumer = KafkaConsumer<BreakerProducer<KafkaProducer>>;

async fn consume_opportunities(
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let consumer: ProductionConsumer = KafkaConsumer::from_env("gateway-opportunities")?;
    loop {
        let envelopes = consumer.consume::<Opportunity>(&[Topic::OPPS_DETECTED]).await?;
        for envelope in envelopes {
            aether_gateway::feed::surface_opportunity(
                &state.pool,
                &state.broadcast_tx,
                &envelope.payload,
            )
            .await?;
        }
        consumer.ack()?;
        consumer.commit_sync()?;
    }
}

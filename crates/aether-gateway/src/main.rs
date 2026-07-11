//! AETHER Gateway binary — WebSocket server entry point.
//!
//! All core logic lives in the library crate; this binary
//! only initializes the DB pool and starts the server.

// Binary entry point: startup failures should crash immediately.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;

use aether_gateway::{auth, AppState};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Initialize Postgres connection pool
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aether:aether@localhost:5432/aether".into());
    let pool = auth::init_db_pool(&database_url);
    let state = AppState { pool };

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

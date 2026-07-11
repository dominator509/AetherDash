//! AETHER Gateway — WebSocket service, session auth, health probes.
//!
//! This library crate is the core of the gateway binary.
//! It is split out from `main.rs` so integration tests (in `tests/`)
//! can import types and functions like `AppState`, `build_router`, and `auth::*`.

use std::collections::HashMap;

use aether_core::{ErrorCode, ErrorEnvelope, Ulid};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use sqlx::PgPool;

pub mod auth;
pub mod frames;
pub mod health;

/// Shared application state available to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

/// Build the Axum router with all routes and shared state.
/// Exposed for integration tests to mount on a random port.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/metrics", get(health::metrics))
        .with_state(state)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<AppState>,
) -> axum::response::Response {
    let token = params.get("token").map(|s| s.as_str());
    match auth::validate_token(Some(&state.pool), token).await {
        Ok(session) => ws.on_upgrade(move |socket| handle_socket(socket, session)),
        Err(e) => {
            tracing::warn!("WS upgrade rejected: {e}");
            (axum::http::StatusCode::UNAUTHORIZED, e.to_string()).into_response()
        }
    }
}

async fn handle_socket(mut socket: WebSocket, session: auth::SessionInfo) {
    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<frames::ClientFrame>(&text) {
                Ok(frame) => {
                    let response = frames::dispatch(frame, &session);
                    let json = match serde_json::to_string(&response) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::error!("Failed to serialize frame: {e}");
                            break;
                        }
                    };
                    if let Err(e) = socket.send(Message::Text(json)).await {
                        tracing::error!("Failed to send WS frame: {e}");
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(%e, "failed to deserialize frame");
                    health::UNKNOWN_FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let error_frame = frames::ServerFrame::Error {
                        id: None,
                        trace_id: Some(uuid::Uuid::new_v4().to_string()),
                        body: ErrorEnvelope::new(
                            ErrorCode::InvalidArgument,
                            "failed to parse frame",
                            Ulid::new(),
                        ),
                    };
                    let json = match serde_json::to_string(&error_frame) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::error!("Failed to serialize error frame: {e}");
                            break;
                        }
                    };
                    if let Err(e) = socket.send(Message::Text(json)).await {
                        tracing::error!("Failed to send WS error frame: {e}");
                        break;
                    }
                }
            }
        }
    }
}

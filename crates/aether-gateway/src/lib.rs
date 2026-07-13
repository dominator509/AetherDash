//! AETHER Gateway — WebSocket service, session auth, health probes.
//!
//! This library crate is the core of the gateway binary.
//! It is split out from `main.rs` so integration tests (in `tests/`)
//! can import types and functions like `AppState`, `build_router`, and `auth::*`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use aether_core::{ErrorCode, ErrorEnvelope, Ulid};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use sqlx::PgPool;
use tokio::sync::broadcast;

pub mod auth;
pub mod frames;
pub mod health;

/// Shared application state available to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Broadcast channel for server-pushed messages.
    /// Subscribe/unsubscribe manage per-client receivers.
    /// TODO(EP-101): actual feed population — currently infrastructure only.
    pub broadcast_tx: broadcast::Sender<String>,
}

impl AppState {
    /// Create a new AppState with a DB pool and a fresh broadcast channel.
    pub fn new(pool: PgPool) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self { pool, broadcast_tx }
    }
}

/// Build the Axum router with all routes and shared state.
/// Exposed for integration tests to mount on a random port.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/auth/validate", post(auth::validate_handler))
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
    let state_clone = state.clone();
    match auth::validate_token(Some(&state.pool), token).await {
        Ok(session) => ws.on_upgrade(move |socket| handle_socket(socket, state_clone, session)),
        Err(e) => {
            tracing::warn!("WS upgrade rejected: {e}");
            (axum::http::StatusCode::UNAUTHORIZED, e.to_string()).into_response()
        }
    }
}

/// Connection-scoped guard that decrements the global connection counter on drop.
struct ConnectionGuard;

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        crate::health::CONNECTION_COUNT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Handle an established WebSocket session.
///
/// Manages per-client broadcast subscription alongside frame dispatch.
/// Uses `tokio::select!` to concurrently read incoming frames and
/// receive broadcast pushes when subscribed.
async fn handle_socket(mut socket: WebSocket, state: AppState, session: auth::SessionInfo) {
    health::CONNECTION_COUNT.fetch_add(1, Ordering::Relaxed);
    let _guard = ConnectionGuard;

    // Receiver from the shared broadcast channel. Cloned on subscribe.
    let mut broadcast_rx = state.broadcast_tx.subscribe();
    let mut subscribed = false;

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<frames::ClientFrame>(&text) {
                            Ok(frame) => {
                                // Manage subscription state for broadcast channel.
                                // Match guards avoid inner `if` blocks.
                                match &frame {
                                    frames::ClientFrame::Subscribe { .. } if !subscribed => {
                                        subscribed = true;
                                        broadcast_rx = state.broadcast_tx.subscribe();
                                        health::SUBSCRIPTION_COUNT.fetch_add(1, Ordering::Relaxed);
                                    }
                                    frames::ClientFrame::Unsubscribe { .. } if subscribed => {
                                        subscribed = false;
                                        health::SUBSCRIPTION_COUNT.fetch_sub(1, Ordering::Relaxed);
                                    }
                                    _ => {}
                                }

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
                                health::UNKNOWN_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
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
                    Some(Ok(_)) => {
                        // Ignore non-text messages (Close, Ping, Pong, Binary)
                        // — tungstenite handles ping/pong at the transport layer.
                    }
                    _ => break,
                }
            }
            broadcast_msg = broadcast_rx.recv(), if subscribed => {
                match broadcast_msg {
                    Ok(msg) => {
                        if let Err(e) = socket.send(Message::Text(msg)).await {
                            tracing::error!("Failed to send broadcast frame: {e}");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // TODO(EP-101): track dropped messages metric
                        tracing::warn!("broadcast receiver lagged, dropped {n} messages");
                        continue;
                    }
                }
            }
        }
    }

    // Clean up subscription if still active.
    if subscribed {
        health::SUBSCRIPTION_COUNT.fetch_sub(1, Ordering::Relaxed);
    }
}

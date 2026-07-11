use std::collections::HashMap;
use std::net::SocketAddr;

use aether_core::{ErrorCode, ErrorEnvelope, Ulid};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::Query,
    response::IntoResponse,
    routing::get,
    Router,
};

mod auth;
mod frames;
mod health;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/metrics", get(health::metrics));

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

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
) -> axum::response::Response {
    let token = params.get("token").map(|s| s.as_str());
    match auth::validate_token(token) {
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
                            tracing::error!("Failed to serialize response: {e}");
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

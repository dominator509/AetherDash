//! AETHER Gateway — WebSocket service, session auth, health probes.
//!
//! This library crate is the core of the gateway binary.
//! It is split out from `main.rs` so integration tests (in `tests/`)
//! can import types and functions like `AppState`, `build_router`, and `auth::*`.

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use aether_core::{ErrorCode, ErrorEnvelope, Ulid};
use axum::{
    body::{to_bytes, Body},
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{header, HeaderMap, Request, StatusCode},
    response::IntoResponse,
    routing::{any, get, post},
    Router,
};
use futures_util::StreamExt;
use reqwest::Url;
use sqlx::PgPool;
use tokio::sync::broadcast;

pub mod auth;
pub mod feed;
pub mod frames;
pub mod health;

/// Shared application state available to all handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Broadcast channel for server-pushed messages.
    /// Subscribe/unsubscribe manage per-client receivers.
    /// Populated from durable `opps.detected` events by the gateway binary.
    pub broadcast_tx: broadcast::Sender<String>,
    mcp_client: reqwest::Client,
    mcp_base_url: Url,
}

impl AppState {
    /// Create a new AppState with a DB pool and a fresh broadcast channel.
    pub fn new(pool: PgPool) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self {
            pool,
            broadcast_tx,
            mcp_client: reqwest::Client::new(),
            mcp_base_url: match Url::parse("http://127.0.0.1:8000/") {
                Ok(url) => url,
                Err(_) => unreachable!("the built-in MCP URL is valid"),
            },
        }
    }

    /// Override the loopback-only MCP upstream used by the control-plane proxy.
    pub fn with_mcp_base_url(mut self, raw: &str) -> Result<Self, String> {
        self.mcp_base_url = validate_mcp_base_url(raw)?;
        Ok(self)
    }
}

fn validate_mcp_base_url(raw: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw).map_err(|_| "AETHER_MCP__URL must be a valid URL")?;
    if url.scheme() != "http" {
        return Err("AETHER_MCP__URL must use http on loopback".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("AETHER_MCP__URL must not contain credentials".into());
    }
    match url.host_str() {
        Some("127.0.0.1" | "localhost" | "::1") => {}
        _ => return Err("AETHER_MCP__URL must target loopback".into()),
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err("AETHER_MCP__URL must not contain a path".into());
    }
    url.set_path("/");
    Ok(url)
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
        .route("/mcp/*path", any(proxy_mcp))
        .with_state(state)
}

const MAX_MCP_REQUEST_BYTES: usize = 1_048_576;

/// Forward the authenticated MCP control plane to its loopback-only service.
/// Only the headers required by the MCP contract cross the process boundary.
async fn proxy_mcp(
    State(state): State<AppState>,
    Path(path): Path<String>,
    request: Request<Body>,
) -> axum::response::Response {
    let (parts, body) = request.into_parts();
    if parts.method != axum::http::Method::GET && parts.method != axum::http::Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    // Mutate only the path of the prevalidated loopback origin. URL joining
    // would let a path beginning with `//` replace the authority (SSRF).
    let mut upstream = state.mcp_base_url.clone();
    upstream.set_path(&format!("/{}", path.trim_start_matches('/')));
    upstream.set_query(parts.uri.query());

    let bytes = match to_bytes(body, MAX_MCP_REQUEST_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "MCP request body too large").into_response()
        }
    };
    let mut builder = state.mcp_client.request(parts.method, upstream);
    for name in [header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT] {
        if let Some(value) = parts.headers.get(&name) {
            builder = builder.header(name, value);
        }
    }
    let response = match builder.body(bytes).send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error_class = %std::any::type_name_of_val(&error), "MCP upstream unavailable");
            return (StatusCode::BAD_GATEWAY, "MCP service unavailable").into_response();
        }
    };

    let status = response.status();
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let stream = response.bytes_stream().map(|chunk| chunk.map_err(std::io::Error::other));
    let mut outbound = axum::response::Response::new(Body::from_stream(stream));
    *outbound.status_mut() = status;
    if let Some(value) = content_type {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, value);
        *outbound.headers_mut() = headers;
    }
    outbound
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::items_after_test_module)]
mod mcp_proxy_tests {
    use std::{convert::Infallible, time::Duration};

    use axum::{
        body::{Body, Bytes},
        http::{header, HeaderMap},
        response::Response,
        routing::post,
        Router,
    };
    use futures_util::{stream, StreamExt};
    use sqlx::postgres::PgPoolOptions;

    use super::{build_router, validate_mcp_base_url, AppState};

    #[test]
    fn mcp_upstream_is_loopback_only() {
        assert!(validate_mcp_base_url("http://127.0.0.1:8000").is_ok());
        assert!(validate_mcp_base_url("http://localhost:8000/").is_ok());
        assert!(validate_mcp_base_url("https://127.0.0.1:8000").is_err());
        assert!(validate_mcp_base_url("http://example.com:8000").is_err());
        assert!(validate_mcp_base_url("http://user:pass@127.0.0.1:8000").is_err());
        assert!(validate_mcp_base_url("http://127.0.0.1:8000/admin").is_err());
    }

    async fn streaming_upstream(headers: HeaderMap) -> Response<Body> {
        assert_eq!(
            headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()),
            Some("Bearer opaque")
        );
        let chunks = stream::unfold(0_u8, |state| async move {
            match state {
                0 => Some((Ok::<_, Infallible>(Bytes::from_static(b"first\n")), 1)),
                1 => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Some((Ok(Bytes::from_static(b"second\n")), 2))
                }
                _ => None,
            }
        });
        let mut response = Response::new(Body::from_stream(chunks));
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            "application/x-ndjson".parse().expect("static content type is valid"),
        );
        response
    }

    #[tokio::test]
    async fn proxy_preserves_auth_status_content_type_and_streaming() {
        let upstream_listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind upstream");
        let upstream_addr = upstream_listener.local_addr().expect("upstream address");
        tokio::spawn(async move {
            axum::serve(
                upstream_listener,
                Router::new().route("/tools/swarm.launch/stream", post(streaming_upstream)),
            )
            .await
            .expect("serve upstream");
        });

        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://aether:aether@127.0.0.1:5432/aether")
            .expect("lazy pool URL");
        let state = AppState::new(pool)
            .with_mcp_base_url(&format!("http://{upstream_addr}"))
            .expect("loopback upstream");
        let gateway_listener =
            tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind gateway");
        let gateway_addr = gateway_listener.local_addr().expect("gateway address");
        tokio::spawn(async move {
            axum::serve(gateway_listener, build_router(state)).await.expect("serve gateway");
        });

        let response = reqwest::Client::new()
            .post(format!("http://{gateway_addr}/mcp/tools/swarm.launch/stream"))
            .header(header::AUTHORIZATION, "Bearer opaque")
            .body("{}")
            .send()
            .await
            .expect("proxy request");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()),
            Some("application/x-ndjson")
        );
        let mut chunks = response.bytes_stream();
        let first = tokio::time::timeout(Duration::from_millis(50), chunks.next())
            .await
            .expect("first chunk was buffered")
            .expect("first chunk exists")
            .expect("first chunk is valid");
        assert_eq!(first, Bytes::from_static(b"first\n"));
        let second = tokio::time::timeout(Duration::from_secs(1), chunks.next())
            .await
            .expect("second chunk timed out")
            .expect("second chunk exists")
            .expect("second chunk is valid");
        assert_eq!(second, Bytes::from_static(b"second\n"));
    }
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
    let mut connection_auth = frames::ConnectionAuthState::default();

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

                                let response = frames::dispatch_with_state(
                                    frame,
                                    &session,
                                    &mut connection_auth,
                                );
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

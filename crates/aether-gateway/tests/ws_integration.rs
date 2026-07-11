//! EP-004 M6: WebSocket integration test.
//! Starts the gateway on a random port, opens a WebSocket with a valid test token,
//! and verifies frame round-trips.
//!
//! Requires: AETHER_INTEGRATION_TEST=1 set in the environment.
//! Run: cargo test -p aether-gateway --test ws_integration -- --ignored
//!
//! Does NOT require a running Postgres because test tokens
//! are validated without DB lookup in debug builds.

// Integration tests: setup failures should panic immediately.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::env;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// Convenience alias for the WebSocket stream type used in tests.
type WsStream = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Start the gateway on a random port and return the bound address.
async fn start_gateway() -> String {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aether:aether@localhost:5432/aether".into());
    let pool = aether_gateway::auth::init_db_pool(&database_url);
    let state = aether_gateway::AppState { pool };
    let app = aether_gateway::build_router(state);

    let listener =
        tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("failed to bind to random port");
    let addr = listener.local_addr().unwrap();
    let addr_str = format!("127.0.0.1:{}", addr.port());

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    addr_str
}

/// Receive the next text frame from the WebSocket with a short timeout.
async fn recv_frame(socket: &mut WsStream) -> String {
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
        .await
        .expect("timeout waiting for WS frame")
        .expect("WS stream ended unexpectedly")
        .expect("WS frame error");

    match msg {
        Message::Text(text) => text.to_string(),
        other => panic!("expected text frame, got: {other:?}"),
    }
}

/// Send a JSON text frame.
async fn send_frame(socket: &mut WsStream, json: &str) {
    socket.send(Message::Text(json.to_string())).await.unwrap();
}

#[test]
#[ignore]
fn ws_ping_pong() {
    if env::var("AETHER_INTEGRATION_TEST").as_deref() != Ok("1") {
        eprintln!("skipping: set AETHER_INTEGRATION_TEST=1 to run");
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let addr = start_gateway().await;
        let ws_url = format!("ws://{addr}/ws?token=test-integration");

        // Connect WebSocket
        let (mut socket, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .expect("failed to connect to gateway WS");

        // --- Ping -> Pong ---
        let ping = r#"{"type":"ping","id":"p1"}"#;
        send_frame(&mut socket, ping).await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"pong\""), "expected pong, got: {response}");
        assert!(response.contains("\"id\":\"p1\""));

        // --- Subscribe -> command_result ---
        let subscribe = r#"{"type":"subscribe","id":"s1","channels":["quotes:mkt:kalshi:BTC-75"]}"#;
        send_frame(&mut socket, subscribe).await;
        let response = recv_frame(&mut socket).await;
        assert!(
            response.contains("\"command_result\""),
            "expected command_result, got: {response}"
        );
        assert!(response.contains("subscribed"));
        assert!(response.contains("actor_id"));

        // --- Order intent -> confirm_required ---
        let order_intent =
            r#"{"type":"order_intent","id":"o1","body":{"market":"BTC-USD","side":"buy"}}"#;
        send_frame(&mut socket, order_intent).await;
        let response = recv_frame(&mut socket).await;
        assert!(
            response.contains("\"confirm_required\""),
            "expected confirm_required, got: {response}"
        );
        assert!(response.contains("ref_id"));
        assert!(response.contains("actor_id"));
        assert!(response.contains("origin_kind"));

        eprintln!("WS integration test PASSED (3 frame types verified)");
    });
}

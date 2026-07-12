//! EP-004 M6: WebSocket integration test.
//! Starts the gateway on a random port, opens a WebSocket with a valid test token,
//! and verifies ALL 6 client frame types, unknown frames, subscribe/unsubscribe,
//! ping/pong, and order_intent flow.
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
    let state = aether_gateway::AppState::new(pool);
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

/// Connect to the gateway WebSocket with the given token.
async fn connect_ws(addr: &str, token: &str) -> WsStream {
    let ws_url = format!("ws://{addr}/ws?token={token}");
    let (socket, _) =
        tokio_tungstenite::connect_async(&ws_url).await.expect("failed to connect to gateway WS");
    socket
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
fn ws_protocol_all_frames() {
    if env::var("AETHER_INTEGRATION_TEST").as_deref() != Ok("1") {
        eprintln!("skipping: set AETHER_INTEGRATION_TEST=1 to run");
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let addr = start_gateway().await;
        // Use a test token whose suffix is a valid ULID so the order_intent
        // dispatch can extract a valid actor ULID from the session.
        let mut socket = connect_ws(&addr, "test-01ARZ3NDEKTSV4RRFFQ69G5FAV").await;

        // ── 1. Ping → Pong ────────────────────────────────────────────
        send_frame(&mut socket, r#"{"type":"ping","id":"p1"}"#).await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"pong\""), "expected pong, got: {response}");
        assert!(response.contains("\"id\":\"p1\""));
        eprintln!("  [ok] ping → pong");

        // ── 2. Subscribe → command_result ─────────────────────────────
        send_frame(
            &mut socket,
            r#"{"type":"subscribe","id":"s1","channels":["quotes:mkt:kalshi:BTC-75"]}"#,
        )
        .await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"command_result\""), "expected command_result, got: {response}");
        assert!(response.contains("subscribed"));
        assert!(response.contains("actor_id"));
        eprintln!("  [ok] subscribe → command_result (subscribed)");

        // ── 3. Unsubscribe → command_result ───────────────────────────
        send_frame(&mut socket, r#"{"type":"unsubscribe","id":"u1"}"#).await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"command_result\""), "expected command_result, got: {response}");
        assert!(response.contains("unsubscribed"));
        eprintln!("  [ok] unsubscribe → command_result (unsubscribed)");

        // ── 4. Subscribe again (idempotent) ───────────────────────────
        send_frame(
            &mut socket,
            r#"{"type":"subscribe","id":"s2","channels":["alerts","system"]}"#,
        )
        .await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"command_result\""), "expected command_result, got: {response}");
        assert!(response.contains("subscribed"));
        assert!(response.contains("actor_id"));
        eprintln!("  [ok] subscribe (again) → command_result");

        // ── 5. Command → command_result with echo ─────────────────────
        send_frame(&mut socket, r#"{"type":"command","id":"c1","text":"hello"}"#).await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"command_result\""), "expected command_result, got: {response}");
        assert!(response.contains("hello"), "echo missing in: {response}");
        assert!(response.contains("actor_id"));
        eprintln!("  [ok] command → command_result with echo");

        // ── 6. Order intent → confirm_required ────────────────────────
        let valid_body = r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAA","market":"mkt:kalshi:BTC-75","side":"buy","order_type":"limit","size":"0.01","size_unit":"contracts","tif":"gtc","limit_price":"65000.00","quote_snapshot":{"market":"mkt:kalshi:BTC-75","bid":"0.65","ask":"0.67","mid":"0.66","ts":"2026-07-10T12:34:56.789Z","source":"snapshot"},"caps_version":"01ARZ3NDEKTSV4RRFFQ69G5FAV","created_ts":"2026-07-10T12:34:56.789Z"}"#;
        let order_intent = format!(r#"{{"type":"order_intent","id":"o1","body":{valid_body}}}"#);
        send_frame(&mut socket, &order_intent).await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"confirm_required\""), "expected confirm_required, got: {response}");
        assert!(response.contains("ref_id"), "missing ref_id: {response}");
        assert!(response.contains("actor_id"), "missing actor_id: {response}");
        assert!(response.contains("origin_kind"), "missing origin_kind: {response}");
        eprintln!("  [ok] order_intent → confirm_required");

        // ── 7. Confirm → command_result ───────────────────────────────
        send_frame(
            &mut socket,
            r#"{"type":"confirm","id":"cf1","ref_id":"abc-123","totp":"654321"}"#,
        )
        .await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"command_result\""), "expected command_result, got: {response}");
        assert!(response.contains("confirmed"));
        assert!(response.contains("totp-provided"));
        eprintln!("  [ok] confirm → command_result (confirmed)");

        // ── 8. Unknown frame → error (not disconnect) ─────────────────
        send_frame(&mut socket, r#"{"type":"bad_frame","x":1}"#).await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"error\""), "expected error frame, got: {response}");
        assert!(response.contains("invalid_argument"), "expected invalid_argument, got: {response}");
        // Socket should still be alive after error
        eprintln!("  [ok] unknown frame → error (socket stays alive)");

        // ── 9. Verify socket is still alive after all tests ───────────
        send_frame(&mut socket, r#"{"type":"ping","id":"final-ping"}"#).await;
        let response = recv_frame(&mut socket).await;
        assert!(response.contains("\"pong\""), "socket dead after error frame: {response}");
        eprintln!("  [ok] socket alive after all frame tests");

        eprintln!("WS integration test PASSED (9 scenarios, 6 client frame types)");
    });
}

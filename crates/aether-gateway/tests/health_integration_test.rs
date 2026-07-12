//! EP-004 M7: Gateway health integration test.
//! Verifies: /healthz returns ok, /readyz flips with Postgres up/down,
//! /metrics reports counters.
//!
//! Requires: AETHER_INTEGRATION_TEST=1 set in the environment, Postgres stack running.
//! Run: cargo test -p aether-gateway --test health_integration_test -- --ignored

// Integration tests: setup failures should panic immediately.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::env;

/// Start the gateway on a random port with the default DATABASE_URL.
async fn start_gateway() -> String {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://aether:aether@localhost:5432/aether".into());
    start_gateway_with_url(&database_url).await
}

/// Start the gateway on a random port with a custom DB URL.
async fn start_gateway_with_url(database_url: &str) -> String {
    let pool = aether_gateway::auth::init_db_pool(database_url);
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

#[tokio::test]
#[ignore]
async fn gateway_health_endpoints() {
    if env::var("AETHER_INTEGRATION_TEST").as_deref() != Ok("1") {
        eprintln!("skipping: set AETHER_INTEGRATION_TEST=1 to run");
        return;
    }

    let client = reqwest::Client::new();

    // ── healthy gateway ─────────────────────────────────────────────────
    let addr = start_gateway().await;
    let base_url = format!("http://{addr}");

    // Brief pause for server startup.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // ── /healthz ────────────────────────────────────────────────────
    let resp =
        client.get(format!("{base_url}/healthz")).send().await.expect("failed to GET /healthz");
    assert_eq!(resp.status(), 200, "/healthz should return 200");
    let body = resp.text().await.unwrap();
    assert!(body.contains("\"status\":\"ok\""), "expected ok status: {body}");
    assert!(body.contains("\"service\":\"gateway\""), "expected gateway service: {body}");
    eprintln!("  [ok] /healthz returns ok");

    // ── /readyz (healthy DB) ────────────────────────────────────────
    let resp =
        client.get(format!("{base_url}/readyz")).send().await.expect("failed to GET /readyz");
    assert_eq!(resp.status(), 200, "/readyz should return 200 when DB is reachable");
    let body = resp.text().await.unwrap();
    assert!(body.contains("\"status\":\"ok\""), "expected ok status: {body}");
    assert!(body.contains("\"service\":\"gateway\""), "expected gateway service: {body}");
    assert!(body.contains("\"database\":\"ok\""), "expected database ok: {body}");
    eprintln!("  [ok] /readyz returns 200 (DB reachable)");

    // ── /readyz (unhealthy DB) ──────────────────────────────────────
    // Start a separate gateway instance with a bad DB URL.
    let bad_addr =
        start_gateway_with_url("postgres://aether:aether@localhost:5433/nonexistent").await;
    let bad_base_url = format!("http://{bad_addr}");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let resp = client
        .get(format!("{bad_base_url}/readyz"))
        .send()
        .await
        .expect("failed to GET /readyz (bad DB)");
    assert_eq!(resp.status(), 503, "/readyz should return 503 when DB is unreachable");
    let body = resp.text().await.unwrap();
    assert!(body.contains("\"status\":\"degraded\""), "expected degraded status: {body}");
    assert!(body.contains("\"database\":\"unreachable\""), "expected database unreachable: {body}");
    eprintln!("  [ok] /readyz returns 503 (DB unreachable)");

    // ── /metrics ────────────────────────────────────────────────────
    let resp =
        client.get(format!("{base_url}/metrics")).send().await.expect("failed to GET /metrics");
    assert_eq!(resp.status(), 200, "/metrics should return 200");
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("gateway_connections_total"),
        "metrics missing gateway_connections_total:\n{body}"
    );
    assert!(
        body.contains("gateway_subscriptions_total"),
        "metrics missing gateway_subscriptions_total:\n{body}"
    );
    assert!(
        body.contains("gateway_unknown_frames_total"),
        "metrics missing gateway_unknown_frames_total:\n{body}"
    );
    eprintln!("  [ok] /metrics reports all expected metrics");

    eprintln!("Health integration test PASSED (4 endpoints verified)");
}

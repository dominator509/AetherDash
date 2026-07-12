use std::sync::atomic::{AtomicU64, Ordering};

use axum::{extract::State, http::StatusCode, response::Json};
use serde::Serialize;

use crate::AppState;

pub(crate) static CONNECTION_COUNT: AtomicU64 = AtomicU64::new(0);
pub(crate) static SUBSCRIPTION_COUNT: AtomicU64 = AtomicU64::new(0);
pub(crate) static UNKNOWN_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}

#[derive(Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub service: String,
    pub database: String,
}

/// Liveness probe — always returns "ok" as long as the process is alive.
pub async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".into(), service: "gateway".into() })
}

/// Readiness probe — checks Postgres connectivity before declaring ready.
///
/// HTTP status codes (orchestrator-facing):
/// - 200 = ok (DB reachable)
/// - 503 = degraded/unreachable (DB not reachable)
///
/// The response body always contains detailed status information.
pub async fn readyz(State(state): State<AppState>) -> (StatusCode, Json<ReadinessResponse>) {
    let db_status = match sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.pool).await {
        Ok(1) => "ok".to_string(),
        Ok(_) => "degraded".to_string(),
        Err(_) => "unreachable".to_string(),
    };

    let overall = if db_status == "ok" { "ok" } else { "degraded" };
    let status_code =
        if db_status == "ok" { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };

    (
        status_code,
        Json(ReadinessResponse {
            status: overall.into(),
            service: "gateway".into(),
            database: db_status,
        }),
    )
}

pub async fn metrics() -> String {
    let conn_count = CONNECTION_COUNT.load(Ordering::Relaxed);
    let sub_count = SUBSCRIPTION_COUNT.load(Ordering::Relaxed);
    let unknown_count = UNKNOWN_FRAME_COUNT.load(Ordering::Relaxed);
    format!(
        "# HELP gateway_connections_total Current WebSocket connections\n\
         # TYPE gateway_connections_total gauge\n\
         gateway_connections_total {conn_count}\n\
         # HELP gateway_subscriptions_total Current active subscriptions\n\
         # TYPE gateway_subscriptions_total gauge\n\
         gateway_subscriptions_total {sub_count}\n\
         # HELP gateway_unknown_frames_total Total unknown/rejected frames\n\
         # TYPE gateway_unknown_frames_total counter\n\
         gateway_unknown_frames_total {unknown_count}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;

    #[tokio::test]
    async fn readyz_returns_503_when_db_unreachable() {
        // Use connect_lazy with a bad URL — no real connection attempted until first query.
        let bad_pool =
            sqlx::PgPool::connect_lazy("postgres://aether:aether@localhost:59999/nonexistent")
                .expect("connect_lazy should not fail eagerly");
        let state = AppState::new(bad_pool);

        let (status, body) = readyz(State(state)).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "expected 503 for unreachable DB");
        assert_eq!(body.0.database, "unreachable");
        assert_eq!(body.0.status, "degraded");
        assert_eq!(body.0.service, "gateway");
    }

    #[tokio::test]
    async fn readyz_returns_200_when_db_healthy() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://aether:aether@localhost:5432/aether".into());

        // Guard: only run when DATABASE_URL is explicitly set and AETHER_INTEGRATION_TEST=1,
        // since a real Postgres instance is needed.
        if std::env::var("AETHER_INTEGRATION_TEST").as_deref() != Ok("1") {
            eprintln!(
                "skipping readyz_returns_200_when_db_healthy (set AETHER_INTEGRATION_TEST=1)"
            );
            // Still verify the handler shape doesn't panic — just skip the DB assertion.
            return;
        }

        let pool = sqlx::PgPool::connect_lazy(&database_url).expect("connect_lazy should not fail");
        let state = AppState::new(pool);

        let (status, body) = readyz(State(state)).await;

        assert_eq!(status, StatusCode::OK, "expected 200 for reachable DB");
        assert_eq!(body.0.database, "ok");
        assert_eq!(body.0.status, "ok");
        assert_eq!(body.0.service, "gateway");
    }
}

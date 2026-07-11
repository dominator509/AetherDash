use std::sync::atomic::{AtomicU64, Ordering};

use axum::{extract::State, response::Json};
use serde::Serialize;

use crate::AppState;

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
/// Returns "ok" if DB is reachable, "degraded" if not.
pub async fn readyz(State(state): State<AppState>) -> Json<ReadinessResponse> {
    let db_status = match sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.pool).await {
        Ok(1) => "ok".to_string(),
        Ok(_) => "degraded".to_string(),
        Err(_) => "unreachable".to_string(),
    };

    let overall = if db_status == "ok" { "ok" } else { "degraded" };

    Json(ReadinessResponse {
        status: overall.into(),
        service: "gateway".into(),
        database: db_status,
    })
}

pub async fn metrics() -> String {
    let count = UNKNOWN_FRAME_COUNT.load(Ordering::Relaxed);
    format!(
        "# HELP gateway_connections_total Total WebSocket connections\n\
         # TYPE gateway_connections_total counter\n\
         gateway_connections_total 0\n\
         # HELP gateway_unknown_frames_total Total unknown/rejected frames\n\
         # TYPE gateway_unknown_frames_total counter\n\
         gateway_unknown_frames_total {count}\n"
    )
}

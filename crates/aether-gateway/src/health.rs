use std::sync::atomic::{AtomicU64, Ordering};

use axum::response::Json;
use serde::Serialize;

pub(crate) static UNKNOWN_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}

pub async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".into(), service: "gateway".into() })
}

pub async fn readyz() -> Json<HealthResponse> {
    // EP-401: check Postgres connectivity
    Json(HealthResponse { status: "ok".into(), service: "gateway".into() })
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

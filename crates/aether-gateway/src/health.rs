use axum::response::Json;
use serde::Serialize;

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
    "# HELP gateway_connections_total Total WebSocket connections\n# TYPE gateway_connections_total counter\ngateway_connections_total 0\n".into()
}

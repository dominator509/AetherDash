//! Hyperliquid venue health checks.
//!
//! Provides a function to probe the Hyperliquid Info API and report overall
//! health status, latency, and remaining rate-limit capacity.

use crate::client::HlClient;
use aether_proto::aether::venue::v1::VenueHealth;
use std::time::Instant;

/// Probe the Hyperliquid Info API and return a [`VenueHealth`] snapshot.
///
/// Health is determined by:
///
/// - **Latency**: time to complete a lightweight API call (`get_meta`).
/// - **Status**: `"ok"` if latency < 2 s, `"degraded"` if >= 2 s,
///   `"down"` if the call fails entirely.
/// - **Rate remaining**: current capacity in the pack-enforced token bucket.
pub async fn check_health(client: &HlClient) -> VenueHealth {
    let start = Instant::now();
    let rate_remaining = client.rate_remaining().await;

    let status = match client.get_meta().await {
        Ok(_) => {
            let lag_ms = start.elapsed().as_millis() as u64;
            if lag_ms < 2000 {
                VenueHealth { status: "ok".into(), lag_ms, rate_remaining }
            } else {
                VenueHealth { status: "degraded".into(), lag_ms, rate_remaining }
            }
        }
        Err(_) => VenueHealth { status: "down".into(), lag_ms: 0, rate_remaining: 0 },
    };

    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_returns_down_when_api_unreachable() {
        let client = HlClient::new("http://127.0.0.1:1");
        let health = check_health(&client).await;
        assert_eq!(health.status, "down");
    }
}

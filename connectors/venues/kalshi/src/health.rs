//! Kalshi venue health checks.
//!
//! Provides a function to probe the Kalshi API and report overall health
//! status, latency, and remaining rate-limit capacity.

use crate::client::KalshiClient;
use aether_proto::aether::venue::v1::VenueHealth;
use std::time::Instant;

/// Probe the Kalshi API and return a [`VenueHealth`] snapshot.
///
/// Health is determined by:
///
/// - **Latency**: time to complete a lightweight API call
///   (`get_markets` with `limit=1`).
/// - **Status**: `"ok"` if latency < 2 s, `"degraded"` if >= 2 s,
///   `"down"` if the call fails entirely.
/// - **Rate remaining**: current capacity in the pack-enforced token bucket.
pub async fn check_health(client: &KalshiClient) -> VenueHealth {
    let start = Instant::now();
    let rate_remaining = client.rate_remaining().await;

    let status = match client.get_markets(1, None).await {
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
    use crate::auth::KalshiAuth;

    const TEST_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDJJgEmkCH8nR55
pqhp/MIFR4hIr/dvbhrY+Ja3VM+qnq9vUD0lvPkPSdvwMVT05n6YVtMMM3ionLcA
bjSX2qjMBQozVih7xZonMKCLryJehbZNLGzPZD4aOv2P8PtctY/pNisa7tG73OvC
OXdlefIz+jMoiHNVNzl/HoVH0HxR4YHASe4lDaPtbbAciw60mpC2G8XWGJFmZGYj
WYfSmZ5tt3nqyQSOZpgzD4TiVXMOGRtjJIk0FdHd1sgo/dDIn6uKoH9j4qV3Mfr8
Z1alWAmt+Pfkwkw6Tcx2Jwtvhh6WNaEXk5+9UyEw+D+U5DqdMOPuD7fFL/y9icDC
fJ+Y9zgfAgMBAAECggEABJkDybfdrwKAYdN3YgTPAoPiD5dGFpvzrSXxe/tKS+IY
rHivDR/GqZzMlC7sfDSQjDbf2BWNGn2KiU37kcUDurYax5Wek0WvAlpQMSEtre9s
fVMYoZzu9naGuTWO6U2VHoWIcrMmxB6GnQfnPMCO0rVTWgfUaww6Gje+YCfZz51H
iJNNrLS9qFiWwO/DbEIOIyKRmAwF+h62Tfc7UQG2HIkJtMagRvCos2/+/gcDJ183
Tnno5XisuJ1B3LVvzh1BNqZaWKiXJZmZA5vpz2cFlaKGFE/IVgzgvKxvDrEt2d7D
j5uYVUb+6oft7BIZem2jkQQLQKez1ZMRmNSXa83BAQKBgQD5O1dcPfvVSczN4/6X
NzrjgkLQ5nNP57PM+gS1LGIVXztFywEQjftTF0R9tKFFqi6rVq5VO5Zjg7P1BiGc
Rk7rZy8mQnZo54MT2JYTpVhX9gUYXwSEOnc9sFyx+ncBPmKkwTvSwZhVdkhCEggw
CZI3VZgpJB0damAWhajQOcOa3wKBgQDOnGNRTYkHiD5Cr76l+CpKONqKiNUaKiZx
ZehBsKMCAfv9z77i/H/Wsbgn/HxDinmIBskF71fAKUOOGcBusJ4cGJRh2B6vBy6g
hm+b+2nSawgWaF7+ttRfVzFGH+nETHClzRaHc3h0p2ccnSxwVu6nW1p1jyMx0c/q
gtynUKVKwQKBgDpVPEY3r7ilFE1gPpdP8vWK6G6ScYzTM08XeYCaCb7s0iessuwX
/ynceUhevZxbj57Eo/sI/lL+YWFI9RbpkdEhDnUK+0HkZdaAS+f/PCUiTOD+ZEU6
lewXWirB75aX7miXXZQfgbMHAzSLmeT8aH+RBhMjA7l9y02aLP/HdVPLAoGAJyR5
rG2ECGlHYlrpQ4hAes9Kl/RUayCRJ+qmlcthFoBJvUweXeJ4VbRVrz2mTSVu4NZo
PzeY6E7o/YLjchUD307IzcCkD4TM0JyniGWZJsQgRB6B4L/CfE2IiECDiSzyKncw
TXkS2QbeAg3E3YOasxobiSoVANs/CK7CHvCoYAECgYEA7+emQFZmbSrWlhn7xeEy
OMQVeC/F6xKe4lGiuXsnjKEO1K6bi3qvltRoUdhH7bnR+k55hbDZG1sRZpl+N5VV
L/pwyKxACFxRoBxJqeozXdOqWB/2nw+byZNtK1KfQLnAyGqADXPnXPBUxVFE+c/2
8jqtMyHz94du+Z7Y/kOyNns=
-----END PRIVATE KEY-----";

    #[tokio::test]
    async fn health_returns_down_when_api_unreachable() {
        let auth = KalshiAuth::from_pem_bytes("test", TEST_KEY_PEM.as_bytes()).unwrap();
        let client = KalshiClient::new("http://127.0.0.1:1", auth);
        let health = check_health(&client).await;
        assert_eq!(health.status, "down");
    }
}

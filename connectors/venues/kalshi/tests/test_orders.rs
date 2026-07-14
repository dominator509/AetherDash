//! Integration tests for the Kalshi order-operations module.
//!
//! These tests verify order submission, cancellation, and balance queries
//! against a mock HTTP server. Sandbox endpoint gating and price-conversion
//! logic are tested via in-crate unit tests in `orders.rs`.

#![allow(clippy::unwrap_used)]

use aether_core::ids::{MarketKey, Ulid, VenueId};
use aether_core::order::{OrderIntent, OrderType, Origin, OriginKind, Side, SizeUnit, TimeInForce};
use aether_core::quote::{Quote, QuoteSource};
use aether_core::time::UtcTime;
use aether_venue_kalshi::KalshiAuth;
use aether_venue_kalshi::KalshiClient;
use aether_venue_kalshi::KalshiOrders;
use rust_decimal::Decimal;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn test_market_key() -> MarketKey {
    MarketKey::new(&VenueId::new("kalshi").unwrap(), "BTC-75").unwrap()
}

fn test_quote() -> Quote {
    Quote {
        market: test_market_key(),
        bid: Some(Decimal::new(65, 2)),
        ask: Some(Decimal::new(67, 2)),
        mid: Some(Decimal::new(66, 2)),
        last: None,
        bid_size: Some(Decimal::new(1000, 0)),
        ask_size: Some(Decimal::new(500, 0)),
        ts: UtcTime::from_unix_millis(1_783_200_000_000).unwrap(),
        source: QuoteSource::Stream,
        seq: Some(1),
    }
}

fn test_intent() -> OrderIntent {
    OrderIntent {
        id: Ulid::new(),
        market: test_market_key(),
        side: Side::Buy,
        order_type: OrderType::Limit,
        limit_price: Some(Decimal::new(65, 2)), // 0.65
        size: Decimal::new(10, 0),              // 10 contracts
        size_unit: SizeUnit::Contracts,
        tif: TimeInForce::Day,
        paper: true,
        origin: Origin::new(OriginKind::Agent, 3, Ulid::new()).unwrap(),
        quote_snapshot: test_quote(),
        caps_version: Ulid::new(),
        created_ts: UtcTime::now(),
    }
}

// ---------------------------------------------------------------------------
// Demo gate (no HTTP needed — the gate fires before any network call)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_live_order_rejected_in_demo() {
    let auth = KalshiAuth::from_pem_bytes("test-key", TEST_KEY_PEM.as_bytes()).unwrap();
    let orders = KalshiOrders::new(KalshiClient::new("https://external-api.kalshi.com", auth));
    let result = orders.submit_order(&test_intent()).await;

    assert!(
        matches!(&result, Err(e) if e.to_string().contains("sandbox endpoint")),
        "expected DemoRequired error, got {result:?}"
    );
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 2048];
    loop {
        let read = stream.read(&mut buffer).await.unwrap();
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end + 4]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_length {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}

async fn spawn_order_server(
    responses: Vec<(u16, String)>,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            requests.push(read_http_request(&mut stream).await);
            let reason = if status == 201 { "Created" } else { "OK" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        requests
    });
    (format!("http://{address}"), task)
}

#[tokio::test]
async fn v2_submit_is_idempotent_and_cancel_uses_delete() {
    let intent = test_intent();
    let client_order_id = intent.id.to_string();
    let response = serde_json::json!({
        "order_id": "order-123",
        "remaining_count": "10.00",
        "client_order_id": client_order_id,
        "ts_ms": 1_783_944_000_000_i64
    })
    .to_string();
    let (base_url, server) =
        spawn_order_server(vec![(201, response.clone()), (201, response), (200, "{}".into())])
            .await;
    let auth = KalshiAuth::from_pem_bytes("test-key", TEST_KEY_PEM.as_bytes()).unwrap();
    let orders = KalshiOrders::new(KalshiClient::new(base_url, auth));

    let first = orders.submit_order(&intent).await.unwrap();
    let second = orders.submit_order(&intent).await.unwrap();
    assert_eq!(first.venue_ref, "order-123");
    assert_eq!(second.venue_ref, first.venue_ref);
    orders.cancel_order(&first.venue_ref).await.unwrap();

    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 3);
    for request in &requests[..2] {
        assert!(request.starts_with("POST /trade-api/v2/portfolio/events/orders HTTP/1.1"));
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(json["client_order_id"], intent.id.to_string());
        assert_eq!(json["side"], "bid");
        assert_eq!(json["price"], "0.65");
        assert_eq!(json["count"], "10");
        assert_eq!(json["time_in_force"], "good_till_canceled");
    }
    assert!(
        requests[2].starts_with("DELETE /trade-api/v2/portfolio/events/orders/order-123 HTTP/1.1")
    );
}

#[tokio::test]
async fn v2_submit_retries_after_429_without_changing_client_order_id() {
    let intent = test_intent();
    let response = serde_json::json!({
        "order_id": "order-after-backoff",
        "remaining_count": "10.00",
        "client_order_id": intent.id.to_string(),
        "ts_ms": 1_783_944_000_000_i64
    })
    .to_string();
    let (base_url, server) = spawn_order_server(vec![(429, "{}".into()), (201, response)]).await;
    let auth = KalshiAuth::from_pem_bytes("test-key", TEST_KEY_PEM.as_bytes()).unwrap();
    let orders = KalshiOrders::new(KalshiClient::new(base_url, auth));

    let ack = orders.submit_order(&intent).await.unwrap();
    assert_eq!(ack.venue_ref, "order-after-backoff");
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 2);
    let first_body: serde_json::Value =
        serde_json::from_str(requests[0].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    let second_body: serde_json::Value =
        serde_json::from_str(requests[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(first_body["client_order_id"], intent.id.to_string());
    assert_eq!(second_body["client_order_id"], first_body["client_order_id"]);
}

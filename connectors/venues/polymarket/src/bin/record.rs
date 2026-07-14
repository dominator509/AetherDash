//! AETHER Terminal -- Polymarket market-data recorder.
//!
//! Periodically fetches Polymarket market data (Gamma API) and records it as
//! JSON fixture files that can be used for offline replay testing.  Wallet
//! addresses and any credential-like fields are scrubbed from the recorded
//! output.
//!
//! # Usage
//!
//! ```text
//! cargo run -p aether-venue-polymarket --bin record -- \
//!     --duration 60 \
//!     --interval 10 \
//!     --tickers TOKEN_ID_1,TOKEN_ID_2 \
//!     --output testdata/polymarket/
//! ```
//!
//! # Scrubbing
//!
//! - Any JSON string value matching an Ethereum address (`0x` + 40 hex chars)
//!   is replaced with `"<scrubbed>"`.
//! - The market metadata responses from Gamma are public and contain no
//!   credential material, so they are passed through as-is.

#![allow(clippy::expect_used)]

use futures::{SinkExt, StreamExt};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    let args = parse_args();

    let client = aether_venue_polymarket::client::GammaClient::from_env();

    let output_dir = &args.output;
    std::fs::create_dir_all(output_dir).expect("failed to create output directory");

    let start = Instant::now();
    let duration = Duration::from_secs(args.duration);
    let mut iteration = 0u64;

    let ws_task = if args.tickers.is_empty() {
        None
    } else {
        let tickers = args.tickers.clone();
        let output = output_dir.join("ws_recording.jsonl");
        let duration = args.duration;
        Some(tokio::spawn(async move { record_websocket(tickers, output, duration).await }))
    };

    println!(
        "recording Polymarket market data every {}s for {}s -> {}",
        args.interval,
        args.duration,
        output_dir.display()
    );

    while start.elapsed() < duration {
        iteration += 1;
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();

        // Fetch markets from Gamma API
        match client.get_markets(100, 0).await {
            Ok(resp) => {
                let fixture_path =
                    output_dir.join(format!("polymarket_markets_{ts}_{iteration}.json"));
                let scrubbed = scrub_markets_response(&resp);
                if let Ok(json) = serde_json::to_string_pretty(&scrubbed) {
                    std::fs::write(&fixture_path, &json).unwrap_or_else(|e| {
                        eprintln!("failed to write {fixture_path:?}: {e}");
                    });
                    println!("[{iteration}] wrote {fixture_path:?}");
                }
            }
            Err(e) => {
                eprintln!("[{iteration}] failed to fetch markets: {e}");
            }
        }

        tokio::time::sleep(Duration::from_secs(args.interval)).await;
    }

    if let Some(task) = ws_task {
        match task.await {
            Ok(Ok(count)) => println!("recorded {count} scrubbed WebSocket frame(s)"),
            Ok(Err(error)) => eprintln!("WebSocket recording failed: {error}"),
            Err(error) => eprintln!("WebSocket recorder task failed: {error}"),
        }
    }

    println!("recording complete -- {iteration} snapshot(s) saved");
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

struct Args {
    duration: u64,
    interval: u64,
    output: PathBuf,
    tickers: Vec<String>,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut duration = 60u64;
    let mut interval = 10u64;
    let mut output = PathBuf::from("testdata/polymarket");
    let mut tickers = Vec::new();

    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--duration" => {
                i += 1;
                duration = raw.get(i).and_then(|s| s.parse().ok()).unwrap_or(60);
            }
            "--interval" => {
                i += 1;
                interval = raw.get(i).and_then(|s| s.parse().ok()).unwrap_or(10);
            }
            "--output" => {
                i += 1;
                if let Some(p) = raw.get(i) {
                    output = PathBuf::from(p);
                }
            }
            "--tickers" => {
                i += 1;
                tickers = raw
                    .get(i)
                    .map(|value| {
                        value
                            .split(',')
                            .filter(|ticker| !ticker.trim().is_empty())
                            .map(|ticker| ticker.trim().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
            }
            _ => {
                eprintln!("unknown flag: {}", raw[i]);
                eprintln!(
                    "Usage: cargo run -p aether-venue-polymarket --bin record -- \
                     --duration SECS --tickers T1,T2 --output PATH"
                );
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Args { duration, interval, output, tickers }
}

// ---------------------------------------------------------------------------
// WebSocket recording
// ---------------------------------------------------------------------------

/// Connect to the Polymarket CLOB WebSocket, subscribe to the given asset IDs,
/// and record scrubbed text frames to a JSONL file for `duration_secs`.
async fn record_websocket(
    tickers: Vec<String>,
    output: PathBuf,
    duration_secs: u64,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let stream = aether_venue_polymarket::stream::PolymarketStream::from_env();
    let websocket = stream.connect().await?;
    let (mut sink, mut source) = websocket.split();
    stream.subscribe(&mut sink, &tickers).await?;

    let file = std::fs::OpenOptions::new().create(true).append(true).open(output)?;
    let mut writer = std::io::BufWriter::new(file);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(duration_secs);
    let mut count = 0;

    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let next = tokio::time::timeout(remaining.min(Duration::from_secs(1)), source.next()).await;
        let Some(message) = next.ok().flatten() else {
            continue;
        };
        match message? {
            Message::Text(text) => {
                let mut frame: serde_json::Value = serde_json::from_str(&text)?;
                scrub_value(&mut frame);
                let recorded = aether_venue_polymarket::replay::RecordedFrame {
                    received_ts: chrono::Utc::now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                    frame,
                };
                serde_json::to_writer(&mut writer, &recorded)?;
                writer.write_all(b"\n")?;
                writer.flush()?;
                count += 1;
            }
            Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
            Message::Close(_) => break,
            _ => {}
        }
    }

    Ok(count)
}

// ---------------------------------------------------------------------------
// Scrubbing
// ---------------------------------------------------------------------------

/// Recursively walk a JSON value and replace any string that looks like an
/// Ethereum wallet address (`0x` + exactly 40 hex characters) with `"<scrubbed>"`.
fn scrub_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for child in object.values_mut() {
                scrub_value(child);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                scrub_value(child);
            }
        }
        serde_json::Value::String(s) if is_wallet_address(s) => {
            *s = "<scrubbed>".to_string();
        }
        _ => {}
    }
}

/// Returns `true` only for an Ethereum address. Public 32-byte condition IDs,
/// token hashes, and transaction hashes must remain intact for deterministic
/// replay and provenance.
fn is_wallet_address(s: &str) -> bool {
    s.starts_with("0x") && s.len() == 42 && s[2..].chars().all(|c| c.is_ascii_hexdigit())
}

use aether_venue_polymarket::client::MarketsResponse;
use serde_json::Value;

/// Scrub sensitive information from a `MarketsResponse` fixture.
///
/// The Gamma API market list is public data and contains no wallet addresses.
/// This pass-through wraps the response in a `Value` for a consistent return
/// type; if portfolio-level endpoints are added later, integrate `scrub_value`
/// here.
fn scrub_markets_response(resp: &MarketsResponse) -> Value {
    serde_json::to_value(resp).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_scrubber_removes_sensitive_fields_recursively() {
        let mut value = serde_json::json!({
            "event_type": "last_trade_price",
            "asset_id": "0x1234567890abcdef1234567890abcdef12345678",
            "market": "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd",
            "price": "0.65",
            "nested": {
                "wallet": "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            }
        });
        scrub_value(&mut value);

        // Address fields are scrubbed.
        assert_eq!(value["asset_id"], "<scrubbed>");
        assert_eq!(value["market"], "<scrubbed>");
        assert_eq!(value["nested"]["wallet"], "<scrubbed>");

        // Non-address fields are preserved.
        assert_eq!(value["event_type"], "last_trade_price");
        assert_eq!(value["price"], "0.65");
    }

    #[test]
    fn scrubber_does_not_touch_short_hex_strings() {
        let mut value = serde_json::json!({
            "price": "0.65",
            "size": "10.0",
            "short": "0xabc"
        });
        scrub_value(&mut value);

        // Strings shorter than 42 chars are not treated as wallet addresses.
        assert_eq!(value["price"], "0.65");
        assert_eq!(value["size"], "10.0");
        assert_eq!(value["short"], "0xabc");
    }

    #[test]
    fn scrubber_preserves_non_string_values() {
        let mut value = serde_json::json!({
            "active": true,
            "volume": 1000000.0,
            "count": 42,
            "meta": null
        });
        scrub_value(&mut value);

        assert_eq!(value["active"], true);
        assert_eq!(value["volume"], 1000000.0);
        assert_eq!(value["count"], 42);
        assert_eq!(value["meta"], serde_json::Value::Null);
    }
}

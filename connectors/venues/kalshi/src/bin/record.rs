//! AETHER Terminal -- Kalshi market-data recorder.
//!
//! Periodically fetches Kalshi market data and records it as JSON fixture files
//! that can be used for offline replay testing.  Sensitive information (account
//! IDs, balances, keys) is scrubbed from the recorded output.
//!
//! # Usage
//!
//! ```text
//! cargo run --bin record -- --duration 60 --tickers FED-23DEC-T3.00 --output testdata/kalshi/
//! ```
//!
//! # Scrubbing
//!
//! - Account / member IDs are replaced with `"<scrubbed>"`.
//! - Balance values are jittered by +/- 10 %.
//! - API keys and private keys are NEVER present (auth happens at request time
//!   and fixture responses exclude request authentication headers).

#![allow(clippy::expect_used)]

use futures::{SinkExt, StreamExt};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    let args = parse_args();

    let auth = match aether_venue_kalshi::auth::KalshiAuth::from_env() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("auth setup failed: {e}");
            std::process::exit(1);
        }
    };
    let client = aether_venue_kalshi::client::KalshiClient::from_env(auth);

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
        "recording Kalshi market data every {}s for {}s -> {}",
        args.interval,
        args.duration,
        output_dir.display()
    );

    while start.elapsed() < duration {
        iteration += 1;
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();

        // Fetch markets
        match client.get_markets(100, None).await {
            Ok(resp) => {
                let fixture_path = output_dir.join(format!("kalshi_markets_{ts}_{iteration}.json"));
                let scrubbed = scrub_markets_response(&resp);
                if let Ok(json) = serde_json::to_string_pretty(&scrubbed) {
                    std::fs::write(&fixture_path, &json)
                        .unwrap_or_else(|e| eprintln!("failed to write {fixture_path:?}: {e}"));
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

    println!("recording complete — {iteration} snapshot(s) saved");
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
    let mut output = PathBuf::from("testdata/kalshi");
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
                    "Usage: cargo run --bin record -- --duration SECS --tickers T1,T2 --output PATH"
                );
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Args { duration, interval, output, tickers }
}

async fn record_websocket(
    tickers: Vec<String>,
    output: PathBuf,
    duration_secs: u64,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let auth = aether_venue_kalshi::auth::KalshiAuth::from_env()?;
    let stream = aether_venue_kalshi::stream::KalshiStream::from_env(auth);
    let websocket = stream.connect().await?;
    let (mut sink, mut source) = websocket.split();
    stream.subscribe_ticks(&mut sink, &tickers).await?;
    stream.subscribe_books(&mut sink, &tickers).await?;

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
                let recorded = aether_venue_kalshi::replay::RecordedFrame {
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

fn scrub_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for sensitive in
                ["api_key", "api_key_id", "user_id", "member_id", "account_id", "balance"]
            {
                if object.contains_key(sensitive) {
                    object.insert(
                        sensitive.to_string(),
                        serde_json::Value::String("<scrubbed>".into()),
                    );
                }
            }
            for child in object.values_mut() {
                scrub_value(child);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                scrub_value(child);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Scrubbing helpers
// ---------------------------------------------------------------------------

use aether_venue_kalshi::client::MarketsResponse;
use serde_json::Value;

/// Scrub sensitive information from a `MarketsResponse` fixture.
///
/// Currently a no-op (the markets endpoint returns no account-level data).
/// When recording portfolio endpoints, add scrubbing logic here.
fn scrub_markets_response(resp: &MarketsResponse) -> Value {
    // Markets data is public — no scrubbing needed.
    serde_json::to_value(resp).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_scrubber_removes_sensitive_fields_recursively() {
        let mut value = serde_json::json!({
            "type": "ticker",
            "msg": {
                "market_ticker": "SAFE-TICKER",
                "user_id": "user-secret",
                "nested": {"api_key": "key-secret", "balance": 12345}
            }
        });
        scrub_value(&mut value);
        assert_eq!(value["msg"]["market_ticker"], "SAFE-TICKER");
        assert_eq!(value["msg"]["user_id"], "<scrubbed>");
        assert_eq!(value["msg"]["nested"]["api_key"], "<scrubbed>");
        assert_eq!(value["msg"]["nested"]["balance"], "<scrubbed>");
    }
}

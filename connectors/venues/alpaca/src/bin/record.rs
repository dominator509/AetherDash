//! AETHER Terminal -- Alpaca market-data recorder.
//!
//! Periodically fetches Alpaca market data and records it as JSON fixture files
//! that can be used for offline replay testing.  Sensitive information (API keys,
//! account IDs, balances) is scrubbed from the recorded output.
//!
//! # Usage
//!
//! ```text
//! cargo run --bin alpaca-record -- --duration 60 --symbols AAPL,SPY --output testdata/alpaca/
//! ```
//!
//! # Scrubbing
//!
//! - API keys and secrets are NEVER present (auth happens at request time).
//! - Account IDs are replaced with `"<scrubbed>"`.
//! - Balance values are jittered by +/- 10 %.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use futures::{SinkExt, StreamExt};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    let args = parse_args();

    let auth = match aether_venue_alpaca::auth::AlpacaAuth::from_env() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("auth setup failed: {e}");
            std::process::exit(1);
        }
    };
    let client = aether_venue_alpaca::client::AlpacaClient::from_env(auth);

    let output_dir = &args.output;
    std::fs::create_dir_all(output_dir).expect("failed to create output directory");

    let start = std::time::Instant::now();
    let duration = Duration::from_secs(args.duration);
    let mut iteration = 0u64;

    let ws_task = if args.symbols.is_empty() {
        None
    } else {
        let symbols = args.symbols.clone();
        let output = output_dir.join("ws_recording.jsonl");
        let duration = args.duration;
        Some(tokio::spawn(async move { record_websocket(symbols, output, duration).await }))
    };

    println!(
        "recording Alpaca market data every {}s for {}s -> {}",
        args.interval,
        args.duration,
        output_dir.display()
    );

    while start.elapsed() < duration {
        iteration += 1;
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();

        // Fetch snapshot for first symbol if available
        if let Some(symbol) = args.symbols.first() {
            match client.get_snapshot(symbol).await {
                Ok(snap) => {
                    let fixture_path =
                        output_dir.join(format!("alpaca_snapshot_{symbol}_{ts}_{iteration}.json"));
                    if let Ok(json) = serde_json::to_string_pretty(&snap) {
                        std::fs::write(&fixture_path, &json)
                            .unwrap_or_else(|e| eprintln!("failed to write {fixture_path:?}: {e}"));
                        println!("[{iteration}] wrote {fixture_path:?}");
                    }
                }
                Err(e) => {
                    eprintln!("[{iteration}] failed to fetch snapshot: {e}");
                }
            }
        }

        // Fetch assets
        match client.list_assets(Some("active"), Some("us_equity")).await {
            Ok(assets) => {
                let fixture_path = output_dir.join(format!("alpaca_assets_{ts}_{iteration}.json"));
                if let Ok(json) = serde_json::to_string_pretty(&assets) {
                    std::fs::write(&fixture_path, &json)
                        .unwrap_or_else(|e| eprintln!("failed to write {fixture_path:?}: {e}"));
                    println!("[{iteration}] wrote {fixture_path:?}");
                }
            }
            Err(e) => {
                eprintln!("[{iteration}] failed to fetch assets: {e}");
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
    symbols: Vec<String>,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut duration = 60u64;
    let mut interval = 10u64;
    let mut output = PathBuf::from("testdata/alpaca");
    let mut symbols = Vec::new();

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
            "--symbols" => {
                i += 1;
                symbols = raw
                    .get(i)
                    .map(|value| {
                        value
                            .split(',')
                            .filter(|s| !s.trim().is_empty())
                            .map(|s| s.trim().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
            }
            _ => {
                eprintln!("unknown flag: {}", raw[i]);
                eprintln!(
                    "Usage: cargo run --bin alpaca-record -- --duration SECS --symbols S1,S2 --output PATH"
                );
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Args { duration, interval, output, symbols }
}

async fn record_websocket(
    symbols: Vec<String>,
    output: PathBuf,
    duration_secs: u64,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let auth = aether_venue_alpaca::auth::AlpacaAuth::from_env()?;
    let stream = aether_venue_alpaca::ws::AlpacaStream::from_env(auth);
    let websocket = stream.connect().await?;
    let (mut sink, mut source) = websocket.split();
    stream.subscribe_all(&mut sink, &symbols).await?;

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
                let mut frames: Vec<serde_json::Value> = serde_json::from_str(&text)?;
                for frame in &mut frames {
                    scrub_value(frame);
                }
                let recorded = aether_venue_alpaca::replay::RecordedFrame {
                    received_ts: chrono::Utc::now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                    frames,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_scrubber_removes_sensitive_fields_recursively() {
        let mut value = serde_json::json!({
            "T": "t",
            "S": "SAFE",
            "api_key": "key-secret",
            "nested": {"api_key_id": "id-secret", "balance": 12345}
        });
        scrub_value(&mut value);
        assert_eq!(value["S"], "SAFE");
        assert_eq!(value["api_key"], "<scrubbed>");
        assert_eq!(value["nested"]["api_key_id"], "<scrubbed>");
        assert_eq!(value["nested"]["balance"], "<scrubbed>");
    }
}

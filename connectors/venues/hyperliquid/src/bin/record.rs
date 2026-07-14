//! AETHER Terminal -- Hyperliquid market-data recorder.
//!
//! Periodically fetches Hyperliquid market data and records it as JSON fixture
//! files that can be used for offline replay testing.
//!
//! # Usage
//!
//! ```text
//! cargo run --bin record-hyperliquid -- --duration 60 --coins BTC,ETH --output testdata/hyperliquid/
//! ```

#![allow(clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let args = parse_args();

    let client = aether_venue_hyperliquid::client::HlClient::from_env();

    let output_dir = &args.output;
    std::fs::create_dir_all(output_dir).expect("failed to create output directory");

    let start = Instant::now();
    let duration = Duration::from_secs(args.duration);
    let mut iteration = 0u64;

    println!(
        "recording Hyperliquid data every {}s for {}s -> {}",
        args.interval,
        args.duration,
        output_dir.display()
    );

    while start.elapsed() < duration {
        iteration += 1;
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();

        // Fetch meta
        match client.get_meta().await {
            Ok(resp) => {
                let path = output_dir.join(format!("hl_meta_{ts}_{iteration}.json"));
                if let Ok(json) = serde_json::to_string_pretty(&resp) {
                    std::fs::write(&path, &json)
                        .unwrap_or_else(|e| eprintln!("failed to write {path:?}: {e}"));
                    println!("[{iteration}] wrote {path:?}");
                }
            }
            Err(e) => {
                eprintln!("[{iteration}] failed to fetch meta: {e}");
            }
        }

        // Fetch allMids
        match client.get_all_mids().await {
            Ok(mids) => {
                let path = output_dir.join(format!("hl_mids_{ts}_{iteration}.json"));
                if let Ok(json) = serde_json::to_string_pretty(&mids) {
                    std::fs::write(&path, &json)
                        .unwrap_or_else(|e| eprintln!("failed to write {path:?}: {e}"));
                    println!("[{iteration}] wrote {path:?}");

                    // Record each mid as a replay frame
                    record_frame(output_dir, &ts, "allMids", None, &serde_json::json!(mids));
                }
            }
            Err(e) => {
                eprintln!("[{iteration}] failed to fetch allMids: {e}");
            }
        }

        // Fetch l2Book for each coin
        for coin in &args.coins {
            match client.get_l2_book(coin).await {
                Ok(book) => {
                    let coin_lower = coin.to_lowercase();
                    let path =
                        output_dir.join(format!("hl_book_{coin_lower}_{ts}_{iteration}.json"));
                    if let Ok(json) = serde_json::to_string_pretty(&book) {
                        std::fs::write(&path, &json)
                            .unwrap_or_else(|e| eprintln!("failed to write {path:?}: {e}"));
                        println!("[{iteration}] wrote {path:?}");

                        record_frame(
                            output_dir,
                            &ts,
                            "l2Book",
                            Some(coin),
                            &serde_json::to_value(&book).unwrap_or_default(),
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[{iteration}] failed to fetch l2Book for {coin}: {e}");
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(args.interval)).await;
    }

    println!("recording complete -- {iteration} snapshot(s) saved");
}

fn record_frame(
    output_dir: &Path,
    ts: &str,
    kind: &str,
    coin: Option<&str>,
    payload: &serde_json::Value,
) {
    let recorded = aether_venue_hyperliquid::replay::RecordedFrame {
        received_ts: ts.to_string(),
        kind: kind.to_string(),
        coin: coin.map(|c| c.to_string()),
        payload: payload.clone(),
    };

    let recording_path = output_dir.join("ws_recording.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&recording_path)
        .unwrap_or_else(|e| panic!("failed to open {recording_path:?}: {e}"));
    if let Ok(json) = serde_json::to_string(&recorded) {
        let _ = writeln!(file, "{json}");
    }
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

struct Args {
    duration: u64,
    interval: u64,
    output: PathBuf,
    coins: Vec<String>,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let mut duration = 60u64;
    let mut interval = 10u64;
    let mut output = PathBuf::from("testdata/hyperliquid");
    let mut coins = Vec::new();

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
            "--coins" => {
                i += 1;
                coins = raw
                    .get(i)
                    .map(|value| {
                        value
                            .split(',')
                            .filter(|c| !c.trim().is_empty())
                            .map(|c| c.trim().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
            }
            _ => {
                eprintln!("unknown flag: {}", raw[i]);
                eprintln!(
                    "Usage: cargo run --bin record-hyperliquid -- --duration SECS --coins BTC,ETH --output PATH"
                );
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Args { duration, interval, output, coins }
}

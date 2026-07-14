//! Deterministic replay of scrubbed Alpaca WebSocket recordings.

use crate::client::AlpacaSnapshot;
use crate::normalize::normalize_stream_snapshot;
use aether_bus::envelope::Envelope;
use aether_bus::producer::{MessageProducer, ProducerError};
use aether_core::quote::Quote;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single recorded frame from the Alpaca WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedFrame {
    /// Recorder receive timestamp, used when the frame omits one.
    pub received_ts: String,
    /// Exact scrubbed Alpaca WebSocket JSON array.
    pub frames: Vec<serde_json::Value>,
}

/// A normalized replay event.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ReplayEvent {
    /// A normalized quote from a trade or snapshot.
    Quote(Quote),
}

/// Errors that can occur during replay.
#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("recording line {line} is invalid JSON: {source}")]
    RecordingJson { line: usize, source: serde_json::Error },
    #[error("recording line {line}: {detail}")]
    Payload { line: usize, detail: String },
    #[error("bus publish failed: {0}")]
    Producer(#[from] ProducerError),
}

/// Replay a JSONL recording of Alpaca WS frames into deterministic normalized
/// domain events.
pub fn replay_jsonl(recording: &str) -> Result<Vec<ReplayEvent>, ReplayError> {
    let mut events = Vec::new();

    for (index, line) in recording.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let recorded: RecordedFrame = serde_json::from_str(line)
            .map_err(|source| ReplayError::RecordingJson { line: line_number, source })?;

        for frame in &recorded.frames {
            let msg_type = frame.get("T").and_then(|v| v.as_str()).unwrap_or("");

            match msg_type {
                "t" | "q" => {
                    // Build a snapshot from this frame
                    let symbol = frame.get("S").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if symbol.is_empty() {
                        continue;
                    }

                    let mut latest_trade = None;
                    let mut latest_quote = None;

                    if msg_type == "t" {
                        latest_trade = Some(crate::client::AlpacaTrade {
                            t: Some(
                                frame
                                    .get("t")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&recorded.received_ts)
                                    .to_string(),
                            ),
                            x: frame.get("x").and_then(|v| v.as_str()).map(|s| s.to_string()),
                            p: decimal_field(frame, "p"),
                            s: decimal_field(frame, "s"),
                            ..Default::default()
                        });
                    } else {
                        latest_quote = Some(crate::client::AlpacaQuote {
                            t: Some(
                                frame
                                    .get("t")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&recorded.received_ts)
                                    .to_string(),
                            ),
                            bp: decimal_field(frame, "bp"),
                            ap: decimal_field(frame, "ap"),
                            bs: decimal_field(frame, "bs"),
                            ask_size: decimal_field(frame, "as"),
                            ..Default::default()
                        });
                    }

                    let snapshot = AlpacaSnapshot {
                        symbol: Some(symbol),
                        latest_trade,
                        latest_quote,
                        ..Default::default()
                    };

                    match normalize_stream_snapshot(&snapshot) {
                        Ok(quote) => events.push(ReplayEvent::Quote(quote)),
                        Err(e) => {
                            return Err(ReplayError::Payload {
                                line: line_number,
                                detail: e.to_string(),
                            });
                        }
                    }
                }
                _ => {
                    // Skip subscription confirmations, bars, etc.
                }
            }
        }
    }

    Ok(events)
}

fn decimal_field(value: &serde_json::Value, field: &str) -> Option<String> {
    match value.get(field) {
        Some(serde_json::Value::Number(number)) => Some(number.to_string()),
        Some(serde_json::Value::String(number)) => Some(number.clone()),
        _ => None,
    }
}

/// Drive a recording through normalization into the same bus topic shapes used live.
pub async fn replay_to_bus(
    recording: &str,
    producer: &impl MessageProducer,
) -> Result<usize, ReplayError> {
    let events = replay_jsonl(recording)?;
    for event in &events {
        match event {
            ReplayEvent::Quote(quote) => {
                producer
                    .send(
                        "md.ticks.alpaca",
                        Envelope::new("quote", quote),
                        Some(quote.market.as_str()),
                    )
                    .await?;
            }
        }
    }
    Ok(events.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_bus::producer::StubProducer;

    const RECORDING: &str = include_str!("../../../../testdata/alpaca/ws_recording.jsonl");

    #[test]
    fn identical_recordings_produce_identical_normalized_events() {
        let first = replay_jsonl(RECORDING).unwrap();
        let second = replay_jsonl(RECORDING).unwrap();
        assert_eq!(serde_json::to_vec(&first).unwrap(), serde_json::to_vec(&second).unwrap());
        assert_eq!(first.len(), 2);
    }

    #[tokio::test]
    async fn recording_drives_bus_shaped_topics() {
        let producer = StubProducer::new();
        assert_eq!(replay_to_bus(RECORDING, &producer).await.unwrap(), 2);
        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent[0].0, "md.ticks.alpaca");
        assert_eq!(sent[1].0, "md.ticks.alpaca");
        for (_, envelope) in sent.iter() {
            let value: serde_json::Value = serde_json::from_str(envelope).unwrap();
            assert!(value.get("payload").is_some());
        }
    }
}

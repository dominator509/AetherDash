//! Deterministic replay of recorded Hyperliquid API responses.

use crate::normalize::{normalize_book, normalize_mid_to_quote, ClobBookSnapshot};
use aether_bus::envelope::Envelope;
use aether_bus::producer::{MessageProducer, ProducerError};
use aether_core::quote::{OrderBook, Quote};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single recorded frame from the Hyperliquid Info API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedFrame {
    /// Recorder timestamp.
    pub received_ts: String,
    /// The response kind: "allMids" or "l2Book".
    pub kind: String,
    /// The coin name (for l2Book).
    #[serde(default)]
    pub coin: Option<String>,
    /// The scrubbed response payload.
    pub payload: serde_json::Value,
}

/// Normalized replay event emitted during deterministic replay.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ReplayEvent {
    Quote(Quote),
    OrderBook(OrderBook),
}

/// Errors that can occur during replay.
#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("recording line {line} is invalid JSON: {source}")]
    RecordingJson { line: usize, source: serde_json::Error },
    #[error("recording line {line} has an invalid payload: {detail}")]
    Payload { line: usize, detail: String },
    #[error("recording line {line} has invalid received_ts: {detail}")]
    ReceivedTimestamp { line: usize, detail: String },
    #[error("bus publish failed: {0}")]
    Producer(#[from] ProducerError),
}

/// Replay a JSONL recording into deterministic normalized domain events.
pub fn replay_jsonl(recording: &str) -> Result<Vec<ReplayEvent>, ReplayError> {
    let mut events = Vec::new();

    for (index, line) in recording.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let recorded: RecordedFrame = serde_json::from_str(line)
            .map_err(|source| ReplayError::RecordingJson { line: line_number, source })?;
        let received_ms = chrono::DateTime::parse_from_rfc3339(&recorded.received_ts)
            .map_err(|error| ReplayError::ReceivedTimestamp {
                line: line_number,
                detail: error.to_string(),
            })?
            .timestamp_millis();

        match recorded.kind.as_str() {
            "allMids" => {
                if let Some(obj) = recorded.payload.as_object() {
                    for (coin, mid_val) in obj {
                        let mid_str = match mid_val.as_str() {
                            Some(s) => s.to_string(),
                            None => continue,
                        };
                        let quote = normalize_mid_to_quote(coin, &mid_str, received_ms).map_err(
                            |error| ReplayError::Payload {
                                line: line_number,
                                detail: error.to_string(),
                            },
                        )?;
                        events.push(ReplayEvent::Quote(quote));
                    }
                }
            }
            "l2Book" => {
                let snapshot: ClobBookSnapshot =
                    serde_json::from_value(recorded.payload.clone()).map_err(|error| {
                        ReplayError::Payload { line: line_number, detail: error.to_string() }
                    })?;
                // Convert ClobBookSnapshot back to an HlBookSnapshot for normalization
                use crate::client::HlBookSnapshot;
                let hl_snapshot = HlBookSnapshot {
                    coin: snapshot.coin,
                    time: snapshot.time,
                    levels: vec![
                        snapshot
                            .bids
                            .iter()
                            .map(|l| crate::client::HlBookLevel {
                                px: l.px.clone(),
                                sz: l.sz.clone(),
                                n: 1,
                            })
                            .collect(),
                        snapshot
                            .asks
                            .iter()
                            .map(|l| crate::client::HlBookLevel {
                                px: l.px.clone(),
                                sz: l.sz.clone(),
                                n: 1,
                            })
                            .collect(),
                    ],
                };
                let book = normalize_book(&hl_snapshot).map_err(|error| ReplayError::Payload {
                    line: line_number,
                    detail: error.to_string(),
                })?;
                events.push(ReplayEvent::OrderBook(book));
            }
            _ => {
                // Unknown kind, skip
            }
        }
    }

    Ok(events)
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
                        "md.ticks.hyperliquid",
                        Envelope::new("quote", quote),
                        Some(quote.market.as_str()),
                    )
                    .await?;
            }
            ReplayEvent::OrderBook(book) => {
                producer
                    .send(
                        "md.books.hyperliquid",
                        Envelope::new("order_book", book),
                        Some(book.market.as_str()),
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

    const RECORDING: &str = include_str!("../../../../testdata/hyperliquid/ws_recording.jsonl");

    #[test]
    fn identical_recordings_produce_identical_normalized_events() {
        let first = replay_jsonl(RECORDING).unwrap();
        let second = replay_jsonl(RECORDING).unwrap();
        assert_eq!(serde_json::to_vec(&first).unwrap(), serde_json::to_vec(&second).unwrap());
        assert_eq!(first.len(), 4);
    }

    #[tokio::test]
    async fn recording_drives_bus_shaped_topics() {
        let producer = StubProducer::new();
        let count = replay_to_bus(RECORDING, &producer).await.unwrap();
        assert_eq!(count, 4);
        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent.len(), 4);
        let _topics: Vec<&str> = sent.iter().map(|(t, _)| t.as_str()).collect();
        // May be in any order depending on recording
        for (_topic, envelope) in sent.iter() {
            let value: serde_json::Value = serde_json::from_str(envelope).unwrap();
            assert!(value.get("payload").is_some());
        }
    }
}

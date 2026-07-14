//! Deterministic replay of scrubbed Kalshi WebSocket recordings.

use crate::normalize::{
    normalize_tick, KalshiBookDelta, KalshiBookSnapshot, KalshiBookState, KalshiTick,
};
use crate::stream::KalshiWsMessage;
use aether_bus::envelope::Envelope;
use aether_bus::producer::{MessageProducer, ProducerError};
use aether_core::quote::{OrderBook, Quote};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedFrame {
    /// Recorder receive timestamp, used only when Kalshi snapshots omit one.
    pub received_ts: String,
    /// Exact scrubbed Kalshi WebSocket JSON object.
    pub frame: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ReplayEvent {
    Quote(Quote),
    OrderBook(OrderBook),
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("recording line {line} is invalid JSON: {source}")]
    RecordingJson { line: usize, source: serde_json::Error },
    #[error("recording line {line} has an invalid WebSocket envelope: {source}")]
    Envelope { line: usize, source: serde_json::Error },
    #[error("recording line {line} has an invalid payload: {detail}")]
    Payload { line: usize, detail: String },
    #[error("sequence gap for subscription {sid}: expected {expected}, got {actual}")]
    Gap { sid: u64, expected: u64, actual: u64 },
    #[error("order-book delta arrived before its snapshot for {ticker}")]
    DeltaBeforeSnapshot { ticker: String },
    #[error("bus publish failed: {0}")]
    Producer(#[from] ProducerError),
}

/// Replay a JSONL recording into deterministic normalized domain events.
pub fn replay_jsonl(recording: &str) -> Result<Vec<ReplayEvent>, ReplayError> {
    let mut events = Vec::new();
    let mut books: HashMap<String, KalshiBookState> = HashMap::new();
    let mut sequences: HashMap<u64, u64> = HashMap::new();

    for (index, line) in recording.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let recorded: RecordedFrame = serde_json::from_str(line)
            .map_err(|source| ReplayError::RecordingJson { line: line_number, source })?;
        let message: KalshiWsMessage = serde_json::from_value(recorded.frame)
            .map_err(|source| ReplayError::Envelope { line: line_number, source })?;

        if let (Some(sid), Some(seq)) = (message.sid, message.seq) {
            if let Some(previous) = sequences.insert(sid, seq) {
                let expected = previous + 1;
                if seq != expected {
                    return Err(ReplayError::Gap { sid, expected, actual: seq });
                }
            }
        }

        match message.msg_type.as_str() {
            "ticker" => {
                let tick: KalshiTick = serde_json::from_value(message.data).map_err(|error| {
                    ReplayError::Payload { line: line_number, detail: error.to_string() }
                })?;
                let quote = normalize_tick(&tick).map_err(|error| ReplayError::Payload {
                    line: line_number,
                    detail: error.to_string(),
                })?;
                events.push(ReplayEvent::Quote(quote));
            }
            "orderbook_snapshot" => {
                let mut snapshot: KalshiBookSnapshot = serde_json::from_value(message.data)
                    .map_err(|error| ReplayError::Payload {
                        line: line_number,
                        detail: error.to_string(),
                    })?;
                if snapshot.ts.is_none() {
                    snapshot.ts = Some(recorded.received_ts);
                }
                let ticker = snapshot.ticker.clone();
                let state =
                    KalshiBookState::from_snapshot(&snapshot, message.seq).map_err(|error| {
                        ReplayError::Payload { line: line_number, detail: error.to_string() }
                    })?;
                let book = state.to_order_book().map_err(|error| ReplayError::Payload {
                    line: line_number,
                    detail: error.to_string(),
                })?;
                books.insert(ticker, state);
                events.push(ReplayEvent::OrderBook(book));
            }
            "orderbook_delta" => {
                let delta: KalshiBookDelta =
                    serde_json::from_value(message.data).map_err(|error| ReplayError::Payload {
                        line: line_number,
                        detail: error.to_string(),
                    })?;
                let state = books.get_mut(&delta.market_ticker).ok_or_else(|| {
                    ReplayError::DeltaBeforeSnapshot { ticker: delta.market_ticker.clone() }
                })?;
                state.apply_delta(&delta, message.seq).map_err(|error| ReplayError::Payload {
                    line: line_number,
                    detail: error.to_string(),
                })?;
                events.push(ReplayEvent::OrderBook(state.to_order_book().map_err(|error| {
                    ReplayError::Payload { line: line_number, detail: error.to_string() }
                })?));
            }
            _ => {}
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
                        "md.ticks.kalshi",
                        Envelope::new("quote", quote),
                        Some(quote.market.as_str()),
                    )
                    .await?;
            }
            ReplayEvent::OrderBook(book) => {
                producer
                    .send(
                        "md.books.kalshi",
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

    const RECORDING: &str = include_str!("../../../../testdata/kalshi/ws_recording.jsonl");

    #[test]
    fn identical_recordings_produce_identical_normalized_events() {
        let first = replay_jsonl(RECORDING).unwrap();
        let second = replay_jsonl(RECORDING).unwrap();
        assert_eq!(serde_json::to_vec(&first).unwrap(), serde_json::to_vec(&second).unwrap());
        assert_eq!(first.len(), 3);
    }

    #[tokio::test]
    async fn recording_drives_bus_shaped_topics() {
        let producer = StubProducer::new();
        assert_eq!(replay_to_bus(RECORDING, &producer).await.unwrap(), 3);
        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent[0].0, "md.ticks.kalshi");
        assert_eq!(sent[1].0, "md.books.kalshi");
        assert_eq!(sent[2].0, "md.books.kalshi");
        for (_, envelope) in sent.iter() {
            let value: serde_json::Value = serde_json::from_str(envelope).unwrap();
            assert!(value.get("payload").is_some());
        }
    }
}

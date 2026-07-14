//! Deterministic replay of scrubbed Polymarket WebSocket recordings.

#![allow(dead_code)]

use crate::normalize::{normalize_book, normalize_tick, ClobBookSnapshot};
use crate::stream::{parse_ws_messages, ticks_from_ws};
use aether_bus::envelope::Envelope;
use aether_bus::producer::{MessageProducer, ProducerError};
use aether_core::quote::{OrderBook, Quote};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// One line of a recorded JSONL fixture.
///
/// `received_ts` is the recorder's clock when the frame landed; `frame` is the
/// exact scrubbed WebSocket JSON object as it arrived from the CLOB API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedFrame {
    /// Recorder receive timestamp (RFC 3339 millisecond precision).
    pub received_ts: String,
    /// Exact scrubbed Polymarket CLOB WebSocket JSON object.
    pub frame: serde_json::Value,
}

/// A deterministic domain event produced by replaying a recording.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ReplayEvent {
    /// A normalised price tick.
    Quote(Quote),
    /// A normalised order-book snapshot.
    OrderBook(OrderBook),
}

/// Errors that can occur during recording replay.
#[derive(Debug, Error)]
pub enum ReplayError {
    /// A line in the JSONL recording is not valid JSON.
    #[error("recording line {line} is invalid JSON: {source}")]
    RecordingJson {
        /// 1-based line number.
        line: usize,
        /// The underlying JSON parse error.
        source: serde_json::Error,
    },

    /// The frame's `event_type` could not be extracted from the envelope.
    #[error("recording line {line} has an invalid WebSocket envelope: {source}")]
    Envelope {
        /// 1-based line number.
        line: usize,
        /// The underlying JSON parse error.
        source: serde_json::Error,
    },

    /// The frame payload for the matched event type is malformed.
    #[error("recording line {line} has an invalid payload: {detail}")]
    Payload {
        /// 1-based line number.
        line: usize,
        /// Human-readable description of the malformation.
        detail: String,
    },

    /// Bus publish failed.
    #[error("bus publish failed: {0}")]
    Producer(#[from] ProducerError),
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

/// Replay a JSONL recording into deterministic normalised domain events.
///
/// Each line is parsed as a [`RecordedFrame`].  The `event_type` field of the
/// embedded WebSocket message determines how the frame is interpreted:
///
/// | `event_type`        | Parsed as            | Produced event       |
/// |---------------------|----------------------|----------------------|
/// | `last_trade_price`  | `ClobTick`           | `Quote`              |
/// | `price_change`      | `ClobTick`           | `Quote`              |
/// | `book`              | `ClobBookSnapshot`   | `OrderBook`          |
/// | _other_             | —                    | silently skipped     |
///
/// # Errors
///
/// Returns [`ReplayError::RecordingJson`] if a line is not valid JSON.
/// Returns [`ReplayError::Envelope`] if the frame's top-level `event_type`
/// cannot be read.  Returns [`ReplayError::Payload`] if the frame data for
/// a recognised event type is malformed.
pub fn replay_jsonl(recording: &str) -> Result<Vec<ReplayEvent>, ReplayError> {
    let mut events = Vec::new();

    for (index, line) in recording.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() {
            continue;
        }

        let recorded: RecordedFrame = serde_json::from_str(line)
            .map_err(|source| ReplayError::RecordingJson { line: line_number, source })?;

        let encoded = serde_json::to_string(&recorded.frame)
            .map_err(|source| ReplayError::Envelope { line: line_number, source })?;
        let mut messages = parse_ws_messages(&encoded)
            .map_err(|source| ReplayError::Envelope { line: line_number, source })?;

        for message in &mut messages {
            if message.timestamp.is_none() {
                message.timestamp = Some(recorded.received_ts.clone());
            }
            match message.event_type.as_str() {
                "last_trade_price" | "price_change" | "best_bid_ask" => {
                    let ticks = ticks_from_ws(message).map_err(|error| ReplayError::Payload {
                        line: line_number,
                        detail: error.to_string(),
                    })?;
                    for tick in ticks {
                        let quote = normalize_tick(tick).map_err(|error| ReplayError::Payload {
                            line: line_number,
                            detail: error.to_string(),
                        })?;
                        events.push(ReplayEvent::Quote(quote));
                    }
                }
                "book" => {
                    let snapshot = ClobBookSnapshot {
                        market: message.market.clone().unwrap_or_default(),
                        asset_id: message.asset_id.clone().unwrap_or_default(),
                        timestamp: message.timestamp.clone().unwrap_or_default(),
                        bids: message.bids.clone().unwrap_or_default(),
                        asks: message.asks.clone().unwrap_or_default(),
                        hash: message.hash.clone(),
                    };
                    let book = normalize_book(snapshot).map_err(|error| ReplayError::Payload {
                        line: line_number,
                        detail: error.to_string(),
                    })?;
                    events.push(ReplayEvent::OrderBook(book));
                }
                _ => {
                    // Unknown event types are deliberately skipped.
                }
            }
        }
    }

    Ok(events)
}

// ---------------------------------------------------------------------------
// Bus integration
// ---------------------------------------------------------------------------

/// Drive a recording through normalisation into the same bus topic shapes
/// used live.
///
/// - `md.ticks.polymarket`   for `Quote` events
/// - `md.books.polymarket`   for `OrderBook` events
///
/// Returns the total number of events replayed.
///
/// # Errors
///
/// Delegates to [`replay_jsonl`] for parse / normalisation errors and to the
/// [`MessageProducer`] for publish failures.
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
                        "md.ticks.polymarket",
                        Envelope::new("quote", quote),
                        Some(quote.market.as_str()),
                    )
                    .await?;
            }
            ReplayEvent::OrderBook(book) => {
                producer
                    .send(
                        "md.books.polymarket",
                        Envelope::new("order_book", book),
                        Some(book.market.as_str()),
                    )
                    .await?;
            }
        }
    }

    Ok(events.len())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aether_bus::producer::StubProducer;

    /// Minimal two-line recording: one `last_trade_price` tick and one `book`
    /// snapshot.  Used by both tests below.
    const RECORDING: &str = r#"{"received_ts":"2026-07-10T12:34:56.789Z","frame":{"event_type":"price_change","market":"0xabc","price_changes":[{"asset_id":"12345","price":"0.65","size":"10.0","side":"BUY","best_bid":"0.65","best_ask":"0.67"}],"timestamp":"1752150896000"}}
{"received_ts":"2026-07-10T12:34:57.000Z","frame":{"event_type":"book","asset_id":"12345","market":"0xabc","timestamp":"1752150897000","bids":[{"price":"0.65","size":"100.5"}],"asks":[{"price":"0.67","size":"50.2"}],"hash":"0xdef"}}
"#;

    #[test]
    fn identical_recordings_produce_identical_normalized_events() {
        let first = replay_jsonl(RECORDING).unwrap();
        let second = replay_jsonl(RECORDING).unwrap();

        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap(),
            "deterministic replay must produce byte-identical output"
        );
        assert_eq!(first.len(), 2, "two non-empty lines -> two events");
    }

    #[tokio::test]
    async fn recording_drives_bus_shaped_topics() {
        let producer = StubProducer::new();

        assert_eq!(
            replay_to_bus(RECORDING, &producer).await.unwrap(),
            2,
            "two events should be published"
        );

        let sent = producer.sent.lock().unwrap();
        assert_eq!(sent.len(), 2, "two events -> two bus messages");

        // First event goes to the tick topic.
        assert_eq!(sent[0].0, "md.ticks.polymarket", "price-change event publishes to ticks topic");

        // Second event goes to the book topic.
        assert_eq!(sent[1].0, "md.books.polymarket", "book event publishes to books topic");

        // Every envelope carries a payload.
        for (_, envelope) in sent.iter() {
            let value: serde_json::Value = serde_json::from_str(envelope).unwrap();
            assert!(value.get("payload").is_some(), "envelope must contain a payload field");
        }
    }
}

//! Deterministic replay harness for bus message streams.
//!
//! Records canonical-serialized events with SHA-256 fingerprints,
//! persists them to JSON files, loads them back, replays through
//! a user-supplied handler, and verifies output equivalence.
//!
//! # Example
//!
//! ```ignore
//! let mut cap = ReplayHarness::new_capture();
//! cap.record_event("opportunity", "trace-1", &opp).unwrap();
//! cap.persist(Path::new("capture.json")).unwrap();
//!
//! let replay = ReplayHarness::load(Path::new("capture.json")).unwrap();
//! let result = replay.replay_events(&mut |ev| Ok(ev.payload_bytes.clone())).unwrap();
//! assert!(result.matched);
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single captured event with its type tag and canonical-serialized payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedEvent {
    /// Monotonic sequence number (0-based).
    pub seq: u64,
    /// Trace or correlation identifier.
    pub trace_id: String,
    /// Event type discriminator (e.g. `"opportunity"`, `"fill"`).
    pub event_type: String,
    /// SHA-256 hex digest of the canonical payload bytes.
    pub payload_sha256: String,
    /// Canonical-serialized payload bytes (JSON).
    pub payload_bytes: Vec<u8>,
    /// Wall-clock offset in milliseconds from capture start.
    pub wall_clock_offset_ms: i64,
}

/// Result of comparing an original capture against a replay run.
#[derive(Debug, Clone)]
pub struct ReplayResult {
    /// Number of events compared.
    pub event_count: usize,
    /// Aggregate SHA-256 of all original payload bytes.
    pub original_sha256: String,
    /// Aggregate SHA-256 of all handler-produced output bytes.
    pub replay_sha256: String,
    /// `true` when the two aggregate hashes are identical.
    pub matched: bool,
    /// Per-event mismatch descriptions (empty when `matched` is `true`).
    pub mismatches: Vec<String>,
}

/// Errors produced by replay operations.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("capture not found at path: {0}")]
    CaptureNotFound(String),
    #[error("hash mismatch at event {index}: expected {expected}, got {actual}")]
    HashMismatch { index: usize, expected: String, actual: String },
    #[error("replay handler returned error: {0}")]
    Handler(String),
}

/// Whether the harness is recording or replaying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayMode {
    Capture,
    Replay,
}

// ---------------------------------------------------------------------------
// ReplayHarness
// ---------------------------------------------------------------------------

/// Deterministic replay harness.
///
/// Call `new_capture()` to start recording. Record events with `record_event()`.
/// Persist with `persist()`, reload with `load()`, and replay with `replay_events()`.
#[derive(Debug)]
pub struct ReplayHarness {
    events: Vec<CapturedEvent>,
    mode: ReplayMode,
    wall_start_ms: i64,
}

impl ReplayHarness {
    /// Create a new harness in capture mode.
    pub fn new_capture() -> Self {
        Self {
            events: Vec::new(),
            mode: ReplayMode::Capture,
            wall_start_ms: Self::now_ms(),
        }
    }

    /// Create a harness from previously captured events (replay mode).
    pub fn new_replay(events: Vec<CapturedEvent>) -> Self {
        Self {
            events,
            mode: ReplayMode::Replay,
            wall_start_ms: 0,
        }
    }

    /// Append an event to the capture buffer.
    ///
    /// The payload is serialized to canonical JSON bytes and SHA-256 hashed.
    /// Only valid in `Capture` mode — panics otherwise.
    ///
    /// # Panics
    ///
    /// Panics if called on a harness created with `new_replay()`.
    pub fn record_event(
        &mut self,
        event_type: &str,
        trace_id: &str,
        payload: &impl Serialize,
    ) -> Result<(), ReplayError> {
        assert!(
            self.mode == ReplayMode::Capture,
            "record_event called on a Replay-mode harness; \
             load capture data with ReplayHarness::new_replay() or use load()"
        );
        let payload_bytes = serde_json::to_vec(payload)?;
        let hash = hex_sha256(&payload_bytes);
        let offset = Self::now_ms() - self.wall_start_ms;
        self.events.push(CapturedEvent {
            seq: self.events.len() as u64,
            trace_id: trace_id.to_string(),
            event_type: event_type.to_string(),
            payload_sha256: hash,
            payload_bytes,
            wall_clock_offset_ms: offset,
        });
        Ok(())
    }

    /// Feed all captured events through `handler` and compare output hashes.
    ///
    /// The handler receives each `CapturedEvent` and must return the canonical
    /// output bytes that would have been produced during live processing.
    /// Returns a `ReplayResult` with per-event mismatch details.
    pub fn replay_events(
        &self,
        handler: &mut impl FnMut(&CapturedEvent) -> Result<Vec<u8>, ReplayError>,
    ) -> Result<ReplayResult, ReplayError> {
        let original_combined: Vec<u8> = self
            .events
            .iter()
            .flat_map(|e| e.payload_bytes.clone())
            .collect();
        let original_sha256 = hex_sha256(&original_combined);

        let mut replay_outputs: Vec<Vec<u8>> = Vec::with_capacity(self.events.len());
        for event in &self.events {
            let out = handler(event).map_err(|e| ReplayError::Handler(e.to_string()))?;
            replay_outputs.push(out);
        }

        let replay_combined: Vec<u8> = replay_outputs.iter().flat_map(|v| v.iter()).copied().collect();
        let replay_sha256 = hex_sha256(&replay_combined);

        let matched = original_sha256 == replay_sha256;
        let mut mismatches = Vec::new();
        if !matched {
            for (i, (orig, replay)) in self
                .events
                .iter()
                .zip(replay_outputs.iter())
                .enumerate()
            {
                let orig_hash = hex_sha256(&orig.payload_bytes);
                let replay_hash = hex_sha256(replay);
                if orig_hash != replay_hash {
                    mismatches.push(format!(
                        "event[{}] ({}): expected {} got {}",
                        i, orig.event_type, orig_hash, replay_hash
                    ));
                }
            }
        }

        Ok(ReplayResult {
            event_count: self.events.len(),
            original_sha256,
            replay_sha256,
            matched,
            mismatches,
        })
    }

    /// Persist the captured events to a JSON file.
    pub fn persist(&self, path: &Path) -> Result<(), ReplayError> {
        let json = serde_json::to_string_pretty(&self.events)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load a capture file previously written by `persist`.
    ///
    /// Returns `ReplayError::CaptureNotFound` when the path does not exist.
    pub fn load(path: &Path) -> Result<Self, ReplayError> {
        if !path.exists() {
            return Err(ReplayError::CaptureNotFound(
                path.display().to_string(),
            ));
        }
        let bytes = fs::read(path)?;
        let events: Vec<CapturedEvent> = serde_json::from_slice(&bytes)?;
        Ok(Self::new_replay(events))
    }

    /// Return a reference to the captured events.
    pub fn events(&self) -> &[CapturedEvent] {
        &self.events
    }

    /// Return the current mode.
    pub fn mode(&self) -> ReplayMode {
        self.mode
    }

    /// SHA-256 hex digest of arbitrary bytes.
    pub fn sha256(bytes: &[u8]) -> String {
        hex_sha256(bytes)
    }

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Debug, Clone, Serialize)]
    struct TestPayload {
        value: i32,
        label: String,
    }

    #[test]
    fn capture_and_replay_identity_handler_matches() {
        let mut harness = ReplayHarness::new_capture();

        let p1 = TestPayload { value: 42, label: "alpha".into() };
        let p2 = TestPayload { value: 99, label: "beta".into() };

        harness.record_event("test", "trace-a", &p1).unwrap();
        harness.record_event("test", "trace-b", &p2).unwrap();

        assert_eq!(harness.events().len(), 2);
        assert_eq!(harness.events()[0].seq, 0);
        assert_eq!(harness.events()[1].seq, 1);

        // Identity handler: return the original payload bytes unchanged.
        let result = harness
            .replay_events(&mut |ev: &CapturedEvent| Ok(ev.payload_bytes.clone()))
            .unwrap();

        assert!(result.matched, "identity handler must produce matching hashes");
        assert_eq!(result.event_count, 2);
        assert!(result.mismatches.is_empty());
    }

    #[test]
    fn persist_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capture.json");

        let mut harness = ReplayHarness::new_capture();
        let payload = TestPayload { value: 1, label: "persist".into() };
        harness.record_event("test", "trace-p", &payload).unwrap();

        // Persist
        harness.persist(&path).unwrap();

        // Load and verify metadata matches
        let loaded = ReplayHarness::load(&path).unwrap();
        assert_eq!(loaded.events().len(), 1);
        assert_eq!(loaded.events()[0].event_type, "test");
        assert_eq!(loaded.events()[0].trace_id, "trace-p");
        assert_eq!(
            loaded.events()[0].payload_sha256,
            harness.events()[0].payload_sha256
        );
    }

    #[test]
    fn hash_mismatch_is_detected() {
        let mut harness = ReplayHarness::new_capture();
        let payload = TestPayload { value: 100, label: "original".into() };
        harness.record_event("test", "trace-m", &payload).unwrap();

        // Handler returns tampered bytes.
        let result = harness
            .replay_events(&mut |_ev: &CapturedEvent| {
                Ok(br#"{"value":0,"label":"tampered"}"#.to_vec())
            })
            .unwrap();

        assert!(!result.matched);
        assert_eq!(result.mismatches.len(), 1);
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = ReplayHarness::load(Path::new("/nonexistent/capture.json"));
        match result {
            Err(ReplayError::CaptureNotFound(_)) => {}
            other => panic!("expected CaptureNotFound error, got: {other:?}"),
        }
    }

    #[test]
    fn sha256_output_is_hex_64_chars() {
        let hash = ReplayHarness::sha256(b"hello world");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sha256_is_deterministic() {
        let input = b"deterministic input";
        let h1 = ReplayHarness::sha256(input);
        let h2 = ReplayHarness::sha256(input);
        assert_eq!(h1, h2);
    }

    #[test]
    fn record_event_in_replay_mode_panics() {
        let harness = ReplayHarness::new_replay(vec![]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut h = harness;
            h.record_event("test", "t", &TestPayload { value: 0, label: "x".into() })
                .ok();
        }));
        assert!(result.is_err(), "record_event on replay harness should panic");
    }

    #[test]
    fn multiple_events_maintain_order() {
        let mut harness = ReplayHarness::new_capture();
        for i in 0..5 {
            let p = TestPayload { value: i, label: format!("ev-{i}") };
            harness.record_event("order", &format!("trace-{i}"), &p).unwrap();
        }
        assert_eq!(harness.events().len(), 5);
        for (i, ev) in harness.events().iter().enumerate() {
            assert_eq!(ev.seq, i as u64);
            assert_eq!(ev.trace_id, format!("trace-{i}"));
        }
    }

    #[test]
    fn event_type_and_sha256_are_recorded() {
        let mut harness = ReplayHarness::new_capture();
        let payload = TestPayload { value: 7, label: "check".into() };

        harness.record_event("custom.event", "trace-chk", &payload).unwrap();

        let ev = &harness.events()[0];
        assert_eq!(ev.event_type, "custom.event");
        assert_eq!(ev.payload_sha256.len(), 64);
        // Re-compute and compare
        let bytes = serde_json::to_vec(&payload).unwrap();
        let expected_hash = hex_sha256(&bytes);
        assert_eq!(ev.payload_sha256, expected_hash);
    }
}

//! Golden vector integration tests — verify canonical bytes against stored SHA-256.
//! Covers ALL SPEC-001 types across all generated golden files.

use aether_core::canonical::canonical_sha256;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct GoldenEntry {
    name: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    typ: String,
    value: serde_json::Value,
    sha256: String,
}

fn load_goldens(file: &str) -> Vec<GoldenEntry> {
    let path = format!("{}/../../testdata/golden/core/{}", env!("CARGO_MANIFEST_DIR"), file);
    let data = fs::read_to_string(&path).unwrap_or_else(|_| panic!("read golden file: {path}"));
    serde_json::from_str(&data).unwrap_or_else(|_| panic!("parse golden file: {path}"))
}

const ALL_FILES: &[&str] = &[
    "money.json",
    "market_key.json",
    "confidence.json",
    "edge.json",
    "quote.json",
    "order_book.json",
    "order_intent.json",
    "risk_verdict.json",
    "order.json",
    "fill.json",
    "position.json",
    "caps_snapshot.json",
    "market.json",
    "price_semantics.json",
    "opportunity.json",
    "audit_event.json",
    "error_envelope.json",
];

#[test]
fn all_golden_vectors_verify_sha256() {
    for file in ALL_FILES {
        let entries = load_goldens(file);
        assert!(!entries.is_empty(), "no entries in {file}");
        for entry in &entries {
            let hash = canonical_sha256(&entry.value).unwrap();
            assert_eq!(hash, entry.sha256, "{}::{}: SHA-256 mismatch", file, entry.name);
        }
    }
}

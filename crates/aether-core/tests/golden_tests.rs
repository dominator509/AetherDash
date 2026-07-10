//! Typed golden vector round-trip tests.
//! Every vector is deserialized into its real Rust type (enforcing all invariants),
//! re-serialized canonically, and verified against the stored SHA-256.
//! Unknown type labels are rejected.

use aether_core::canonical::canonical_sha256;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct GoldenEntry {
    name: String,
    #[serde(rename = "type")]
    typ: String,
    value: serde_json::Value,
    sha256: String,
}

fn load_goldens(file: &str) -> Vec<GoldenEntry> {
    let path = format!("{}/../../testdata/golden/core/{}", env!("CARGO_MANIFEST_DIR"), file);
    let data = fs::read_to_string(&path).unwrap_or_else(|_| panic!("read golden file: {path}"));
    serde_json::from_str(&data).unwrap_or_else(|_| panic!("parse golden file: {path}"))
}

fn verify_type<T: serde::Serialize + serde::de::DeserializeOwned>(
    file: &str,
    entry: &GoldenEntry,
    expected_type: &str,
) {
    assert_eq!(entry.typ, expected_type, "{}/{}: wrong type label", file, entry.name);
    let typed: T = serde_json::from_value(entry.value.clone()).unwrap_or_else(|e| {
        panic!("{}/{}: deserialize as {expected_type} failed: {e}", file, entry.name)
    });
    let hash = canonical_sha256(&typed)
        .unwrap_or_else(|e| panic!("{}/{}: canonical_sha256 failed: {e}", file, entry.name));
    assert_eq!(hash, entry.sha256, "{}/{}: typed SHA-256 mismatch", file, entry.name);
}

const FILES: &[&str] = &[
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
    // P1-7: Adversarial canonical vectors (cross-language)
    "unicode.json",
    "ordering.json",
    "null_omission.json",
    "empty_collections.json",
];

#[test]
fn typed_golden_round_trips() {
    for file in FILES {
        let entries = load_goldens(file);
        assert!(!entries.is_empty(), "{file}: no entries");
        for entry in &entries {
            match entry.typ.as_str() {
                "Money" => verify_type::<aether_core::ids::Money>(file, entry, "Money"),
                "MarketKey" => verify_type::<aether_core::ids::MarketKey>(file, entry, "MarketKey"),
                "Confidence" => {
                    verify_type::<aether_core::decimal::Confidence>(file, entry, "Confidence")
                }
                "EdgeDecomposition" => verify_type::<aether_core::opportunity::EdgeDecomposition>(
                    file,
                    entry,
                    "EdgeDecomposition",
                ),
                "Quote" => verify_type::<aether_core::quote::Quote>(file, entry, "Quote"),
                "OrderBook" => {
                    verify_type::<aether_core::quote::OrderBook>(file, entry, "OrderBook")
                }
                "OrderIntent" => {
                    verify_type::<aether_core::order::OrderIntent>(file, entry, "OrderIntent")
                }
                "RiskVerdict" => {
                    verify_type::<aether_core::order::RiskVerdict>(file, entry, "RiskVerdict")
                }
                "Order" => verify_type::<aether_core::order::Order>(file, entry, "Order"),
                "Fill" => verify_type::<aether_core::order::Fill>(file, entry, "Fill"),
                "Position" => verify_type::<aether_core::order::Position>(file, entry, "Position"),
                "CapsSnapshot" => {
                    verify_type::<aether_core::order::CapsSnapshot>(file, entry, "CapsSnapshot")
                }
                "Market" => verify_type::<aether_core::market::Market>(file, entry, "Market"),
                "PriceSemantics" => verify_type::<aether_core::market::PriceSemantics>(
                    file,
                    entry,
                    "PriceSemantics",
                ),
                "Opportunity" => {
                    verify_type::<aether_core::opportunity::Opportunity>(file, entry, "Opportunity")
                }
                "AuditEvent" => {
                    verify_type::<aether_core::audit::AuditEvent>(file, entry, "AuditEvent")
                }
                "ErrorEnvelope" => {
                    verify_type::<aether_core::error::ErrorEnvelope>(file, entry, "ErrorEnvelope")
                }
                other => panic!("{file}/{}: unknown type label: {other}", entry.name),
            }
        }
    }
}

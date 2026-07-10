//! Cross-language canonical adversarial tests.
//! Verifies that canonical serialization is byte-identical regardless of
//! insertion order, handles Unicode, null semantics, empty collections,
//! and nested maps correctly across all three language implementations.
//!
//! These tests serve as the Rust reference for P1-7 adversarial vectors.

use aether_core::canonical::{canonical_json_bytes, canonical_sha256};
use aether_core::ids::{MarketKey, Money, Ulid, VenueId};
use aether_core::json::JsonObject;
use aether_core::order::CapsSnapshot;
use aether_core::market::{InstrumentKind, Market, MarketStatus};
use aether_core::time::UtcTime;
use rust_decimal::Decimal;
use serde_json::json;

// ── Helpers ──────────────────────────────────────────────────────────

#[allow(clippy::unwrap_used)]
fn mk(v: &str, n: &str) -> MarketKey {
    MarketKey::new(&VenueId::new(v).unwrap(), n).unwrap()
}
fn ulid() -> Ulid {
    Ulid::new()
}
#[allow(clippy::unwrap_used)]
fn ts() -> UtcTime {
    UtcTime::from_unix_millis(1752150896789).unwrap()
}

// ── Unicode ──────────────────────────────────────────────────────────

#[test]
fn unicode_market_title_deterministic() {
    let m = Market {
        key: mk("kalshi", "UNICODE-1"),
        venue: VenueId::new("kalshi").unwrap(),
        kind: InstrumentKind::BinaryContract,
        title: "BTC — above $75k?".into(),   // em dash
        description_ref: "BTC-75K-JUL10".into(),
        status: MarketStatus::Open,
        close_ts: None,
        resolve_ts: None,
        outcome: None,
        jurisdiction_flags: vec!["US".into()],
        venue_ref: JsonObject::new(json!({"ticker": "BTC-75K"})).unwrap(),
        meta: JsonObject::new(json!({"tick_size": "0.01"})).unwrap(),
    };
    let bytes1 = canonical_json_bytes(&m).unwrap();
    let bytes2 = canonical_json_bytes(&m).unwrap();
    assert_eq!(bytes1, bytes2, "Unicode must produce deterministic bytes");
    let s = String::from_utf8(bytes1).unwrap();
    assert!(s.contains("BTC — above"), "em dash must be preserved, got: {s}");
}

#[test]
fn unicode_japanese_title_deterministic() {
    let m = Market {
        key: mk("kalshi", "JP-1"),
        venue: VenueId::new("kalshi").unwrap(),
        kind: InstrumentKind::BinaryContract,
        title: "日経平均が40,000円を超える".into(),
        description_ref: "N225-40K".into(),
        status: MarketStatus::Open,
        close_ts: None,
        resolve_ts: None,
        outcome: None,
        jurisdiction_flags: vec!["JP".into()],
        venue_ref: JsonObject::new(json!({"ticker": "N225"})).unwrap(),
        meta: JsonObject::new(json!({"unit": "yen"})).unwrap(),
    };
    let bytes1 = canonical_json_bytes(&m).unwrap();
    let bytes2 = canonical_json_bytes(&m).unwrap();
    assert_eq!(bytes1, bytes2);
}

// ── Reverse insertion order ──────────────────────────────────────────

#[test]
fn json_object_reverse_insertion_order_same_hash() {
    // Build two JsonObjects with the same keys inserted in opposite order
    let jo1 = JsonObject::new(json!({"a": "1", "b": "2", "c": "3"})).unwrap();
    let jo2 = JsonObject::new(json!({"c": "3", "b": "2", "a": "1"})).unwrap();
    let h1 = canonical_sha256(&jo1).unwrap();
    let h2 = canonical_sha256(&jo2).unwrap();
    assert_eq!(h1, h2, "Reverse insertion order must produce same hash");
}

#[test]
fn venue_ref_insertion_order_independent() {
    let t = ts();
    let mk_val = mk("kalshi", "BTC-75");
    let o1 = aether_core::order::Order {
        order_id: ulid(),
        market: mk_val.clone(),
        side: aether_core::order::Side::Buy,
        price: Decimal::new(66, 2),
        size: Decimal::new(10, 0),
        fee: Money::new(Decimal::new(50, 2), "USD"),
        venue_ref: JsonObject::new(json!({"order_ref": "kalshi-123", "venue": "kalshi"})).unwrap(),
        ts: t,
        paper: false,
    };
    let o2 = aether_core::order::Order {
        order_id: o1.order_id,
        market: mk_val,
        side: aether_core::order::Side::Buy,
        price: Decimal::new(66, 2),
        size: Decimal::new(10, 0),
        fee: Money::new(Decimal::new(50, 2), "USD"),
        venue_ref: JsonObject::new(json!({"venue": "kalshi", "order_ref": "kalshi-123"})).unwrap(),
        ts: t,
        paper: false,
    };
    let h1 = canonical_sha256(&o1).unwrap();
    let h2 = canonical_sha256(&o2).unwrap();
    assert_eq!(h1, h2, "venue_ref key order must not affect hash");
}

// ── Nested maps ──────────────────────────────────────────────────────

#[test]
fn nested_json_object_sorted_recursively() {
    let jo1 = JsonObject::new(json!({
        "outer": {
            "z": "last",
            "a": "first",
            "m": {"y": "Y", "x": "X"}
        }
    })).unwrap();
    let jo2 = JsonObject::new(json!({
        "outer": {
            "a": "first",
            "z": "last",
            "m": {"x": "X", "y": "Y"}
        }
    })).unwrap();
    let h1 = canonical_sha256(&jo1).unwrap();
    let h2 = canonical_sha256(&jo2).unwrap();
    assert_eq!(h1, h2, "Nested maps must be sorted recursively");
}

// ── Explicit null ────────────────────────────────────────────────────

#[test]
fn explicit_null_preserved_in_canonical() {
    // Rust: serialize null fields explicitly
    // Verify that a field with explicit null is serialized (not omitted)
    // Using serde_json::Value directly to test null behavior
    let v: serde_json::Value = json!({"field": null, "other": "value"});
    let bytes = canonical_json_bytes(&v).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    // serde_json serializes null fields by default
    assert!(s.contains("null"), "null must be serialized, got: {s}");
}

// ── Omitted optional fields ──────────────────────────────────────────

#[test]
fn omitted_optional_fields_not_in_output() {
    let q = aether_core::quote::Quote {
        market: mk("kalshi", "BTC-75"),
        bid: Some(Decimal::new(65, 2)),
        ask: None,
        mid: None,
        last: None,
        bid_size: None,
        ask_size: None,
        ts: ts(),
        source: aether_core::quote::QuoteSource::Snapshot,
        seq: None,
    };
    let bytes = canonical_json_bytes(&q).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    assert!(!s.contains("null"), "omitted optional fields must not appear, got: {s}");
    assert!(!s.contains("\"ask\":"), "None ask must not appear, got: {s}");
    assert!(s.contains("\"bid\":"), "Some bid must appear, got: {s}");
}

// ── Empty maps ───────────────────────────────────────────────────────

#[test]
fn empty_json_object_serializes_as_empty_braces() {
    let jo = JsonObject::new(json!({})).unwrap();
    let bytes = canonical_json_bytes(&jo).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    assert_eq!(s.trim(), "{}", "Empty JsonObject must serialize as {{}}");
}

#[test]
fn caps_snapshot_empty_maps_omitted_or_empty() {
    let cs = CapsSnapshot {
        version: ulid(),
        per_order_max: Money::new(Decimal::new(50000, 2), "USD"),
        daily_max: Money::new(Decimal::new(100000, 2), "USD"),
        per_venue: JsonObject::default(),
        per_kind: JsonObject::default(),
    };
    let bytes = canonical_json_bytes(&cs).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    // per_venue and per_kind should be omitted (skip_serializing_if empty)
    // OR serialized as {} if they're not skipped
    assert!(
        !s.contains("per_venue") || s.contains("\"per_venue\":{}"),
        "empty per_venue must be omitted or empty object, got: {s}"
    );
}

// ── Empty arrays ─────────────────────────────────────────────────────

#[test]
fn empty_jurisdiction_flags_preserved() {
    let m = Market {
        key: mk("kalshi", "EMPTY-FLAGS"),
        venue: VenueId::new("kalshi").unwrap(),
        kind: InstrumentKind::BinaryContract,
        title: "Empty flags test".into(),
        description_ref: "EMPTY-1".into(),
        status: MarketStatus::Open,
        close_ts: None,
        resolve_ts: None,
        outcome: None,
        jurisdiction_flags: vec![],
        venue_ref: JsonObject::new(json!({})).unwrap(),
        meta: JsonObject::new(json!({})).unwrap(),
    };
    let bytes = canonical_json_bytes(&m).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    assert!(s.contains("\"jurisdiction_flags\":[]"), "empty arrays must be preserved, got: {s}");
}

#[test]
fn empty_reasons_array_preserved() {
    let v = aether_core::order::RiskVerdict {
        intent_id: ulid(),
        verdict: aether_core::order::RiskVerdictStatus::Allow,
        reasons: vec![],
        ts: ts(),
    };
    let bytes = canonical_json_bytes(&v).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    // reasons vec is skip_serializing_if empty, so it may be omitted
    assert!(
        !s.contains("\"reasons\":") || s.contains("\"reasons\":[]"),
        "empty reasons must be omitted or [], got: {s}"
    );
}

// ── Decimal precision ────────────────────────────────────────────────

#[test]
fn decimal_precision_preserved() {
    let m1 = Money::new(Decimal::new(100, 2), "USD");   // "1.00"
    let m2 = Money::new(Decimal::new(1, 0), "USD");     // "1"
    let b1 = canonical_json_bytes(&m1).unwrap();
    let b2 = canonical_json_bytes(&m2).unwrap();
    let s1 = String::from_utf8(b1).unwrap();
    let s2 = String::from_utf8(b2).unwrap();
    assert!(s1.contains("\"1.00\""), "precision must be preserved: {s1}");
    assert!(s2.contains("\"1\""), "integer must not gain decimals: {s2}");
    // They must differ since precision differs
    assert_ne!(s1, s2, "different precision must produce different output");
}

// ── High-precision decimal ───────────────────────────────────────────

#[test]
fn high_precision_decimal_18_digits() {
    let m = Money::new(Decimal::new(123456789012345678i64, 18), "BTC");
    let bytes = canonical_json_bytes(&m).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    assert!(s.contains("0.123456789012345678"), "18-digit decimal must be preserved: {s}");
}

// ── Negative values ──────────────────────────────────────────────────

#[test]
fn negative_decimal_serialization() {
    let m = Money::new(Decimal::new(-5000, 2), "USD");
    let bytes = canonical_json_bytes(&m).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    assert!(s.contains("\"-50.00\""), "negative decimal must serialize correctly: {s}");
}

// ── Adversarial golden vector generation ────────────────────────────────
// Run with: cargo test -p aether-core --test canonical_adversarial_tests \
//           gen_adversarial_goldens -- --ignored --nocapture

#[test]
#[ignore]
fn gen_adversarial_goldens() {
    use serde::Serialize;
    use std::fs;

    #[derive(Serialize)]
    struct Entry {
        name: String,
        #[serde(rename = "type")]
        typ: String,
        value: serde_json::Value,
        sha256: String,
    }

    fn entry(name: &str, typ: &str, value: serde_json::Value) -> Entry {
        let bytes = canonical_json_bytes(&value).unwrap();
        use sha2::{Digest, Sha256};
        let hash = hex::encode(Sha256::digest(&bytes));
        Entry { name: name.into(), typ: typ.into(), value, sha256: hash }
    }

    let dir = format!("{}/../../testdata/golden/core", env!("CARGO_MANIFEST_DIR"));

    // --- unicode.json ---
    let entries: Vec<Entry> = vec![
        entry("unicode_em_dash", "Market", serde_json::to_value(Market {
            key: mk("kalshi", "UNICODE-1"),
            venue: VenueId::new("kalshi").unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: "BTC — above $75k?".into(),
            description_ref: "BTC-75K-JUL10".into(),
            status: MarketStatus::Open,
            close_ts: None, resolve_ts: None, outcome: None,
            jurisdiction_flags: vec!["US".into()],
            venue_ref: JsonObject::new(json!({"ticker": "BTC-75K"})).unwrap(),
            meta: JsonObject::new(json!({"tick_size": "0.01"})).unwrap(),
        }).unwrap()),
        entry("unicode_japanese", "Market", serde_json::to_value(Market {
            key: mk("kalshi", "JP-1"),
            venue: VenueId::new("kalshi").unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: "日経平均が40,000円を超える".into(),
            description_ref: "N225-40K".into(),
            status: MarketStatus::Open,
            close_ts: None, resolve_ts: None, outcome: None,
            jurisdiction_flags: vec!["JP".into()],
            venue_ref: JsonObject::new(json!({"ticker": "N225"})).unwrap(),
            meta: JsonObject::new(json!({"unit": "yen"})).unwrap(),
        }).unwrap()),
    ];
    let json = serde_json::to_string_pretty(&entries).unwrap();
    fs::write(format!("{dir}/unicode.json"), &json).unwrap();
    println!("Wrote {dir}/unicode.json");

    // --- ordering.json (multi-key dynamic maps via domain types) ---
    let t = ts();
    let ordering_entries: Vec<Entry> = vec![
        entry("order_multi_key_venue_ref", "Order", serde_json::to_value(
            aether_core::order::Order {
                order_id: ulid(),
                market: mk("kalshi", "ORDERING-1"),
                side: aether_core::order::Side::Buy,
                price: Decimal::new(66, 2),
                size: Decimal::new(10, 0),
                fee: Money::new(Decimal::new(50, 2), "USD"),
                venue_ref: JsonObject::new(json!({
                    "order_ref": "kalshi-123",
                    "venue": "kalshi",
                    "account": "main"
                })).unwrap(),
                ts: t,
                paper: false,
            }
        ).unwrap()),
    ];
    let json = serde_json::to_string_pretty(&ordering_entries).unwrap();
    fs::write(format!("{dir}/ordering.json"), &json).unwrap();
    println!("Wrote {dir}/ordering.json");

    // --- null_omission.json ---
    let entries: Vec<Entry> = vec![
        entry("explicit_null", "Money", serde_json::to_value(Money::new(Decimal::new(100, 2), "USD")).unwrap()),
        entry("omitted_optional", "Quote", serde_json::to_value(aether_core::quote::Quote {
            market: mk("kalshi", "BTC-75"),
            bid: Some(Decimal::new(65, 2)),
            ask: None, mid: None, last: None,
            bid_size: None, ask_size: None,
            ts: ts(),
            source: aether_core::quote::QuoteSource::Snapshot,
            seq: None,
        }).unwrap()),
    ];
    let json = serde_json::to_string_pretty(&entries).unwrap();
    fs::write(format!("{dir}/null_omission.json"), &json).unwrap();
    println!("Wrote {dir}/null_omission.json");

    // --- empty_collections.json ---
    let t = ts();
    let empty_entries: Vec<Entry> = vec![
        entry("empty_array_flags", "Market", serde_json::to_value(Market {
            key: mk("kalshi", "EMPTY-FLAGS"),
            venue: VenueId::new("kalshi").unwrap(),
            kind: InstrumentKind::BinaryContract,
            title: "Empty flags".into(),
            description_ref: "EMPTY-1".into(),
            status: MarketStatus::Open,
            close_ts: None, resolve_ts: None, outcome: None,
            jurisdiction_flags: vec![],
            venue_ref: JsonObject::new(json!({})).unwrap(),
            meta: JsonObject::new(json!({})).unwrap(),
        }).unwrap()),
        entry("empty_risk_reasons", "RiskVerdict", serde_json::to_value(
            aether_core::order::RiskVerdict {
                intent_id: ulid(),
                verdict: aether_core::order::RiskVerdictStatus::Allow,
                reasons: vec![],
                ts: t,
            }
        ).unwrap()),
        entry("empty_caps_maps", "CapsSnapshot", serde_json::to_value(
            aether_core::order::CapsSnapshot {
                version: ulid(),
                per_order_max: Money::new(Decimal::new(50000, 2), "USD"),
                daily_max: Money::new(Decimal::new(100000, 2), "USD"),
                per_venue: aether_core::json::JsonObject::default(),
                per_kind: aether_core::json::JsonObject::default(),
            }
        ).unwrap()),
    ];
    let json = serde_json::to_string_pretty(&empty_entries).unwrap();
    fs::write(format!("{dir}/empty_collections.json"), &json).unwrap();
    println!("Wrote {dir}/empty_collections.json");

    println!("All adversarial golden vectors generated in {dir}");
}

// ── Determinism across runs ──────────────────────────────────────────

#[test]
fn canonical_bytes_fully_deterministic() {
    let m = Market {
        key: mk("kalshi", "DET-1"),
        venue: VenueId::new("kalshi").unwrap(),
        kind: InstrumentKind::BinaryContract,
        title: "Determinism test".into(),
        description_ref: "DET-1".into(),
        status: MarketStatus::Open,
        close_ts: None,
        resolve_ts: None,
        outcome: None,
        jurisdiction_flags: vec!["US".into(), "EU".into()],
        venue_ref: JsonObject::new(json!({"ticker": "DET-1", "venue": "kalshi"})).unwrap(),
        meta: JsonObject::new(json!({"tick_size": "0.01"})).unwrap(),
    };
    // Multiple serializations must produce identical bytes
    let prev = canonical_json_bytes(&m).unwrap();
    for _ in 0..100 {
        let cur = canonical_json_bytes(&m).unwrap();
        assert_eq!(prev, cur, "canonical bytes must be fully deterministic");
    }
}

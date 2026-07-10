//! Golden vector generator — produces test vectors with SHA-256 hashes.
//! Run: cargo run -p aether-core --features golden-gen --bin gen-goldens
//! Feature-gated so D1 stays clean (no IO in core lib).

use aether_core::canonical::canonical_sha256;
use aether_core::decimal::Confidence;
use aether_core::ids::{MarketKey, Money};
use aether_core::opportunity::{EdgeCosts, EdgeDecomposition};
use rust_decimal::Decimal;
use serde::Serialize;
use std::fs;

#[derive(Serialize)]
struct GoldenEntry {
    name: String,
    #[serde(rename = "type")]
    typ: String,
    value: serde_json::Value,
    sha256: String,
}

fn main() {
    let out_dir = "testdata/golden/core";
    fs::create_dir_all(out_dir).expect("create testdata/golden/core");

    // ── Money vectors ──
    let money_vecs: Vec<GoldenEntry> = [
        ("money_usd", Money::new(Decimal::new(12345, 2), "USD")),
        ("money_zero", Money::zero("USDC")),
        ("money_negative", Money::new(Decimal::new(-5000, 2), "USD")),
        ("money_18dec", Money::new(Decimal::new(123456789012345678i64, 18), "BTC")),
    ]
    .into_iter()
    .map(|(name, val)| make_golden(name, "Money", &val))
    .collect();
    write_goldens(out_dir, "money.json", &money_vecs);

    // ── MarketKey vectors ──
    let mk_vecs: Vec<GoldenEntry> = [
        ("market_key_kalshi", MarketKey::from_string("mkt:kalshi:BTC-75")),
        ("market_key_polymarket", MarketKey::from_string("mkt:polymarket:WILL-BTC-TOUCH-100K")),
    ]
    .into_iter()
    .map(|(name, val)| make_golden(name, "MarketKey", &val))
    .collect();
    write_goldens(out_dir, "market_key.json", &mk_vecs);

    // ── Confidence vectors ──
    let conf_vecs: Vec<GoldenEntry> = [
        ("confidence_zero", Confidence::new(Decimal::ZERO).unwrap()),
        ("confidence_half", Confidence::new(Decimal::new(5, 1)).unwrap()),
        ("confidence_one", Confidence::new(Decimal::ONE).unwrap()),
    ]
    .into_iter()
    .map(|(name, val)| make_golden(name, "Confidence", &val))
    .collect();
    write_goldens(out_dir, "confidence.json", &conf_vecs);

    // ── EdgeDecomposition vectors ──
    let edge_vecs: Vec<GoldenEntry> = [
        ("edge_positive_net", {
            EdgeDecomposition::compute(
                Decimal::new(100, 2),
                EdgeCosts {
                    fees: Decimal::new(10, 2),
                    slippage_est: Decimal::new(5, 2),
                    funding_cost: Decimal::ZERO,
                    gas_cost: Decimal::new(2, 2),
                    bridge_cost: Decimal::ZERO,
                    settlement_mismatch_discount: Decimal::ZERO,
                    liquidity_haircut: Decimal::new(3, 2),
                    staleness_penalty: Decimal::ZERO,
                    confidence_penalty: Decimal::ZERO,
                },
            )
        }),
        ("edge_zero_net", {
            EdgeDecomposition::compute(
                Decimal::new(20, 2),
                EdgeCosts {
                    fees: Decimal::new(10, 2),
                    slippage_est: Decimal::new(5, 2),
                    funding_cost: Decimal::ZERO,
                    gas_cost: Decimal::new(2, 2),
                    bridge_cost: Decimal::ZERO,
                    settlement_mismatch_discount: Decimal::ZERO,
                    liquidity_haircut: Decimal::new(3, 2),
                    staleness_penalty: Decimal::ZERO,
                    confidence_penalty: Decimal::ZERO,
                },
            )
        }),
        ("edge_explicit_zeros", {
            EdgeDecomposition::compute(
                Decimal::new(10, 2),
                EdgeCosts {
                    fees: Decimal::ZERO,
                    slippage_est: Decimal::ZERO,
                    funding_cost: Decimal::ZERO,
                    gas_cost: Decimal::ZERO,
                    bridge_cost: Decimal::ZERO,
                    settlement_mismatch_discount: Decimal::ZERO,
                    liquidity_haircut: Decimal::new(10, 2),
                    staleness_penalty: Decimal::ZERO,
                    confidence_penalty: Decimal::ZERO,
                },
            )
        }),
    ]
    .into_iter()
    .map(|(name, val)| make_golden(name, "EdgeDecomposition", &val))
    .collect();
    write_goldens(out_dir, "edge.json", &edge_vecs);

    println!("Golden vectors generated in {out_dir}");
}

fn make_golden<T: Serialize>(name: &str, typ: &str, value: &T) -> GoldenEntry {
    let json_val = serde_json::to_value(value).expect("serialize value");
    let sha256 = canonical_sha256(value).expect("compute sha256");
    GoldenEntry { name: name.into(), typ: typ.into(), value: json_val, sha256 }
}

fn write_goldens(dir: &str, file: &str, entries: &[GoldenEntry]) {
    let json = serde_json::to_string_pretty(entries).expect("serialize goldens");
    fs::write(format!("{dir}/{file}"), json).expect("write golden file");
}

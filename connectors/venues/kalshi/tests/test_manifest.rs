use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
struct Manifest {
    slug: String,
    display_name: String,
    pack_version: String,
    capabilities: Vec<String>,
    asset_kinds: Vec<String>,
    jurisdictions: Jurisdictions,
    endpoints: Endpoints,
    rate_limits: RateLimits,
    data_sources: BTreeMap<String, DataSource>,
    freshness: Freshness,
}

#[derive(Debug, Deserialize)]
struct Jurisdictions {
    allowed: Vec<String>,
    blocked: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Endpoints {
    prod: String,
    sandbox: String,
}

#[derive(Debug, Deserialize)]
struct RateLimits {
    rest_per_min: u32,
    ws_subscriptions: u32,
}

#[derive(Debug, Deserialize)]
struct DataSource {
    rung: String,
}

#[derive(Debug, Deserialize)]
struct Freshness {
    tick_stale_ms: u64,
    book_stale_ms: u64,
}

#[test]
fn manifest_loads_with_the_spec_009_schema() {
    let manifest: Manifest = toml::from_str(include_str!("../venue.toml")).unwrap();
    assert_eq!(manifest.slug, "kalshi");
    assert_eq!(manifest.display_name, "Kalshi");
    assert_eq!(manifest.pack_version, "0.1.0");
    assert_eq!(manifest.capabilities, ["markets", "ticks", "books", "orders", "balances"]);
    assert_eq!(manifest.asset_kinds, ["binary_contract", "categorical_contract"]);
    assert_eq!(manifest.jurisdictions.allowed, ["US"]);
    assert!(manifest.jurisdictions.blocked.is_empty());
    assert_eq!(manifest.endpoints.prod, "https://external-api.kalshi.com");
    assert_eq!(manifest.endpoints.sandbox, "https://external-api.demo.kalshi.co");
    assert_eq!(manifest.rate_limits.rest_per_min, 100);
    assert_eq!(manifest.rate_limits.ws_subscriptions, 100);
    assert_eq!(manifest.data_sources["markets"].rung, "official_api");
    assert_eq!(manifest.data_sources["ticks"].rung, "official_api");
    assert_eq!(manifest.freshness.tick_stale_ms, 2_000);
    assert_eq!(manifest.freshness.book_stale_ms, 500);
}

#[test]
fn registry_seed_matches_manifest_identity_and_shape() {
    let migration = include_str!("../../../../infra/migrations/0025_seed_kalshi_venue.up.sql");
    assert!(migration.contains("'kalshi'"));
    assert!(migration.contains("'[\"markets\",\"ticks\",\"books\",\"orders\",\"balances\"]'"));
    assert!(migration.contains("'{\"allowed\":[\"US\"],\"blocked\":[]}'"));
    assert!(migration.contains("'0.1.0'"));
    assert!(migration.contains("ON CONFLICT (slug) DO NOTHING"));
}

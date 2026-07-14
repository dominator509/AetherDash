//! Manifest validation tests for the Hyperliquid venue pack.
//!
//! Ensures `venue.toml` parses correctly, matches the SPEC-009 shape,
//! and proves the migration matches the pack identity.

#![allow(dead_code)]

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct VenueManifest {
    slug: String,
    display_name: String,
    pack_version: String,
    capabilities: Vec<String>,
    asset_kinds: Vec<String>,
    jurisdictions: Jurisdictions,
    #[serde(default)]
    endpoints: Endpoints,
    #[serde(default)]
    rate_limits: RateLimits,
    #[serde(default)]
    data_sources: DataSources,
    #[serde(default)]
    freshness: Freshness,
}

#[derive(Debug, Deserialize)]
struct Jurisdictions {
    allowed: Vec<String>,
    blocked: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Endpoints {
    prod: Option<String>,
    #[serde(default)]
    sandbox: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RateLimits {
    rest_per_min: Option<u32>,
    ws_subscriptions: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct DataSources {
    #[serde(default)]
    markets: Option<DataSourceRef>,
    #[serde(default)]
    ticks: Option<DataSourceRef>,
    #[serde(default)]
    books: Option<DataSourceRef>,
    #[serde(default)]
    resolution: Option<DataSourceRef>,
}

#[derive(Debug, Deserialize)]
struct DataSourceRef {
    rung: String,
}

#[derive(Debug, Deserialize, Default)]
struct Freshness {
    tick_stale_ms: Option<u64>,
    book_stale_ms: Option<u64>,
}

fn load_manifest() -> VenueManifest {
    let raw = include_str!("../venue.toml");
    #[allow(clippy::expect_used)]
    toml::from_str(raw).expect("Hyperliquid venue.toml must be valid TOML")
}

#[test]
fn manifest_has_correct_slug() {
    let m = load_manifest();
    assert_eq!(m.slug, "hyperliquid");
}

#[test]
fn manifest_is_read_only() {
    let m = load_manifest();
    assert!(m.capabilities.contains(&"markets".to_string()));
    assert!(m.capabilities.contains(&"ticks".to_string()));
    assert!(m.capabilities.contains(&"books".to_string()));
    assert!(!m.capabilities.contains(&"orders".to_string()));
    assert!(!m.capabilities.contains(&"balances".to_string()));
    assert_eq!(m.capabilities.len(), 3);
}

#[test]
fn manifest_blocks_us_jurisdiction() {
    let m = load_manifest();
    assert!(m.jurisdictions.allowed.is_empty());
    assert!(m.jurisdictions.blocked.contains(&"US".to_string()));
}

#[test]
fn manifest_declares_perp_and_spot_assets() {
    let m = load_manifest();
    assert!(m.asset_kinds.contains(&"perp".to_string()));
    assert!(m.asset_kinds.contains(&"spot".to_string()));
}

#[test]
fn manifest_has_valid_endpoints() {
    let m = load_manifest();
    assert!(m.endpoints.prod.is_some());
    assert!(m.endpoints.prod.as_deref().unwrap().starts_with("https://"));
    assert!(m.endpoints.sandbox.is_none(), "Hyperliquid does not publish a sandbox endpoint");
}

#[test]
fn manifest_data_sources_are_official_api() {
    let m = load_manifest();
    for source in [
        m.data_sources.markets.as_ref(),
        m.data_sources.ticks.as_ref(),
        m.data_sources.books.as_ref(),
    ]
    .iter()
    .flatten()
    {
        assert_eq!(source.rung, "official_api", "all data sources must be official_api rung");
    }
}

#[test]
fn manifest_freshness_has_reasonable_defaults() {
    let m = load_manifest();
    let tick_ms = m.freshness.tick_stale_ms.unwrap_or(0);
    let book_ms = m.freshness.book_stale_ms.unwrap_or(0);
    assert!(tick_ms > 0, "tick_stale_ms must be positive");
    assert!(book_ms > 0, "book_stale_ms must be positive");
    assert!(tick_ms <= 30_000, "tick_stale_ms should be <= 30s");
    assert!(book_ms <= 10_000, "book_stale_ms should be <= 10s");
}

#[test]
fn manifest_rate_limits_are_positive() {
    let m = load_manifest();
    assert!(m.rate_limits.rest_per_min.unwrap_or(0) > 0);
}

#[test]
fn migration_0027_matches_pack_identity() {
    let m = load_manifest();
    let migration = include_str!("../../../../infra/migrations/0027_seed_hyperliquid_venue.up.sql");
    assert!(migration.contains("'hyperliquid'"), "migration must reference slug 'hyperliquid'");
    let expected_caps = serde_json::to_string(&m.capabilities).unwrap();
    let cap_fragment = &expected_caps[1..expected_caps.len() - 1]; // strip brackets
    assert!(
        migration.contains(cap_fragment),
        "migration capabilities must match manifest: {}",
        expected_caps
    );
}

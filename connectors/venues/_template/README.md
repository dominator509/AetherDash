# AETHER Terminal -- Venue Pack Template

This directory is a **skeleton template** for implementing a new venue adapter
in the AETHER Terminal monorepo.  Copy this directory, rename it, and fill in
the `TODO` / `TO COMPLETE` markers.

## Quick start

```bash
cp -r connectors/venues/_template connectors/venues/my-venue
# Edit Cargo.toml: package.name, description, dependencies
# Edit venue.toml: slug, display_name, endpoints, capabilities
# Implement auth.rs, normalize.rs, orders.rs, health.rs
```

## Reference

- **ARCHITECTURE.md §13** -- Venue Pack Interface (pack lifecycle, gRPC contract)
- **SPEC-009** -- Canonical market normalisation rules
- **Kalshi venue pack** (`connectors/venues/kalshi/`) -- Complete reference implementation
  with RSA auth, REST client, market normalisation, WebSocket streams, orders, and health

## Steps

1. **`Cargo.toml`** -- Set the package name to `aether-venue-{slug}`.  Add
   dependencies your venue needs (HTTP client, crypto, WebSocket, etc.).
2. **`venue.toml`** -- Fill in the venue manifest (slug, capabilities,
   endpoints, rate limits).
3. **`src/auth.rs`** -- Implement venue-specific authentication (API key,
   RSA/Ed25519 signing, OAuth, EIP-712, etc.).
4. **`src/normalize.rs`** -- Map the venue's raw market format to
   `aether_core::Market` per SPEC-009.
5. **`src/orders.rs`** -- Implement order lifecycle (submit, cancel, balances).
6. **`src/health.rs`** -- Implement health checks / latency probes.
7. **`tests/`** -- Write integration tests with recorded fixtures.

## Workspace integration

Add the crate to the root `Cargo.toml` workspace members and (optionally)
`default-members` for CI builds.

## Demo gating

For venues with a demo/live separation, gate order operations behind an
`AETHER_VENUE__{SLUG}_DEMO=true` environment variable to prevent accidental
live trading.

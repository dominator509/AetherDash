# Changelog

All notable changes to AETHER Terminal will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- EP-000: Repository discovery & blueprint pack installation
- EP-001: Monorepo scaffold with Rust (cargo), TypeScript (pnpm), and Python (uv) workspaces
- EP-002: Core domain types (`crates/aether-core`), proto contracts (`proto/aether/core/v1/`), TS mirror (`packages/types`), Python mirror (`pylib/aether_py`)
  - 17 SPEC-001 types: Ulid, MarketKey, VenueId, Money, InstrumentKind, Market, Quote, OrderBook, OrderIntent, RiskVerdict, Order, Fill, Position, Opportunity, EdgeDecomposition, AuditEvent, ErrorEnvelope
  - Canonical JSON serialization with cross-language SHA-256 verification (Rust ↔ TypeScript ↔ Python)
  - Feature-gated golden vector generator (`gen-goldens` binary)
  - Deserialize-guarded constructors on all invariant-bearing types

[Unreleased]: https://github.com/operator/aetherdash/compare/v0.1.0...HEAD

# Changelog

All notable changes to AETHER Terminal will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- EP-207 recall v2: add a versioned graded nDCG/MRR benchmark, bounded one-hop Kuzu entity/market expansion, kind-specific age decay, EP-206 source-reliability weighting, optional cache-first local reranking, and a 100 ms wall-clock breaker that returns immutable v1 results on overload or stage failure.
- EP-206 ingestion fleet: add durable source/rung audit, six compliance-ladder adapters with explicit downgrade evidence, bounded scheduler cursor semantics, real RapidOCR/ONNX screenshot reprocessing, source reliability scoring, loopback health/readiness/metrics/audit surfaces, and a hardened single-scheduler systemd deployment.
- EP-307 completion repair: add the production bus-driven scanner, durable open-chain lifecycle/outbox, gateway feed surfacing, venue-adjacent conservative fee schedules, real Rust-backed simulator fill/sensitivity output, canonical scan histogram/counters, deterministic three-venue restart replay, and Postgres-backed closure/attribution evidence.
- EP-306 S8/M5 repair: replace the Wallet Guardian banner stub with a loopback tonic service backed by durable proposal/event storage, credential-only key and TOTP custody, current grant/session revalidation, authoritative reference pricing and value derivation, atomic single-use step-up approval, exact contract-selector policy, a stdin-only Guardian action client, and restart-safe custody broadcast jobs with durable nonces and receipt reconciliation.
- EP-203/EP-308 hardening: wire MCP `sim.run` to the canonical Rust simulator; add the loopback action service and `aether-execute-paper` transport that reload current grants/approvals, run shared router authz/risk, persist fills, and drain the outbox before completion; and add signed SMS, TLS email, plus single-use audited approval flows with mandatory client step-up for live-order and Guardian actions.
- EP-000: Repository discovery & blueprint pack installation
- EP-001: Monorepo scaffold with Rust (cargo), TypeScript (pnpm), and Python (uv) workspaces
- EP-002: Core domain types (`crates/aether-core`), proto contracts (`proto/aether/core/v1/`), TS mirror (`packages/types`), Python mirror (`pylib/aether_py`)
  - 17 SPEC-001 types: Ulid, MarketKey, VenueId, Money, InstrumentKind, Market, Quote, OrderBook, OrderIntent, RiskVerdict, Order, Fill, Position, Opportunity, EdgeDecomposition, AuditEvent, ErrorEnvelope
  - Canonical JSON serialization with cross-language SHA-256 verification (Rust <=> TypeScript <=> Python)
  - Feature-gated golden vector generator (`gen-goldens` binary)
  - Deserialize-guarded constructors on all invariant-bearing types
- EP-003: Data & persistence substrate
  - Dev compose stack: Postgres+pgvector, ClickHouse, Qdrant, Redis, Redpanda, MinIO, Kuzu
  - sqlx migrations (Postgres, paired up/down), ClickHouse idempotent DDL apply (`infra/clickhouse/apply.sh`)
  - Database setup commands in COMMANDS.md, integration test harness (`scripts/test-integration.sh`)
- EP-004: Service contracts & event bus
  - gRPC proto contracts (buf-managed) in `proto/`
  - Real rdkafka producer/consumer with trace headers via `crates/aether-bus`
  - Quarantine producer with MinIO storage (raw-payload isolation)
  - PostgreSQL sessions/grants, DB-backed auth, WS gateway skeleton
  - Python + TS proto type mirrors with cross-language round-trip verification
- EP-101: Tauri v2 desktop shell with keyboard navigation, command-line launch, encrypted local cache
- EP-102: Opportunity feed and explain views (mark-to-market, attribution, market-context panels)
- EP-103: Command room harness (MCP client, slash commands, tier surface, agent feedback)
- EP-104: Advanced panels: undockable layout, order books, depth-of-market (active)
- EP-201: Brain v1 object model with provenance tracking, tiered recall, vault-view generator
- EP-202: LLM router with cache-first prompt construction, LiteLLM library-mode integration, local fallback
- EP-203: Alert engine & comms (Telegram, Discord, Slack, inline action buttons) (revise)
- EP-204: Agentic inbox (Gmail Pub/Sub push, MS Graph webhooks, parse/scan/file pipeline)
- EP-205: Research swarms & decision packets (draft)
- EP-206: Ingestion fleet, OCR pipeline, source-reliability scoring (active)
- EP-207: Tiered recall v2: hybrid fusion, graph traversal, decay-based scoring, cross-encoder rerank (active)
- EP-301: Kalshi reference venue pack with current RSA-PSS authentication, fixed-point REST/WebSocket normalization, deterministic scrubbed replay fixtures, raw-payload quarantine, demo-only V2 order contract tests, rate-limit/backoff enforcement, health/feed-lag reporting, registry migration, and the reusable venue template
- EP-302: Read-only Polymarket venue pack with Gamma discovery, current CLOB REST/WebSocket schemas, fixed-point probability normalization, Polygon CTF resolution reads, deterministic scrubbed replay, quarantine, health reporting, and registry migration
- EP-303: Venue packs: Hyperliquid read-only adapter, OpenBB data provider foundation, Alpaca paper-trading adapter
- EP-304: Paper trading ledger & fill recording
  - Shared deterministic fixed-point fill model and bus-driven paper ledger
  - Pessimistic two-sided depth exhaustion, correct long/short/partial/flip P&L
  - Transactional paper-segregated Postgres writes, lifecycle attribution
  - Quote-driven unrealized P&L, restart idempotency, durable `orders.fills` outbox
- EP-305: Order router & risk engine (paper-first with caps, liveness, liquidity, jurisdiction checks)
- EP-306: Wallet Guardian & WalletConnect v2 integration (isolated signing policy service)
- EP-307: Arbitrage scanner & trade simulator with net-edge math
- EP-401: Shared five-tier authorization with fail-closed hard-denies, Argon2id local-password primitives, RFC 6238 TOTP and single-use step-up challenges, opaque hashed sessions, immediate grant revocation/scopes, append-only caps activation and lower-of-two enforcement, connection-bound gateway confirmations, and MCP tool enforcement
- EP-402: Audit chain end-to-end with hash-chained append-only audit log, hourly incremental verify, and full P&L attribution lifecycle
- EP-403: Plugin runtime with signed manifests, sandboxed execution, and capability-scoped host (active)
- EP-404: Observability baseline
  - Structured JSON logging (tracing/structlog) with redaction layer for key patterns
  - Prometheus `/metrics` endpoints on every service
  - Core metric series: `aether_llm_cache_hit_ratio`, `aether_scan_cycle_ms`, `aether_order_submit_latency_ms`, `aether_router_decisions_total`, `aether_guardian_proposals_total`, `aether_feed_lag_ms`, `aether_brain_recall_latency_ms`, `aether_alerts_sent_total`, `aether_opportunity_lifecycle_open`, `aether_audit_chain_verified`
  - `/healthz` (liveness) and `/readyz` (readiness) endpoints on all services
  - Trace ID propagation end-to-end (gateway -> bus -> services)
  - Alert rules defined: audit-chain failure (SEV1), feed lag (SEV2), router reject-all / guardian queue stuck (SEV2), disk/backup/audit (SEV3), LLM cache-hit ratio regression (SEV3)
- EP-405: Testing hardening: replay harness with deterministic fixture replay, lifecycle assertions, regression test suite (active)
- EP-406: Code-writing agent, cron jobs, backtesting agent (draft)
- EP-407: Deployment & release engineering: systemd units per plane host, compose bundles, production compose variant (`compose.prod.yml`), deploy tooling, rollback procedure (active)

### EP-408: Production readiness closure
- `scripts/production-readiness-check.sh`: Full production readiness gate that chains verify.sh -> integration tests -> e2e tests -> security-check.sh -> dependency-audit.sh -> smoke-test.sh -> health-check.sh, then audits PRODUCTION_READINESS.md for unchecked REQUIRED items
- `aether-blueprint/PRODUCTION_READINESS.md`: Comprehensive v1 gate checklist with evidence references for all 12 checklist sections (functional completeness, testing, security, performance, accessibility, observability, deployment, documentation)
- `scripts/backup.sh`: Database backup script for Postgres (pg_dump -Fc), ClickHouse (BACKUP DATABASE or SELECT fallback), Kuzu (tarball), and Qdrant (snapshot API) with configurable per-service retention policies and automatic pruning
- `scripts/restore.sh`: Database restore script with dry-run mode, per-service restore commands, and HUMAN-supervised guard (S6 requires --confirm flag)
- `scripts/health-check.sh`: Comprehensive health endpoint checker curling /healthz, /readyz, /ping for every service defined in ENVIRONMENT.md and OBSERVABILITY.md, including infrastructure (Postgres, ClickHouse, Redis, Qdrant, MinIO, Redpanda), app services (Gateway, Brain, LLM Router, Alerts, Inbox), gRPC services via grpc-health-probe (Order Router, Risk Engine, Wallet Guardian), and venue adapters (Kalshi, Polymarket, Hyperliquid, Alpaca, OpenBB)
- CHANGELOG.md: Updated with complete EP-102 through EP-408 entries

### Changed
- `scripts/production-readiness-check.sh`: Upgraded from stub to full gate with integrated health-check, per-step pass/fail reporting, and PRODUCTION_READINESS.md checklist auditing
- EP-306 S8 hardening: serialize rolling-limit reservation across concurrent proposals, count all live proposal exposure, fail closed on malformed EIP-1559 signing fields, consume approval challenges after five invalid TOTP attempts, and enforce the 24-hour destination/selector allowlist cooldown at Guardian startup.
- EP-307 audit hardening: replaced the no-op scanner path with deterministic canonical opportunity generation/publication; added same-event, status, venue, freshness, dedupe-kind, and bounded-work gates; loaded settlement mismatch discounts from the router-owned table; preserved negative net edge under the canonical sum law; and made simulator fill errors fail closed.
- EP-307 status remains `revise` only for the operator-owned literal 24-hour paper-run artifact; all six code milestones and accelerated database-backed evidence gates are complete.

[Unreleased]: https://github.com/operator/aetherdash/compare/v0.1.0...HEAD

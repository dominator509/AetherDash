Layer: 5 - Execution

# EP-303: Venue Packs - Hyperliquid (Read), OpenBB (Foundation), Alpaca (Paper)

**Band:** 3xx Connectors | **Phase:** 1 | **Status:** implemented (local validation complete; owner live checks deferred) | **Blocked by:** EP-301

## Purpose / Big Picture
Complete the Phase-1 asset-class spread: crypto perps data (Hyperliquid), TradFi equities/options data (OpenBB), and the first order-capable brokerage in paper mode (Alpaca). Three packs, one plan - each independently shippable, all template-derived.

## Scope
Three packs from `_template`: `hyperliquid/` (info API: mids, books, funding - read-only), `openbb/` (Python pack: equities/options quotes + reference data via the OpenBB platform library), `alpaca/` (paper trading: markets/ticks/orders/balances against the paper endpoint). Fixtures + replay for each; seed migrations; AGPL flag handling for OpenBB.

## Non-goals
No Hyperliquid execution (Phase 2+ decision, needs Guardian + jurisdiction review); no live Alpaca (paper endpoint only - live is EP-305 `live_enabled` domain and a brokerage-config ceremony); no OpenBB "everything" (foundation = quotes/reference for the instruments the scanner needs, not the full platform surface); no core edits (INV-7 x3).

## Context and Orientation
SPEC-009 allows non-Rust packs where the ecosystem demands it: OpenBB is Python, so that pack is a Python gRPC service implementing the same proto contract (the contract is the boundary, not the language - D7). PROJECT_BRIEF flags OpenBB AGPL exposure: the pack isolates OpenBB as a separate service (network boundary), and the flag is re-recorded in the Decision Log for the Phase-5 legal review. A-13 fixes Alpaca-paper as the first brokerage.

## Files to Read First
1. SPEC-009; the `_template` README; EP-301/302 Decision Logs (template deviations to date).
2. A-13, A-15 context; PROJECT_BRIEF AGPL flag; SPEC-001 kinds (perp, equity, option).
3. Each venue's API docs (record versions).

## Files to Change (Expected Changed Files)
`connectors/venues/hyperliquid/**` (Rust), `connectors/venues/openbb/**` (Python: pyproject via uv member, service, adapter), `connectors/venues/alpaca/**` (Rust), `testdata/{hyperliquid,openbb,alpaca}/**`, three seed migrations, workspace member appends (cargo x2, uv x1), ENVIRONMENT rows finalized for `AETHER_VENUE__HYPERLIQUID_*`, CHANGELOG, this file.

## Interfaces and Contracts
Hyperliquid: capabilities `["markets","ticks","books"]`, kinds `perp|spot`, funding rate exposed in perpetual Market meta (SPEC-012 funding_cost input); perpetual MarketKey `mkt:hyperliquid:{coin}` and spot MarketKey `mkt:hyperliquid:@{index}`. OpenBB: capabilities `["markets","ticks"]` (quotes; books N/A), kinds `equity|option`; MarketKey `mkt:openbb:{symbol}` / `{occ_symbol}` for options. Alpaca: capabilities `["markets","ticks","orders","balances"]`, kind `equity`, paper endpoint fixed in manifest; order port loopback (router-only).

## Milestones
1. **Hyperliquid pack.** Info API mids/books/funding -> normalized stream; funding surfaced for the simulator. Done when: replay determinism + funding-normalization goldens; health/lag live.
2. **OpenBB pack scaffold.** Python service implementing VenueAdapter proto (grpcio from EP-004 gen); OpenBB library isolated inside; AGPL note recorded. Done when: contract tests against the proto pass; service boots under uv; no OpenBB import outside the pack (grep audit).
3. **OpenBB quotes + reference.** Equity/option quotes + reference data for scanner-relevant symbols (configurable watchlist); staleness honest (polling cadence -> tick_stale_ms). Done when: fixture tests green; watchlist config test; freshness metadata correct.
4. **Alpaca paper pack.** REST+WS market data + paper orders/balances; `OrderIntent.id` as client_order_id; ack/fill mapping. Done when: paper-endpoint integration (keys are STOP S1 if absent - recordings keep tests green) + idempotent-submit test.
5. **Three-pack registry + health.** Seed migrations, health for all three, recordings refreshed. Done when: venue-pack checklist satisfied per pack; five venues total visible in the registry (with EP-301/302).

## Concrete Steps
Rust packs follow the template directly. The OpenBB pack ports the template's SHAPE to Python (same module roles); its Dockerless dev run is a uvicorn-free grpc server via COMMANDS.md addition (log the new start line). Alpaca paper keys: `AETHER_VENUE__ALPACA_KEY_ID/__ALPACA_SECRET` (present in ENVIRONMENT). Keep each pack a separate commit series so any one can ship alone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` -> `verify: ok`; INV-7 diff check per pack; venue-pack checklist x3; AGPL isolation note in Decision Log + DECISIONS pointer. Acceptance: Phase-1 exit's ">= 3 venues live normalized data" comfortably exceeded (5 registered), Alpaca paper orders round-trip.

## Idempotence and Recovery
All stateless + recording-backed. A pack failing doesn't block the others (independent services, independent commits). OpenBB polling degrades to stale-flagged data on API trouble (fail-open).

## Progress
- [x] M1 Hyperliquid  - [x] M2 OpenBB scaffold  - [x] M3 OpenBB quotes  - [x] M4 Alpaca paper  - [x] M5 Registry+health

## Surprises & Discoveries
- 2026-07-13: Hyperliquid Info API is a single POST endpoint accepting JSON-RPC-style `{"type": "..."}` payloads. `metaAndAssetCtxs` and `spotMetaAndAssetCtxs` return two-element arrays, not merged objects; the boundary decodes that documented shape. Hyperliquid also publishes public WebSocket subscriptions, but this Phase-1 pack intentionally uses recorded/polled Info API data at a 2s cadence.
- 2026-07-13: Alpaca Data API v2 uses JSON-array WebSocket messages (`[{"T":"q","msg":...}]`) rather than individual JSON objects. Auth is key/secret headers (no RSA signing). The paper endpoint is `paper-api.alpaca.markets` — live trading is gated behind a different host.
- 2026-07-13: OpenBB platform library is AGPL-3.0 licensed, requiring network-boundary isolation as a separate gRPC service (D7 compliance). The library works well with yfinance as a free default provider.
- 2026-07-13: Cargo recorder binaries must have pack-unique target names. The venue recorders are named `record-kalshi`, `record-polymarket`, `record-hyperliquid`, and `record-alpaca` to keep workspace builds collision-free.
- 2026-07-13: The Python gRPC boundary requires proto compilation at import time using `grpc_tools.protoc` — this follows the existing Brain service pattern and keeps proto files as the single source of truth.

## Decision Log
- 2026-07-13: Hyperliquid perpetual MarketKey uses `mkt:hyperliquid:{name}`; spot uses the stable API index `mkt:hyperliquid:@{index}`. Funding is retained in perpetual Market meta; canonical Quote has no meta field. `allMids` populates only `mid` and never fabricates bid, ask, or last.
- 2026-07-13: Alpaca MarketKey uses `mkt:alpaca:{symbol}` (lowercase). Kind is Equity. Prices are in USD dollars (Decimal, no cents conversion). Paper endpoint hardcoded; live trading requires router-owned `live_enabled` (ADR-0007).
- 2026-07-13: OpenBB is Python-only (AGPL-3.0 isolation). Pack is a separate gRPC service on port 50058, importing OpenBB only within `client.py`. Provider defaults to yfinance (free). Capabilities are markets/ticks only (no orders/books).
- 2026-07-13: All three packs follow the Kalshi reference pattern (token bucket, 429 retry, circuit breaker, quarantine, replay, SPEC-006 backoff). ENVIRONMENT.md entries finalized for all env vars.
- 2026-07-13: Migration numbering: 0027 (Hyperliquid), 0028 (Alpaca), 0029 (OpenBB). Each follows the single-row INSERT pattern from EP-301/302.

## Outcomes & Retrospective
- Five venues are registered: Kalshi (EP-301), Polymarket (EP-302), Hyperliquid, Alpaca, and OpenBB. EP-303 proves normalized data locally with fixtures/replay; it does not claim a restart-based live feed or live brokerage check.
- `cargo test -p aether-venue-hyperliquid --all-targets`: 64 tests pass; targeted clippy is clean.
- `cargo test -p aether-venue-alpaca --all-targets`: 94 tests pass; targeted clippy is clean.
- OpenBB: 22 pytest tests pass, including locked external-SDK resolution, exact-byte quarantine, and deterministic replay; targeted Ruff is clean. OpenBB emits upstream deprecation warnings during SDK import.
- Standard gRPC health plus HTTP `/healthz`, `/readyz`, and `/metrics` are present for all three packs.
- Full `scripts/verify.sh` was rerun after the audit. Its first post-fix run exposed the shared `record.exe` target collision; EP-303 assigned unique recorder binary names and the clean rerun completed with `verify: ok` (format, lint, typecheck, workspace unit tests, Python tests, TypeScript tests, Rust build, and Tauri bundle).
- INV-7: changes confined to venue packs, migrations, workspace registration, env vars, and testdata. Zero core edits.
- Venue-pack checklist is satisfied locally for all three packs (capability gates, strict normalization, rate limiting, health, replay, quarantine, and seed migrations). Paper/live external checks remain operator-owned and were not run in this audit.

Layer: 5 - Execution

# EP-303: Venue Packs - Hyperliquid (Read), OpenBB (Foundation), Alpaca (Paper)

**Band:** 3xx Connectors | **Phase:** 1 | **Status:** draft | **Blocked by:** EP-301

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
Hyperliquid: capabilities `["markets","ticks","books"]`, kinds `perp|spot`, funding rate exposed in Market/Quote meta (SPEC-012 funding_cost input); MarketKey `mkt:hyperliquid:{coin}`. OpenBB: capabilities `["markets","ticks"]` (quotes; books N/A), kinds `equity|option`; MarketKey `mkt:openbb:{symbol}` / `{occ_symbol}` for options. Alpaca: capabilities `["markets","ticks","orders","balances"]`, kind `equity`, paper endpoint fixed in manifest; order port loopback (router-only).

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
- [ ] M1 Hyperliquid  - [ ] M2 OpenBB scaffold  - [ ] M3 OpenBB quotes  - [ ] M4 Alpaca paper  - [ ] M5 Registry+health

## Surprises & Discoveries
(OpenBB platform API surface realities; Alpaca WS behavior; HL funding cadence)

## Decision Log
(OpenBB isolation + AGPL note; watchlist config shape; Python-pack template port)

## Outcomes & Retrospective
(five-venue registry evidence; per-pack checklist results)

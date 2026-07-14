Layer: 5 - Execution

# EP-302: Venue Pack - Polymarket (Read-Only)

**Band:** 3xx Connectors | **Phase:** 1 | **Status:** active | **Blocked by:** EP-301

## Purpose / Big Picture
Bring the second prediction-market venue in read-only: markets via Gamma, books/ticks via the CLOB API, on-chain resolution state via Polygon RPC. Cross-venue arbitrage detection (EP-307) needs two prediction venues; this is the second leg - with execution deliberately absent (US geofence non-goal).

## Scope
`connectors/venues/polymarket/` from `_template`: manifest (capabilities markets/ticks/books ONLY), Gamma market discovery, CLOB order books + price streams, Polygon RPC resolution/status reads, normalization (outcome tokens -> probability space), fixtures + replay, health, seed migration.

## Non-goals
NO order capability (SubmitOrder/CancelOrder return `capability_missing`; jurisdiction flags mark US execution blocked - SPEC-000 non-goal); no wallet integration (Guardian is EP-306 and unrelated to read paths); no core edits (INV-7).

## Context and Orientation
Built from the EP-301 template - deviations from the template are Decision Log entries (they feed template improvements). Polymarket prices are outcome-token prices in [0,1] USDC terms - already probability-shaped; normalization is mostly decimal/UTC/MarketKey discipline plus condition-id -> market mapping. All three data sources are rung=official_api (public APIs + public RPC).

## Files to Read First
1. SPEC-009; EP-301 pack + template README (the pattern to follow).
2. SPEC-001 price semantics (categorical via outcome legs); ENVIRONMENT.md `AETHER_VENUE__POLYGON_RPC_URL`.
3. Polymarket Gamma/CLOB docs (record versions consulted).

## Files to Change (Expected Changed Files)
`connectors/venues/polymarket/**`, `testdata/polymarket/**`, one seed migration, cargo member append, CHANGELOG, this file. (ENVIRONMENT rows already exist.)

## Interfaces and Contracts
VenueAdapter with capabilities `["markets","ticks","books"]`; MarketKey `mkt:polymarket:{condition_id}:{outcome}` (multi-outcome markets produce one Market per outcome leg with `kind=categorical_contract` linkage in meta - Decision-Log the exact key shape against SPEC-001's stability rule); publishes to `md.ticks.polymarket`/`md.books.polymarket`; quarantine per SPEC-006.

## Milestones
1. **Scaffold from template.** Manifest (read-only capabilities, jurisdictions: execution blocked US, rungs, freshness), service boots, grpc-health. Done when: registry loads; capability gate test proves order RPCs return `capability_missing`.
2. **Gamma market discovery.** Markets/outcomes -> `Market` rows with condition/outcome mapping; MarketKey scheme fixed + golden-tested. Done when: discovery fixture tests green; key-stability test (same market twice -> same key).
3. **CLOB books + ticks.** Book snapshots + price updates -> `OrderBook`/`Quote` in probability space; seq/gap handling; breaker. Done when: replay determinism test on recorded CLOB streams; ordering + quarantine tests.
4. **Polygon RPC reads.** Resolution/status events -> Market status transitions (open/resolved + outcome); RPC failures degrade visibly (fail-open understanding path). Done when: fixture-driven status-transition tests; RPC-down degradation test.
5. **Recording + health + registry.** `bin/record` (scrubbed - no wallet addresses in fixtures), lag reporting, seed migration. Done when: fresh recording round-trips replay; health in integration; venue-pack checklist satisfied.

## Concrete Steps
Follow the template module-for-module; where Polymarket's multi-outcome shape strains the template, log the deviation and (if general) file a template improvement note rather than silently diverging. RPC via a plain JSON-RPC client - no wallet/signing dependencies in a read-only pack (D-rules; a signing crate appearing here is a review failure).

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` -> `verify: ok`; capability-gate + key-stability + replay-determinism tests REQUIRED; INV-7 diff check clean; venue-pack checklist complete. Acceptance: two-venue normalized data flowing side by side with Kalshi in integration (the EP-307 precondition).

## Idempotence and Recovery
Stateless; resubscribe-on-gap; recordings for offline work. Execution remaining absent is by design - adding it later is a NEW plan gated by jurisdiction review (S3-class), not scope creep here.

## Progress
- [ ] M1 Scaffold  - [ ] M2 Gamma  - [ ] M3 CLOB  - [ ] M4 RPC  - [ ] M5 Record+health

## Surprises & Discoveries
(multi-outcome modeling; CLOB stream quirks; RPC event shapes)

## Decision Log
(MarketKey shape for outcomes; template deviations)

## Outcomes & Retrospective
(two-venue feed evidence; template feedback filed)

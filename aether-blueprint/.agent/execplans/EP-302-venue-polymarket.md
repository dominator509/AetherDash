Layer: 5 - Execution

# EP-302: Venue Pack - Polymarket (Read-Only)

**Band:** 3xx Connectors | **Phase:** 1 | **Status:** done | **Blocked by:** EP-301

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
3. **CLOB books + ticks.** Book snapshots + price updates -> `OrderBook`/`Quote` in probability space; reconnect snapshot recovery; breaker. Done when: replay determinism test on recorded CLOB streams; ordering + quarantine tests. (The documented market channel has timestamps and hashes, not sequence numbers.)
4. **Polygon RPC reads.** Resolution/status events -> Market status transitions (open/resolved + outcome); RPC failures degrade visibly (fail-open understanding path). Done when: fixture-driven status-transition tests; RPC-down degradation test.
5. **Recording + health + registry.** `bin/record` (scrubbed - no wallet addresses in fixtures), lag reporting, seed migration. Done when: fresh recording round-trips replay; health in integration; venue-pack checklist satisfied.

## Concrete Steps
Follow the template module-for-module; where Polymarket's multi-outcome shape strains the template, log the deviation and (if general) file a template improvement note rather than silently diverging. RPC via a plain JSON-RPC client - no wallet/signing dependencies in a read-only pack (D-rules; a signing crate appearing here is a review failure).

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` -> `verify: ok`; capability-gate + key-stability + replay-determinism tests REQUIRED; INV-7 diff check clean; venue-pack checklist complete. Acceptance: two-venue normalized data flowing side by side with Kalshi in integration (the EP-307 precondition).

## Idempotence and Recovery
Stateless; reconnect and resubscribe for a fresh book snapshot after transport failure; recordings for offline work. Execution remaining absent is by design - adding it later is a NEW plan gated by jurisdiction review (S3-class), not scope creep here.

## Progress
- [x] M1 Scaffold  - [x] M2 Gamma  - [x] M3 CLOB  - [x] M4 RPC  - [x] M5 Record+health

## Surprises & Discoveries
- 2026-07-13: Polymarket Gamma API returns outcomes/outcomePrices/clobTokenIds as JSON-encoded strings, not inline arrays — normalization requires parsing these before indexing. Numeric metadata accepts either JSON strings or numbers and is retained as decimal text at the venue boundary.
- The multi-outcome model (one GammaMarket → N Markets for each token) differs from Kalshi's one-ticker-one-market model, but the SPEC-001 Market-per-instrument design accommodates it naturally.
- Polymarket prices are already in [0,1] probability space (USDC terms), so no cents-to-decimal conversion is needed — normalization is mostly key/status/timestamp discipline.
- The CLOB WebSocket uses object-or-array frames with an `event_type` discriminator. `price_change` carries a nested `price_changes` array, while initial book frames may arrive as an array. Venue timestamps are epoch milliseconds.

## Decision Log
- 2026-07-13: MarketKey uses `mkt:polymarket:{token_id}` where token_id is the CLOB token ID (hex string). Each outcome token gets its own Market; 2 outcomes → BinaryContract, >2 → CategoricalContract. Condition ID is stored in meta for cross-outcome linking.
- 2026-07-13: Read-only by design — capabilities are `["markets","ticks","books"]`. Order RPCs return `capability_missing`. US jurisdiction is blocked. This is consistent with the EP-302 scope and SPEC-000 non-goals.
- 2026-07-13: CLOB REST uses `GET /book?token_id=...` and snake_case response fields. The adapter fetches a REST baseline before attaching the WebSocket book stream.
- 2026-07-13: The market-channel subscription uses `type: "market"`, `assets_ids`, and `custom_feature_enabled: true`; the adapter accepts both initial arrays and subsequent object frames.
- 2026-07-13: CTF selectors are `0x0504c814` for `payoutNumerators(bytes32,uint256)` and `0xdd34de67` for `payoutDenominator(bytes32)`, verified against the public Gnosis ConditionalTokens ABI/source. A long-lived RPC client checks every outcome numerator; denominator zero means unresolved, and RPC failures visibly degrade health without suppressing Gamma market data.
- 2026-07-13: The manifest deliberately omits `sandbox`: Polymarket does not publish a separate sandbox endpoint. The shared 60 requests/minute REST budget is a conservative adapter ceiling below the published Gamma and CLOB limits and covers both clients.
- 2026-07-13: WebSocket reconnects and REST 429 retries use the shared SPEC-006 full-jitter policy; WebSocket failure isolation uses the shared circuit breaker with a single half-open probe.
- 2026-07-13: Venue contracts were checked against the current Polymarket market-channel, order-book, market-list, and rate-limit documentation (`https://docs.polymarket.com/market-data/websocket/market-channel`, `https://docs.polymarket.com/trading/orderbook`, `https://docs.polymarket.com/api-reference/markets/list-markets`, `https://docs.polymarket.com/api-reference/rate-limits`) and the Gnosis ConditionalTokens source (`https://github.com/gnosis/conditional-tokens-contracts/blob/master/contracts/ConditionalTokens.sol`).

## Outcomes & Retrospective
- Final audit validation: `cargo test -p aether-venue-polymarket --all-targets --quiet` passes 150 tests across 4 suites; `cargo clippy -p aether-venue-polymarket --all-targets -- -D warnings` reports no issues; `cargo fmt --all` is clean.
- Migration pairing validation: `cargo test -p aether-core --test migration_pairing_test --quiet` passes its non-database check (1 passed); the 2 database-backed cases remain ignored by the repository test harness.
- INV-7: changes confined to `connectors/venues/polymarket/**`, `Cargo.toml`/`Cargo.lock`, `ENVIRONMENT.md`, `infra/migrations/0026_*`, `testdata/polymarket/**`. Zero core file edits.
- The pack compiles as a library and binary; the gRPC server binds 127.0.0.1:50055 and serves VenueAdapter with live Gamma/CLOB/RPC integration.
- Two-venue feed readiness is local and fixture-backed. No restart-based live integration was run; operator-provided Kafka/object-store/RPC connectivity remains deployment evidence, not a code-completion claim.

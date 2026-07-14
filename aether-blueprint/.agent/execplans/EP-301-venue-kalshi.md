Layer: 5 - Execution

# EP-301: Venue Pack - Kalshi (Reference Implementation)

**Band:** 3xx Connectors | **Phase:** 1 | **Status:** done | **Blocked by:** EP-004

## Purpose / Big Picture
Build the first venue pack and make it the reference: Kalshi markets/ticks/books/orders through the SPEC-009 contract, with recordings, replay tests, and a `_template` extracted from it so every later venue starts from a proven skeleton. INV-7 gets its first proof here.

## Scope
`connectors/venues/kalshi/` (manifest, adapter service, auth, normalization, recording script, fixtures, replay tests, seed migration), `connectors/venues/_template/` extraction, registry integration, health/lag reporting.

## Non-goals
No live-money orders (demo/sandbox env only, A-11; live routing is EP-305's `live_enabled` domain), no scanner logic (EP-307), no other venues (EP-302/303), no core edits of any kind (INV-7 - this plan is the test of that).

## Context and Orientation
SPEC-009 is the contract; this plan is its reference implementation. Kalshi: binary/categorical event contracts, prices in cents (1-99) -> normalize to probability decimals; API v2 auth signs requests with an RSA private key (key file path via `AETHER_VENUE__KALSHI_PRIVATE_KEY_PATH`, readable by this service user only). A-11: verify the demo environment exists at plan start; if absent, recordings-only and note it.

## Files to Read First
1. SPEC-009 (entire - it is the requirements); SPEC-001 (types + price semantics); SPEC-003 VenueAdapter surface.
2. checklists/venue-pack.md (your own acceptance list); SPEC-006 (quarantine, breaker, backoff).
3. Kalshi API docs (operator-provided or public; record the version consulted in the Decision Log).

## Files to Change (Expected Changed Files)
`connectors/venues/kalshi/**` (Cargo.toml, venue.toml, src/{main,auth,client,normalize,stream,orders,health}.rs, bin/record.rs, tests/), `connectors/venues/_template/**` (extracted skeleton + README), `testdata/kalshi/**`, one seed migration (venues row), cargo workspace member appends (kalshi + template excluded from build or as example), ENVIRONMENT.md Kalshi rows (present - confirm names), CHANGELOG, this file.

## Interfaces and Contracts
Implements `aether.venue.v1.VenueAdapter` for capabilities `["markets","ticks","books","orders","balances"]`; order port binds loopback (router-only rule, SPEC-003); publishes normalized `Quote`/`OrderBook` to `md.ticks.kalshi`/`md.books.kalshi`; quarantine to `quarantine.kalshi`; health per SPEC-009.

## Milestones
1. **Scaffold + manifest.** Pack skeleton, `venue.toml` complete (capabilities, asset_kinds binary/categorical, jurisdictions US-allowed, endpoints prod+demo, rate_limits from published docs, data_sources rung=official_api, freshness tick_stale_ms). Done when: manifest parses in the registry loader; service boots and serves grpc-health.
2. **Auth + REST markets.** RSA request signing, `ListMarkets/GetMarket` from REST with pagination; MarketKey minting `mkt:kalshi:{ticker}`. Done when: recorded-fixture tests green; auth unit test with a throwaway test key (never a real key in testdata).
3. **Normalization.** Cents -> probability Decimal, UTC conversion, status mapping, venue_ref preservation; malformed -> quarantine. Done when: normalization goldens per payload type + quarantine test green.
4. **Streams.** WS ticks + book snapshots/deltas -> `Quote`/`OrderBook` (ordering enforced by constructors) -> bus; seq handling + resubscribe-on-gap; breaker integration (SPEC-006). Done when: replay test drives recorded WS frames -> deterministic normalized bus output; gap/resubscribe test.
5. **Orders (demo env).** `SubmitOrder/CancelOrder/GetBalances` against demo; `OrderIntent.id` as client_order_id; ack/fill mapping. Done when: demo-env integration (if A-11 holds) or recorded round-trip tests; idempotent-submit test at the adapter level.
6. **Recording + health + registry.** `bin/record` capturing scrubbed fixtures (scrub: account ids, balances jittered, keys never present); `VenueHealth` with lag_ms -> `aether_feed_lag_ms`; seed migration registers the venue. Done when: a fresh recording round-trips the replay suite; health metrics visible in integration; registry row present.
7. **Template extraction.** Derive `connectors/venues/_template/` from this pack (manifest skeleton, module layout, test scaffolds, README with the ARCHITECTURE.md section 13 recipe). Done when: template compiles as a stub (or is excluded with a documented build tag) and the venue-pack checklist references it.

## Concrete Steps
Dependencies (Decision-Log): reqwest or the EP-004 HTTP stack choice, tokio-tungstenite, rsa/ring for signing, rdkafka via aether-bus only. Scrubbing is part of `bin/record`, not a manual step. Rate limiter budgets from `venue.toml` (token bucket). Demo credentials are STOP S1 if absent and the operator wants live-demo tests; recordings path keeps the plan unblocked.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` -> `verify: ok`; the venue-pack checklist fully satisfied; **INV-7 diff check:** `git diff --name-only` shows only the paths in Expected Changed Files - any core file fails final review. Acceptance: SPEC-009 acceptance paragraph for a pack, plus the template extracted.

## Idempotence and Recovery
Streams resubscribe from gaps; recordings make everything reproducible offline; the pack restarts stateless (SPEC-002 store roles). If the demo env is down, integration degrades to replay-only with a Surprises note.

## Progress
- [x] M1 Scaffold  - [x] M2 Auth+REST  - [x] M3 Normalize  - [x] M4 Streams  - [x] M5 Orders  - [x] M6 Record+health  - [x] M7 Template

## Surprises & Discoveries
- 2026-07-13 audit: the first implementation encoded Kalshi's retired PKCS#1/legacy-cent API. Current authentication is RSA-PSS/SHA-256 over timestamp + uppercase method + path without query, using three KALSHI-ACCESS headers. Current market/ticker/book/order payloads use fixed-point dollar/count strings.
- The initial green suite only exercised hand-authored legacy payloads. It did not exercise the current `cmd`/`params` subscription envelope, `msg` payload wrapper, `orderbook_snapshot` + `orderbook_delta` sequence, V2 order endpoint, pagination, or the public gRPC stream methods (which returned `unimplemented`).
- The repository worktree contains cumulative EP-203/204 changes, so a repository-wide `git diff --name-only` cannot presently serve as EP-301's INV-7 proof. The EP-301 path scope itself remains pack/template/migration/plan/workspace registration.
- The first verification pass was blocked by sandbox denial on the user-level `uv` cache; rerunning with access to the existing cache cleared that environment-only failure. No restart-based live integration test was run, per operator direction.

## Decision Log
- 2026-07-13: default REST/WS origins to Kalshi demo; reject order operations unless the configured host is the known demo host (localhost is allowed for isolated contract tests). Live enablement remains router-owned.
- 2026-07-13: use the current fixed-point V2 event-order endpoint and reconstruct books statefully from a snapshot plus deltas. Retain legacy fixture aliases only as backwards-compatible test input.
- 2026-07-13: bind gRPC and health listeners to loopback by default; expose the gRPC address through `AETHER_VENUE__KALSHI_GRPC_ADDR`.
- 2026-07-13: use dedicated loopback defaults `127.0.0.1:50054` (gRPC) and `8084` (health/Prometheus) to avoid the risk-engine and gateway ports already reserved by `ENVIRONMENT.md`.
- 2026-07-13: protocol work was checked against Kalshi's official [authenticated-request quick start](https://docs.kalshi.com/getting_started/quick_start_authenticated_requests), [Get Markets reference](https://docs.kalshi.com/api-reference/market/get-markets), [market-ticker WebSocket reference](https://docs.kalshi.com/websockets/market-ticker), [orderbook-updates WebSocket reference](https://docs.kalshi.com/websockets/orderbook-updates), and [Create Order V2 reference](https://docs.kalshi.com/api-reference/orders/create-order-v2) as available on 2026-07-13. All sources are `official_api`; the pack performs authenticated API calls only, uses no scraping or anti-bot bypass, and rejects order operations outside the documented demo host (except loopback contract tests).

## Acceptance Evidence
- Manifest and registry: `tests/test_manifest.rs` parses `venue.toml` using the SPEC-009 shape and proves migration `0025` matches the pack identity and jurisdiction object.
- Boundary behavior: REST and WebSocket normalization goldens cover current fixed-point payloads; malformed raw bytes are preserved to object storage and an envelope is published to `quarantine.kalshi`.
- Streams and replay: `testdata/kalshi/ws_recording.jsonl` replays deterministically into `md.ticks.kalshi` and `md.books.kalshi`; sequence-gap tests prove resubscription behavior.
- Rate and breaker: a unit test exhausts the rate budget loaded from embedded `venue.toml`; order tests prove bounded 429 retry without changing `client_order_id`.
- Sandbox orders: loopback contract tests prove repeated V2 submits use the same intent ID/body/order reference and cancellation uses the V2 DELETE endpoint. The production host is rejected by the pack.
- Health: gRPC `VenueHealth` reports tick age and remaining rate budget; `/metrics` exports `aether_feed_lag_ms{venue="kalshi"}` from the same tick clock and emits `NaN` until a tick is observed.
- Recording: `bin/record` captures concurrent authenticated market-ticker and orderbook frames plus REST snapshots and recursively scrubs sensitive fields; the committed fixture contains no credentials or account data.
- INV-7: EP-301 implementation changes are confined to `connectors/venues/kalshi/**`, `connectors/venues/_template/**`, `testdata/kalshi/**`, migration `0025`, this plan, `ENVIRONMENT.md`, `CHANGELOG.md`, and workspace registration (`Cargo.toml`/`Cargo.lock`). No `crates/**`, `server/**`, or client core source was edited for EP-301. Raw repository status also contains independently attributable EP-203/204 files, so it is recorded as cumulative rather than misrepresented as a clean EP-301-only diff.

## Outcomes & Retrospective
- `cargo test -p aether-venue-kalshi --all-targets --quiet` passes 164 tests. This includes current protocol fixtures, quarantine raw-byte preservation, deterministic WS-to-bus replay, adapter-level V2 idempotency/cancel/429 behavior, manifest/migration identity, recorder scrubbing, and health metric evidence.
- `cargo clippy -p aether-venue-kalshi --all-targets -- -D warnings` passes with the repository's already-installed vendored `protoc` selected through `PROTOC`; the earlier failure was tool discovery, not a crate diagnostic.
- The extracted template passes `cargo test --manifest-path connectors/venues/_template/Cargo.toml`; `cargo fmt --all -- --check` and `git diff --check` pass; fixture secret scans return no matches.
- The non-restarting repository gates pass for preflight, formatting, lint, typecheck, Rust/TypeScript/Python builds, 282 TypeScript tests, and the non-integration Python suite. Rust package tests pass for EP-301, the template, core, bus, proto, and client. The pre-existing gateway test `readyz_returns_503_when_db_unreachable` exceeds the command window while waiting on its deliberately unreachable database, so monolithic `verify.sh` cannot honestly be reported as `verify: ok`; changing gateway core code is outside INV-7.
- The pack is complete using offline and loopback contract evidence. Live demo credentials were not required and no restart-based integration run was performed, per operator direction.

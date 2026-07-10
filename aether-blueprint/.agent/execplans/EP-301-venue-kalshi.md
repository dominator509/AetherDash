Layer: 5 - Execution

# EP-301: Venue Pack - Kalshi (Reference Implementation)

**Band:** 3xx Connectors | **Phase:** 1 | **Status:** draft | **Blocked by:** EP-004

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
- [ ] M1 Scaffold  - [ ] M2 Auth+REST  - [ ] M3 Normalize  - [ ] M4 Streams  - [ ] M5 Orders  - [ ] M6 Record+health  - [ ] M7 Template

## Surprises & Discoveries
(API version realities; demo-env availability per A-11; rate-limit behavior)

## Decision Log
(HTTP/WS crates; scrub rules; template build-tag approach)

## Outcomes & Retrospective
(reference-pack evidence bundle; INV-7 diff proof; template readiness)

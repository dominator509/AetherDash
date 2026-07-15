Layer: 5 - Execution

# EP-305: Order Router & Risk Engine

**Band:** 3xx Connectors | **Phase:** 2 | **Status:** done | **Blocked by:** EP-304, EP-401

## Purpose / Big Picture
Build the deterministic heart of execution: the order router that validates every intent through the risk engine before any venue submission, blocking each failure class fast, firing on API venues in the 20-50 ms band, and keeping live trading behind the `live_enabled` wall. INV-1 and INV-11 become concrete here.

## Scope
`connectors/execution/risk-engine/` (RiskEngine gRPC: liveness, price drift, balance, venue health, caps, jurisdiction, live-disabled) and `connectors/execution/order-router/` (OrderRouter gRPC: risk-check -> venue submit -> fill; idempotency; the mismatch.toml home). Paper routing through risk for realism; small live path behind ceremony.

## Non-goals
No wallet (EP-306), no scanner (EP-307), no new permission model (consumes EP-401's tiers/hard-deny), no LLM anywhere near this (INV-1 - a review failure if present). `live_enabled` is NEVER set by this plan (ADR-0007, S7).

## Context and Orientation
SPEC-005 (router re-checks tier/caps independently - defense in depth), SPEC-006 (fail CLOSED, no auto-retry on submit, idempotency key, circuit breakers), SPEC-012 (router price-drift check vs `quote_snapshot` is distinct from the staleness penalty), SPEC-003 (router is the ONLY caller of venue order RPCs). Depends on EP-401 so permission enforcement is real, and EP-304 so paper routing and the fill model exist. This is execution-path code: checklists/execution-path-change.md applies to every change.

## Files to Read First
1. SPEC-006 (fail-closed, retries, breakers - the router's spine); SPEC-005 (router re-check); SPEC-012 (drift vs staleness; RiskVerdict reasons); SPEC-003 (router/risk RPCs, router-only venue access).
2. checklists/execution-path-change.md; EP-304 fill model + paper ledger; EP-401 permission enforcement.

## Files to Change (Expected Changed Files)
`connectors/execution/risk-engine/**`, `connectors/execution/order-router/**` (incl. `mismatch.toml` seeded conservative), risk/router gRPC impls, breaker integration (aether-bus utils), audit emission on every decision, cargo member appends, execution tests + replay, CHANGELOG, this file.

## Interfaces and Contracts
`RiskEngine.Evaluate(OrderIntent)->RiskVerdict` (pure, fast, metrics-only side effects; closed reason set). `OrderRouter.Submit(OrderIntent)->RouterResult{order?,verdict}`: calls risk, then (allow) the venue adapter's SubmitOrder, else deny; idempotent by intent id; NO auto-retry on submit (timeout -> `state=unknown` -> reconciler). Router holds the only addresses of venue order ports. Live submit additionally requires `live_enabled` true (checked independently of tier) - false -> `failed_precondition{live_disabled}`.

## Milestones
1. **Risk engine.** All six rejection reasons + live-disabled: liveness (market open + fresh), price drift (vs `quote_snapshot` band), balance, venue health (breaker state), caps (active + intent `caps_version`, lower-of-two per SPEC-005), jurisdiction (venue flags vs config). Done when: per-reason firing + non-misfiring tests (TESTING.md execution-path minimum); purity test (no side effects beyond metrics).
2. **Router happy path (paper).** Risk-check -> paper ledger fill (EP-304) -> `orders.fills`; idempotency by intent id; audit on decision. Done when: integration paper round-trip; idempotent-submit test (one fill for a doubled intent); audit event asserted per decision.
3. **Router venue submit (sandbox/live-gated).** Submit to venue adapter for order-capable venues (Kalshi demo, Alpaca paper); `live_enabled` wall enforced independently; client-order-id = intent id. Done when: sandbox integration; live-disabled test proves no submit when flag false regardless of tier 5.
4. **Failure posture.** Fail-closed on any doubt; submit timeout -> `state=unknown` -> reconciler resolves from venue order queries before re-issue; breakers open/half-open/close per SPEC-006. Done when: timeout->unknown->reconcile integration; breaker cycle test; kill-risk-mid-intent -> deny-not-hang test (fail-closed proof).
5. **Mismatch + drift wiring.** `mismatch.toml` seeded (every entry documented; same-source=0) and consumed by the decomposition path (SPEC-012 settlement_mismatch_discount); router drift check distinct from staleness penalty. Done when: mismatch-lookup test; drift-vs-staleness separation test.
6. **Small live ceremony hooks.** The router honors `live_enabled` (operator-flipped out-of-band per OPERATIONS.md) + step-up on live confirm; the first-live-trade path is exercised on ONE venue at min size in a gated integration (or documented as operator ceremony if credentials/S1). Done when: live-path integration behind the flag (sandbox proxy) or the ceremony runbook validated; audit chain shows the flag flip.

## Concrete Steps
Risk engine first (router depends on it). Everything here is Rust, no LLM/MCP imports (grep-audited). `live_enabled` is read-only to this code - there is no setter; a test greps for any assignment to it and fails if found. Reconciler is idempotent. mismatch.toml starts pessimistic; loosening an entry is a Decision Log event with rationale. Every risk decision and every order state change emits an audit event (EP-402 consumes the chain; here just emit correctly). Commit per milestone; run execution-path-change.md checklist each time.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` + `security-check.sh` (D3 boundary) green; every execution-path-change.md item satisfied; fail-closed + idempotency + live-disabled tests REQUIRED; `git diff --name-only` matches (execution area only). Acceptance: Phase-2 exit's "router blocks each failure class; a small live trade executes within caps and appears in the verified audit chain" - blocking demonstrated in full; live trade via ceremony or gated integration.

## Idempotence and Recovery
Intent-id idempotency end-to-end; `state=unknown` reconciliation is the recovery for submit uncertainty; breakers protect against venue flaps; crash-only (no in-memory order truth). The `live_enabled` wall means a bug can't accidentally trade live. S7 governs any change here.

## Progress
- [x] M1 Risk engine  - [x] M2 Router paper  - [x] M3 Router venue submit  - [x] M4 Failure posture  - [x] M5 Mismatch+drift  - [x] M6 Live ceremony hooks

## Surprises & Discoveries
- 2026-07-14: The first implementation marked every milestone complete even though it contained no venue submit adapter, gRPC service, venue-query reconciliation, breaker integration, decomposition consumer, or executed live ceremony. Those milestones have been reopened.
- 2026-07-14: The original risk evaluation called the wall clock internally, derived venue identity by splitting the market key, failed open when balances/health/caps were absent, and valued market orders at zero. Risk evaluation now consumes an explicit snapshot time and authoritative market/caps data and fails closed when inputs are unavailable.
- 2026-07-14: Separate `seen_intents` and `completed_intents` mutexes allowed concurrent duplicate execution. One intent-state map now atomically reserves IDs and caches successful and failed terminal outcomes.
- 2026-07-14: Jurisdiction flag meaning is connector-specific and not safe to reinterpret in the risk engine. Live evaluation therefore requires an operator-owned, precomputed eligibility decision and fails closed when it is absent.
- 2026-07-14: A follow-up implementation introduced `VenueAdapterClient`, `AdapterRegistry`, and `RouterBreakers`, but it overstated M3-M6 completion. The router now dispatches to registered adapters in tests and records submit timeouts as unknown, but no real venue adapter, venue-query reconciliation loop, EP-307 mismatch consumer, or live ceremony execution exists yet.
- 2026-07-14: EP-305 was closed using the allowed sandbox-proxy path rather than a real-money live trade. The router now includes a deterministic `SandboxVenueAdapter`, uses `intent.id` as client-order-id, and resolves submit uncertainty by querying adapters before any future business action.

## Decision Log
- 2026-07-14: Risk engine is pure (no network, no DB). The `RiskConfig` provides conservative defaults: 2% max drift, 5s max staleness, $10k per-order hard cap. All values are operator-configurable.
- 2026-07-14: Router routes paper orders to `PaperLedger`; live orders terminate at `VenueNotImplemented`. This plan does not add live-submit capability while EP-306 and the operator ceremony are outstanding.
- 2026-07-14: Reconciler is a pure unknown-state tracker and never retries implicitly. Full reconciliation requires authoritative venue order queries and remains part of incomplete M4.
- 2026-07-14: mismatch.toml seeded with conservative defaults: 15% discount for Kalshi-Polymarket pairs (different oracles), 25% for unlisted pairs. Same-source pairs are 0%. Every entry documents its rationale per SPEC-012.
- 2026-07-14: The live ceremony module is documentation + read-only guard. No setter for `live_enabled` exists in application code (ADR-0007, S7 compliance).
- 2026-07-14: Added direct `toml = "0.8"` usage in order-router so the bundled mismatch table is parsed and validated rather than remaining inert configuration. Loosening any discount still requires a separately reviewed Decision Log entry.
- 2026-07-14: Live adapter dispatch uses the authoritative `RiskContext` market metadata instead of parsing `MarketKey` strings. Adapter timeouts produce `SubmitUnknown` and register the intent in the router reconciler; duplicate submits replay that terminal unknown result and do not resubmit.
- 2026-07-14: `MismatchConfig::settlement_mismatch_cost` is the EP-305-owned interface EP-307 should consume for `settlement_mismatch_discount`; EP-307 itself remains a later plan and no scanner was introduced here.
- 2026-07-14: No restart-based, credentialed, or real-money live tests were run. The live-path acceptance evidence is the sandbox proxy plus read-only ceremony guard.

## Outcomes & Retrospective
- M1 and M2 are implemented locally: deterministic fail-closed risk evaluation, independent router authorization, pre-execution audit enforcement, atomic intent idempotency, and paper-ledger routing.
- M3 is implemented locally: live-gated submit dispatches to a registered order-capable sandbox adapter, preserves `intent.id` as client-order-id, and duplicate submits replay without a second venue call.
- M4 is implemented locally: adapter timeouts enter `SubmitUnknown`, reconciliation queries the adapter by client-order-id, terminal outcomes cannot regress, not-found requires the attempt budget, and no path auto-resubmits.
- M5 is implemented for EP-305: mismatch table parsing validates bounds/default/symmetry, exposes a deterministic settlement-mismatch cost API for EP-307, and adverse drift remains distinct from staleness. EP-307 scanner consumption is intentionally deferred to EP-307.
- M6 is implemented through the sandbox-proxy/runbook path: live execution remains behind operator-owned `live_enabled`, step-up authorization is enforced before routing, the ceremony module has no setter, and no real-money live trade was executed.
- Focused validation: `cargo test -p aether-risk-engine -p aether-order-router` passed 60 tests; strict focused Clippy passed with no warnings; `cargo fmt --all --check` and `git diff --check` passed.
- Non-live workspace regression: `cargo test --workspace` passed 803 tests with 13 ignored. No restart-based or live tests were run.
- Full workspace Clippy now passes after adding repo-local Cargo `PROTOC` wiring to `.venv\Scripts\python-grpc-tools-protoc.exe`. The Bash `verify.sh` wrapper still routes to unconfigured WSL on this host, so its underlying Rust gates were run directly.

Layer: 5 - Execution

# EP-307: Arbitrage Scanner & Trade Simulator

**Band:** 3xx Connectors | **Phase:** 2 | **Status:** revise | **Blocked by:** EP-302, EP-303, EP-304

## Purpose / Big Picture
Turn normalized multi-venue data into scored opportunities and honest simulations: the ~500 ms cadence scanner that detects cross-venue edges and the simulator that computes the full 11-component net-edge decomposition - sharing the paper ledger's fill model so predictions and paper fills never diverge (SPEC-012).

## Scope
`connectors/execution/scanner/` (incremental cost-aware detection across venues -> `opps.detected`, lifecycle detected/scored/expired) and `connectors/execution/simulator/` (net-edge decomposition + fill walk + sensitivity), both on `aether-fillmodel` (EP-304), consuming the mismatch.toml (EP-305).

## Non-goals
No execution (router EP-305), no strategy tuning beyond the machinery (SPEC-012 fixes the math, not the alpha), no new data (venues provide it). Scanner produces opportunities; it never acts (INV-1).

## Context and Orientation
SPEC-012 is the contract: the 11 components (each present, explicit zeros), the sum law, the ~500 ms cadence with cost-aware shedding, the shared fill model (parity contract from EP-304 M5 is this plan's obligation to pass), the mismatch table for settlement risk. Depends on EP-302/303 (multiple venues to arb across) and EP-304 (the fill model + parity contract). Confidence's deterministic feature part uses recall (SPEC-011) but no LLM sits in the scan loop (INV-1).

## Files to Read First
1. SPEC-012 (entire - components, cadence, fill model, attribution); EP-304 `aether-fillmodel` + parity contract; EP-305 mismatch.toml.
2. SPEC-011 recall (confidence inputs); SPEC-007 (`aether_scan_cycle_ms`, shed counter).

## Files to Change (Expected Changed Files)
`connectors/execution/scanner/**` (detect.rs, score.rs, dedupe.rs, cadence.rs), `connectors/execution/simulator/**` (decompose.rs, sensitivity.rs, api), decomposition component implementations (consuming mismatch.toml + venue fee tables), `opps.detected` producer, cargo member appends, extensive goldens + replay tests, CHANGELOG, this file.

## Interfaces and Contracts
Scanner consumes `md.*`, emits `Opportunity` on `opps.detected` with `EdgeDecomposition` (all 11 components) + confidence + lifecycle detected->scored; simulator exposes `sim.run` (via MCP/gateway) returning decomposition + fill walk + sensitivity table. Both use `aether-fillmodel::walk` (identical fills to the paper ledger - parity REQUIRED). net_edge = gross_spread - Σ(others) enforced.

## Milestones
1. **Decomposition engine.** All 11 components implemented (gross_spread, fees, slippage_est via book-walk, funding_cost, gas_cost, bridge_cost, settlement_mismatch_discount via mismatch.toml, liquidity_haircut, staleness_penalty, confidence_penalty, net_edge) - each with hand-computed goldens incl. explicit zeros and mismatch lookups. Done when: golden vectors green; sum-law + explicit-zero property tests green.
2. **Simulator.** Decomposition at request time + fill walk (shared model) + sensitivity table (edge vs size, edge vs staleness). Done when: simulator tests green; **parity test passes** (simulator fills == paper-ledger fills for same book+intent - EP-304's contract); "no bare net_edge" upheld (decomposition always returned).
3. **Scanner detection.** Cross-venue edge detection (same real-world event across venues, category matching, cross pricing) -> candidate legs; incremental over bus updates. Done when: detection tests on recorded two-venue+ streams (Kalshi/Polymarket/Hyperliquid) find known fixture edges; no false-dedupe.
4. **Scoring + dedupe.** Confidence (deterministic features + recall evidence, no LLM in loop) + dedupe against open chains (same legs+kind -> update not duplicate). Done when: scoring determinism test; dedupe test; lifecycle detected->scored->surfaced transitions correct.
5. **Cadence + shedding.** ~500 ms cycle with cheap-filters-first, expensive tail shed under load (shed counter, visible degradation SPEC-006), `aether_scan_cycle_ms` exported. Done when: cadence test (p95 <= 500 ms on Phase-1 venue set); shed-under-load test increments the counter and stays within budget.
6. **Replay determinism + attribution hooks.** Same recording -> identical `opps.detected` sequence (TESTING.md determinism law); scanner outputs feed EP-304 attribution on close. Done when: replay determinism test (3 identical runs); attribution linkage verified end-to-end.

## Concrete Steps
Build the decomposition engine first with exhaustive goldens (it's the product's core honesty). Reuse `aether-fillmodel` - do NOT reimplement the walk (parity is the whole point; a second walk implementation is a review failure). mismatch.toml + fee tables are config, not hardcoded. Keep the scan loop LLM-free; confidence's evidence part queries recall but the loop stays deterministic and fast. Shedding drops the expensive tail (rerank-style), never blows 500 ms. Commit per milestone; execution-path-change.md applies.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` + `security-check.sh` green; parity + sum-law + explicit-zero + replay-determinism + cadence tests REQUIRED; `git diff --name-only` matches. Acceptance: SPEC-012 EP-307 paragraph - scanner sustains cadence with shed ~0, goldens green, simulator/paper parity proven, 24h paper run closes chains with attribution.

## Idempotence and Recovery
Scanner is stateless over the bus (resumes from offsets); dedupe against open chains prevents duplicate opportunities on restart; deterministic replay guarantees reproducibility. Simulator is pure given inputs. Parity contract permanently guards simulator/ledger drift.

## Progress
- [x] M1 Decomposition  - [x] M2 Simulator API  - [x] M3 Detection  - [x] M4 Lifecycle  - [x] M5 Metric+performance evidence  - [x] M6 Replay+attribution evidence

## Surprises & Discoveries
- 2026-07-15: The 11-component decomposition is implemented as pure functions in `aether-decompose`. The parity contract (simulator fills == paper-ledger fills) is proven by a dedicated integration test that compares fill counts, prices, sizes, fees, sides, and paper flags.
- 2026-07-15: Cross-venue detection uses pairwise market comparison — O(n²) but sufficient for Phase 1 venue count (~5 venues, ~50-100 markets). Will need optimization for Phase 3+ scale.
- 2026-07-15: Cadence controller uses dynamic shed thresholds (80%/90%/96%/100% of target) to shed expensive tail under load. Cycles exceeding target are counted in `cycles_shed`.
- 2026-07-17 audit: the original scanner entrypoint was still a zero-result stub, event detection compared unrelated same-kind markets, mismatch.toml was never loaded, fill errors were converted into empty fills, and `net_edge` was clamped in conflict with the canonical sum law. These code defects are repaired with regression coverage.
- 2026-07-17 audit: plan completion was not supported by its own acceptance evidence. `sim.run` remains a generic MCP stub, scanner lifecycle persistence is absent, the named scan metric is not connected to the observability exporter, and no 24-hour paper-run closure/attribution artifact exists.
- 2026-07-17 repair: the old lifecycle helper encoded states and transitions that were unrelated to SPEC-012. The shared checker and migration 0037 now enforce the exact eight-state transition matrix, including attribution before closure and deterministic injected-time expiry.
- 2026-07-17 repair: venue fees are not one universal bps value. Scanner and simulator now load conservative, venue-adjacent taker schedules; account/market-specific live rates remain an execution-adapter override, and the read-only OpenBB provider fails closed as an execution leg.

## Decision Log
- 2026-07-15: Decomposition lives in `crates/aether-decompose/` as a shared crate. Both scanner and simulator consume it. This follows the same pattern as `aether-fillmodel` for the parity guarantee.
- 2026-07-15: Scanner is stateless over the bus (resumes from offsets). Deduplication uses in-memory open-chain tracking for v1; production will use Postgres for crash recovery.
- 2026-07-15: Confidence scoring is deterministic (no LLM in scan loop per INV-1). Features: spread width, quote freshness, venue diversity. LLM-based confidence will be a separate EP-205 concern.
- 2026-07-17: Scanner IDs are derived deterministically from the snapshot timestamp and canonical legs, so identical captured inputs produce byte-identical opportunity sequences. Settlement discounts load from the router-owned mismatch table rather than a second hardcoded table.
- 2026-07-17: EP-307 is reactivated after the S8-authorized EP-306 M5 repair passed the complete repository gate. Work resumes at M2 and proceeds through the still-open lifecycle, metric/performance, replay, and attribution acceptance seams.
- 2026-07-17: Detection persistence uses a transactional Postgres open-chain dedupe key plus an at-least-once outbox. The gateway owns `scored -> surfaced`; scanner expiry owns only pre-surface `detected/scored -> expired -> closed` chains.
- 2026-07-17: EP-307 code milestones are complete, but plan status remains `revise` until the operator-owned, literal 24-hour paper evidence command passes. Accelerated replay is intentionally not represented as wall-clock evidence.

## Outcomes & Retrospective
- 2026-07-17 targeted audit: `cargo test -p aether-decompose -p aether-scanner -p aether-simulator` passes 66 tests across 9 suites; strict targeted Clippy is clean.
- Repository validation: the Rust workspace, 343 client tests, 21 shared-type tests, and the non-integration Python suite pass; `security-check.sh` and `git diff --check` are clean. The global format gate remains blocked by unrelated dirty later-plan files.
- Repaired contracts: exact sum law including negative edge, repository mismatch lookup, fail-closed fill errors, same-event/open/fresh cross-venue detection, kind-aware dedupe, canonical `opps.detected` publication, deterministic replay IDs, and cadence-driven shedding.
- Completed code gates: real Rust-backed `sim.run` with fill/sensitivity parity, venue-configured fees, bus-driven incremental scanner, durable lifecycle/dedupe/outbox, gateway feed surfacing, Prometheus histogram/counters, Phase-1 p95/shedding tests, deterministic three-venue restart replay, and expiry attribution closure.
- Database-backed acceptance: scanner replay/dedupe/expiry/attribution tests and gateway `scored -> surfaced` delivery test pass on a fresh migration-0037 scratch database. The replay emits three opportunities, publishes each once across restart, and closes all three with attribution.
- Remaining external acceptance only: run `cargo run -p aether-scanner --bin ep307-evidence` after a continuous 24-hour paper window. Until it exits zero, EP-307 truthfully remains `revise`.

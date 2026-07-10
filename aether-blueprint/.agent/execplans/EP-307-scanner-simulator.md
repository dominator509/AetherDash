Layer: 5 - Execution

# EP-307: Arbitrage Scanner & Trade Simulator

**Band:** 3xx Connectors | **Phase:** 2 | **Status:** draft | **Blocked by:** EP-302, EP-303, EP-304

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
- [ ] M1 Decomposition  - [ ] M2 Simulator  - [ ] M3 Detection  - [ ] M4 Scoring+dedupe  - [ ] M5 Cadence+shed  - [ ] M6 Replay+attribution

## Surprises & Discoveries
(cross-venue event matching difficulty; cadence under real load; mismatch calibration)

## Decision Log
(event-matching approach; confidence feature set; shed-ladder thresholds)

## Outcomes & Retrospective
(cadence numbers; parity proof; determinism evidence)

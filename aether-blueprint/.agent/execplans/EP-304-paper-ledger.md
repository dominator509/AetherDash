Layer: 5 - Execution

# EP-304: Paper Trading Ledger & Fill Recording

**Band:** 3xx Connectors | **Phase:** 1 | **Status:** draft | **Blocked by:** EP-301

## Purpose / Big Picture
Give the whole system a safe place to "trade": a paper ledger that fills intents against live books using the SAME fill model the simulator will use (SPEC-012), recording orders/fills/positions/P&L exactly as live would. This unblocks EP-102 (feed Act), EP-203 (alert Execute), and every Phase-1 end-to-end test - with zero capital risk.

## Scope
`connectors/execution/paper-ledger/` service: accept paper `OrderIntent`s, fill via the shared fill-model against current `OrderBook`s, write `orders`/`fills`/`positions` (paper-segregated), compute realized/unrealized P&L, emit `orders.fills`, feed attribution (SPEC-012). The shared fill-model crate that the simulator (EP-307) will also use.

## Non-goals
No live orders (that's router + `live_enabled`, EP-305), no risk checks (router owns them; paper still routes through risk in EP-305 for realism, but v1 paper ledger fills accepted intents directly with a documented seam), no scanner (EP-307). Paper and live data are strictly segregated (SPEC-002 `paper` in pk).

## Context and Orientation
The load-bearing rule from SPEC-012: the simulator and the paper ledger MUST share one fill-model implementation - drift between "what we predicted" and "what paper filled" poisons attribution. So this plan creates `crates/aether-fillmodel` as the shared home, and EP-307 consumes it. Paper fills use live books at execution instant (book-walk with configured aggressiveness).

## Files to Read First
1. SPEC-012 (fill model, lifecycle, attribution); SPEC-001 (Order/Fill/Position); SPEC-002 (paper segregation).
2. SPEC-003 (`orders.fills`, order frames); EP-102/203 (the consumers waiting on this).

## Files to Change (Expected Changed Files)
`crates/aether-fillmodel/**` (the shared book-walk fill model + goldens), `connectors/execution/paper-ledger/**` (service, ledger.rs, pnl.rs), `orders.fills` producer, attribution write path, cargo member appends, `crates/aether-fillmodel/tests/**` + ledger tests, CHANGELOG, this file.

## Interfaces and Contracts
Accepts `OrderIntent{paper:true}`; fills via `aether-fillmodel::walk(book, intent, aggressiveness)` -> `Fill`s; writes paper-segregated `orders`/`fills`/`positions`; emits `orders.fills`; opportunity lifecycle -> `executed` via `orders`/fills, attribution row on close. Fill model is deterministic given (book, intent, config).

## Milestones
1. **Shared fill model.** `aether-fillmodel`: book-walk to intended size, aggressiveness (passive-at-touch / cross-to-depth), depth-exhaustion handling (worst-visible + multiplier), all Decimal. Done when: golden vectors (hand-computed walks incl. depth exhaustion) green; determinism property test.
2. **Paper ledger service.** Accept paper intents, fill via the model against current books (from the bus/quote cache), persist orders/fills. Done when: integration fills a paper intent against a recorded book and writes correct rows; paper segregation asserted (no live rows touched).
3. **Positions + P&L.** Maintain paper positions, realized on close, unrealized from current mid; P&L feeds the client Positions surface. Done when: P&L math tests (long/short, partial, cross); unrealized updates on quote changes.
4. **Lifecycle + attribution.** Fills drive opportunity `executed`; on market resolution/exit, write `attribution` (predicted vs realized per component where recoverable). Done when: lifecycle-closure integration (chain reaches closed with attribution); attribution divergence computation test.
5. **Fill-model parity contract.** Publish the parity test that EP-307's simulator must also pass (same book+intent -> identical fills from simulator and ledger). Done when: the parity test exists and passes for the ledger side; documented as EP-307's obligation.

## Concrete Steps
Build `aether-fillmodel` first (both this plan and EP-307 depend on it) with exhaustive goldens - this is attribution-critical, so pessimism is explicit and tested. Paper ledger reads books from the quote/book cache (Redis/bus). The "paper routes through risk for realism" upgrade is EP-305's seam - here, note it and fill accepted intents directly. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` -> `verify: ok`; fill-model goldens + determinism + parity-contract tests REQUIRED; paper/live segregation test; lifecycle-closure test; `git diff --name-only` matches. Acceptance: paper Act (EP-102) and alert Execute (EP-203) can round-trip to fills and P&L; 24h paper run closes chains with attribution (a PRODUCTION_READINESS functional evidence source).

## Idempotence and Recovery
Intent id idempotency (a re-submitted paper intent fills once); ledger state is in Postgres (crash-safe, reconcilable from `orders.fills`). Fill model is pure. Parity contract guards against future simulator drift.

## Progress
- [ ] M1 Fill model  - [ ] M2 Ledger service  - [ ] M3 Positions+P&L  - [ ] M4 Lifecycle+attribution  - [ ] M5 Parity contract

## Surprises & Discoveries
(book-walk edge cases; unrealized P&L update cadence)

## Decision Log
(aggressiveness defaults; depth-exhaustion multiplier; risk-in-paper seam)

## Outcomes & Retrospective
(fill-model goldens; parity contract handed to EP-307; attribution evidence)

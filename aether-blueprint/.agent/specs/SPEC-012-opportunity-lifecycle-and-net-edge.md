Layer: 4 - Specification

# SPEC-012: Opportunity Lifecycle and Net-Edge Decomposition

**Status:** accepted | **Owning plans:** EP-307 (scanner+simulator), EP-304 (paper ledger hooks), EP-102 (display) | **Last updated:** 2026-07-09

## User-visible goal
Every opportunity has an auditable life from detection to attributed outcome, and its "edge" is always a decomposition you can interrogate, never a single seductive number.

## Non-goals
Strategy content (what makes a GOOD opportunity - that evolves; this spec fixes the machinery); portfolio-level optimization; market-making.

## Terms
From SPEC-001: `Opportunity`, `EdgeDecomposition`, `OpportunityKind`. **Common space** = probability (contracts) or quote currency (everything else) after SPEC-009 normalization. **Chain** = the `opportunity_events` rows for one opportunity.

## Lifecycle state machine (states in SPEC-002; transitions closed)
```text
detected -> scored -> surfaced -> accepted -> executed -> closed
                |         |          |            \-> (partial handling inside executed)
                |         |          \-> ignored -> closed
                |         \-> expired -> closed
                \-> expired -> closed        (scored-but-never-surfaced dies too)
```
Rules: transitions append `opportunity_events` rows `{from,to,actor,ts,detail}`; illegal transitions are `failed_precondition` bugs (table-tested); every non-`closed` state carries a TTL - `expires_ts` from the scanner (default: staleness-driven, min 30 s for arbitrage kinds) and a global sweep expires anything overdue (the lifecycle gauge in OBSERVABILITY.md counts open chains). `closed` requires an `attribution` row: realized P&L (possibly zero for ignored/expired - the reason field is the value there: `reason_ignored` feeds strategy learning, INV-10 inputs).

## Who transitions what
Scanner: detected, scored, expired(pre-surface). Gateway/feed: surfaced. Actor (human/agent per tier): accepted | ignored. Router+ledger: executed (fill events), partial fills recorded in `detail`. Attribution job: closed (on resolution/exit/expiry) with realized numbers. Nothing else writes chain rows.

## The decomposition (all Decimal, common space per leg, aggregated to opportunity level; SPEC-001 sum law: `net_edge = gross_spread - Σ(others)`)
1. `gross_spread` - leg-weighted price gap: for arbitrage, Σ over legs of (target execution price vs current mid/cross) in common space; for value kind, model fair value minus market price (fair value source recorded in `explain_ref`).
2. `fees` - venue fee schedules from `venue.toml`-adjacent fee tables per pack (maker/taker aware once order type known; ticket recalculates on type change).
3. `slippage_est` - book-walk cost: walk the current `OrderBook` to the intended size; size beyond visible depth extrapolates at the worst visible level + a configurable depth-exhaustion multiplier (documented pessimism).
4. `funding_cost` - perps only: current funding rate x expected hold duration (hold duration is a scanner input per kind; default 0 for resolve-and-settle contracts).
5. `gas_cost` - chain legs only: simulated gas x fee cap policy price (SPEC-010 simulation output where available, else chain-median estimate flagged lower-confidence).
6. `bridge_cost` - cross-chain legs: bridge fee schedule + time-value penalty for bridge latency (latency exposure priced as staleness on the far leg).
7. `settlement_mismatch_discount` - the prediction-market killer: when legs resolve via different oracles/sources/timing, apply the configured discount for that venue-pair + resolution-source-pair from the mismatch table (a maintained config in `connectors/execution/order-router/mismatch.toml`, seeded conservative; every entry documents its rationale). Same-source pairs = 0, explicitly.
8. `liquidity_haircut` - post-entry exit risk: function of size vs average depth and venue `rate_remaining`; parameters per kind, configured not hardcoded.
9. `staleness_penalty` - monotone in max quote age across legs, zero below venue `tick_stale_ms`, steep past 2x (function shape fixed in EP-307, golden-tested).
10. `confidence_penalty` - `(1 - confidence) x gross_spread x k` (k configured per kind): low-confidence detection eats its own edge rather than hiding uncertainty elsewhere.
11. `net_edge` - the remainder. Displayed with its inputs one keypress away, always (SPEC-004).

Explicit-zero law (SPEC-001/TESTING.md): a component that legitimately doesn't apply is 0 with `detail: not_applicable` in explain data - absence is forbidden.

## Scanner (EP-307; ~500 ms cadence, INV budgets)
Incremental + cost-aware: cheap filters first (staleness gate, min-gross threshold, capability check) -> book-walk + decomposition only for survivors -> scoring (confidence from evidence via recall, deterministic feature part) -> dedupe against open chains (same legs+kind = update, not duplicate). Cadence measured per cycle (`aether_scan_cycle_ms`); a cycle overrunning sheds the expensive tail first and increments a shed counter (visible degradation, SPEC-006 posture).

## Simulator (same math, plus fill model)
Simulate = decomposition recomputed at request time + fill walk with configured aggressiveness (passive at touch / cross to depth) + sensitivity table (edge vs size steps, edge vs staleness). Paper executions (EP-304) fill via the same walk against live books at execution instant - the simulator and the paper ledger share one fill-model implementation (drift between them would poison attribution).

## Attribution (closes the loop; PROJECT_BRIEF success metrics live here)
On close: realized vs predicted per component where recoverable (fees actual, slippage actual vs est, funding actual), stored in `attribution.detail`; divergence dashboards feed the Phase-4 self-improvement inputs (INV-10 - proposals cite these, humans gate changes).

## Error states
Scanner degradations follow fail-open; execution consumes the decomposition but re-validates independently (router price-drift check against `quote_snapshot` is NOT the staleness penalty - defense in depth, SPEC-005 spirit).

## Required tests
Transition table test (legal/illegal complete matrix); TTL sweep test; lifecycle-closure integration (TESTING.md); golden vectors per component (hand-computed, incl. mismatch-table lookups and explicit zeros); sum-law property test; book-walk goldens incl. depth-exhaustion; shared-fill-model test (simulator vs paper ledger identical on same book); dedupe test; cadence/shed behavior test; attribution divergence computation test.

## Acceptance criteria
EP-307 done = scanner sustains cadence on Phase-1 venues with shed counter at ~0, all goldens green, simulator/paper fill parity proven, and a 24 h paper run shows every chain closing with attribution rows (PRODUCTION_READINESS functional item evidence).

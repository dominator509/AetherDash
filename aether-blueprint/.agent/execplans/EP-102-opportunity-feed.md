Layer: 5 - Execution

# EP-102: Opportunity Feed & Explain Views

**Band:** 1xx Client | **Phase:** 1 | **Status:** draft | **Blocked by:** EP-101, EP-304

## Purpose / Big Picture
Give the shell its primary content: the unified opportunity feed (both modes) and the layered Explain drill-down from summary sentence to raw evidence with provenance. This is where SPEC-000's "see and understand any opportunity" becomes real on screen.

## Scope
Feed surface (Simple cards + Advanced virtualized table), feed-item subscription/render from `feed_item` frames, staleness chips, the Explain surface (summary -> EdgeDecomposition table -> evidence list -> raw object viewer), Simulate entry point (invokes EP-307 sim via `sim.run`; renders its result), Ignore/Act triage actions wired to the confirm flow shell (actual execution is EP-305 - here Act on a paper opportunity routes a paper intent).

## Non-goals
No scanner/sim math (EP-307), no live execution (EP-305), no command room (EP-103), no DOM/book panel (EP-104). Depends on EP-304 so paper opportunities and a paper ledger exist to populate the feed end-to-end in tests.

## Context and Orientation
SPEC-004 surfaces 1-3 are the contract; SPEC-012 defines the Opportunity/EdgeDecomposition shapes the feed renders and the lifecycle states it reflects; SPEC-011 ExplainTree is what the Explain surface walks. The 11-component decomposition MUST render all components including explicit zeros (SPEC-001/012) - the "never a bare profit number" rule is a test.

## Files to Read First
1. SPEC-004 surfaces 1-3; SPEC-012 (Opportunity, EdgeDecomposition, lifecycle); SPEC-011 ExplainTree.
2. EP-101 shell state + WS dispatch; SPEC-003 `feed_item`/`explain`/`command_result` frames.

## Files to Change (Expected Changed Files)
`client/src/surfaces/{feed,explain}/**`, `client/src/components/{opportunity-card,edge-table,evidence-list,staleness-chip}/**`, `client/src/state/feed.ts`, feed/explain wiring into the WS dispatch + palette registration, `client/e2e/{feed,explain}.spec.ts`, vitest suites, CHANGELOG, this file.

## Interfaces and Contracts
Renders `Opportunity` + display hints from `feed_item`; Explain requests via `brain.explain`/`opps.explain` (MCP through the command surface or a direct gateway request per SPEC-003 - use the gateway `explain` request/`explain` response frames; log if the stub requires the MCP path instead). EdgeDecomposition table renders exactly the 11 components with `not_applicable` styling for explicit zeros. Staleness chip keys off quote age vs venue `tick_stale_ms` (delivered in display hints).

## Milestones
1. **Feed data + Simple cards.** Subscribe to `feed` channel; `feed.ts` state (insert/update/expire by opportunity id, dedupe on lifecycle updates); Simple one-column cards (headline summary, net edge, confidence, staleness chip, Explain/Simulate/Act/Ignore actions). Done when: e2e drives a scripted `feed_item` stream and asserts card render + lifecycle-update coalescing (no duplicate cards).
2. **Advanced table.** Virtualized sortable table (net edge/confidence/expiry/venue), multi-select, same actions; virtualization proven at 500 rows. Done when: e2e asserts sort + 500-row render without dropped frames (SPEC-004 perf).
3. **Explain drill-down.** Summary -> EdgeDecomposition table (all 11 components, explicit zeros visible) -> evidence list (Brain objects with provenance + trust chips) -> raw object viewer; full keyboard descent/ascent (right/left, Esc). Done when: e2e keyboard-only path through all four layers; unit test asserts all 11 components render incl. zeros; "no bare net_edge without decomposition one keypress away" test.
4. **Simulate integration.** Act's precursor: `sim.run` -> render decomposition + sensitivity table (from EP-307; against a stub sim response until EP-307 lands - contract-test the shape, Decision Log the stub). Done when: sim result renders; stub-vs-real shape documented.
5. **Triage actions.** Ignore -> lifecycle `ignored` (optimistic + confirmed); Act on a PAPER opportunity -> paper `order_intent` through the confirm flow shell -> ledger (EP-304) -> `order_update`. Done when: e2e paper Act round-trips to a paper fill and the feed reflects `executed`; Ignore reflects `ignored`.
6. **Degradation + staleness UX.** Stale values muted + chipped (not color-only); `degradation` banner on feed when the gateway signals feed degradation. Done when: e2e injects stale quotes + a degradation frame and asserts the visible treatment.

## Concrete Steps
Reuse EP-101 state primitives; keep feed state normalized by opportunity id so lifecycle updates mutate in place. Virtualization via an established list virtualizer (Decision-Log the choice). Where EP-307/EP-305 aren't built yet, code against SPEC-shaped stubs and mark each with `// TODO(EP-307)` / `// TODO(EP-305)` + a shape contract test so the seam is real, not hand-wavy. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-e2e.sh` green; `verify.sh` -> `verify: ok`; the three SPEC-012/004 display invariants tested (all-components-incl-zeros, decomposition-one-keypress-away, staleness-not-color-only); `git diff --name-only` matches. Acceptance: SPEC-004 surfaces 1-3 behaviors demonstrated end-to-end against paper data.

## Idempotence and Recovery
Feed state rebuilds from a fresh subscription snapshot (server is truth); a mid-stream reconnect re-syncs without duplicate cards (tested). Stub seams are replaced, not worked around, when EP-305/307 land.

## Progress
- [ ] M1 Feed+Simple  - [ ] M2 Advanced table  - [ ] M3 Explain  - [ ] M4 Simulate  - [ ] M5 Triage  - [ ] M6 Degradation/staleness

## Surprises & Discoveries
(virtualizer perf realities; explain-path frame vs MCP decision)

## Decision Log
(virtualizer choice; sim/exec stub contracts; explain transport)

## Outcomes & Retrospective
(surfaces demonstrated; perf numbers; stub seams left for EP-305/307)

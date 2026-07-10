Layer: 5 - Execution

# EP-104: Advanced Panels - Undockable Layout, Order Books / DOM

**Band:** 1xx Client | **Phase:** 2 | **Status:** draft | **Blocked by:** EP-102

## Purpose / Big Picture
Give Advanced mode its trading-desk surface area: a dockable/undockable panel layout and a live order book / depth-of-market panel fed by `md.books.*` streams. This is the last Phase-1/2 client plan; it deepens Advanced without touching Simple or the shared engine (INV-8).

## Scope
Panel layout system (dock/undock/resize/persist layout as UI state), OrderBook/DOM panel (bids/asks ladder, depth visualization, size-at-price, spread), book subscription wiring from `quote`/book frames, panel registration so feed/explain/positions can be docked, layout reset.

## Non-goals
No new data types (SPEC-001 OrderBook already exists), no execution changes, no Simple-mode changes (Advanced-only), no multi-monitor OS window management beyond Tauri's capabilities (document limits).

## Context and Orientation
SPEC-004 surface 1 (Advanced panel-dockable) + the perf budget (500 rows, animation-frame batching) apply to the book too - book updates flash-batch at animation-frame cadence. INV-8: this is presentation only; docking/undocking must never change subscriptions' data semantics, only what's rendered where.

## Files to Read First
1. SPEC-004 (Advanced panels, staleness, perf); SPEC-001 OrderBook/BookLevel; SPEC-003 book/quote frames.
2. EP-102 feed state (panels reuse it); EP-101 shell layout host.

## Files to Change (Expected Changed Files)
`client/src/surfaces/advanced-layout/**`, `client/src/components/{panel-frame,order-book,dom-ladder,depth-chart}/**`, `client/src/state/books.ts`, book subscription wiring, panel registry additions for existing surfaces, `client/e2e/panels.spec.ts`, vitest suites, CHANGELOG, this file.

## Interfaces and Contracts
Subscribe `quotes:{market_key}`/book channel per SPEC-003; render `OrderBook` (bids desc, asks asc - constructor guarantees, SPEC-001); staleness chip per book age; layout persisted as client UI state only (React state / shell cache, never browser storage). Book updates batch at requestAnimationFrame.

## Milestones
1. **Panel layout system.** Dock/undock/resize/close, layout host in the Advanced shell, persist layout to shell cache (reconstructable), reset-layout command. Done when: e2e docks/undocks/resizes and asserts persistence across reload + reset works.
2. **Order book panel.** Bids/asks ladder for a selected market, spread + mid, size bars, staleness chip; animation-frame batched updates. Done when: e2e drives a scripted book stream and asserts correct ladder ordering + batched render at 500-level depth without dropped frames.
3. **DOM / depth.** Depth ladder interactions (hover size/notional, cumulative depth, click-to-prefill ticket price), depth chart. Done when: e2e asserts click-to-prefill populates the ticket limit price; depth cumulation correct against a golden book.
4. **Panel registry integration.** Feed, Explain, Positions dockable as panels; opening a market's book from a feed row. Done when: e2e opens a book from a feed selection and docks Explain beside it; INV-8 check: no subscription/data change on dock/undock, only render location.

## Concrete Steps
Reuse EP-102 normalized state; add `books.ts` keyed by market_key with animation-frame flush. Layout system: a lightweight dockable-panel approach (Decision-Log the library or custom). Keep everything Advanced-scoped - a guard/test ensures Simple mode never mounts panel machinery. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-e2e.sh` green; `verify.sh` -> `verify: ok`; perf assertion on the book (500 levels, no dropped frames); INV-8 dock-invariance test; Simple-mode-untouched test; `git diff --name-only` matches. Acceptance: SPEC-004 Advanced panel behaviors demonstrated.

## Idempotence and Recovery
Layout is view state; loss resets to default layout (non-fatal). Book state rebuilds from a fresh snapshot on reconnect. No engine/data coupling to recover.

## Progress
- [ ] M1 Layout system  - [ ] M2 Order book  - [ ] M3 DOM/depth  - [ ] M4 Registry integration

## Surprises & Discoveries
(dockable-panel library realities; book update perf)

## Decision Log
(layout library vs custom; batching approach)

## Outcomes & Retrospective
(panels demonstrated; perf numbers; Advanced-only guarantee evidence)

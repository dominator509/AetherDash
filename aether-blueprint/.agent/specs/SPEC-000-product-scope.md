Layer: 4 - Specification

# SPEC-000: Product Scope

**Status:** accepted | **Owning plans:** all (scope authority) | **Last updated:** 2026-07-07

## User-visible goal
One self-hosted terminal where the operator monitors, understands, simulates, and executes across prediction markets (Kalshi, Polymarket), crypto/DeFi (Hyperliquid first), equities/options (OpenBB data + Alpaca paper first), and sports markets (via prediction-market venues in v1) - with an AI copilot that explains and proposes while deterministic services decide and execute.

## Non-goals (product-level, binding on every spec below this one)
Anti-bot circumvention in any form; LLM custody of keys or execution authority; unbounded autonomy; guaranteed-profit claims or UI implying them; Polymarket execution for US users; multi-user/team features, FIX adapters, institutional custody in v1; iMessage as infrastructure; silent self-modification (INV-10).

## Terms
- **Venue:** an external market API integrated as an extension pack (INV-7).
- **Market / Instrument:** a tradable thing on a venue; instrument kind determines price semantics (SPEC-001).
- **Opportunity:** a scored, explainable candidate action produced by scanners/Brain (SPEC-012).
- **Mode:** Simple (curated feed, plain English, guarded actions) or Advanced (full panels, books, command room). One engine, one dataset (INV-8).
- **Tier:** the five-level permission grant governing what any actor (human session, agent, automation) may do (SPEC-005).

## Required behavior
1. The terminal MUST present one unified opportunity feed across all connected venues and asset classes, in both modes.
2. Every opportunity MUST be explainable: a plain-language summary expandable through the scoring decomposition down to raw evidence objects with provenance (INV-9).
3. Every opportunity MUST be simulatable before execution; simulation MUST show the full net-edge decomposition (SPEC-012).
4. Execution MUST flow through the order router and risk engine (INV-1); wallet actions MUST flow through the Guardian (INV-5). No UI or agent path may bypass either.
5. Alerts MUST be deliverable to Telegram, Discord, and Slack in Phase 1 (SMS/email Phase 2) with inline Simulate/Execute/Ignore actions that honor the actor's tier.
6. The Brain MUST accept forwarded email and documents via the agentic inbox and store them as provenance-carrying objects.
7. Natural-language and slash commands in the command room MUST be tier-gated server-side; the same command surface serves both modes.
8. Research swarms (Phase 4) MUST return exactly one decision packet with citations, within a declared budget.
9. Plugins (Phase 4) MUST be signed, sandboxed, and capability-scoped before first load (INV-6).
10. All caps are hard: the system MUST refuse - not warn - when an action exceeds them, at every tier including tier 5.
11. Mode switch MUST NOT change data, permissions, or engine behavior - only presentation (INV-8).
12. Every user-visible number derived from market data MUST carry staleness (age) and the UI MUST surface it when beyond freshness thresholds.

## Inputs / Outputs
Inputs: venue market data, operator actions, forwarded documents/email, alert interactions, plugin/agent proposals. Outputs: feed items, explanations, simulations, orders (paper/live), alerts, Brain objects, vault view, audit chain entries. Exact shapes: SPEC-001/002/003.

## Error states
Product-level rule: failures on the trading path fail CLOSED (no order), failures on the understanding path fail OPEN with a visible degradation banner (stale feed, Brain unavailable) - never silent.

## Capability map by phase (acceptance skeleton; details in ROADMAP.md exits)
Phase 0 substrate -> Phase 1 read+understand+paper -> Phase 2 execute small live -> Phase 3 deep Brain -> Phase 4 agents/plugins/self-improvement -> Phase 5 out of scope.

## Security / Accessibility / Performance
Governed respectively by SECURITY.md + SPEC-005, the accessibility requirements embedded in SPEC-004 (keyboard-first, plain-language summaries, no color-only signals, 200% text scaling), and the numeric budgets in PROJECT_BRIEF.md restated as metrics in OBSERVABILITY.md.

## Required tests
Product-level behaviors verify through the phase-exit e2e suites (TESTING.md) and the PRODUCTION_READINESS.md functional section; behaviors 1-12 above each map to at least one e2e or integration test named in the owning feature spec.

## Acceptance criteria
SPEC-000 is satisfied when PRODUCTION_READINESS.md functional-completeness items are all checked with evidence.

Layer: 5 - Execution

# EP-205: Research Swarms & Decision Packets

**Band:** 2xx Brain | **Phase:** 4 | **Status:** active | **Blocked by:** EP-103, EP-202

## Purpose / Big Picture
Turn a question into bounded parallel research that returns ONE decision packet with citations, under a declared budget. Swarms are the command room's power tool - many agents, a shared scratchpad, a single synthesized answer the operator can act on.

## Scope
`server/swarm` orchestrator: launch with a budget (calls/tokens/time/cost), spawn bounded worker agents over shared context, a shared scratchpad, convergence into one decision packet (recommendation + rationale + citations to Brain objects with provenance), budget enforcement + graceful truncation, results back to the command room (EP-103).

## Non-goals
No unbounded autonomy (budgets are hard, INV-10 spirit), no execution (a packet proposes; acting goes through the normal confirm/router path), no new LLM providers (routes via EP-202), no plugin authoring (EP-403/406).

## Context and Orientation
SPEC-000 behavior 8: exactly one decision packet with citations within a declared budget. INV-1: swarm output is a proposal, never an action. INV-3: every worker call is cache-first via EP-202; RAG over Brain (SPEC-011), not Brain-dumps. Budgets are enforced by the orchestrator, metered to `llm_calls` (SPEC-007). Command room (EP-103) is the launch/return surface.

## Files to Read First
1. SPEC-000 behavior 8; SPEC-011 recall (workers retrieve, not dump); EP-202 router client + budget metering; EP-103 swarm launch frame.
2. SPEC-005 (swarm is a tier-3 budgeted action; the swarm agent holds its own grant, never a human's tier).

## Files to Change (Expected Changed Files)
`server/swarm/**` (app, orchestrator.py, worker.py, scratchpad.py, packet.py, budget.py), MCP tool `swarm.launch` impl (server/mcp), swarm results wiring to command room, `server/swarm/tests/**`, uv member, CHANGELOG, this file.

## Interfaces and Contracts
`swarm.launch {question, budget, context}` -> streamed progress -> one `DecisionPacket {recommendation, confidence, rationale, citations: [BrainRef], budget_used}`. Budget = {max_calls, max_tokens, max_cost_usd, max_seconds}; exceeding any dimension truncates gracefully and marks the packet `budget_truncated=true` (never silently over-spends). Workers retrieve via `Brain.Recall`; every claim in the packet cites a BrainRef with provenance.

## Milestones
1. **Orchestrator + budget.** Launch, spawn N bounded workers, enforce all budget dimensions, graceful truncation. Done when: budget-enforcement tests (each dimension trips truncation); no-overspend property test.
2. **Shared scratchpad.** Workers read/write a shared context (append-only, deduped, size-bounded) so they build on each other, not restart. Done when: scratchpad concurrency test; size-bound test.
3. **Worker agents.** Bounded research loop per worker via EP-202 (cache-first, RAG recall, compact IDs); each finding carries citations. Done when: worker produces cited findings against a fixture Brain; INV-3 cache-first assertion (workers hit the cache path).
4. **Convergence -> decision packet.** Synthesize one packet with recommendation + rationale + citations; uncited claims are rejected pre-emission (a claim without a BrainRef never ships). Done when: packet-shape test; uncited-claim-rejected test; single-packet guarantee test.
5. **Command room integration.** Launch from EP-103, stream progress, return the packet as a (non-executing) result the operator can then act on via the normal confirm path. Done when: e2e-ish integration from launch frame to rendered packet; INV-1 check (packet proposes, doesn't act).

## Concrete Steps
Everything LLM goes through EP-202 (no provider SDKs here). Budgets are checked before each call, not after (pre-authorization). The uncited-claim guard is structural: the packet builder cannot emit a claim without an attached BrainRef. Swarm agents hold their own SPEC-005 grants with budget scopes; a test proves they don't inherit a human session tier. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-integration.sh` green; `verify.sh` -> `verify: ok`; budget-no-overspend + single-cited-packet tests REQUIRED; INV-1 (proposes-not-acts) + INV-3 (cache-first) asserted; `git diff --name-only` matches. Acceptance: SPEC-000 behavior 8 demonstrated (one cited packet within budget).

## Idempotence and Recovery
A killed swarm leaves a partial scratchpad and no packet (fail-open understanding path); relaunch is a fresh budgeted run. No side effects beyond `llm_calls` accounting and the returned packet. Budgets bound worst-case cost by construction.

## Progress
- [ ] M1 Orchestrator+budget  - [ ] M2 Scratchpad  - [ ] M3 Workers  - [ ] M4 Convergence/packet  - [ ] M5 Command room integration

## Surprises & Discoveries
(worker coordination patterns; budget accounting granularity)

## Decision Log
- 2026-07-18: Activated after EP-207 completed its five milestones and all validation gates. Both declared dependencies, EP-103 and EP-202, are done.

## Outcomes & Retrospective
(packet quality on fixtures; budget adherence evidence)

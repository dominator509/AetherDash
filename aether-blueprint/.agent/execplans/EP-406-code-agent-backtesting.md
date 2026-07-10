Layer: 5 - Execution

# EP-406: Code-Writing Agent, Cron Jobs, Backtesting Agent

**Band:** 4xx Cross-cutting | **Phase:** 4 | **Status:** draft | **Blocked by:** EP-403

## Purpose / Big Picture
Give AETHER bounded self-extension: a code-writing agent that generates plugins (which still pass the full EP-403 gate), scheduled automation (cron) under tier grants, and a backtesting agent that evaluates strategies against recorded history - all human-gated, metric-driven, never silently self-modifying (INV-10).

## Scope
Code-writing agent (generates plugin code + manifest -> EP-403 signing/sandbox/capability review -> human approval), cron/scheduler for automations (under SPEC-005 automation grants), backtesting agent (strategy eval over recorded data via the replay harness), the self-improvement proposal flow (metric-cited, human-gated diffs).

## Non-goals
No autonomous deployment of self-written code (every generated plugin goes through EP-403 + human approval - INV-6/INV-10), no unbounded automation (grants + budgets bound it), no rewriting AETHER's own rules/weights silently (INV-10 - proposals are human-gated diffs).

## Context and Orientation
INV-10 is the wall: self-improvement is metric-driven and human-gated; the system never silently rewrites its own rules/weights. Generated plugins are not special - they pass the same EP-403 gate as hand-written ones (signing, sandbox, capability review, dep-scan). Automations run under SPEC-005 automation grants (30-day default, budget-scoped). The backtesting agent uses EP-405's replay harness. Self-improvement proposals cite EP-402 attribution + EP-404 metrics (no metric, no proposal).

## Files to Read First
1. ARCHITECTURE.md INV-10 (self-improvement gate) + INV-6; EP-403 (plugin gate every generated plugin passes); SPEC-005 (automation grants, budgets).
2. EP-405 replay harness (backtest substrate); EP-402/404 (metric/attribution inputs for proposals); EP-205 (swarm patterns to reuse).

## Files to Change (Expected Changed Files)
`server/agents/code_writer/**` (generation -> manifest -> EP-403 submission), `server/agents/backtester/**` (strategy eval over replay), `server/scheduler/**` (cron under automation grants + budgets), self-improvement proposal flow (metric-cited diffs -> human review), tests, CHANGELOG, this file.

## Interfaces and Contracts
Code-writing agent output = a plugin (code + signed-after-approval manifest) that MUST pass EP-403's full gate; nothing loads unapproved. Cron automations hold SPEC-005 automation grants (budget + scope + expiry); an automation exceeding budget/scope is denied (EP-401). Backtester runs a strategy over recorded data via EP-405 harness, returns performance with the same net-edge honesty (SPEC-012). Self-improvement proposals = human-reviewable diffs citing specific metrics/attribution; applying one is a human action, never automatic.

## Milestones
1. **Code-writing agent -> plugin gate.** Generate plugin code + manifest via EP-202 (cache-first); submit to EP-403 (signing/sandbox/capability/dep-scan); nothing loads without human approval (EP-401 step-up). Done when: a generated plugin passes the full EP-403 gate end-to-end and loads only after approval; a generated plugin that violates capabilities is rejected by the gate (not special-cased).
2. **Backtesting agent.** Strategy definition -> eval over recorded data via EP-405 harness -> performance report (net-edge honest, SPEC-012). Done when: backtest over a recorded period produces a report; determinism (same strategy+data -> same result); no live/paper side effects.
3. **Cron / scheduler.** Automations under SPEC-005 automation grants + budgets; schedule/pause/revoke; budget/scope enforcement (EP-401). Done when: scheduler runs an automation under a grant; budget-exceeded-denied test; revoke-immediate test.
4. **Self-improvement proposal flow.** Proposals as metric-cited human-reviewable diffs (from EP-402 attribution + EP-404 metrics); no metric -> no proposal; applying is human-only. Done when: a proposal is generated citing real metrics; the no-metric-no-proposal guard test; the applying-is-human-only test (no auto-apply path exists).
5. **Integration + safety proof.** The whole loop (metrics -> proposal -> human -> optionally a generated plugin -> EP-403 gate -> approval) with INV-10 proven: no silent self-modification anywhere. Done when: end-to-end integration; INV-10 audit (grep + test: no code path applies a self-modification without human approval).

## Concrete Steps
The load-bearing constraint: generated code is NOT trusted - it's a plugin that passes the same gate as any other (EP-403), and self-improvement is proposals-not-actions (INV-10). All generation is cache-first via EP-202. Automations hold their own grants (never a human session's tier, SPEC-005) with budgets. The backtester reuses EP-405 (no second replay implementation). A test proves no auto-apply path for self-modification exists. Run security-review.md every milestone. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` + `security-check.sh` green; the INV-10 proofs (no-silent-self-modification, no-metric-no-proposal, applying-is-human-only) + generated-plugin-passes-EP-403-gate tests REQUIRED; `git diff --name-only` matches. Acceptance: Phase-4 exit - self-improvement proposals arrive as human-gated diffs with backtest evidence; a generated plugin passes the full gate.

## Idempotence and Recovery
Generated plugins are isolated (EP-403 sandbox) and revocable; automations are grant-bounded + budgeted (bounded worst case); backtests are deterministic + side-effect-free; proposals are inert until a human applies them. INV-10's human gate is the ultimate recovery - nothing self-modifies without approval. S9-adjacent: any path that would auto-apply self-modification is a hard line.

## Progress
- [ ] M1 Code-writer->gate  - [ ] M2 Backtester  - [ ] M3 Scheduler  - [ ] M4 Proposal flow  - [ ] M5 Integration+safety proof

## Surprises & Discoveries
(generation quality; backtest fidelity vs live; automation budget accounting)

## Decision Log
(strategy definition format; proposal diff format; scheduler design)

## Outcomes & Retrospective
(INV-10 proof bundle; generated-plugin gate evidence; backtest examples)

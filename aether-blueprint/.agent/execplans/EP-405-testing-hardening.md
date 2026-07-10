Layer: 5 - Execution

# EP-405: Testing Hardening - Replay Harness, Lifecycle Assertions, Regression

**Band:** 4xx Cross-cutting | **Phase:** 3 | **Status:** draft | **Blocked by:** EP-305, EP-307

## Purpose / Big Picture
Turn the testing conventions into enforced infrastructure: a first-class replay harness that reproduces recorded market days deterministically, shared lifecycle-closure and spec-traceability checkers, and a regression suite - so correctness on the execution path is provable and stays proven (INV-11, TESTING.md).

## Scope
The replay harness (recorded venue streams -> bus -> downstream assertions), shared lifecycle checker (every opportunity chain closes), spec-traceability auditor (every spec MUST maps to a named test), regression suite + fixtures, chaos-test scaffolding (RECOMMENDED), CI wiring for the new suites.

## Non-goals
No new product behavior (this hardens what exists), no coverage-percentage gate (TESTING.md stance - the gates are traceability/execution-path/replay/lifecycle), no load/perf tooling beyond what the budgets need.

## Context and Orientation
TESTING.md defines the replay harness as first-class and the lifecycle-closure + traceability gates; INV-11 makes the paper/backtest harness a first-class validation surface. EP-301/302/303 produced recordings; EP-304/307 produced the fill model + scanner whose determinism this harness proves; EP-402 produced the lifecycle/attribution this checker verifies. This plan centralizes and enforces.

## Files to Read First
1. TESTING.md (entire - replay, lifecycle, execution-path minimums, traceability); INV-11.
2. EP-307 replay determinism law; EP-304 parity contract; EP-402 lifecycle/attribution; the specs' Required Tests sections (traceability targets).

## Files to Change (Expected Changed Files)
`testing/replay-harness/**` (the harness: load recordings -> replay to bus with timing control -> assert downstream), `testing/checkers/{lifecycle,traceability}/**` (shared checkers), `testing/regression/**` (suite + fixtures), chaos scaffolding, CI wiring (nightly + a replay job), COMMANDS.md additions if new test entrypoints, tests-of-the-tests, CHANGELOG, this file.

## Interfaces and Contracts
Replay harness: given a recording + consumer set, replays with original or compressed timing and asserts byte-exact or tolerance-bounded downstream outputs (normalized quotes, `opps.detected`, simulator numbers, router decisions); determinism law: same recording -> same `opps.detected` sequence. Lifecycle checker: consumes `opportunity_events`, fails if any chain is open at teardown or reaches an illegal transition. Traceability auditor: parses specs' Required Tests + the test suite, fails on any MUST without a mapped named test.

## Milestones
1. **Replay harness core.** Load recordings, replay to the bus with a deterministic clock + timing control, capture downstream. Done when: harness replays a recorded Kalshi+Polymarket day and asserts deterministic normalized output; timing-compression mode works.
2. **Downstream assertions.** Assert scanner `opps.detected` sequence, simulator decomposition, router decisions against recorded expectations; determinism across 3 runs. Done when: determinism law test (3 identical runs -> identical opps sequence); decomposition + router-decision replay assertions green.
3. **Lifecycle checker.** Shared checker over `opportunity_events`; integrated into integration-suite teardown. Done when: checker fails a deliberately-left-open chain fixture and passes a clean run; wired into `test-integration.sh`.
4. **Spec-traceability auditor.** Parse each spec's Required Tests + the suite; map MUSTs to named tests; report gaps. Done when: auditor runs across SPEC-001..012 and reports a complete mapping (or names gaps); a deliberately-unmapped MUST fixture is caught.
5. **Regression suite + CI.** Curated regression fixtures (past bugs -> tests), nightly replay + regression job, a replay determinism gate. Done when: regression suite green; nightly CI runs replay + regression; a re-introduced past bug is caught by the suite.
6. **Chaos scaffolding (RECOMMENDED).** Kill-a-service-mid-flow harness proving clean recovery + no orphaned chains (SPEC-006 crash-only). Done when: chaos scaffold kills each execution service mid paper-flow and asserts recovery + lifecycle closure (RECOMMENDED - deferred-with-reason acceptable if time-boxed).

## Concrete Steps
Build the harness on the existing recordings (EP-301/302/303) and the fill model (EP-304) - it proves determinism, it doesn't create new behavior. The lifecycle checker and traceability auditor become standing gates (wired into CI), not one-off scripts. Regression fixtures capture real bugs as they're found. Chaos is RECOMMENDED - time-box it and defer-with-reason if needed (PRODUCTION_READINESS allows). Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-integration.sh` green (now includes lifecycle checker + replay assertions); `verify.sh` green; determinism law + lifecycle-closure + traceability tests REQUIRED; `git diff --name-only` matches. Acceptance: Phase-3 testing exit - deterministic replay reproduces recorded days bit-identically into the scanner; lifecycle + traceability gates enforced in CI.

## Idempotence and Recovery
The harness is deterministic and re-runnable (recordings + fixed clock); checkers are stateless gates; regression fixtures are versioned. The harness itself is the recovery-confidence tool (chaos proves crash-only recovery). This plan makes the whole system's correctness reproducible.

## Progress
- [ ] M1 Harness core  - [ ] M2 Downstream assertions  - [ ] M3 Lifecycle checker  - [ ] M4 Traceability auditor  - [ ] M5 Regression+CI  - [ ] M6 Chaos scaffolding

## Surprises & Discoveries
(timing-replay fidelity; bus determinism under replay; traceability parsing)

## Decision Log
(harness timing model; checker integration points; chaos scope)

## Outcomes & Retrospective
(determinism evidence; gates wired; regression coverage)

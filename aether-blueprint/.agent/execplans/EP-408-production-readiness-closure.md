Layer: 5 - Execution

# EP-408: Production Readiness Closure

**Band:** 4xx Cross-cutting | **Phase:** 4 | **Status:** draft | **Blocked by:** all above

## Purpose / Big Picture
Drive the production-readiness gate to green: implement the gate's evidence-enforcement mechanics (SPEC-008), run the full battery, gather real evidence for every REQUIRED item, verify all eleven invariants under test, and produce the archived gate output that declares AETHER v1 production-ready. This is the last plan; it closes the loop the whole pack was built to reach.

## Scope
SPEC-008 gate mechanics (evidence-placeholder guard, staleness enforcement), the full readiness sweep against PRODUCTION_READINESS.md, invariant verification across INV-1..11, evidence collection + archival, gate-behavior tests, the final release-readiness declaration.

## Non-goals
No new features (it closes what exists), no flipping checkboxes by agent (SPEC-008: humans/final-review flip boxes - this plan makes them flippable with real evidence and runs the battery), no enabling live trading (ceremony, OPERATIONS.md - the gate records its state, not flips it).

## Context and Orientation
SPEC-008 is the contract: the gate fails while any `- [ ] REQUIRED` exists, runs the full battery before the checklist parse, and (per rule 5, pre-authorized) gains an evidence-placeholder guard. PRODUCTION_READINESS.md holds the items. Every prior plan produced the evidence sources (tests, metrics, audit chain, deploy tooling). This plan verifies the eleven invariants hold TOGETHER under test and archives the result. Boxes are flipped by a human or the final-review prompt under instruction (bounded autonomy) - this plan gathers evidence and runs the gate.

## Files to Read First
1. SPEC-008 (gate mechanics + amendment rule 5); PRODUCTION_READINESS.md (every item); checklists/production-readiness.md.
2. The invariant ledger (GENERATION-STATE.md / ARCHITECTURE.md INV-1..11); every prior plan's Acceptance (the evidence sources).

## Files to Change (Expected Changed Files)
`scripts/production-readiness-check.sh` (add the evidence-placeholder guard + staleness check per SPEC-008 rule 5), gate-behavior tests, an invariant-verification suite (`testing/invariants/**` asserting INV-1..11 hold together), evidence-collection helpers, PRODUCTION_READINESS.md (checked items WITH real evidence - done by human/final-review, prepared here), ops-log archival of the gate output, CHANGELOG, this file.

## Interfaces and Contracts
Gate: full battery (verify + integration + e2e + security + dependency-audit + smoke) THEN checklist parse; fails on any `- [ ] REQUIRED`; fails on a checked REQUIRED line with placeholder evidence (`Evidence: ...` unresolved); time-sensitive items (backup streak, cache-hit ratio, audit-verify streak) require evidence within 30 days. Invariant suite: each of INV-1..11 has an explicit test that fails if the invariant is violated.

## Milestones
1. **Gate mechanics (SPEC-008 rule 5).** Add the evidence-placeholder guard + staleness enforcement to `production-readiness-check.sh`; gate-behavior tests (unchecked-REQUIRED fails; placeholder-evidence fails; RECOMMENDED-unchecked passes; battery-failure short-circuits). Done when: gate-behavior tests green on a staging copy of the checklist.
2. **Invariant verification suite.** Explicit tests that INV-1..11 hold together: no LLM/MCP on trading path (INV-1/2 - grep + runtime), cache-first prefix stability (INV-3), no anti-bot code + rung audit (INV-4), wallet isolation + withdrawal-human (INV-5), plugin gate (INV-6), venue-additive INV-7 diff checks across all packs, mode-parity (INV-8), db-truth + vault-generated (INV-9), no-silent-self-mod (INV-10), separate-tested-services + paper-harness (INV-11). Done when: the invariant suite runs and all eleven pass.
3. **Full readiness sweep.** Run the whole battery; walk every PRODUCTION_READINESS.md item; identify real evidence or the remaining gap for each. Done when: a readiness report lists every item with its evidence source or gap; battery green.
4. **Evidence collection + closure.** Gather evidence (command outputs, metric snapshots, audit ids, ops-log lines) for each REQUIRED item; where a gap remains, either close it (small) or STOP with the specific blocker (S-class). Done when: every REQUIRED item has real evidence attached OR a documented human-decision blocker; time-sensitive streaks (7-day audit-verify, backup, cache-hit) satisfied.
5. **Gate green + declaration.** With evidence in place (boxes flipped by human/final-review per SPEC-008), `production-readiness-check.sh` prints `production readiness: ok`; archive the output in the ops log with the release tag; the v1 production-readiness declaration. Done when: gate exits 0; output archived; v1 declared ready (or the exact remaining human decisions enumerated).

## Concrete Steps
Build the gate mechanics + invariant suite first (they're the teeth), then run the sweep and collect evidence. The eleven-invariant suite is the capstone proof - it asserts the properties that made the whole architecture worth the discipline, holding TOGETHER not just individually. Boxes are flipped by a human or the final-review prompt under explicit instruction (SPEC-008 bounded autonomy) - this plan prepares real evidence and runs the battery; it does not self-certify. Time-sensitive streaks require actual elapsed time (7-day windows) - plan for that, don't fake it. Where a REQUIRED item can't be met, STOP with the specific blocker rather than checking a box dishonestly. Run every relevant checklist.

## Validation and Acceptance
Per-milestone; full battery green; `verify.sh` + `security-check.sh` + `dependency-audit.sh` green; gate-behavior + invariant-suite tests REQUIRED; `git diff --name-only` matches. Acceptance: `scripts/production-readiness-check.sh` prints `production readiness: ok` with every REQUIRED item backed by real evidence, all eleven invariants verified under test, and the gate output archived - OR the precise set of remaining human decisions/blockers enumerated for the operator. This is the pack's terminal acceptance.

## Idempotence and Recovery
The gate is re-runnable and evidence decays (SPEC-008 rule 6) - readiness is a maintained state, not a one-time stamp; re-running per release keeps it honest. The invariant suite is a standing regression guard. If a REQUIRED item regresses, the gate goes red and blocks release (RELEASE.md) - exactly as intended.

## Progress
- [ ] M1 Gate mechanics  - [ ] M2 Invariant suite  - [ ] M3 Readiness sweep  - [ ] M4 Evidence+closure  - [ ] M5 Gate green+declaration

## Surprises & Discoveries
(gaps found during the sweep; time-sensitive streak logistics; invariant-together edge cases)

## Decision Log
(evidence-placeholder guard implementation; invariant test approaches; any deferred RECOMMENDED items)

## Outcomes & Retrospective
(the archived gate output; invariant-suite results; v1 readiness declaration or remaining decisions)

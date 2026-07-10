Layer: 5 - Execution

# EP-XXX: <Title>

**Band:** 0xx|1xx|2xx|3xx|4xx | **Phase:** N | **Status:** draft | **Blocked by:** EP-YYY, ...
(Keep this header in sync with .agent/PLANS.md.)

## Purpose / Big Picture
One paragraph: what exists after this plan that did not before, and why it matters to AETHER.

## Scope
Bulleted, concrete deliverables. If it is not listed here, it is not in this plan.

## Non-goals
Explicitly excluded work, including tempting adjacent work. Binding.

## Context and Orientation
What an agent with zero conversation history must know: relevant invariants (INV-x), prior plans this builds on, key architecture sections.

## Files to Read First
Ordered list with one-line reasons (specs, existing modules, contracts).

## Files to Change (Expected Changed Files)
Exhaustive list of files this plan may create or modify. Final review diffs against this.

## Interfaces and Contracts
Proto messages, bus topics, traits, API routes, DB tables touched - with exact names. New names must be registered where ARCHITECTURE.md says they live.

## Milestones
Numbered, 6-10, each independently validatable. Each milestone states: goal, done-when.

## Concrete Steps
Per milestone: the actual sequence of edits/commands, precise enough that a lower-tier agent never guesses. Reference COMMANDS.md entries by name.

## Validation and Acceptance
Per milestone: exact command(s) + expected output. Plus plan-level acceptance criteria: the checklist that defines done, each criterion machine-checkable.

## Idempotence and Recovery
How to safely re-run steps; what to do if the repo differs from the plan's expectations; migration/rollback notes; which failures trigger STOP vs retry ladder.

## Progress
- [ ] M1 ...
(Agent updates as work happens. Falsified entries are corrected, not deleted.)

## Surprises & Discoveries
Dated notes: unexpected repo states, failed hypotheses (R10 step 3), upstream quirks.

## Decision Log
Dated entries: decision, alternatives, why, scope of impact. Durable ones also become ADRs.

## Outcomes & Retrospective
Filled at completion: what shipped, deviations from plan, remaining risks, follow-ups filed.

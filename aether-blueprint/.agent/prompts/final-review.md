Layer: 1 - Governance

# Prompt: Final Review of a Completed ExecPlan

Read AGENTS.md, .agent/EXECUTION_RULES.md, the ExecPlan under review, and COMMANDS.md. Act as a skeptical reviewer, not the author.

Checks, in order; report each PASS/FAIL with evidence:
1. `scripts/verify.sh` prints `verify: ok`.
2. Every acceptance criterion in the plan passes when its stated command/check is run now.
3. `git diff --name-only` (or the plan's recorded diff) contains only Expected Changed Files; every extra file has a Decision Log justification; no file falls in S7-S9 territory.
4. No forbidden architecture moves (ARCHITECTURE.md section 12) and no D1-D7 dependency violations in the diff.
5. `scripts/security-check.sh` passes; no secrets, no `.env` values, no key material anywhere in diff, logs, or plan text.
6. Tests were added/updated per AGENTS.md section 10; no test was deleted or weakened to pass.
7. Docs moved with code (R13): specs, COMMANDS.md, ENVIRONMENT.md, DECISIONS.md as applicable.
8. Plan hygiene: Progress current, Surprises & Discoveries honest, Decision Log complete, Outcomes & Retrospective written, remaining risks stated.

Verdict: APPROVE (set plan `done` in .agent/PLANS.md if not already) or REQUEST CHANGES with the exact list of defects, each tied to a rule (AGENTS.md / EXECUTION_RULES / ARCHITECTURE) and the smallest fix.

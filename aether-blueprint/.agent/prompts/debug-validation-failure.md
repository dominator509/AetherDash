Layer: 1 - Governance

# Prompt: Debug a Failing Validation Command

Read AGENTS.md, COMMANDS.md, .agent/EXECUTION_RULES.md, and the active ExecPlan. You are given one failing validation command from COMMANDS.md or the plan.

Procedure:
1. Re-run the exact failing command; capture the full error output.
2. State the likely root cause in one sentence before editing anything.
3. Apply the bounded-retry ladder (R10): smallest targeted fix -> narrower diagnostic (single test, single crate/package, verbose flags) -> after a third same-root failure, abandon the approach, record failed hypotheses in Surprises & Discoveries, and choose a simpler spec-consistent path.
4. Never delete, skip, or weaken a failing test to pass. Never patch around the same error without a new hypothesis. Never guess APIs or commands - verify in the repo (R4, R7, R9).
5. If the failure requires a missing secret/service, threatens data, or needs a judgment the specs do not resolve, STOP per AGENTS.md section 4 and report per R11.

When the command passes: re-run the plan's full validation set for the current milestone, update Progress, and report - failing command, root cause, fix applied, files changed, evidence of the pass.

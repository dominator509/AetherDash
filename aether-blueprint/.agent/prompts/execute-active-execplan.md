Layer: 1 - Governance

# Prompt: Execute the Active ExecPlan

Read AGENTS.md, COMMANDS.md, .agent/EXECUTION_RULES.md, .agent/PLANS.md, and the ExecPlan marked `active`.

Implement that ExecPlan to completion.

- Do not ask for next steps. Do not stop after partial work.
- Do not implement from ROADMAP.md. Do not touch other plans.
- Do not broaden scope; the plan's Non-goals are binding.
- Run `scripts/preflight.sh` before the first milestone.
- Complete milestones in order; run each milestone's validation commands; update the plan's Progress section after each.
- Use only commands from COMMANDS.md.
- Apply the bounded-retry ladder (EXECUTION_RULES.md R10) to failures.
- Stop only for STOP conditions in AGENTS.md section 4; report per R11.
- The AETHER hard lines (R12) are absolute: no live trading, no wallet/key code, no weakening of permissions/audit/redaction, no anti-bot tooling.

At the end: run `scripts/verify.sh`, run `git diff --name-only`, reconcile against Expected Changed Files, update Outcomes & Retrospective, set the plan `done` in .agent/PLANS.md, and produce the final response per EXECUTION_RULES.md R14.

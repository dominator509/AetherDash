Layer: 1 - Governance

# Prompt: Continue a Partially Completed ExecPlan

Read AGENTS.md, COMMANDS.md, .agent/EXECUTION_RULES.md, .agent/PLANS.md, and the ExecPlan marked `active`, including its Progress, Surprises & Discoveries, and Decision Log sections.

Re-derive the true state before writing anything:
1. Run `scripts/preflight.sh` and `git status --short`.
2. Run the validation commands of every milestone marked complete in Progress; a failing one means Progress is wrong - correct Progress first and note it in Surprises & Discoveries.
3. Identify the first incomplete milestone from evidence (files present, tests passing), not from memory or conversation history.

Then continue implementing from that milestone under the same rules as execute-active-execplan.md: milestone order, per-milestone validation, Progress updates, bounded retry, STOP conditions only, AETHER hard lines absolute.

Do not redo completed milestones except to fix a falsified Progress entry. Do not ask for next steps. Finish the plan and produce the final response per EXECUTION_RULES.md R14.

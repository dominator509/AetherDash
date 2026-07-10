Layer: 6 - Verification & Operations

# Checklist: Before Writing Any Code (per ExecPlan)

- [ ] Read AGENTS.md, COMMANDS.md, ARCHITECTURE.md, the active ExecPlan, and its owning spec(s).
- [ ] Confirmed exactly one plan is `active` in `.agent/PLANS.md`; blocked-by plans are `done` (or justified in Decision Log).
- [ ] Ran `scripts/preflight.sh` -> `preflight: ok` (resolved MISSING TOOL as S1 if needed).
- [ ] Ran `git status --short`; tree matches the plan's expected starting state.
- [ ] Inspected the actual files to be changed; confirmed imports/APIs/tables/topics exist (no assumptions, R4/R9).
- [ ] Confirmed no needed secret/service is missing (else STOP S1 before starting).
- [ ] Identified which milestone failures are STOP conditions vs retry-ladder.
- [ ] Confirmed Expected Changed Files respect D1-D7 and ARCHITECTURE.md section 12.

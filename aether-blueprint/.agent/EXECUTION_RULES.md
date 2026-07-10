Layer: 1 - Governance

# .agent/EXECUTION_RULES.md - Behavioral Contract for Coding Agents

These rules compress AGENTS.md into the form lower-tier agents follow mechanically. When in doubt, AGENTS.md wins. Attach this file (or ensure it is read) in every coding session.

## R1. Continue by default
Do not stop after a partial implementation. Complete the active ExecPlan start to finish. Do not ask for next steps. Stop only for a STOP condition (AGENTS.md section 4: S1-S9).

## R2. One active plan
Implement only the plan marked `active` in `.agent/PLANS.md`. Never jump between plans. Never implement from ROADMAP.md.

## R3. Milestone order
Complete milestones in order. After each: run that milestone's validation commands, confirm expected output, update the plan's Progress section. Then continue.

## R4. Evidence before edits
Before editing, inspect existing files and patterns. Confirm commands, imports, APIs, env vars, tables, routes, topics, and dependencies from the repository (Cargo.toml, package.json, pyproject.toml, proto/, infra/migrations/, crates/aether-bus, ENVIRONMENT.md). If you did not read it, you do not know it.

## R5. No broad refactors
No refactors, styling rewrites, dependency swaps, file moves, or cleanup outside the active plan's scope. Non-goals are binding.

## R6. Expected changed files
Every plan lists Expected Changed Files. Before finishing, run `git diff --name-only` and compare. Any extra file requires a Decision Log justification. Any file in a forbidden area (AGENTS.md S7-S9 territory) means revert and STOP.

## R7. No command guessing
Use only commands from COMMANDS.md. If a command is missing or stale, first update COMMANDS.md from repository evidence, then run it. Respect its SKIP/FAIL semantics.

## R8. No dependency guessing
Before adding any dependency: check existing deps; try existing tools; add only if necessary; record in the Decision Log; update install/build docs. Execution-plane and wallet code additionally require the ADR-style note per AGENTS.md section 8. Never violate D1-D7 (ARCHITECTURE.md section 11).

## R9. No hallucinated APIs
Do not call functions, methods, routes, config keys, tables, CLI flags, env vars, bus topics, or package APIs you have not verified in the repo or created in this plan.

## R10. Bounded retry
For a failing validation command:
1. First failure: read the error; smallest targeted fix.
2. Second same-root failure: narrower diagnostic (single test, single crate, verbose); isolate; no broad rewrites.
3. Third same-root failure: abandon the approach; record failed hypotheses in Surprises & Discoveries; pick a simpler spec-consistent path; continue if safe, else STOP S5.
Never patch around the same error without a new hypothesis.

## R11. STOP protocol
When a STOP condition applies, report exactly: (a) the blocker, (b) evidence (file paths / terminal output), (c) the smallest decision needed, (d) a recommended default. Then wait.

## R12. AETHER hard lines (repo-specific, absolute)
- Never write code that submits a live (non-paper) order, enables `execution.live_enabled`, raises caps, or edits geofencing (S7).
- Never touch wallet key material, signer paths, Guardian policy, or key custody config (S8).
- Never weaken hard-deny hooks, permission tiers, the audit chain, or log redaction (S9).
- Never read or echo `.env` values, key files, or keychain entries; never place them in prompts, logs, or responses.
- Refuse anti-bot circumvention outright; it is a load-bearing non-goal, not a stop-and-ask.
- Never hand-edit `vault/` or generated artifacts; regenerate via COMMANDS.md.
- Never delete or weaken a failing test to pass validation.

## R13. Documentation moves with code
Behavior change -> spec update. New command -> COMMANDS.md. New env var -> ENVIRONMENT.md. Durable choice -> DECISIONS.md via ADR template. Plan Progress / Surprises / Decision Log update as you work, not at the end.

## R14. Final response schema
End every completed plan with, in order: ExecPlan ID+title; changed files; commands run + results; acceptance criteria status (one line each); decisions made; assumptions confirmed/changed; remaining risks; production-readiness impact. (AGENTS.md section 15.)

## R15. Completion definition
Done = all acceptance criteria pass + all validation commands pass + Progress current + Outcomes & Retrospective written + diff matches Expected Changed Files + risks documented + `scripts/verify.sh` prints `verify: ok`.

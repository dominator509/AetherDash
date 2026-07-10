Layer: 2 - Product & Decisions

# CONTRIBUTING.md

One operator, many agents. "Contributor" means either; the rules are the same, the enforcement differs (humans get judgment, agents get AGENTS.md).

## Before anything
Read, in order: AGENTS.md -> COMMANDS.md -> ARCHITECTURE.md -> the active ExecPlan (`.agent/PLANS.md`). Agents follow the executor prompts in `.agent/prompts/`; humans follow the same workflow minus the prompt wrapper. No work exists outside an ExecPlan (even one-line fixes ride the active plan's Decision Log or a micro-plan from the template).

## Branch and commit conventions
- Branches: `ep-XXX/<slug>` (one plan, one branch). Micro-fixes: `fix/<slug>`.
- Conventional commits with plane/crate scope: `feat(router): reject stale quotes past drift band`, `fix(kalshi): handle empty book snapshot`, `chore(infra): pin clickhouse 24.8`. Types: feat, fix, chore, docs, test, refactor (refactor only when the plan scopes it - EXECUTION_RULES R5).
- Every commit compiles and passes `scripts/format-check.sh` + `scripts/lint.sh` locally (fast pair); `verify.sh` gates the branch, not each commit.

## PR / merge checklist (self-review for solo work; the final-review prompt is the reviewer)
1. `scripts/verify.sh` -> `verify: ok`; security-check clean.
2. Diff matches Expected Changed Files; extras justified in the Decision Log.
3. Specs/COMMANDS/ENVIRONMENT/DECISIONS updated with the change (R13); CHANGELOG Unreleased entry added.
4. New behavior has tests per TESTING.md; nothing weakened.
5. No D1-D7 violations, no forbidden moves (ARCHITECTURE.md 12), no HARD-DENY grazes (SECURITY.md).

## Code style
Formatting is whatever the formatters say - rustfmt, prettier, ruff-format - with zero local overrides; style debates are config PRs, not review comments. Naming follows the domain vocabulary in SPEC-001 terms; do not coin synonyms (a Market is not a "contract", an Opportunity is not a "signal"). Errors: Rust `thiserror` per-crate error enums, `anyhow` only in binaries; Python raises typed exceptions mapped at service edges to the SPEC-003 error envelope; TS never throws strings.

## Adding things (recipes)
- **Venue:** ARCHITECTURE.md section 13 recipe; one EP-3xx plan; INV-7 means zero core edits.
- **Bus topic / proto method / env var / command:** register in `crates/aether-bus` / `proto/` / ENVIRONMENT.md / COMMANDS.md respectively in the same plan - unregistered names do not exist (R9).
- **Spec:** from `spec-template.md`, numbered next in sequence, linked from the owning plan.
- **ADR:** from `adr-template.md`, appended to DECISIONS.md, referenced from the Decision Log entry that spawned it.
- **Plugin (Phase 4+):** starts from the plugin template pack, must pass manifest signing + sandbox suite before load; never bypasses capability review (INV-6).

## What gets a change rejected outright
Secrets anywhere; LLM/MCP on the execution path (INV-1/2); venue-specific branches in core (INV-7); hand edits to `vault/` or generated artifacts; weakened tests; scope outside the active plan; anti-bot anything (load-bearing non-goal - this one is a refusal, not a discussion).

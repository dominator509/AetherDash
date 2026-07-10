Layer: 6 - Verification & Operations

# USAGE.md - How to Use This Blueprint Pack

This pack is a complete, self-contained specification for building AETHER Terminal with coding agents (or humans). It is designed so a lower-tier coding LLM can execute it without the original design conversation. Read this once before starting.

## What this pack is
A six-layer blueprint (see THE SIX-LAYER PARADIGM in the master prompt): governance, product/decisions, architecture, specification, execution, verification/operations. Every file declares its layer. Nothing here is application code yet - it is the instructions, contracts, and guardrails for producing that code, one ExecPlan at a time.

## The reading order for a new agent
1. `AGENTS.md` - how you must behave (STOP conditions S1-S9, anti-drift, bounded retry).
2. `.agent/EXECUTION_RULES.md` - the compressed behavioral contract (R1-R15).
3. `COMMANDS.md` - the only legal commands.
4. `ARCHITECTURE.md` - boundaries, the eleven invariants (INV-1..11), dependency rules D1-D7.
5. `.agent/PLANS.md` - the ledger; find the one plan marked `active`.
6. That plan, plus the spec(s) it names.

## The execution loop
- Exactly ONE plan is `active` at a time. Implement only it. Never implement from `ROADMAP.md`.
- Use the executor prompts in `.agent/prompts/`: `execute-active-execplan` (start), `continue-execplan` (resume), `debug-validation-failure` (a red command), `final-review` (before done).
- Work milestones in order; validate each with its exact commands; update the plan's Progress as you go.
- Continue autonomously; stop only for a STOP condition (AGENTS.md section 4). Refuse anti-bot circumvention outright (it is a load-bearing non-goal, not a stop-and-ask).
- Finish with the final-response schema (EXECUTION_RULES R14) and set the plan `done`.

## The build order (phases -> plans)
Follow `.agent/PLANS.md` dependencies. The intended sequence:
- **Phase 0 (foundation):** EP-000 -> EP-001 -> EP-002 -> EP-003 -> EP-004. After this, all three stacks build and `scripts/verify.sh` is meaningful.
- **Phase 1 (terminal core):** client EP-101/102/103/104, brain EP-201/202/203/204, connectors EP-301/302/303/304. Real multi-venue data, a Brain, alerts, paper trading - no live money.
- **Phase 2 (execution):** EP-305 (router+risk), EP-306 (Guardian), EP-307 (scanner+simulator), EP-308 (comms), EP-401 (permissions), EP-402 (audit+attribution), EP-404 (observability). Small live trades behind the `live_enabled` wall.
- **Phase 3 (deep brain):** EP-206 (ingestion fleet), EP-207 (recall v2), EP-405 (testing hardening).
- **Phase 4 (agents/plugins):** EP-205 (swarms), EP-403 (plugins), EP-406 (code+backtest agents), EP-407 (deploy), EP-408 (production-readiness closure).
Note the cross-band blocks (e.g., EP-305 needs EP-401; EP-102 needs EP-304) - the ledger's Blocked-by column is authoritative.

## The load-bearing rules (violate none)
The eleven invariants in ARCHITECTURE.md section 10 are non-negotiable. The ones that most often tempt shortcuts:
- AI is pilot, not engine - no LLM/MCP on the order or wallet path (INV-1/2).
- Wallet isolated behind the Guardian; withdrawals always human-approved; keys never enter model context (INV-5).
- Venues are additive extension packs; adding one edits zero core files (INV-7) - the mechanical diff check is in `checklists/venue-pack.md`.
- Databases are truth; the `vault/` is a generated view - never hand-edit it (INV-9).
- Self-improvement is human-gated; nothing silently rewrites its own rules (INV-10).
- No anti-bot circumvention anywhere (INV-4 / PROJECT_BRIEF non-goals) - a refusal, not a discussion.

## Checklists
`.agent/checklists/` gates the work: `new-execplan` (authoring), `pre-implementation`, `per-milestone`, `pre-completion`, `security-review`, `venue-pack`, `execution-path-change`, `release`, `production-readiness`. Run the relevant ones; the execution-path and security checklists are mandatory for router/risk/guardian/permission changes.

## Verification
Everything routes through `scripts/` (see COMMANDS.md). The gate is `scripts/verify.sh` -> `verify: ok`. Scripts are marker-gated: they SKIP a stack whose marker file is absent (correct on an empty repo) and FAIL on real problems - a SKIP is not a failure. The terminal gate is `scripts/production-readiness-check.sh` (EP-408).

## Adding a venue later (the common extension)
Follow ARCHITECTURE.md section 13 and `checklists/venue-pack.md`: copy `connectors/venues/_template/` (produced by EP-301), fill `venue.toml`, implement the adapter against recorded fixtures, add replay tests, register via the single seed migration, write an EP-3xx plan. The INV-7 diff check must pass: only the pack's own paths change.

## Regenerating or resuming pack GENERATION (meta)
If the pack itself is being generated across sessions, `GENERATION-STATE.md` is the manifest; its resumption prompt regenerates the next pending files in output order. This is separate from executing the pack (above) - one produces the blueprint, the other builds the product.

## Where things live (quick map)
Governance/behavior -> `AGENTS.md`, `.agent/`. Product/why -> `PROJECT_BRIEF.md`, `ROADMAP.md`, `DECISIONS.md`, `ASSUMPTIONS.md`. Boundaries -> `ARCHITECTURE.md`, `SECURITY.md`, `ENVIRONMENT.md`. Contracts -> `.agent/specs/`. Work -> `.agent/execplans/`, `.agent/PLANS.md`. Proof/ops -> `COMMANDS.md`, `TESTING.md`, `OBSERVABILITY.md`, `DEPLOYMENT.md`, `OPERATIONS.md`, `RELEASE.md`, `ROLLBACK.md`, `PRODUCTION_READINESS.md`, `scripts/`.

## Start now
Set EP-000 `active` in `.agent/PLANS.md` (it already is the intended first activation), open `.agent/prompts/execute-active-execplan.md`, and go. The pack does the rest.

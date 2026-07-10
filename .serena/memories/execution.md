# Execution Model

## ExecPlan Workflow
1. Read AGENTS.md
2. Read COMMANDS.md
3. Read `.agent/PLANS.md`; identify single active ExecPlan; read it fully
4. Run `scripts/preflight.sh`
5. Complete milestones in order
6. After each milestone: run milestone validation; update ExecPlan Progress
7. Continue autonomously to next milestone
8. Stop only under STOP condition (S1-S9)
9. At completion: `scripts/verify.sh`, `git diff --name-only`, update Outcomes & Retrospective, final response

## STOP Conditions (S1-S9)
- S1: Missing secret/credential/paid service/external account
- S2: May destroy user or production data
- S3: Legal/security/financial judgment needed, spec doesn't resolve
- S4: Materially different user-visible behavior choice not resolved by spec
- S5: Required tests can't run after recovery ladder exhausted
- S6: Production deployment or irreversible migration without permission
- S7 (AETHER): Any change that could submit live order, enable live trading, raise/remove caps, alter geofencing
- S8 (AETHER): Any change touching wallet key material, signer paths, Guardian policy, key custody
- S9 (AETHER): Any change disabling/bypassing/weakening hard-deny hooks, permission tiers, audit chain, log redaction

## Anti-Drift
- Implement only what active ExecPlan scopes
- No broad refactors, styling rewrites, dependency swaps, file reorganizations, unrelated cleanup
- Final review: `git diff --name-only` vs Expected Changed Files; extras justified
- Never edit files outside active ExecPlan's plane band unless plan names them
- Do not implement from ROADMAP.md directly

## Anti-Hallucination
- Confirm package APIs, command names, env vars, DB tables, routes, config keys, gRPC methods, bus topics by reading repository files
- Commands from COMMANDS.md only
- Before adding dependency: check existing; verify existing tools don't suffice; record decision; update docs
- Record every assumption in Decision Log and ASSUMPTIONS.md if durable

## Bounded Retry (Anti-Fixation)
1. First failure: read error, likely cause, smallest fix
2. Second same-root: narrower diagnostic, isolate, no broad rewrites
3. Third same-root: stop approach, record failed hypotheses, choose simpler path
Never patch blindly around same error indefinitely.

## Definition of Done
- Every acceptance criterion passes
- All required validation commands pass
- ExecPlan Progress current; Outcomes & Retrospective written
- Final diff reviewed; extras justified
- Remaining risks documented
- `scripts/verify.sh` prints `verify: ok`
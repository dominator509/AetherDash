Layer: 6 - Verification & Operations

# Checklist: Before Declaring an ExecPlan Done

- [ ] Every acceptance criterion passes when its command/check is run now.
- [ ] All milestone validation commands pass.
- [ ] `scripts/verify.sh` prints `verify: ok`.
- [ ] `scripts/security-check.sh` prints `security: ok`.
- [ ] `git diff --name-only` matches Expected Changed Files; extras justified in Decision Log; nothing in S7-S9 territory.
- [ ] No D1-D7 violation, no forbidden move (ARCHITECTURE.md 12) in the diff.
- [ ] Docs moved with code: specs, COMMANDS.md, ENVIRONMENT.md, DECISIONS.md, CHANGELOG Unreleased (R13).
- [ ] Tests added/updated; none deleted or weakened to pass.
- [ ] Progress current; Surprises & Discoveries honest; Decision Log complete; Outcomes & Retrospective written; remaining risks stated.
- [ ] Production-readiness impact assessed and noted.
- [ ] Plan set to `done` in `.agent/PLANS.md`.
- [ ] Final response follows the EXECUTION_RULES R14 schema.

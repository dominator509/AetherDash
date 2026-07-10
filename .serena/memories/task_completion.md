# Task Completion

When a coding task is considered done, run these in order:

## Minimum Gate
```bash
scripts/verify.sh    # preflight -> format-check -> lint -> typecheck -> unit -> build
```
Must print `verify: ok`.

## For ExecPlan Completion
1. All milestone validation commands pass
2. `scripts/verify.sh` prints `verify: ok`
3. `scripts/security-check.sh` prints `security: ok`
4. `git diff --name-only` compared against Expected Changed Files list; extras justified in Decision Log
5. ExecPlan Progress section updated; Outcomes & Retrospective written
6. Remaining risks documented

## For Production-Impacting Changes
```bash
scripts/production-readiness-check.sh   # full gate including integration + e2e + security + audit + smoke
```

## Pre-Commit Checklist
- No secrets in commits (verified by security-check)
- Lockfiles updated if deps changed
- New code follows module conventions (`mem:conventions`)
- Tests exist for new functionality (unit for all, integration for execution-plane)
- New env vars documented in ENVIRONMENT.md
- New commands documented in COMMANDS.md
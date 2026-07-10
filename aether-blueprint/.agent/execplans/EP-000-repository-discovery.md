Layer: 5 - Execution

# EP-000: Repository Discovery & Pack Installation Check

**Band:** 0xx Foundation | **Phase:** 0 | **Status:** draft | **Blocked by:** -

## Purpose / Big Picture
Establish ground truth before anything is built: confirm the repo matches assumption A-01 (greenfield + this pack), put the pack under version control, and prove the verification harness runs on the empty tree. Every later plan trusts the baseline this plan records.

## Scope
Git initialization, baseline commit, preflight/verify baseline run, discovery notes.

## Non-goals
No scaffolding (EP-001), no configs, no directories beyond what git needs, no remote push (A-17: operator confirms remote policy first - treat as S1-class if a remote is requested).

## Context and Orientation
Read AGENTS.md sections 2-4 and COMMANDS.md "Greenfield gating" first. All stack commands are expected to SKIP; that is correct behavior, not failure.

## Files to Read First
1. GENERATION-STATE.md - what exists by design.
2. COMMANDS.md - SKIP/FAIL semantics.
3. ASSUMPTIONS.md A-01, A-17.

## Files to Change (Expected Changed Files)
`.gitignore` (created), this file (Progress/Decision Log/Outcomes). Nothing else.

## Interfaces and Contracts
None introduced.

## Milestones
1. **Discovery.** Goal: verify A-01. Done when: inventory recorded in Surprises & Discoveries.
2. **Version control.** Goal: repo under git with the pack as the first commit. Done when: `git log --oneline` shows exactly one commit.
3. **Baseline verification.** Goal: harness runs clean on empty tree. Done when: `verify.sh` prints `verify: ok`.

## Concrete Steps
M1: `find . -type f -not -path './.git/*' | sort` - compare against GENERATION-STATE.md manifest. Any file NOT in the manifest and not obviously operator-added -> record and STOP S4 if it implies existing code (A-01 falsified). Run `scripts/preflight.sh`; resolve MISSING TOOL (exit 2) results by reporting S1 with the tool list - do not install system tools unasked.
M2: `git init -b main` (if `.git` absent). Create `.gitignore` with exactly:
```text
target/
node_modules/
dist/
.venv/
__pycache__/
*.pyc
.env
.env.*
!.env.example
vault/*
!vault/.gitkeep
data/
*.log
.DS_Store
```
Then `git add -A && git commit -m "chore(repo): install AETHER blueprint pack (S1-S3 output)"`.
M3: `scripts/verify.sh` - expect SKIP notices for rust/ts/py markers and final `verify: ok`. `scripts/security-check.sh` - expect `security: ok` (the pack itself must pass its own scan).

## Validation and Acceptance
- M1: preflight prints `preflight: ok`; inventory note exists.
- M2: `git log --oneline | wc -l` -> 1; `git status --porcelain` empty.
- M3: `verify.sh` -> `verify: ok`; `security-check.sh` -> `security: ok`.
Acceptance criteria: all three validations pass; A-01 confirmed or the plan stopped with evidence; baseline recorded in Outcomes.

## Idempotence and Recovery
Every step is re-runnable: `git init` on an existing repo is a no-op; the commit step is skipped if the tree is clean and committed. If the repo already has non-pack commits, that contradicts A-01: STOP S4 with `git log` evidence.

## Progress
- [ ] M1 Discovery
- [ ] M2 Version control
- [ ] M3 Baseline verification

## Surprises & Discoveries
(record inventory + any deviations here)

## Decision Log
(expected empty; a non-empty log on EP-000 usually means A-01 issues)

## Outcomes & Retrospective
(baseline: file count, commit hash, verify output summary)

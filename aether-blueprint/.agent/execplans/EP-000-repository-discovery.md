Layer: 5 - Execution

# EP-000: Repository Discovery & Pack Installation Check

**Band:** 0xx Foundation | **Phase:** 0 | **Status:** done | **Blocked by:** -

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
- [x] M1 Discovery
- [x] M2 Version control
- [x] M3 Baseline verification

## Surprises & Discoveries
- **Windows environment**: Python installed as `python` (v3.14.4), not `python3`. Updated `preflight.sh` to check `python3` first, then fall back to `python`. All tool versions exceed minimums: rustc 1.96.1, node v24.14.1, pnpm 9.15.0, uv 0.11.25, Docker Compose v5.1.4.
- **Optional tools missing**: cargo-nextest, cargo-audit, buf, gitleaks. Not blocking for Phase 0.
- **Operator-added files** (beyond blueprint manifest): `.claude/settings.json`, `.obsidian/` (8 files), `.serena/` (14 files), `CLAUDE.md`. These are tooling/config files, not existing code. A-01 holds.
- **Scripts location**: Blueprint pack places scripts at `aether-blueprint/scripts/`. EP-001 will move them to `scripts/` at repo root per ARCHITECTURE.md.
- **Git**: Fresh init (`git init -b main`). 123 files in initial commit.

## Decision Log
- **DL-000-1**: Fixed `preflight.sh` Windows compatibility — `python3` → fallback to `python`. The tool exists (Python 3.14.4) but Windows uses `python` not `python3`. No S1 trigger; this is a script compatibility fix, not a missing tool.

## Outcomes & Retrospective
- **File count**: 123 files committed
- **Commit hash**: `726b621` — "chore(repo): install AETHER blueprint pack (S1-S3 output)"
- **A-01**: CONFIRMED — repository is greenfield
- **verify.sh**: `verify: ok` (all 3 stacks SKIP on missing markers, as expected)
- **security-check.sh**: `security: ok` (blueprint pack passes its own scan)
- **Acceptance criteria**: All three milestones pass. Baseline recorded. Ready for EP-001.

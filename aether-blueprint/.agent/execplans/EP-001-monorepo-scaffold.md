Layer: 5 - Execution

# EP-001: Monorepo Scaffold, Toolchains, CI Skeleton

**Band:** 0xx Foundation | **Phase:** 0 | **Status:** draft | **Blocked by:** EP-000

## Purpose / Big Picture
Turn the empty repo into the ADR-0001 monorepo: three workspace roots (cargo/pnpm/uv) with formatter/linter/typechecker configs that make the lint rules in SPEC-006 real, the directory skeleton from ARCHITECTURE.md section 1, and thin CI that only calls scripts (ADR-0009). After this plan, all three stacks are ACTIVE in COMMANDS.md terms and `verify.sh` exercises them.

## Scope
Workspace roots and shared configs; directory skeleton with `.gitkeep`s; CHANGELOG.md; `.editorconfig`; CI workflow files; `infra/dev/.env.example`.

## Non-goals
No domain code (EP-002), no compose stack (EP-003), no proto (EP-004), no dependency additions beyond dev-tooling declared here, no remote push (A-17).

## Context and Orientation
ARCHITECTURE.md section 1 fixes the layout; ENVIRONMENT.md fixes env names for the example file; SPEC-006 "no silent catch" fixes three lint rules that MUST land in configs now (cheap now, painful later).

## Files to Read First
1. ARCHITECTURE.md section 1 (repo map) and 11 (D1-D7).
2. COMMANDS.md (marker semantics your configs must satisfy).
3. SPEC-006 logging/lint rules; ADR-0005/0009.

## Files to Change (Expected Changed Files)
`Cargo.toml`, `rustfmt.toml`, `clippy.toml`, `pnpm-workspace.yaml`, `package.json`, `tsconfig.base.json`, `.eslintrc.cjs`, `.prettierrc`, `pyproject.toml`, `.editorconfig`, `CHANGELOG.md`, `infra/dev/.env.example`, `.github/workflows/verify.yml`, `.github/workflows/nightly.yml`, directory `.gitkeep`s (client/ server/ connectors/venues/ connectors/execution/ connectors/comms/ crates/ packages/ pylib/ proto/ infra/migrations/ infra/clickhouse/ infra/deploy/ vault/), this file.

## Interfaces and Contracts
Workspace member globs (the contract EP-002+ relies on): cargo members `["crates/*", "connectors/execution/*", "connectors/venues/*", "client/src-tauri"]` (empty-glob tolerant via `exclude` until members exist - use explicit empty `members = []` now and each plan appends; record this in the Decision Log). pnpm packages: `["packages/*", "client"]`. uv workspace members: `["pylib", "server/*"]` (same append pattern).

## Milestones
1. **Rust root.** Done when: `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass. Workspace lints: `unused_must_use = "deny"` under `[workspace.lints.rust]`.
2. **TS root.** Done when: `pnpm install` creates the lockfile and `pnpm -r --if-present run lint` passes (no packages yet - passes trivially but proves wiring). eslint config includes `no-empty` (catch blocks) as error.
3. **Python root.** Done when: `uv sync` produces `uv.lock`; ruff config enables `E722` (bare except) and format; mypy strict-ish baseline (`disallow_untyped_defs = true` scoped to `pylib`, relaxed for `server` until code exists).
4. **Skeleton + hygiene.** Done when: all ARCHITECTURE.md directories exist with `.gitkeep`; `.editorconfig` and CHANGELOG.md (Keep-a-Changelog header + Unreleased section) committed; `infra/dev/.env.example` mirrors every dev-defaultable ENVIRONMENT.md variable with dummy secrets.
5. **CI skeleton.** Done when: `verify.yml` (push/PR -> `scripts/verify.sh` + `scripts/security-check.sh`) and `nightly.yml` (cron -> integration + dependency-audit) are valid YAML (`python3 -c "import yaml,sys; yaml.safe_load(open('...'))"` or actionlint if present). Workflows contain NO logic beyond checkout, toolchain setup, and script calls.
6. **Full verification.** Done when: `scripts/verify.sh` prints `verify: ok` with ZERO `SKIP (marker absent)` lines for the three stack markers; `scripts/install.sh` prints `install: ok`.

## Concrete Steps
Per milestone: create the files above with minimal-but-real content; run the milestone's validation immediately. Version pins: rust edition 2021, `rust-version = "1.78"`; TS 5.x, `"type": "module"`; python `requires-python = ">=3.11"`. Dev-tool deps only: eslint+prettier+typescript at TS root; ruff+mypy+pytest in uv dev group. Commit per milestone (`chore(scaffold): ...`).

## Validation and Acceptance
Milestone validations above, then: `git diff --name-only` vs Expected Changed Files; `verify.sh` -> `verify: ok`; `security-check.sh` -> `security: ok`; lockfiles committed (ADR-0005): `git ls-files | grep -E 'Cargo.lock|pnpm-lock.yaml|uv.lock'` shows all three (note: `Cargo.lock` appears once a member with deps exists - if absent at this stage, record in Decision Log and re-verify at EP-002).

## Idempotence and Recovery
All file creations check-first; re-running `uv sync`/`pnpm install` is safe. If a config fights a tool version, prefer pinning the tool in the config over loosening the rule; third same-root failure on any linter -> R10 step 3 (simpler config, never rule deletion for SPEC-006-mandated rules - that would be weakening, R12).

## Progress
- [ ] M1 Rust root
- [ ] M2 TS root
- [ ] M3 Python root
- [ ] M4 Skeleton + hygiene
- [ ] M5 CI skeleton
- [ ] M6 Full verification

## Surprises & Discoveries
(tool version realities vs A-03/04/05 go here)

## Decision Log
(expected: workspace-member append pattern note; Cargo.lock timing note)

## Outcomes & Retrospective
(record: verify output, lockfile hashes, any config deviations)

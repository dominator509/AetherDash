Layer: 5 - Execution

# EP-001: Monorepo Scaffold, Toolchains, CI Skeleton

**Band:** 0xx Foundation | **Phase:** 0 | **Status:** done | **Blocked by:** EP-000

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
Workspace members (the contract EP-002+ appends to):
- **Cargo:** `members = ["crates/aether-core"]` — EP-001 delivers one scaffold crate. EP-002+ appends: `"crates/*"`, `"connectors/execution/*"`, `"connectors/venues/*"`, `"connectors/comms/*"`, `"client/src-tauri"`.
- **pnpm:** `packages: ["packages/*", "client"]` — no packages exist yet; `pnpm -r --if-present` handles empty gracefully.
- **uv:** `members = []` — no Python packages exist yet. Future: `"pylib"`, `"server/*"`.

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
Milestone validations above, then: `git diff --name-only` vs Expected Changed Files; `verify.sh` -> `verify: ok`; `security-check.sh` -> `security: ok`; lockfiles committed (ADR-0005): `git ls-files | grep -E 'Cargo.lock|pnpm-lock.yaml|uv.lock'` shows all three.

## Idempotence and Recovery
All file creations check-first; re-running `uv sync`/`pnpm install` is safe. If a config fights a tool version, prefer pinning the tool in the config over loosening the rule; third same-root failure on any linter -> R10 step 3 (simpler config, never rule deletion for SPEC-006-mandated rules - that would be weakening, R12).

## Progress
- [x] M1 Rust root
- [x] M2 TS root
- [x] M3 Python root
- [x] M4 Skeleton + hygiene
- [x] M5 CI skeleton
- [x] M6 Full verification

## Surprises & Discoveries
- **Windows Python: `python` not `python3`** — fixed in `preflight.sh` (EP-000 DL-000-1). Scripts use `python3` with `python` fallback.
- **uv workspace with empty members:** fails when member dirs lack `pyproject.toml`. Cargo workspace with empty members: fails with "no targets". Solution: cargo gets `crates/aether-core` scaffold now; uv and pnpm use empty members/`--if-present` which handle the empty case gracefully.
- **mypy on empty directories:** fails when dir exists but has no `.py` files. `typecheck.sh` checks for `.py` files first.
- **eslint config format:** Eslint 9.x uses flat config (`eslint.config.js`), not `.eslintrc.cjs`.
- **ruff version:** ruff 0.15.x schema differs — `[tool.ruff.format]` uses `docstring-code-line-length` not `line-length`.
- **rustfmt unstable features:** `imports_granularity`, `group_imports`, `format_code_in_doc_comments`, `format_macro_matchers`, `format_strings` are nightly-only. Commented out until nightly channel adopted.
- **Tool versions all exceed minimums:** rustc 1.96.1, node 24.14.1, pnpm 9.15.0, Python 3.14.4, uv 0.11.25, Docker Compose v5.1.4.
- **Optional tools not installed (not blocking):** cargo-nextest, cargo-audit, buf, gitleaks. CI installs cargo-audit via `taiki-e/install-action`.

## Decision Log
- **DL-001-1**: Cargo workspace starts with `members = ["crates/aether-core"]` — one scaffold crate so all cargo commands exercise real targets from day one. EP-002+ appends members. pnpm and uv start with empty members (`--if-present` handles the empty case).
- **DL-001-2**: Scripts restored to original strict behavior — no `|| true` error-swallowing. The `aether-core` crate provides real targets, so all cargo commands pass genuinely. Empty-workspace handling in scripts is unnecessary now and was removed (audit fix).
- **DL-001-3**: ESLint config uses flat config format (`eslint.config.js`) — eslint 9.x default.
- **DL-001-4**: `Cargo.lock` committed with real crate entry (aether-core). `pnpm-lock.yaml` and `uv.lock` also committed. ADR-0005 satisfied.
- **DL-001-5**: Scripts copied from `aether-blueprint/scripts/` to `scripts/` at repo root per ARCHITECTURE.md section 1.
- **DL-001-6** (audit round 1): Sol [P1] — scripts used `|| true` to swallow cargo errors, a gate-integrity defect. Fixed by creating `crates/aether-core/` as a minimal crate and restoring script integrity.
- **DL-001-7** (audit round 2): Workspace lint policy — `aether-core/Cargo.toml` now has `[lints] workspace = true` to inherit `unused_must_use = "deny"` and other workspace lints.
- **DL-001-8** (audit round 2): Nightly CI now installs `cargo-audit` via `taiki-e/install-action` before calling `dependency-audit.sh`.
- **DL-001-9** (audit round 2): Live-execution flag `AETHER_EXECUTION__LIVE_ENABLED` removed from `.env.example` per ADR-0007 — operator-configured out-of-band only.
- **DL-001-10** (audit round 2): `production-readiness-check.sh` fixed to look for `aether-blueprint/PRODUCTION_READINESS.md` (actual location).
- **DL-001-11** (audit round 2): Obsidian `workspace.json` reset to minimal state and added to `.gitignore` to prevent build-artifact leakage.

## Outcomes & Retrospective
- **verify.sh**: `verify: ok` — cargo fmt/clippy/test/build all pass against real `aether-core` crate
- **pnpm lint/format**: passes trivially (no packages under `packages/*` or `client/`; `--if-present` handles this) — will become meaningful when EP-101 adds the first TS package
- **cargo fmt --all --check**: passes (no rustfmt warnings)
- **cargo clippy --workspace --all-targets -- -D warnings**: passes — `Checking aether-core... Finished`
- **cargo test --workspace**: passes — `running 0 tests... result: ok`
- **cargo build --workspace**: passes — `Finished dev profile`
- **install.sh**: `install: ok`
- **security-check.sh**: `security: ok`
- **Lockfiles**: `Cargo.lock` (real crate), `pnpm-lock.yaml`, `uv.lock` — all committed
- **CI workflows**: `verify.yml` + `nightly.yml` valid YAML; nightly installs cargo-audit; both thin (checkout + tools + scripts only)
- **Gate integrity**: zero `|| true` error-swallowing; any Rust compilation/clippy/test failure WILL fail the gate
- **Committed**: ~55 files (workspace roots, SDK configs, 18 dir skeletons, CI, scripts, `.env.example` with no live flag)

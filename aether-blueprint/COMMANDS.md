Layer: 6 - Verification & Operations

# COMMANDS.md - The Only Legal Commands

Agents must use commands from this file. Do not guess commands. If a command here is stale, fix this file from repository evidence first, then run it.

**Greenfield gating:** the repo starts empty. Each stack's commands become ACTIVE once its marker file exists at repo root: `Cargo.toml` (Rust workspace), `pnpm-workspace.yaml` (TS), `pyproject.toml` (Python), `infra/dev/docker-compose.yml` (dev stack). All `scripts/*.sh` wrappers detect markers and skip missing stacks with an explicit `SKIP (marker absent): ...` notice, so `verify.sh` is meaningful from day zero and tightens as EP-001+ land. A SKIP is not a failure; a FAIL is.

## Preflight
```
scripts/preflight.sh
```
Checks toolchain presence/versions (rustc >= 1.78, cargo, node >= 20, pnpm >= 9, python3 >= 3.11, uv, docker compose, git). Prints `preflight: ok` on success. Optional tools (cargo-nextest, cargo-audit, buf, gitleaks) are reported but not fatal.

## Install
```
scripts/install.sh
```
Runs, per active stack: `cargo fetch` | `pnpm install --frozen-lockfile` | `uv sync`. Prints `install: ok`.

## Lint
```
scripts/lint.sh
```
- Rust: `cargo clippy --workspace --all-targets -- -D warnings`
- TS: `pnpm -r lint` (eslint; each package defines script `lint`)
- Python: `uv run ruff check .`
Prints `lint: ok`.

## Format check
```
scripts/format-check.sh
```
- Rust: `cargo fmt --all --check`
- TS: `pnpm -r format:check` (prettier `--check`)
- Python: `uv run ruff format --check .`
Prints `format: ok`. Auto-fix variants (never in CI): `cargo fmt --all`, `pnpm -r format`, `uv run ruff format .`

## Typecheck / static analysis
```
scripts/typecheck.sh
```
- TS: `pnpm -r typecheck` (tsc `--noEmit`)
- Python: `uv run mypy server pylib` (adjust paths as packages land)
- Rust type checking is inherent to `cargo clippy/build`.
Prints `typecheck: ok`.

## Unit tests
```
scripts/test-unit.sh
```
- Rust: `cargo nextest run --workspace` if nextest installed, else `cargo test --workspace`
- TS: `pnpm -r test -- --run` (vitest)
- Python: `uv run pytest -m "not integration and not e2e" -q`
Prints `unit: ok`.

## Integration tests
```
scripts/test-integration.sh
```
Brings up the dev stack (`docker compose -f infra/dev/docker-compose.yml up -d --wait`), then:
- Rust: `cargo test --workspace -- --ignored` (integration tests are `#[ignore]` by convention)
- Python: `uv run pytest -m integration -q`
Prints `integration: ok`. Requires Docker (A-06); missing Docker is `MISSING TOOL`, exit 2.

## E2E tests
```
scripts/test-e2e.sh
```
- `pnpm --filter @aether/client e2e` (Playwright). ACTIVE after EP-101.
Prints `e2e: ok`.

## Build
```
scripts/build.sh
```
- Rust: `cargo build --workspace`
- TS: `pnpm -r build`
- Python: `uv run python -m compileall -q server pylib`
Prints `build: ok`. Release: `cargo build --workspace --release`; client bundle: `pnpm --filter @aether/client tauri build` (ACTIVE after EP-101).

## Proto generation
```
scripts/proto-gen.sh   # proto codegen (Rust: automatic via build.rs; TS/Python: hand-mirrored per D7)
```

## Security check
```
scripts/security-check.sh
```
- Secret scan: `gitleaks detect --no-banner` if installed, else builtin pattern grep over tracked files (fails on hits either way).
- Forbidden-path guard: fails if `.env` (non-example), `*.pem`, `*.key`, `id_*` are tracked.
- Import-boundary grep: fails if execution-plane crates reference LLM/MCP modules (see ARCHITECTURE.md rule D3).
Prints `security: ok`.

## Dependency audit
```
scripts/dependency-audit.sh
```
- Rust: `cargo audit` (MISSING TOOL exit 2 if absent: `cargo install cargo-audit`)
- TS: `pnpm audit --prod --audit-level high`
- Python: `uv run pip-audit` if available, else explicit SKIP notice.
Prints `audit: ok`.

## Smoke test
```
scripts/smoke-test.sh
```
Verifies the dev stack answers: `pg_isready`, ClickHouse `SELECT 1`, Redis `PING`, Qdrant `/readyz`, Redpanda `rpk cluster health` (inside container), MinIO `/minio/health/live`; after services exist, curls each service `/healthz` listed in OBSERVABILITY.md. Prints `smoke: ok`.

## Full verification (the gate)
```
scripts/verify.sh
```
Chains: preflight -> format-check -> lint -> typecheck -> unit -> build. Prints `verify: ok` on success. This is the command referenced by "Definition of done".

## Local start
```
docker compose -f infra/dev/docker-compose.yml up -d --wait   # data stack (ACTIVE after EP-003)
cargo run -p aether-gateway                                    # WS gateway     (ACTIVE after EP-004)
uv run uvicorn server.brain.app:app --reload                   # brain API      (ACTIVE after EP-201)
pnpm --filter @aether/client tauri dev                         # desktop client (ACTIVE after EP-101)
```

## Database setup and migrations (ACTIVE after EP-003)
```
export DATABASE_URL=postgres://aether:aether@localhost:5432/aether   # dev only; real values via ENVIRONMENT.md
cargo sqlx migrate run --source infra/migrations        # the ONLY schema authority (ADR-0004)
cargo sqlx prepare --workspace                          # refresh offline query data after schema changes
bash infra/clickhouse/apply.sh                          # ClickHouse DDL (idempotent, ordered *.sql)
```
Down-migrations: `cargo sqlx migrate revert --source infra/migrations` (one step). Reversibility rule: AGENTS.md section 13.

## Production readiness check
```
scripts/production-readiness-check.sh
```
Runs verify + integration + e2e + security-check + dependency-audit + smoke, then asserts PRODUCTION_READINESS.md checklist file parses with zero unchecked required items. Prints `production readiness: ok`.

## Placeholders requiring replacement before lower-tier execution
- `DATABASE_URL` and all endpoints in ENVIRONMENT.md are dev defaults; production values are operator-provided (STOP S1 if needed and absent).
- `uv run mypy server pylib` paths must be extended as new Python packages land.
- Playwright/e2e filter name `@aether/client` must match the actual package name created in EP-101.

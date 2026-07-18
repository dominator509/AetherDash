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

## WalletConnect live relay/session proof (EP-306, HUMAN/operator supplied)
```
scripts/walletconnect-live-readiness.sh
scripts/walletconnect-live-evidence-check.sh data/walletconnect-live-evidence.json
```
Uses the official WalletConnect Sign Client to connect to the configured relay, writes a uniquely topic-stamped scannable QR under gitignored `data/`, renders a terminal QR without printing the secret-bearing raw pairing URI, waits for a session granting the configured chain/account, invokes the Rust Guardian to policy-evaluate and assemble the exact transaction, sends that request over the approved session, and writes evidence only after the wallet returns a transaction hash. Missing env is `MISSING`/exit 2; rejection or relay/session/request failure is non-zero and produces no successful evidence.

`walletconnect-live-evidence-check.sh` validates the generated evidence file after wallet approval. The default path is gitignored `data/walletconnect-live-evidence.json`; override it with `AETHER_GUARDIAN__WC_EVIDENCE_PATH`. M6 is not complete until this checker passes on evidence from the real client.

## E2E tests
```
scripts/test-e2e.sh
```
- `pnpm --filter @aether/client e2e` (Playwright). ACTIVE after EP-101.
Prints `e2e: ok`.

## EP-307 24-hour paper-run evidence (HUMAN/operator run)
```
cargo run -p aether-scanner --bin ep307-evidence
```
This read-only verifier examines the preceding 24 hours in `DATABASE_URL`. It passes only when activity spans all 24 hourly buckets, every opportunity is closed with attribution, and at least one chain followed the paper-execution path. A green accelerated replay test is implementation evidence, not a substitute for this wall-clock acceptance artifact.

Production scanner wiring uses `cargo run -p aether-scanner` (or `/usr/local/bin/aether-scanner`) with `DATABASE_URL`, `AETHER_KAFKA_BOOTSTRAP`, explicit `AETHER_SCANNER_TICK_TOPICS` and `AETHER_SCANNER_BOOK_TOPICS`, and optional `AETHER_SCANNER_METRICS_BIND` (default `127.0.0.1:9107`). The gateway consumes `opps.detected` and appends `scored -> surfaced` only when `AETHER_GATEWAY_BUS_ENABLED=1`.

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
uv sync --all-packages                                        # all Python workspace members, including ingestion OCR
cargo run -p aether-gateway                                    # WS gateway     (ACTIVE after EP-004)
uv run uvicorn server.brain.app:app --reload                   # brain API      (ACTIVE after EP-201)
uv run uvicorn server.llm_router.app:app --host 127.0.0.1 --port 8001  # LLM router (ACTIVE after EP-202)
uv run uvicorn server.alerts.app:app --host 127.0.0.1 --port 8002      # Alerts (ACTIVE after EP-203)
uv run uvicorn server.actions.app:app --host 127.0.0.1 --port 8004     # Authoritative alert effects (ACTIVE after EP-203)
set AETHER_INGEST__CONFIG_PATH=aether-blueprint\examples\ingest-sources.example.json
uv run uvicorn server.ingest.app:app --host 127.0.0.1 --port 8005      # Ingestion fleet (ACTIVE after EP-206)
uv run python -m connectors.venues.openbb.src.server                  # OpenBB venue (ACTIVE after EP-303)
cargo run -p aether-venue-hyperliquid                                # Hyperliquid venue (ACTIVE after EP-303)
cargo run -p aether-venue-alpaca                                     # Alpaca paper venue (ACTIVE after EP-303)
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
Runs verify.sh -> integration tests -> e2e tests -> security-check.sh -> dependency-audit.sh -> smoke-test.sh -> health-check.sh, then asserts PRODUCTION_READINESS.md checklist file parses with zero unchecked REQUIRED items. Prints `production readiness: ok`.

## Health check (EP-408)
```
scripts/health-check.sh
```
Curls /healthz and /readyz on every service defined in ENVIRONMENT.md: infrastructure (Postgres pg_isready, ClickHouse ping, Redis PING, Qdrant readyz, MinIO health, Redpanda admin), app services (Gateway :8080, Brain :8000, LLM Router :8001, Alerts :8002, Inbox :8003, Actions :8004), gRPC services via grpc-health-probe (Order Router :50051, Risk Engine :50052, Wallet Guardian :50053), and venue adapters (Kalshi :8084, Polymarket :8085, Hyperliquid :8086, Alpaca :8087, OpenBB :8088). Prints `health: ok`. Services not running are `SKIP` (non-fatal unless `FORCE_FAIL=1`).

## Ingestion fleet audit (EP-206)
```
uv run pytest server/ingest/tests -q
curl -fsS http://127.0.0.1:8005/healthz
curl -fsS http://127.0.0.1:8005/readyz
curl -fsS http://127.0.0.1:8005/metrics
curl -fsS http://127.0.0.1:8005/audit/sources
```
The source audit returns durable object/rung events and explicit downgrade decisions. It never returns source credentials. Integration tests additionally require `AETHER_INTEGRATION_TEST=1` and a disposable `DATABASE_URL` with migrations applied.

## Brain recall-v2 benchmark (EP-207)
```
uv run python -m server.brain.benchmark testdata/brain-bench/graded --ranker v1 --k 5
uv run python -m server.brain.benchmark testdata/brain-bench/graded --ranker graph --k 5
uv run python -m server.brain.benchmark testdata/brain-bench/graded --ranker weighted --k 5
uv run python -m server.brain.benchmark testdata/brain-bench/graded --ranker rerank --k 5
uv run pytest server/brain/tests/test_benchmark.py server/brain/tests/test_recall_v2.py -q
```
The reported micro-latency covers ranking stages only. The required end-to-end 100 ms p95 gate remains `server/brain/tests/test_recall.py::test_recall_p95_budget` against migrated Postgres, Qdrant, and Kuzu. Optional reranking is local-only, bounded to 25 ms, and disabled by default.

## Database backup / restore (EP-408, HUMAN supervised)
```
scripts/backup.sh                          # nightly backup (Postgres, ClickHouse, Kuzu, Qdrant)
scripts/restore.sh                         # dry-run list
scripts/restore.sh --confirm pg <file>     # HUMAN: restore Postgres
scripts/restore.sh --confirm ch <file>     # HUMAN: restore ClickHouse
```
`backup.sh` stores to `BACKUP_DIR` (default `data/backups/`) with per-service retention: Postgres 30d, ClickHouse 14d, Kuzu 14d, Qdrant 7d. `restore.sh` requires `--confirm` (S6 guard) and never runs unattended.

## Placeholders requiring replacement before lower-tier execution
- `DATABASE_URL` and all endpoints in ENVIRONMENT.md are dev defaults; production values are operator-provided (STOP S1 if needed and absent).
- `uv run mypy server pylib` paths must be extended as new Python packages land.
- Playwright/e2e filter name `@aether/client` must match the actual package name created in EP-101.

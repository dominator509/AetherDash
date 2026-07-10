Layer: 6 - Verification & Operations

# DEPLOYMENT.md - Targets and Procedure

Deploy tooling is built by EP-407; this file is the contract it implements. Until EP-407, deployment is manual per "First deploy" below. Production deploys are STOP S6 territory for agents: humans run them.

## Topology
| Host | Runs | Notes |
|---|---|---|
| Operator desktop | Tauri client | self-installed bundle |
| Brain host (VPS or homelab, 24/7) | databases (compose), server plane + connector plane services (systemd) | single host in v1; WireGuard for all admin access |
| GPU host (optional, Phase 3) | OCR/embedding workers | RunPod or self-hosted (A-15); reaches brain host over WireGuard |

## Release layout on the brain host
```text
/opt/aether/
  releases/<version>/        # immutable: binaries, server/ source, uv venv, migrations snapshot
  current -> releases/<v>    # atomic symlink flip
  shared/
    env/                     # EnvironmentFiles + credentials (never in releases/)
    data/kuzu/               # embedded graph
    testdata-cache/
  compose/compose.prod.yml   # database stack (postgres, clickhouse, qdrant, redis, redpanda, minio)
```
Keep the 3 most recent releases (ROLLBACK.md depends on this). Rust services ship as release binaries; Python services run from the release's uv-synced venv via uvicorn.

## Units and processes
One systemd unit per service, named `aether-<service>.service` (gateway, brain, llm-router, alerts, inbox, order-router, risk-engine, wallet-guardian, per-venue adapters `aether-venue-<name>`). Units read `EnvironmentFile=/opt/aether/shared/env/<service>.env` and, for secrets, `LoadCredential=`. `wallet-guardian` additionally: `ProtectSystem=strict`, private tmp, its keystore dir is the only writable path, and no other unit's user can read it (SECURITY.md T4).

## Deploy procedure (EP-407 automates; human-run until then)
1. On CI green (`verify.sh` + integration): build artifacts - `cargo build --workspace --release`, `pnpm -r build`, snapshot `infra/migrations`.
2. rsync artifact set to `releases/<version>` on the brain host over WireGuard.
3. `uv sync` inside the release dir; run `scripts/preflight.sh`.
4. Apply migrations: `cargo sqlx migrate run --source infra/migrations` (paired-down rule, AGENTS.md 13). Databases stay up; app services stop first only if the migration notes demand it.
5. Flip `current` symlink; `systemctl restart 'aether-*'` in dependency order (databases assumed up; guardian and risk before router; router before gateway).
6. Post-deploy gate: `scripts/smoke-test.sh` on the host, audit-chain verify (RELEASE.md), then watch OBSERVABILITY.md golden signals for 15 minutes.
7. Record the deploy in the ops log (OPERATIONS.md).

## Client deployment
`pnpm --filter @aether/client tauri build` produces per-OS bundles. v1 distribution is manual install by the operator; bundles are archived alongside server releases. Auto-update (tauri-updater, signed) is Phase 4 scope inside EP-407.

## CI wiring (ADR-0009)
GitHub Actions first; workflows stay thin and only call `scripts/*.sh`:
- `verify.yml`: on push/PR -> `scripts/verify.sh` + `scripts/security-check.sh`.
- `nightly.yml`: schedule -> `scripts/test-integration.sh`, `scripts/dependency-audit.sh`.
Because logic lives in scripts, migrating to self-hosted Forgejo runners is a runner swap. Push policy to any remote is operator-confirmed first (A-17).

## Config management
All runtime config is env-file based per ENVIRONMENT.md; no config service. Changing prod config = editing files under `shared/env/` + targeted `systemctl restart`; every change gets an ops-log line. `AETHER_EXECUTION__LIVE_ENABLED` follows the ceremony in OPERATIONS.md, never a routine config edit.

## First deploy (bootstrap runbook)
1. Provision brain host (Ubuntu 24), create `aether` user, install docker + compose, WireGuard, rustup toolchain (for sqlx CLI), uv.
2. Lay down `/opt/aether` skeleton; write env files from ENVIRONMENT.md tables (secrets from the operator's password manager - never through an agent).
3. `docker compose -f compose/compose.prod.yml up -d --wait`; run migrations; `scripts/smoke-test.sh`.
4. Deploy release per procedure above; confirm gateway reachable from the desktop client over its URL; paper trade one fixture market end-to-end.

## Zero-downtime stance
v1 accepts brief restarts (single operator). The symlink-flip + ordered-restart pattern keeps the window to seconds. True rolling deploys are Phase 5 scope and out of v1.

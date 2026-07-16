Layer: 6 - Verification & Operations

# OPERATIONS.md - Running AETHER

Operator-facing procedures. Agents may reference these but never execute the sections marked HUMAN.

## Start / stop / status (brain host)
```
docker compose -f /opt/aether/compose/compose.prod.yml up -d --wait   # databases
systemctl start 'aether-*'        # app services (dependency order handled by unit Wants/After)
systemctl status 'aether-*'
systemctl stop 'aether-*'         # databases stay up unless maintenance requires otherwise
journalctl -u aether-<service> -f # logs (JSON in prod; jq-friendly)
```
Dev equivalents live in COMMANDS.md "Local start".

## Ops log
Append-only Markdown at `/opt/aether/shared/ops-log.md`: timestamp, actor (human name or agent+plan), action, result. Deploys, config changes, backups/restores, incidents, and ceremonies all get a line. The audit chain covers in-system events; the ops log covers operator actions around the system.

## Backups (nightly, systemd timers; built in EP-407, contract fixed here)
| Store | Method | Target | Retention |
|---|---|---|---|
| Postgres | `pg_dump -Fc aether` | MinIO `aether-backups/pg/<date>.dump` | 30 daily + 12 monthly |
| ClickHouse | `BACKUP DATABASE aether TO Disk('backups', '<date>')` then sync to MinIO `aether-backups/ch/` | same | 14 daily |
| Kuzu | flush via brain admin endpoint, then tar of `shared/data/kuzu` | MinIO `aether-backups/kuzu/` | 14 daily |
| MinIO (raw lake + backups bucket) | mirror to second disk or offsite via `mc mirror` | operator-chosen | continuous |
| Qdrant | snapshot API per collection | MinIO `aether-backups/qdrant/` | 7 daily |
| Redis / Redpanda | not backed up: cache and replayable streams respectively (Decision: acceptable v1 loss; revisit if bus becomes system-of-record for anything) | - | - |

**Restore drill (HUMAN, quarterly):** restore latest Postgres dump into a scratch database, run row-count sanity queries, record in ops log. A backup that has never been restored is not a backup.

## Scheduled jobs (systemd timers; definitions land with their owning plans)
- Nightly: backups (above); `dependency-audit.sh` report; Brain tiering/decay/archival pass (EP-201/207); vault view regeneration (INV-9).
- Hourly: audit-chain incremental verify (EP-402); staleness sweep marking expired Brain objects.
- Every 5 min: source-health checks for venue feeds (adapters export `feed_lag`; see OBSERVABILITY.md).

## Disk and data hygiene
ClickHouse ticks: 90 days full resolution, then downsampled (SPEC-002 retention). MinIO raw lake grows unbounded by design; monitor `minio_bucket_usage` and expand storage rather than delete raw provenance. Alert at 80% disk on any volume.

## First live trade ceremony (HUMAN; ADR-0007, HARD-DENY 3)
Preconditions: Phase 2 exit criteria met on paper; caps configured and verified in DB; audit chain verifying clean.
1. Announce in ops log: venue, market, max size (small), rationale.
2. Edit `shared/env/order-router.env`: `AETHER_EXECUTION__LIVE_ENABLED=true`; restart `aether-order-router`; confirm router logs the flag flip with an audit entry.
3. Step-up 2FA in the client; place ONE order at minimum viable size via the normal flow (not a bypass).
4. Verify: fill recorded, audit entry chained, P&L attribution row exists, alert confirmation received.
5. Decide: keep enabled (log it) or revert the flag (log it). Either way the ceremony record links the audit entry IDs.

## WalletConnect testnet proof ceremony (HUMAN; EP-306 M6)
Preconditions: WalletConnect project created by the operator, testnet-capable operator wallet ready, no mainnet funds involved, and the four proof variables in ENVIRONMENT.md supplied from an operator-controlled env file or shell. Agents may prepare the command and inspect non-secret output; the operator controls the wallet approval.
1. Set `AETHER_GUARDIAN__WC_PROJECT_ID`, `AETHER_GUARDIAN__WC_RELAY_URL`, `AETHER_GUARDIAN__WC_OPERATOR_ACCOUNT`, and `AETHER_GUARDIAN__WC_TESTNET_CHAIN_ID`.
2. Run `scripts/walletconnect-live-readiness.sh`.
3. Scan/open the emitted `pairing_uri` in the operator wallet on the configured testnet.
4. Verify the wallet shows the expected `eth_sendTransaction` request, destination, value, and chain id; reject if anything differs.
5. Approve externally in the wallet, then record in the ops log: command timestamp, chain id, pairing topic, request id, wallet approval/tx hash if produced, and confirmation that Guardian policy state was `auto_approved` or human-approved before the WC request was built.
6. Copy `aether-blueprint/examples/walletconnect-live-evidence.example.json`, fill it with the real non-secret evidence, and run `scripts/walletconnect-live-evidence-check.sh <evidence.json>`.
7. EP-306 M6 may be marked complete only when both sides are present: the command output plus an evidence JSON file that passes `walletconnect-live-evidence-check.sh`. A repo-side packet without wallet-side approval is readiness evidence, not completion.

## Incident basics
Severity: SEV1 = money or keys at risk (freeze first: stop `aether-order-router` and `aether-wallet-guardian`, then diagnose); SEV2 = trading path degraded (stale feeds, router rejects-all); SEV3 = everything else. Runbooks per `runbook-template.md` are required for: key exposure, venue credential leak, poisoned ingest (SECURITY.md), feed outage, database disk-full, audit-chain verify failure. Runbook set must exist before Phase 2 exit.

## Routine health review (HUMAN, weekly, ~10 min)
Golden-signal dashboard (OBSERVABILITY.md), alert precision trend, LLM cache-hit and cost per opportunity, backup timer status, disk headroom, dependency-audit nightly report, open `revise` entries in GENERATION-STATE/PLANS.

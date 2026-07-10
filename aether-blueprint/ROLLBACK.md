Layer: 6 - Verification & Operations

# ROLLBACK.md - Undoing a Bad Release

Rollbacks are HUMAN operations (STOP S6). Agents may prepare evidence, never execute. The release layout (DEPLOYMENT.md: 3 retained releases + `current` symlink) exists to make everything below boring.

## Decision criteria (roll back vs fix forward)
Roll back immediately when: the trading path is wrong (bad fills, router mis-verdicts), the audit chain fails verify post-deploy, Guardian behavior changed unexpectedly, or data is being written incorrectly. Fix forward when: the defect is isolated, non-financial, and a hotfix passes the full release gate faster than a rollback drill. When unsure with money involved: stop `aether-order-router` first (safe state = not trading), then decide.

## App services (binaries + Python venvs)
1. `systemctl stop 'aether-*'` (databases stay up).
2. Flip `current -> releases/<previous>`.
3. Start in dependency order (guardian, risk, router, then the rest; gateway last).
4. Verify: `scripts/smoke-test.sh`, `/readyz` on all services, audit incremental verify, one paper order end-to-end.
5. Ops-log entry linking the failed release, symptoms, and evidence.

## Database migrations (the risky part)
- Same-schema rollback (previous release runs on current schema): usually true because migrations are additive-biased. Verify by running the previous release's `cargo sqlx prepare` check against the live schema in a scratch shell before flipping.
- Schema must actually revert: `cargo sqlx migrate revert --source infra/migrations` one step at a time, oldest-needed last. Down-migrations that drop data are marked `-- DESTRUCTIVE` in-file (AGENTS.md 13 pairing rule); reverting past one means restore instead:
- **Restore path:** stop app services -> restore latest `pg_dump` per OPERATIONS.md into a scratch DB -> sanity queries -> swap databases -> restart previous release -> reconcile the gap window from ClickHouse/bus history and the audit chain (orders/fills are re-derivable from `orders.fills` events; document any true loss in the ops log).
- ClickHouse DDL is append-only by convention; rolling back means the previous release simply ignores new columns/tables. Dropping ClickHouse objects is never part of a rollback.

## Client
Reinstall the previous archived bundle (checksums in ops log). Client/server version skew tolerance: one MINOR apart maximum; outside that, roll the client to match the server, not vice versa (the server is the source of truth, INV-9).

## Config rollback
Env files under `shared/env/` are edited in place; keep it that way but snapshot the directory (`tar` to `aether-backups/env/<ts>.tar`) before any deploy or ceremony - EP-407 automates the snapshot. Restoring = untar + targeted restarts. `AETHER_EXECUTION__LIVE_ENABLED` reverts to `false` in ANY rollback involving router, risk, guardian, or a venue pack; re-enabling repeats the ceremony (OPERATIONS.md).

## Venue pack rollback
Packs are additive (INV-7): disabling a bad venue = stop `aether-venue-<name>` + mark the venue disabled in the registry table; no code rollback required if the pack merely misbehaves. Full rollback only when a pack change shipped inside the bad release.

## Post-rollback verification (all cases)
`smoke-test.sh` green; `aether_audit_chain_verified == 1`; feed lag normal for enabled venues; one paper order round-trips; lifecycle gauge returns to baseline; ops-log entry complete. Then schedule the fix-forward release through the normal RELEASE.md gate - a rollback is never the end state.

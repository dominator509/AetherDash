Layer: 5 - Execution

# EP-003: Data & Persistence Substrate

**Band:** 0xx Foundation | **Phase:** 0 | **Status:** active | **Blocked by:** EP-002 (completed)

## Purpose / Big Picture
Stand up SPEC-002 for real: the six-service dev compose stack on the ENVIRONMENT.md port contract, every Postgres table via sqlx migrations, ClickHouse DDL, and idempotent bootstraps for Qdrant/MinIO - so every later plan has truth stores to write to and `smoke-test.sh` stops skipping.

## Scope
`infra/dev/docker-compose.yml`; `infra/migrations/` (full SPEC-002 Postgres set, paired up/down); `infra/clickhouse/*.sql` + `apply.sh`; `infra/bootstrap/` (qdrant.sh, minio.sh); sqlx offline data; SPEC-002 required tests that don't need the bus.

## Non-goals
No prod compose (EP-407); no bus topics (EP-004); no Kuzu code (brain owns it, EP-201 - only the data dir convention lands here); no seed/business data beyond the `venues` table shape (packs seed themselves, SPEC-009).

## Context and Orientation
ENVIRONMENT.md port table is a contract: ClickHouse native remaps to 9004 because MinIO owns 9000; smoke-test.sh already assumes the compose service names `postgres clickhouse redis qdrant redpanda minio` and `--wait`-able healthchecks. ADR-0004: sqlx is the only DDL path for Postgres.

## Files to Read First
1. SPEC-002 (entire - it IS this plan's requirements).
2. ENVIRONMENT.md (ports, DATABASE_URL); scripts/smoke-test.sh (the consumer of your service names/healthchecks).
3. ADR-0002/0003/0004; SPEC-006 quarantine (informs `quarantine` deferral note).

## Files to Change (Expected Changed Files)
`infra/dev/docker-compose.yml`, `infra/migrations/*.sql` (~17 paired files), `infra/clickhouse/{001..00N}.sql`, `infra/clickhouse/apply.sh`, `infra/bootstrap/{qdrant.sh,minio.sh}`, `.sqlx/` offline data (via `cargo sqlx prepare` once a querying crate exists - see Decision note), `crates/aether-core/src/redis_keys.rs` (only if EP-002 stubbed it thin), COMMANDS.md (activate the migration section's ACTIVE markers), CHANGELOG, this file.

## Interfaces and Contracts
Compose service names + host ports EXACTLY per ENVIRONMENT.md; volumes named `aether_{pg,ch,qd,rp,minio}_data`; healthchecks: pg_isready / clickhouse `SELECT 1` / redis PING / qdrant `/readyz` / rpk cluster health / minio live - matching smoke-test probes. Migration numbering `NNNN_description.{up,down}.sql` sequential; every `down` real (DESTRUCTIVE ones marked per ROLLBACK.md). ClickHouse DDL idempotent (`CREATE ... IF NOT EXISTS`, ordered files, apply.sh replays all every run).

## Milestones
1. **Compose stack.** All six services with pinned image versions (Decision-Log each pin; ClickHouse 24.x per A-08-adjacent notes, Redpanda latest-stable pin), healthchecks, volumes, port map. Done when: `docker compose -f infra/dev/docker-compose.yml up -d --wait` exits 0 and `scripts/smoke-test.sh` prints `smoke: ok`.
2. **Postgres migrations - identity & market truth.** venues, markets, quotes_latest, users, sessions, permission_grants, caps. Done when: `cargo sqlx migrate run --source infra/migrations` clean on fresh DB; `revert` x N then re-run clean (pairing test).
3. **Postgres migrations - trading & lifecycle.** order_intents, orders, fills, positions, opportunities, opportunity_events, attribution. Done when: same pairing validation; FK graph verified by a psql `\d`-driven check script or query.
4. **Postgres migrations - brain & system.** brain_objects (incl. FTS generated column + GIN index), plugin_manifests, audit_anchor, ops_meta. Done when: pairing validation; FTS smoke query works.
5. **ClickHouse DDL.** md_ticks (TTL 90d + 1m rollup MV), md_book_snapshots (TTL 30d), opportunity_metrics, llm_calls (TTL 180d), ingest_events, audit_events + the three MVs. Done when: `bash infra/clickhouse/apply.sh` twice in a row both exit 0 (idempotence) and `SELECT` from every object works.
6. **Qdrant + MinIO bootstrap.** Collections brain_chunks/market_texts (dims from config env `AETHER_EMBED__DIMS`, default 1024, cosine); buckets aether-{raw,clean,artifacts,backups}. Done when: each script run twice -> identical end state, exit 0.
7. **SPEC-002 tests (bus-independent subset).** Migration pairing test wired as a Rust `#[ignore]` integration test (fresh ephemeral DB via compose); Redis-empty degradation test deferred to first consumer (Decision Log note); Qdrant reconstruction drill scripted (`infra/bootstrap/qdrant-rebuild-drill.sh` - drop, re-bootstrap, assert empty-but-correct schema; full rebuild-from-truth lands with EP-201 data). Done when: `scripts/test-integration.sh` green.

## Concrete Steps
Write compose first (M1 unblocks everything); then migrations in the three batches with the pairing loop after each batch: `run -> revert --target 0 -> run` (or stepwise reverts) asserting clean; column definitions follow SPEC-001 scalar rules (NUMERIC for decimals, TIMESTAMPTZ, TEXT ULIDs with CHECK length). `quarantine.*` note: the SPEC-002 quarantine-path test needs the bus - record explicit deferral to EP-004 acceptance in both plans' Decision Logs.

## Validation and Acceptance
`smoke-test.sh` -> `smoke: ok`; migration pairing green from zero and from head; apply.sh idempotent; bootstraps idempotent; `test-integration.sh` -> `integration: ok`; `verify.sh` -> `verify: ok`; `git diff --name-only` matches. Acceptance: SPEC-002 acceptance paragraph satisfied minus the explicitly-deferred bus-dependent items.

## Idempotence and Recovery
Everything here is rebuild-from-nothing by design: `docker compose down -v` then M1-M6 replay is the recovery for any corrupted dev state (document as the first runbook seed). Never fix a bad migration by editing an applied one - new migration forward, always (ADR-0004 discipline).

## Progress
- [x] M1 Compose  - [x] M2 Identity/market  - [x] M3 Trading  - [x] M4 Brain/system
- [x] M5 ClickHouse  - [x] M6 Bootstraps  - [x] M7 Tests

## Surprises & Discoveries
- Qdrant image (distroless) has no curl/wget — healthcheck uses `/proc/net/tcp` grep on port 6333 (hex 0x18BD) instead of the planned `/readyz` endpoint
- All 6 services healthy and smoke-test.sh passes
- Windows path mangling with `docker exec` — use CMD form for healthchecks
- alert_precision_daily is a plain SummingMergeTree table, not a materialized view — alert data comes from EP-203

## Decision Log
- Image pins: pgvector/pgvector:pg17, clickhouse/clickhouse-server:24.12-alpine, redis:7.4-alpine, qdrant/qdrant:v1.18.2, redpandadata/redpanda:v24.3.2, minio/minio:RELEASE.2024-12-13T22-19-12Z
- .sqlx offline data deferred: needs `cargo sqlx prepare` after first querying crate exists (EP-004+)
- Redis-empty degradation test deferred to first consumer
- Quarantine path test deferred to EP-004 acceptance (needs bus)
- alert_precision_daily is a placeholder SummingMergeTree table (not an MV) — proper MV reading from audit_events deferred to EP-203 alert engine

## Outcomes & Retrospective
- 18 Postgres tables (36 paired migrations) with FK graph, FTS on brain_objects
- 7 ClickHouse tables + 3 MVs with idempotent apply.sh
- 2 Qdrant collections, 4 MinIO buckets, both with idempotent bootstrap
- smoke-test.sh prints `smoke: ok` — all 6 services healthy
- Rust: 85 pass, 3 ignored (1 static pairing check + 2 DB-dependent integration)
- SPEC-002 acceptance satisfied minus explicitly-deferred bus-dependent items

Layer: 5 - Execution

# EP-003: Data & Persistence Substrate

**Band:** 0xx Foundation | **Phase:** 0 | **Status:** draft | **Blocked by:** EP-002

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
- [ ] M1 Compose  - [ ] M2 Identity/market  - [ ] M3 Trading  - [ ] M4 Brain/system
- [ ] M5 ClickHouse  - [ ] M6 Bootstraps  - [ ] M7 Tests

## Surprises & Discoveries
(image-pin realities, healthcheck quirks, sqlx NUMERIC mapping notes)

## Decision Log
(image pins; .sqlx timing; deferrals to EP-004/EP-201)

## Outcomes & Retrospective
(migration count, object inventory vs SPEC-002, smoke output)

Layer: 4 - Specification

# SPEC-002: Data Model

**Status:** accepted | **Owning plans:** EP-003 (primary), EP-201/402 extend | **Last updated:** 2026-07-07

## User-visible goal
Databases are the source of truth (INV-9); this spec fixes which truth lives where, under what names, so no store is invented ad hoc (R9).

## Non-goals
Full DDL (EP-003 writes migrations from this); Brain retrieval algorithms (SPEC-011); vault view templates (EP-201).

## Store roles (one sentence each, binding)
Postgres = relational truth and anything transactional. ClickHouse = append-heavy time series and analytics. Qdrant = vectors only (payload carries refs, never truth). Kuzu = the event knowledge graph. MinIO = immutable raw/clean artifact lake + backups. Redis = cache and ephemera; losing it must never lose truth.

## Postgres (database `aether`; sqlx migrations are the sole DDL path, ADR-0004)
Tables (columns abbreviated to the load-bearing ones; all have `id` ULID pk unless noted, `created_ts/updated_ts`):
- `venues` - `slug pk, display_name, capabilities jsonb, jurisdictions jsonb, enabled bool, pack_version`.
- `markets` - `key text pk (MarketKey), venue fk, kind, title, status, close_ts, resolve_ts, outcome, venue_ref jsonb, meta jsonb`. Index: `(venue, status)`, `(status, close_ts)`.
- `quotes_latest` - `market_key pk fk, bid, ask, mid, last, ts, seq` - upsert-only hot row per market (history lives in ClickHouse).
- `order_intents` - full `OrderIntent` shape + `verdict, verdict_reasons jsonb, state: pending|denied|routed|failed`.
- `orders` - `intent_id fk, venue_ref jsonb, state: open|partial|filled|cancelled|rejected, paper bool`.
- `fills` - `order_id fk, price, size, fee_amount, fee_currency, venue_ref jsonb, ts, paper`.
- `positions` - `market_key pk, side_exposure, avg_price, size, realized_pnl, ts` (paper and live segregated by `paper bool` in pk).
- `caps` - `version pk serial, body jsonb (CapsSnapshot), active bool` - append-only; `active` moves forward only via the SPEC-005 flow.
- `users`, `sessions`, `permission_grants` - `grants: (actor_id, actor_kind: human|agent|automation, tier 1..5, scopes jsonb, expires_ts)`.
- `opportunities` - `Opportunity` shape flattened + `state: detected|scored|surfaced|accepted|ignored|expired|executed|closed`.
- `opportunity_events` - `opportunity_id fk, from_state, to_state, actor, ts, detail jsonb` - the lifecycle chain TESTING.md asserts over.
- `attribution` - `opportunity_id fk, predicted_net_edge, realized_pnl, outcome, closed_ts, reason_ignored?`.
- `brain_objects` - index row per object: `id, kind, source, origin, trust, provenance_hash, minio_raw_ref, minio_clean_ref, summary, staleness_rule, expires_ts, tier: hot|warm|cold` (bodies/embeddings live elsewhere; SPEC-011).
- `plugin_manifests` - `name, version, capabilities jsonb, signature, signer, status: approved|revoked` (Phase 4).
- `audit_anchor` - `seq bigint pk, hash, anchored_ts` - periodic anchors of the ClickHouse audit stream for O(1) verify starts (EP-402).
- `ops_meta` - schema version, backup markers, job leases.

## ClickHouse (database `aether`; ordered idempotent DDL under `infra/clickhouse/`)
- `md_ticks` - `(venue, market_key, ts DateTime64(6), bid, ask, mid, last, bid_size, ask_size, seq)` ENGINE MergeTree ORDER BY `(venue, market_key, ts)` TTL 90 days -> `md_ticks_1m` rollup (avg/ohlc) kept indefinitely.
- `md_book_snapshots` - top-N levels jsonb-ish `String` + `ts`, TTL 30 days.
- `opportunity_metrics` - one row per opportunity state change (mirrors `opportunity_events`) for analytics.
- `llm_calls` - `(ts, provider, model, purpose, prompt_tokens, completion_tokens, cached_tokens, cost_usd Decimal, cache_hit UInt8, latency_ms, trace_id)` TTL 180 days - source of OBSERVABILITY.md cost metrics.
- `ingest_events` - `(ts, source, ladder_rung, object_id, bytes, status)` - INV-4 audit surface.
- `audit_events` - the full hash chain `(seq, prev_hash, hash, ts, actor, action, subject, payload_hash)` ENGINE MergeTree ORDER BY seq; Postgres `audit_anchor` checkpoints it.
Materialized views: `md_ticks_1m`, `llm_cost_daily`, `alert_precision_daily`.

## Qdrant (collections; vectors 1024-d default, cosine; named in EP-003, dims finalized by embedding choice in EP-206 - config-driven, not hardcoded)
- `brain_chunks` - payload `{object_id, provenance_hash, kind, ts, trust, market_keys[]}`.
- `market_texts` - titles/descriptions for semantic market search; payload `{market_key, venue}`.
Rule: Qdrant payloads carry refs and filter fields only; deleting Qdrant and re-embedding from MinIO/Postgres MUST reconstruct it fully.

## Kuzu (embedded at `AETHER_KUZU__PATH`; brain service is the single writer)
Nodes: `Event {id, ts, kind, summary, confidence}`, `Entity {id, name, kind}`, `Market {key}`, `Outcome {id, resolved_ts, value}`, `Source {id, domain, reliability}`.
Edges: `MENTIONS (Event->Entity)`, `AFFECTS (Event->Market, weight, direction)`, `RESOLVES_TO (Market->Outcome)`, `DERIVED_FROM (Event->Source)`, `RELATES (Entity->Entity, kind)`.
Export law (ADR-0003): a nightly job can serialize the full graph to MinIO as newline-JSON so migration off Kuzu is a data move.

## MinIO (buckets)
- `aether-raw` - immutable originals: `raw/{source}/{yyyy}/{mm}/{dd}/{sha256}`.
- `aether-clean` - normalized/extracted text keyed by the same hash.
- `aether-artifacts` - vault exports, graph exports, replay recordings, reports.
- `aether-backups` - per OPERATIONS.md layout.
Rule: objects are write-once; corrections write new objects with provenance links, never overwrite (INV-9).

## Redis (key prefixes; all keys carry TTLs)
`q:quote:{market_key}` hot quote mirror (ms TTL semantics via short expiry); `cache:llm:{prompt_hash}` response cache; `cache:sem:{embedding_hash}` semantic cache (Phase 3); `sess:{session_id}`; `rl:{scope}` rate-limit counters; `lease:{job}` scheduler leases. Rule: any consumer must function (degraded latency acceptable) with Redis empty.

## Retention & tiering summary
Ticks 90d full / 1m-bars forever; books 30d; llm_calls 180d; raw lake forever; Brain objects tier hot->warm->cold by decay job (SPEC-011); audit chain forever (it is small and it is the point).

## Required tests
Migration up/down pairing test (fresh DB -> head -> revert-all -> head); quarantine path (bad payload never reaches `md_ticks`); Qdrant reconstruction drill (drop collection, rebuild from truth, count+spot-check); Kuzu export/import round-trip; Redis-empty degradation test on quote reads.

## Acceptance criteria
EP-003 milestones validate: compose stack up, all Postgres tables + ClickHouse objects created via the official paths, smoke-test green, and the tests above pass in `test-integration.sh`.

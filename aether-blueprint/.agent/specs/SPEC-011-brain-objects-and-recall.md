Layer: 4 - Specification

# SPEC-011: Brain Objects and Recall

**Status:** accepted | **Owning plans:** EP-201 (v1), EP-206/207 (fleet + recall v2) | **Last updated:** 2026-07-09

## User-visible goal
Everything AETHER knows is an object with provenance you can click through to, recall is fast enough to sit on the opportunity path, and the Obsidian vault is a faithful generated window into it (INV-9).

## Non-goals
Ingestion source connectors themselves (EP-204 inbox, EP-206 fleet); embedding-model choice (config, decided in EP-206 with an ADR); agent reasoning over recall results.

## Terms
**Object** = the unit of Brain knowledge (index row in `brain_objects`, artifacts in MinIO, vectors in Qdrant, graph presence in Kuzu). **Draft** = pre-pipeline input via `Brain.Store`. **Recall** = `Brain.Recall` (SPEC-003). **Tier** = hot | warm | cold storage/ranking class (distinct from permission tiers).

## Object kinds (closed set, v1)
`document | email | filing | news | note | market_description | event | screenshot | report | transcript`. New kinds amend this spec (R13).

## Required fields (every object; ARCHITECTURE.md section 8 made concrete)
`id (Ulid)`, `kind`, `source` (feed/domain/inbox-address), `origin` (`ingest_fleet | inbox | operator | system`), `trust` (`low | medium | high` - inbox starts low, SECURITY.md), `author_or_publisher?`, `published_ts?`, `ingested_ts`, `url_or_ref?`, `raw_ref` (MinIO `aether-raw` sha256 key), `clean_ref` (`aether-clean`), `provenance_hash` (sha256 over canonical JSON of {source, raw sha256, ingested_ts} - SPEC-001 canonical bytes), `summary` (<= 500 chars, plain language), `entities []`, `linked_events []`, `market_keys []`, `confidence (0..1)`, `staleness_rule`, `expires_ts?`, `tier`.

## Ingestion pipeline (stages; each idempotent, resumable, emitting `ingest_events` rows - INV-4 rung recorded at intake)
`intake` (raw -> MinIO, dedupe by content hash - a seen hash short-circuits to linking) -> `clean` (extract text; OCR path for screenshots/PDF-scans lands EP-206) -> `summarize` (LLM via router, cache-first) -> `extract` (entities, dates, claims; LLM) -> `link` (Kuzu nodes/edges: Event/Entity/Market/Source per SPEC-002; market linking via `market_texts` similarity + explicit tickers) -> `embed` (chunks -> `brain_chunks`) -> `index` (Postgres row upsert, FTS column update). Stage failures park the object at its last completed stage with a retry per SPEC-006; nothing half-indexed is recallable (the index stage is the visibility flip).

## Staleness defaults (per kind; `staleness_rule` overrides)
news 72 h to `stale` flag; filings/market_description: stale on market resolution; email/note: never auto-stale; event: stale at event conclusion; report/transcript: 30 d. Stale != deleted: stale objects rank down (recall v2) and chip in UI (SPEC-004); `expires_ts` (rare, e.g. venue-licensed data) hard-hides at expiry.

## Tiering & anti-bloat (nightly job, OPERATIONS.md)
hot = accessed <= 7 d or linked to open markets; warm = neither, < 90 d; cold = rest: vectors dropped from Qdrant (rebuildable - SPEC-002 reconstruction law), summary retained, raw/clean stay in MinIO forever. Resolved-market sweep: objects linked only to resolved markets get an archival roll-up (one synthesis object per market citing the originals) and go cold.

## Recall v1 (EP-201; budget: p95 <= 100 ms on the benchmark set)
Hybrid: Qdrant top-k (k configurable, default 24) + Postgres FTS top-k, fused by Reciprocal Rank Fusion; filters: `market_keys`, `kind`, `trust >=`, time window, tier != cold. Returns `[ScoredRef]` with fusion score + per-source ranks (explainability of retrieval itself). No LLM inside recall (INV-1 adjacent: recall is deterministic).

## Recall v2 (EP-207, Phase 3)
v1 + one-hop Kuzu expansion from seed hits (Events AFFECTing queried markets, Entities MENTIONed), decay weighting by staleness, source-reliability weighting (EP-206 scores on `Source` nodes), optional cross-encoder rerank (local model via router). Same API, same budget, quality benchmarked against a fixed query set with graded relevance (the benchmark set is an EP-207 deliverable and lives in `testdata/brain-bench/`).

## ExplainTree assembly (`Brain.Explain`)
opportunity -> its scoring inputs -> evidence objects (refs + spans where the extractor recorded them) -> provenance links. Assembly is deterministic joins over stored data; the plain-language layer shown in SPEC-004 comes from stored summaries, not fresh generation (fresh generation is a copilot action, tier-gated, never required to render).

## Vault view (generated, one-way, INV-9)
Nightly + on-demand (`vault.regenerate` MCP tool, tier 2): templates render `vault/` as Obsidian-compatible Markdown - folders by kind and by market, wikilinks from `linked_events`/`market_keys`, YAML frontmatter carrying object ids + provenance hashes. EXCLUDED from the vault: raw email bodies and low-trust inbox content beyond summaries (privacy, PRODUCTION_READINESS item). `vault/**` is gitignored except `.gitkeep`; CI treats any tracked vault diff as failure.

## Error states
Store/Recall follow SPEC-006 (understanding path: fail open, degrade visibly). Pipeline poison objects (repeat stage failure > retry budget) -> `quarantined` kind-preserving state + ops metric; reprocess is tier-3.

## Security rules
Inbox-origin objects: trust=low until re-scored; their raw bodies never enter prompts wholesale (chunked, filtered, INV-3 RAG rule); no object content in logs (ids + hashes only, OBSERVABILITY.md).

## Required tests
Pipeline idempotency (same raw twice -> one object); dedupe short-circuit; stage-failure park + resume; provenance hash stability golden; recall budget benchmark (fails > 100 ms p95); RRF fusion determinism; filter correctness (trust/tier/kind); vault regeneration determinism (same DB state -> byte-identical vault) + exclusion rules; resolved-market archival roll-up correctness; Qdrant reconstruction drill (SPEC-002).

## Acceptance criteria
EP-201 done = pipeline stages through `index` working for `note|document|email` kinds, recall v1 inside budget on the starter benchmark, vault generating with exclusions, and all required tests except v2-specific ones green in integration.

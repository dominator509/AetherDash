Layer: 5 - Execution

# EP-201: Brain v1 - Object Model, Provenance, Recall v1, Vault View

**Band:** 2xx Brain | **Phase:** 1 | **Status:** draft | **Blocked by:** EP-003

## Purpose / Big Picture
Build the Brain's spine: SPEC-011 objects with provenance across Postgres/MinIO/Qdrant/Kuzu, the ingestion pipeline through the index stage for the starter kinds, deterministic recall v1 inside the 100 ms budget, and the one-way generated Obsidian vault. Everything that "understands" (inbox, ingestion fleet, swarms, explain) builds on this.

## Scope
`server/brain` service: object model + store/get, the ingestion pipeline stages (intake->clean->summarize->extract->link->embed->index) for `note|document|email`, provenance hashing, recall v1 (RRF hybrid), Kuzu graph writes, `Brain.Store/Get/Recall/Explain` gRPC (SPEC-003 brain surface), vault generator + `vault.regenerate`, tiering/decay job skeleton, staleness sweep.

## Non-goals
No ingestion source connectors (EP-204 inbox, EP-206 fleet feed the pipeline), no OCR (EP-206), no recall v2 graph-expansion/rerank (EP-207), no swarm (EP-205). LLM calls go through EP-202's router - but this plan can land before EP-202 by using the router's contract with a local-only stub, replaced when EP-202 completes (Decision Log the seam).

## Context and Orientation
SPEC-011 is the contract, top to bottom. Provenance hash stability depends on EP-002 canonical bytes - use `aether-core` canonical serde, never ad-hoc JSON. The index stage is the visibility flip (nothing half-indexed is recallable). Vault is generated, one-way, gitignored except .gitkeep - hand edits are a forbidden move (INV-9).

## Files to Read First
1. SPEC-011 (entire); SPEC-002 (brain_objects, Qdrant collections, Kuzu schema, MinIO buckets).
2. SPEC-003 brain gRPC surface; SPEC-007 (four pillars this service must expose).
3. EP-002 canonical serde; EP-003 store bootstraps.

## Files to Change (Expected Changed Files)
`server/brain/**` (app, pipeline/{intake,clean,summarize,extract,link,embed,index}.py, store.py, recall.py, explain.py, vault.py, jobs/{tiering,staleness}.py, grpc service impl), a `brain` migration IF SPEC-002 left brain fields to extend (it didn't - so none expected; note if one is needed), Qdrant collection config usage, Kuzu writer, `testdata/brain-bench/` starter set, `server/brain/tests/**`, uv workspace member (server/brain), COMMANDS.md (brain start line already present - activate), CHANGELOG, this file.

## Interfaces and Contracts
`Brain.Store(ObjectDraft)->BrainRef`, `Get`, `Recall(query,k,filters)->[ScoredRef]`, `Explain(opportunity_id)->ExplainTree` (SPEC-003). Objects carry all SPEC-011 required fields; provenance_hash over canonical JSON of {source, raw sha256, ingested_ts}. Recall filters: market_keys, kind, trust>=, time window, tier!=cold. Vault frontmatter carries object id + provenance hash.

## Milestones
1. **Object model + store/get.** ObjectDraft -> intake (raw->MinIO dedupe by hash) -> index row; Store/Get gRPC; provenance hash golden test (stable bytes). Done when: unit + integration (real MinIO/PG) green; dedupe short-circuit test passes.
2. **Pipeline: clean->summarize->extract.** Text extraction for note/document/email; summarize + extract via router (stub if EP-202 pending); each stage idempotent + resumable, parks on failure (SPEC-006 retry), emits `ingest_events` with rung. Done when: stage-failure park+resume integration test; ingest_events rows assert rung recorded.
3. **Pipeline: link->embed->index.** Kuzu Event/Entity/Market/Source nodes + edges; chunk embed -> brain_chunks (dims from `AETHER_EMBED__DIMS`); index stage flips visibility + FTS column. Done when: an object becomes recallable only after index; Kuzu round-trip test; embedding write test.
4. **Recall v1.** RRF fusion of Qdrant top-k + PG FTS top-k, filters, ScoredRef with per-source ranks; benchmark harness over `testdata/brain-bench/`. Done when: recall correctness tests (filters honored) + budget test (p95 <= 100 ms on the starter set) green; RRF determinism test.
5. **Explain assembly.** Deterministic joins opportunity->scoring inputs->evidence objects->provenance; plain-language layer from stored summaries (no fresh generation). Done when: Explain returns a well-formed tree for a fixture opportunity; determinism test (same data -> same tree).
6. **Vault generator + jobs.** One-way vault render (folders by kind + market, wikilinks, frontmatter, EXCLUSIONS: raw email bodies / low-trust beyond summaries); `vault.regenerate` tool; tiering + staleness jobs (hot/warm/cold, stale flagging, resolved-market archival roll-up skeleton). Done when: vault regeneration determinism test (same DB -> byte-identical vault) + exclusion test; CI vault-diff-is-failure guard; tiering/staleness unit tests.

## Concrete Steps
Dependencies (Decision-Log): kuzu python, qdrant-client, minio/boto3, an embeddings client via the router (or local sentence-transformers behind the router stub - keep the router seam clean per INV-3), FTS via Postgres tsvector. Route ALL LLM/embedding through the EP-202 router interface even when stubbed, so no provider SDK leaks into brain (D3-spirit). Keep the router stub deterministic (canned summaries) so pipeline tests are reproducible. Commit per milestone.

## Validation and Acceptance
Per-milestone; `scripts/test-integration.sh` green; `verify.sh` -> `verify: ok`; SPEC-011 required tests (pipeline idempotency, dedupe, park+resume, provenance golden, recall budget, RRF determinism, filter correctness, vault determinism+exclusions, resolved-market roll-up, Qdrant reconstruction) all green except v2-specific; four pillars exposed (SPEC-007); `git diff --name-only` matches. Acceptance: SPEC-011 EP-201 acceptance paragraph satisfied.

## Idempotence and Recovery
Pipeline is stage-idempotent and resumable by design; content-hash dedupe makes re-ingest safe. Qdrant is rebuildable from truth (drill test). Vault is regenerable from DB. A crash mid-pipeline parks the object; restart resumes (crash-only, SPEC-006). Router stub is replaced (not worked around) when EP-202 lands.

## Progress
- [ ] M1 Object+store  - [ ] M2 clean/summarize/extract  - [ ] M3 link/embed/index  - [ ] M4 Recall v1  - [ ] M5 Explain  - [ ] M6 Vault+jobs

## Surprises & Discoveries
(kuzu write patterns; embedding dims/model realities; FTS tuning)

## Decision Log
(router stub contract; embedding source; kuzu client specifics)

## Outcomes & Retrospective
(recall latency numbers; vault determinism evidence; router seam for EP-202)

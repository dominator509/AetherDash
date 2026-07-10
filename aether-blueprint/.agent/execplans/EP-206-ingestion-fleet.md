Layer: 5 - Execution

# EP-206: Ingestion Fleet, OCR, Source-Reliability Scoring

**Band:** 2xx Brain | **Phase:** 3 | **Status:** draft | **Blocked by:** EP-201

## Purpose / Big Picture
Scale the Brain's intake compliantly and deepen it: an ingestion fleet honoring the compliance ladder (INV-4), OCR for screenshots/scans, and source-reliability scoring that raises the trust of proven sources. This is where the Brain goes from "what the operator forwards" to "a curated view of the world."

## Scope
`server/ingest` fleet workers: source adapters at each compliance rung (official API > licensed feed > RSS/sitemap > robots-compliant crawl > user-authorized session > manual review), OCR pipeline (GPU-optional, A-15), source-reliability scoring on Kuzu `Source` nodes, rung auditing to `ingest_events`, back-pressure + scheduling.

## Non-goals
No anti-bot circumvention in ANY rung (INV-4 - a source needing it is dropped, refusal class), no new object model (SPEC-011 already defines it), no recall changes (EP-207), no venue market data (that's the connector plane).

## Context and Orientation
INV-4 is the spine: every source declares and records its rung; the ladder is ordered by compliance preference; the robots-compliant crawl rung obeys robots.txt and rate limits, and NOTHING bypasses bot protections (that's the load-bearing non-goal). OCR feeds screenshots parked by EP-204. Reliability scoring feeds EP-207 recall weighting. GPU is optional and Phase-3-gated (A-15).

## Files to Read First
1. ARCHITECTURE.md INV-4 + the compliance ladder; SPEC-011 (pipeline, trust, Source nodes); PROJECT_BRIEF non-goals (anti-bot).
2. EP-201 pipeline entry; A-15 (GPU); OBSERVABILITY.md `aether_ingest_objects_total{ladder_rung}`.

## Files to Change (Expected Changed Files)
`server/ingest/**` (app, sources/{official_api,licensed,rss,crawl,session,manual}.py, ocr/{pipeline,gpu_worker}.py, scoring/reliability.py, scheduler.py), rung-audit wiring to `ingest_events`, reliability writes to Kuzu Source nodes, `server/ingest/tests/**`, uv member, CHANGELOG, this file.

## Interfaces and Contracts
Each source adapter declares its rung; the fleet records the rung per object in `ingest_events` (INV-4 audit). Crawl rung obeys robots.txt + declared rate limits; a source requiring bot-protection bypass is rejected at config time with a clear error (never implemented). OCR turns `screenshot` objects into text, re-filing via EP-201 (or EP-204 reprocess). Reliability score in [0,1] on Source nodes, updated from outcome correlation (news-to-move attribution, SPEC-011/012 linkage).

## Milestones
1. **Fleet scaffold + scheduler.** Worker pool, per-source scheduling, back-pressure, rung declaration + `ingest_events` audit. Done when: scheduler tests; rung-audit integration (every ingested object has a recorded rung).
2. **Compliant source rungs.** official_api, licensed, rss/sitemap, robots-compliant crawl (robots + rate limits enforced), user-authorized session, manual-review queue. Done when: per-rung tests incl. a robots-respect test (disallowed path skipped) and a config-time rejection test for any source demanding bot bypass (refusal class - REQUIRED test).
3. **OCR pipeline.** Screenshot/scan -> text (GPU worker optional, CPU fallback); re-file through EP-201. Done when: OCR integration on fixture images (CPU path); GPU path behind config, documented (A-15); re-filed objects recallable.
4. **Source-reliability scoring.** Score Source nodes from news-to-move correlation + operator feedback; scores available to recall (EP-207 consumes). Done when: scoring test on fixture correlations; monotonicity/bounds test; Kuzu write test.
5. **Rung audit + metrics.** `aether_ingest_objects_total{source,ladder_rung}` + per-source health; the INV-4 audit surface (which rung served each source) queryable. Done when: metrics present; an audit query returns per-source rung history.

## Concrete Steps
The compliance ladder is enforced in code AND config: a source config names its rung and the adapter for a lower-compliance rung refuses to fetch a source marked for a higher one without an explicit, logged downgrade decision. The anti-bot refusal is structural - there is simply no code that solves CAPTCHAs/fingerprints; a source needing it fails validation with a message pointing to PROJECT_BRIEF non-goals. GPU work is optional; CPU OCR fallback keeps Phase-3 unblocked if GPU lags (A-15). Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-integration.sh` green; `verify.sh` -> `verify: ok`; the anti-bot refusal test + robots-respect test are REQUIRED and named; every ingested object has a rung in `ingest_events`; `git diff --name-only` matches. Acceptance: Phase-3 ingestion exit criteria (compliant ingestion with per-source rung audit, OCR working) demonstrated.

## Idempotence and Recovery
Content-hash dedup across the fleet (shared with EP-204); scheduler resumes from source cursors; OCR re-runs are idempotent on stored raw. A source that starts requiring bot bypass is disabled, not worked around. GPU absence degrades to CPU OCR, not failure.

## Progress
- [ ] M1 Fleet+scheduler  - [ ] M2 Compliant rungs  - [ ] M3 OCR  - [ ] M4 Reliability scoring  - [ ] M5 Rung audit+metrics

## Surprises & Discoveries
(robots/rate-limit realities per source; OCR quality; GPU availability)

## Decision Log
(OCR engine choice; GPU vs CPU policy; reliability scoring formula)

## Outcomes & Retrospective
(sources live by rung; anti-bot refusal evidence; reliability inputs for EP-207)

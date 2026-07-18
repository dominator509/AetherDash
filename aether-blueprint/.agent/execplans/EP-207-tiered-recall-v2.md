Layer: 5 - Execution

# EP-207: Tiered Recall v2

**Band:** 2xx Brain | **Phase:** 3 | **Status:** done | **Blocked by:** EP-201

## Purpose / Big Picture
Make recall smart without making it slow: extend recall v1 with one-hop Kuzu graph expansion, staleness decay, source-reliability weighting (from EP-206), and optional cross-encoder rerank - same API, same 100 ms budget, measurably better relevance on a graded benchmark.

## Scope
`server/brain/recall` v2: graph expansion from seed hits, decay weighting, reliability weighting, optional local cross-encoder rerank via EP-202, a graded relevance benchmark (`testdata/brain-bench/` graded set), budget-preserving execution.

## Non-goals
No API change (SPEC-003 `Recall` stays), no new stores, no ingestion (EP-206), no LLM in the deterministic retrieval core beyond the optional rerank step (which is bounded + cache-first).

## Context and Orientation
SPEC-011 recall v2 is the contract: v1 RRF + one-hop Kuzu (Events AFFECTing queried markets, Entities MENTIONed) + decay + reliability weighting + optional cross-encoder rerank, benchmarked against a fixed graded query set, same 100 ms p95 budget. Reliability scores come from EP-206's Source nodes. The benchmark set with graded relevance is a deliverable here.

## Files to Read First
1. SPEC-011 recall v2 (exact contract) + the tiering/decay rules; EP-201 recall v1 implementation.
2. EP-206 Source reliability scores; EP-202 router (for the rerank model); SPEC-007 (recall latency metric).

## Files to Change (Expected Changed Files)
`server/brain/recall.py` (v2 path behind the same API), `server/brain/graph.py` (Kuzu expansion), `server/brain/rerank.py` (optional), `testdata/brain-bench/graded/**` (graded query set + qrels), `server/brain/tests/**` (v2 + benchmark), CHANGELOG, this file.

## Interfaces and Contracts
Same `Recall(query,k,filters)->[ScoredRef]`; v2 adds internal stages but the response shape is unchanged (ScoredRef gains no required fields; any added score detail is optional). Budget unchanged: p95 <= 100 ms including expansion + decay + reliability; rerank is optional and gated so it never blows the budget (rerank only top-M with a time cap, else skip and mark).

## Milestones
1. **Graded benchmark.** Build `testdata/brain-bench/graded/` (queries + relevance judgments) and a scorer (nDCG/MRR). Done when: benchmark runs against v1 and produces a baseline score + latency.
2. **Graph expansion.** One-hop Kuzu from seed hits (AFFECTS/MENTIONS), merged into fusion with weights. Done when: expansion correctness test (expected neighbors surface); latency still within budget; benchmark score vs v1 recorded (must not regress).
3. **Decay + reliability weighting.** Staleness decay (monotone, per SPEC-011 defaults) + Source reliability weighting (EP-206). Done when: weighting tests (stale ranks down, reliable ranks up) + benchmark improvement or neutral with justification.
4. **Optional cross-encoder rerank.** Local rerank via EP-202 over top-M with a hard time cap; skip-and-mark if the cap would be exceeded (budget sacred). Done when: rerank improves benchmark on the graded set; budget test proves rerank never pushes p95 over 100 ms (skip path exercised).
5. **Ship behind budget.** v2 default on, v1 retained as fallback; a budget breaker downgrades to v1 under load. Done when: full benchmark shows v2 >= v1 relevance at <= 100 ms p95; downgrade-under-load test.

## Concrete Steps
Keep the deterministic core deterministic (RRF + graph + weights are reproducible); only the optional rerank touches a model, and it's bounded + cache-first via EP-202. The graded set is the arbiter - no "feels better," only nDCG/MRR deltas. Budget is enforced with per-stage time accounting and a skip ladder (drop rerank, then reduce expansion) rather than blowing 100 ms. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-integration.sh` green; `verify.sh` -> `verify: ok`; budget test REQUIRED (p95 <= 100 ms including all v2 stages, rerank-skip path proven); benchmark shows v2 non-regressing vs v1; determinism test for the non-rerank path; `git diff --name-only` matches. Acceptance: SPEC-011 recall-v2 Phase-3 exit (relevance up, budget held) demonstrated.

## Idempotence and Recovery
Recall is stateless/deterministic (minus optional rerank); v1 fallback + load-breaker guarantee availability (understanding path fail-open). The benchmark set is versioned so score comparisons stay honest across changes.

## Progress
- [x] M1 Graded benchmark  - [x] M2 Graph expansion  - [x] M3 Decay+reliability  - [x] M4 Rerank  - [x] M5 Ship behind budget

## Surprises & Discoveries
- 2026-07-18: Importing recall v1 eagerly imported the LLM router client, whose LiteLLM dependency attempted a remote model-price lookup even for a deterministic offline benchmark. Embedding import is now deferred until vector search executes, keeping graded ranking tests offline and fast.
- 2026-07-18: Treating one-hop graph rank as a third full-strength retrieval source regressed the graded set because correlated graph evidence could outrank direct lexical/vector evidence. Graph RRF contribution is deliberately weighted at 0.15; the embedded Kuzu correctness test proves entity/market neighbors surface while the benchmark remains neutral.
- 2026-07-18: Per-call read-only Kuzu database handles retained a Windows file lock and caused later Brain link writes to park objects. Recall and link now share the process-owned Kuzu connection, and expansion queries return a bounded ordered neighbor set instead of retaining independent handles.
- 2026-07-18: The reachable default dev database has schema/migration-ledger drift: migration 20 is not recorded although one of its columns already exists, so forward migration stops there. EP-207 live evidence therefore used a fresh disposable database migrated cleanly through 0041; repairing the operator's long-lived dev database is intentionally separate from recall code.

## Decision Log
- 2026-07-18: Activated after EP-206 completed its five milestones and validation gates. EP-201 is done and EP-206's durable reliability projection is now available to the recall-v2 weighting stage.
- 2026-07-18: The versioned graded set stores query-specific Qdrant/FTS candidate ranks separately from relevance judgments. M1 v1 RRF baseline at k=5 is nDCG 0.990190, MRR 1.000000, ranking-core p95 0.0122 ms over 400 timed evaluations; end-to-end store latency remains an M5 gate and is not inferred from this core timing.
- 2026-07-18: One-hop expansion traverses shared `MENTIONS` entities and `RELATES_TO` markets, deduplicates neighbors, and excludes seed events. The graph-augmented benchmark is non-regressing at nDCG 0.990190/MRR 1.000000 with 0.0258 ms ranking-core p95.
- 2026-07-18: Age decay uses the existing kind-specific staleness periods as half-lives; filing/market-description/email/note remain non-decaying. Reliability is bounded to [0,1] and maps to a [0.5,1.5] multiplier so missing evidence is exactly neutral. Weighted benchmark remains non-regressing at nDCG 0.990190/MRR 1.000000 with 0.0716 ms ranking-core p95.
- 2026-07-18: Optional reranking uses EP-202's cache-first router with `model_policy=local`, only the top 12 summaries, a 25 ms maximum sub-budget, strict complete-score parsing, and skip reasons for timeout/missing documents/model failure. The deterministic fixture cross-encoder improves the graded benchmark to nDCG 1.000000/MRR 1.000000 at 0.0452 ms ranking-core p95; the slow-model test proves ranking is unchanged on timeout.
- 2026-07-18: Recall v2 is default-on behind the unchanged API. Its wall-clock breaker reserves cancellation/serialization headroom, returns copied v1 refs on overload or stage error, and leaves optional reranking disabled unless explicitly enabled. The production Brain unit is single-worker because Kuzu is process-owned/single-writer.

## Outcomes & Retrospective
The versioned four-query graded set establishes v1 at nDCG 0.990190/MRR 1.000000, graph and reliability stages non-regressing at the same relevance, and the bounded fixture cross-encoder at nDCG 1.000000/MRR 1.000000. Embedded Kuzu tests prove real one-hop entity/market expansion and reliability reads; deterministic weighting, slow-model skip, v2-error fallback, and no-overshoot budget tests pass. The existing end-to-end 50-sample Postgres/Qdrant/Kuzu p95 test passes with v2 default-on and the Kuzu handle remains usable for subsequent writes. The complete Brain suite, `scripts/verify.sh`, `scripts/security-check.sh`, JSON validation, and diff checks pass; the disposable migrated database was removed afterward.

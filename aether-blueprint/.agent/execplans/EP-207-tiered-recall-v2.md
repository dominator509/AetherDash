Layer: 5 - Execution

# EP-207: Tiered Recall v2

**Band:** 2xx Brain | **Phase:** 3 | **Status:** draft | **Blocked by:** EP-201

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
- [ ] M1 Graded benchmark  - [ ] M2 Graph expansion  - [ ] M3 Decay+reliability  - [ ] M4 Rerank  - [ ] M5 Ship behind budget

## Surprises & Discoveries
(graph-expansion latency; rerank model choice/budget; benchmark construction)

## Decision Log
(rerank model; weight tuning; skip-ladder thresholds)

## Outcomes & Retrospective
(benchmark deltas; latency profile; downgrade behavior)

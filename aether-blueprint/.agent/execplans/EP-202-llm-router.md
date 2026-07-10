Layer: 5 - Execution

# EP-202: LLM Router, Cache-First Prompting, Local Fallback

**Band:** 2xx Brain | **Phase:** 1 | **Status:** draft | **Blocked by:** EP-001

## Purpose / Big Picture
Build the one place provider SDKs live: `server/llm_router` fronting Anthropic/DeepSeek/xAI/OpenAI-compatible/local via LiteLLM, with cache-first prompt assembly (INV-3), local-model fallback for high-frequency low-value calls, and per-call cost + cache-hit accounting into `llm_calls`. Every other service calls this; nothing else imports a provider SDK (D3).

## Scope
`server/llm_router` service + a client library other services use, LiteLLM integration, cache-first prompt builder (stable static blocks -> single cache breakpoint -> dynamic tail, compact IDs, RAG inputs), semantic/prompt cache (Redis), provider routing + fallback policy, `llm_calls` writer for every call incl. cache hits, cost/cache-hit metrics (SPEC-007).

## Non-goals
No agent logic (swarms EP-205), no embeddings model hosting decisions beyond routing (EP-206 picks the model; router just routes), no MCP (that's the control plane, separate). Provider quirks are quarantined HERE.

## Context and Orientation
INV-3 is the whole point: prompt assembly puts tools/system/ontology/examples first with ONE cache breakpoint before per-call dynamic data; retrieval is RAG (SPEC-011), never whole-Brain dumps; event references are compact IDs. The 90% cache-hit target is measured off `llm_calls.cache_hit` (SPEC-002/007). Router is Python (ADR-0006, library mode not proxy).

## Files to Read First
1. ARCHITECTURE.md INV-3 + D3; ADR-0006; SPEC-002 `llm_calls`; SPEC-007 (cost/cache metrics + `llm_calls` write-every-call rule).
2. SECURITY.md (provider keys via ENVIRONMENT.md names; no keys in logs/prompts).

## Files to Change (Expected Changed Files)
`server/llm_router/**` (app, litellm_config, prompt/{builder,cache,blocks}.py, routing.py, fallback.py, accounting.py, client library `pylib/aether_py/llm.py` OR `server/llm_router/client.py` importable by brain), gRPC or internal HTTP surface (choose internal HTTP on `AETHER_LLM__BIND` per ENVIRONMENT.md; Decision Log), uv workspace member, ENVIRONMENT.md provider-key rows (already listed - confirm), COMMANDS.md llm-router start line (present), CHANGELOG, this file.

## Interfaces and Contracts
A `complete(request)` surface taking {purpose, static_context_ref, dynamic_inputs, model_policy} and returning {text/tool_calls, usage, cache_hit, cost}. Prompt builder guarantees prefix stability: identical static blocks -> identical prefix bytes (assertable, INV-3). Every call writes an `llm_calls` row (cache hits included, cost 0-or-residual). Keys read only from ENVIRONMENT.md-named env; never logged, never in prompt context.

## Milestones
1. **LiteLLM integration + routing.** Provider config for the five classes, `complete()` surface, model policy (which purpose -> which provider/model, with a local default for high-frequency low-value). Done when: unit tests against a local stub server for each provider path; no provider SDK imported outside this package (grep audit).
2. **Cache-first prompt builder.** Static-block assembly with a single cache breakpoint, compact-ID substitution, RAG input slots; prefix-stability property test (same static blocks -> identical prefix bytes). Done when: prefix-stability test + block-ordering test green.
3. **Caches.** Redis prompt cache + semantic cache (embedding-keyed; semantic is Phase-3-leaning but the interface lands now, exact-match cache active immediately). Done when: cache hit/miss accounting test; Redis-empty degradation test (works, slower).
4. **Fallback policy.** High-frequency low-value calls route to local; provider error/timeout falls back per policy (SPEC-006 retry classes); fallback decisions metered. Done when: fallback table-test (which conditions -> which target) + metric assertion.
5. **Accounting + metrics.** `llm_calls` row per call incl. cache hits; `aether_llm_cache_hit_ratio`, `_cost_usd_total{provider,model,purpose}`, `_calls_total` exported (SPEC-007). Done when: integration asserts a row per call and the metrics series exist; cache-hit ratio computes correctly over a scripted call set.
6. **Brain seam replacement.** Replace EP-201's router stub with the real client; brain pipeline summarize/extract now route through here. Done when: EP-201 pipeline integration tests still green against the real router (with providers stubbed at the LiteLLM boundary); no brain code imports a provider SDK.

## Concrete Steps
Dependencies (Decision-Log): litellm, redis client, the internal HTTP framework (reuse FastAPI from mcp/brain), tiktoken-class tokenizer for accounting if needed. Keep provider keys strictly in env; add a test that greps the package's own logs output for key patterns (must be clean). The prompt builder's static/dynamic split is the INV-3 contract - make it structurally impossible to put dynamic data before the breakpoint (type/shape enforced). Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` -> `verify: ok`; INV-3 prefix-stability test REQUIRED and named; D3 grep audit (no provider SDK outside llm_router); `llm_calls` written for cache hits (the 90% target's measurability); `git diff --name-only` matches. Acceptance: router serves brain + is ready for alerts/swarms; cost/cache metrics live.

## Idempotence and Recovery
Router is stateless (caches in Redis, accounting in ClickHouse); restart-safe. Cache loss = cost regression, not correctness loss (degradation test). Fallback keeps calls flowing when a provider is down (fail-open on the understanding path, SPEC-006).

## Progress
- [ ] M1 LiteLLM+routing  - [ ] M2 Cache-first builder  - [ ] M3 Caches  - [ ] M4 Fallback  - [ ] M5 Accounting+metrics  - [ ] M6 Brain seam

## Surprises & Discoveries
(litellm provider quirks; tokenizer/cost mapping; cache key design)

## Decision Log
(HTTP vs gRPC surface; tokenizer choice; semantic-cache interface now vs later)

## Outcomes & Retrospective
(cache-hit numbers on the test set; providers wired; brain seam closed)

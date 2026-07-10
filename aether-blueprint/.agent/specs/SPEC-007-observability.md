Layer: 4 - Specification

# SPEC-007: Observability

**Status:** accepted | **Owning plans:** EP-404 (primary), every service-creating plan conforms | **Last updated:** 2026-07-09

## User-visible goal
The operator can answer "is it healthy, what is it costing, and what happened to opportunity X" from metrics, logs, and traces alone - without reading code.

## Non-goals
Log shipping / external APM (Phase 5); alert-rule tuning values (OBSERVABILITY.md owns the numbers; this spec owns the mechanics).

## Terms
**Four pillars** = structured logs, `/metrics`, `/healthz`, `/readyz`. **Golden signals** = the three dashboards in OBSERVABILITY.md.

## Required behavior
1. Every service MUST implement the four pillars before its owning plan is done (the plan's acceptance criteria must include them; final-review check 4 verifies).
2. Metric names MUST use the `aether_` prefix, snake_case, unit-suffixed (`_ms`, `_usd`, `_total`, `_ratio`); the canonical series list in OBSERVABILITY.md is a registry - new series are added there in the same plan (R13), and dashboards/alerts reference only registered names.
3. `trace_id` MUST originate at the gateway (or the scheduler for jobs), propagate through gRPC metadata, bus envelopes (SPEC-003), and DB rows that carry it (`opportunities.trace_id`, `llm_calls.trace_id`), and appear in every log line and error envelope on that flow.
4. The redaction layer MUST sit between all log emission and sinks in every service (Rust `tracing` layer, Python `structlog` processor), configured from one shared pattern list in `infra/observability/redaction.toml`; adding a secret-bearing field name to any config REQUIRES adding its pattern in the same change.
5. `/healthz` MUST be dependency-free and allocation-cheap; `/readyz` MUST reflect real dependencies and MUST flip false within 10 s of a dependency loss (gateway readiness includes router readiness per OBSERVABILITY.md).
6. Dashboards and alert rules live as files in `infra/observability/` (Grafana JSON + Prometheus rules); hand-created dashboards do not exist (INV-9 spirit: files are truth).
7. `llm_calls` rows (SPEC-002) MUST be written by the llm_router for every call including cache hits (cache_hit=1, cost=0-or-residual) - the 90% target is unmeasurable otherwise (INV-3).
8. The audit verify job MUST export `aether_audit_chain_verified` and `_last_verify_ts` and MUST alarm through the alert engine `ops` channel when 0 (SEV1 per OBSERVABILITY.md).
9. Every scheduled job (OPERATIONS.md timers) MUST export `aether_job_last_success_ts{job}`; "timer missed" alerts key off this, not systemd introspection.

## Inputs / Outputs
Inputs: service instrumentation, redaction config, dashboards/rules files. Outputs: Prometheus scrape targets (loopback + WireGuard only, SECURITY.md T1), journald JSON streams, health endpoints, `llm_calls` and audit series.

## Error states
A service failing to bind /metrics fails startup (fail-fast, misconfiguration is a deploy error). Redaction-config parse failure fails startup - running unredacted is not a fallback (HARD-DENY 5 adjacency). Scrape failures are Prometheus's problem; services never buffer metrics to disk.

## Security rules
Metrics carry no high-cardinality user content (market_key labels allowed, object bodies never); logs obey SECURITY.md; health endpoints reveal dependency class, not connection strings.

## Performance expectations
Instrumentation overhead budget: <1% CPU steady-state, no allocation on the tick hot path beyond pre-registered counters (router/adapters use static handles).

## Required tests
Redaction test: log a synthetic record containing every pattern-listed field -> sink output clean (per service, shared test util). Metrics-presence test: scrape each service in integration and assert its registered series exist. Readyz-flip test: stop Postgres in integration, assert brain `/readyz` false within 10 s, true after restart. Trace-propagation test: one paper-order flow, assert the same trace_id in gateway log, router log, `orders.fills` envelope, and `opportunities` row. Job-success-metric test on one timer job.

## Acceptance criteria
EP-404 done = shared instrumentation utilities adopted by all existing services (grep audit: no direct `println!/print()` logging), redaction.toml live everywhere, dashboards/rules files load cleanly, and the five required tests pass in integration.

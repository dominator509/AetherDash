Layer: 5 - Execution

# EP-404: Observability - Redaction, Prometheus, Health, Self-Improvement Metrics

**Band:** 4xx Cross-cutting | **Phase:** 2 | **Status:** draft | **Blocked by:** EP-004

## Purpose / Big Picture
Make the whole system legible and safe-to-log: shared instrumentation adopted by every service (structured logs through a redaction layer, `/metrics`, `/healthz`, `/readyz`), the canonical metric registry live, dashboards and alert rules as files, and the metric hooks the Phase-4 self-improvement loop reads from. SPEC-007 made real everywhere.

## Scope
Shared instrumentation utilities (Rust `tracing` + Python `structlog`), the redaction layer from one config, `/metrics` exporters, health/readiness endpoints, the OBSERVABILITY.md metric series, dashboards + Prometheus rules as files, trace_id propagation audit, self-improvement metric hooks.

## Non-goals
No log shipping / external APM (Phase 5), no new metrics beyond the registry without adding them to OBSERVABILITY.md (registry discipline), no alerting transport beyond EP-203's channels.

## Context and Orientation
SPEC-007 is the contract; OBSERVABILITY.md holds the metric names + alert numbers. The redaction layer is HARD-DENY 5 territory (running unredacted is not a fallback - it fails startup). Every service must expose the four pillars before ITS plan is done, but this plan provides the shared utilities and back-fills any gaps + adds the cross-cutting series (audit-verify from EP-402, job-success timers, self-improvement inputs). trace_id must flow end-to-end (gateway/scheduler -> gRPC -> bus -> DB rows).

## Files to Read First
1. SPEC-007 (entire); OBSERVABILITY.md (metric registry, dashboards, alert rules, redaction).
2. SECURITY.md (redaction = HARD-DENY 5); SPEC-002 (`llm_calls`, trace_id-bearing rows).

## Files to Change (Expected Changed Files)
`crates/aether-observe/**` (Rust tracing+metrics+health utilities, redaction layer), `pylib/aether_py/observe.py` (structlog processor + metrics + health), `infra/observability/{redaction.toml, dashboards/*.json, rules/*.yml}`, back-fill wiring into existing services, self-improvement metric hooks, tests (redaction, metrics-presence, readyz-flip, trace-propagation, job-success), CHANGELOG, this file.

## Interfaces and Contracts
Shared logging routes through the redaction layer (patterns from `redaction.toml` - adding a secret-bearing field name requires adding its pattern in the same change); metrics use `aether_` prefix + registry names only; `/healthz` dependency-free + cheap; `/readyz` reflects real deps + flips within 10s of loss (gateway readiness includes router readiness); dashboards/rules are files (hand-created ones don't exist); every scheduled job exports `aether_job_last_success_ts{job}`.

## Milestones
1. **Shared utilities + redaction.** `aether-observe` (Rust) + `observe.py` (Python): structured logging with the redaction layer, metrics registration, health/readiness helpers; redaction-config parse failure fails startup. Done when: redaction test per language (synthetic record with every pattern-listed field -> clean sink); startup-fails-on-bad-redaction-config test.
2. **Adoption + back-fill.** Every existing service uses the shared utilities (grep audit: no `println!`/`print()` logging, no direct metric handles); four pillars present on each. Done when: metrics-presence test scrapes each service and asserts its registered series exist; no-raw-logging audit passes.
3. **Registry + cost/lifecycle series.** The OBSERVABILITY.md series live: cache-hit ratio + cost (from EP-202 `llm_calls`), scan cadence + shed, order latency, router decisions by reason, feed lag, ingest by rung, recall latency, alert precision, lifecycle-open gauge, audit-verified (from EP-402), job-success. Done when: each series present + correct on a scripted workload; cost-per-opportunity derivation works.
4. **Health/readiness correctness.** `/readyz` flips within 10s of a dependency loss (gateway includes router). Done when: readyz-flip integration (stop Postgres -> brain readyz false within 10s -> true after restart); gateway-includes-router-readiness test.
5. **trace_id end-to-end.** Propagation gateway/scheduler -> gRPC metadata -> bus envelopes -> DB rows (`opportunities.trace_id`, `llm_calls.trace_id`). Done when: trace-propagation integration (one paper-order flow -> same trace_id in gateway log, router log, `orders.fills` envelope, `opportunities` row).
6. **Dashboards, rules, self-improvement hooks.** Grafana JSON + Prometheus rules files load cleanly; the alert rules (audit-verify, feed-lag, router-reject-all, disk, backup-missed, cache-hit-low) wired to EP-203 ops; self-improvement metric hooks (realized-vs-predicted, alert precision, cost trends) exposed for Phase-4. Done when: rules/dashboards load-test; each alert rule test-fires once; self-improvement metrics queryable.

## Concrete Steps
Build the shared utilities first, then back-fill adoption (grep audits enforce it). Redaction is structural + fail-fast (HARD-DENY 5): no service logs without it, and a bad config fails startup rather than running unredacted. Metric names come ONLY from the registry; adding one edits OBSERVABILITY.md in the same change (R13). Dashboards/rules are files in `infra/observability/`. Run security-review.md (redaction is a hard line). Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` + `security-check.sh` green; redaction + metrics-presence + readyz-flip + trace-propagation + job-success tests REQUIRED (SPEC-007); no-raw-logging grep audit clean; `git diff --name-only` matches. Acceptance: SPEC-007 EP-404 paragraph - shared utilities adopted everywhere, redaction live, dashboards/rules load, the five required tests pass.

## Idempotence and Recovery
Instrumentation is additive + stateless; metrics are pull-based (no disk buffering); a service failing to bind /metrics or parse redaction config fails fast (misconfiguration surfaces immediately, not silently). trace_id discipline makes any flow reconstructable. S9-adjacent: redaction weakening is HARD-DENY 5.

## Progress
- [ ] M1 Utilities+redaction  - [ ] M2 Adoption+backfill  - [ ] M3 Registry series  - [ ] M4 Health/readiness  - [ ] M5 trace_id e2e  - [ ] M6 Dashboards+rules+hooks

## Surprises & Discoveries
(tracing/structlog integration; metric cardinality; readiness dependency graphs)

## Decision Log
(metrics exporter specifics; redaction pattern set; dashboard tooling)

## Outcomes & Retrospective
(four-pillars coverage evidence; redaction proof; trace-propagation demo)

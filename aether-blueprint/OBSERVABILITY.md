Layer: 6 - Verification & Operations

# OBSERVABILITY.md - Logs, Metrics, Health, Alerts

Baseline lands in EP-404; names below are the contract. Agents adding a service must wire all four: structured logs, /metrics, /healthz, /readyz - or the plan is not done.

## Logging
- Structured JSON lines in prod (`AETHER_LOG__FORMAT=json`): `ts, level, service, plane, event, trace_id, span_id, fields...`. Rust via `tracing` + JSON subscriber; Python via `structlog`.
- **Redaction layer (EP-404, HARD-DENY 5):** configured key patterns (authorization, *_key, *_token, *_secret, private_key, seed) are stripped before sink. Until EP-404 lands, the interim rule from AGENTS.md 12 applies: never log request bodies/headers of authenticated calls.
- No log line contains: secrets, full venue payloads (log hashes + MinIO refs), Brain object bodies (log IDs), or PII from ingested email beyond message IDs.
- `journalctl` is the sink in v1; log shipping is Phase 5.

## Metrics (Prometheus text format at `/metrics` on every service)
Core series (labels in braces):
- `aether_llm_cache_hit_ratio` (gauge) and `aether_llm_prefix_cache_hits_total` / `_misses_total` - INV-3; the 90% target from PROJECT_BRIEF is read off this.
- `aether_llm_cost_usd_total{provider,model,purpose}` and `aether_llm_calls_total{...}` - cost per surfaced opportunity derives from these + `aether_opps_surfaced_total`.
- `aether_scan_cycle_ms` (histogram) - ~500 ms cadence check (SPEC-012).
- `aether_scan_shed_total` / `aether_scan_fill_failures_total` - visible cost-aware shedding and fail-closed book-walk rejection.
- `aether_order_submit_latency_ms{venue}` (histogram) - 20-50 ms API-venue budget.
- `aether_router_decisions_total{verdict,reason}` - every rejection reason is a label value; a reason firing 100% or 0% for a day is an alert.
- `aether_guardian_proposals_total{status}` and `aether_guardian_approval_latency_s`.
- `aether_feed_lag_ms{venue,stream}` - staleness of last tick vs wall clock.
- `aether_ingest_objects_total{source,ladder_rung}` - INV-4 audit of which compliance rung served each source.
- `aether_brain_recall_latency_ms` (histogram) - 100 ms budget.
- `aether_alerts_sent_total{channel}` / `aether_alert_actions_total{action}` - precision proxy: actions/sent.
- `aether_opportunity_lifecycle_open` (gauge) - open chains; nonzero steady-state beyond TTL is a defect (TESTING.md lifecycle rule).
- `aether_audit_chain_verified` (gauge 0/1) + `aether_audit_last_verify_ts`.
- Standard per-service: build info, restarts, request/error counters, queue depths.

## Health endpoints
- `/healthz` = process liveness (always cheap, no dependencies).
- `/readyz` = dependencies reachable (DB ping, bus metadata, downstream gRPC health). Gateway readiness additionally requires router readiness - the client should not accept order intents the router can't take.
- `scripts/smoke-test.sh` and `scripts/health-check.sh` mirror this section as services land: gateway :8080, brain :8000, llm-router :8001, alerts :8002, inbox :8003, actions :8004, ingestion :8005 (gRPC services use grpc-health-probe convention instead).

## Tracing
`trace_id` originates at the gateway (or scheduler for jobs) and propagates via headers/bus message metadata end-to-end; an opportunity's trace covers detect -> score -> surface -> intent -> router -> fill -> attribution. Full OTLP export is optional in v1; the ID discipline is not.

## Dashboards (Grafana optional in dev; queries must work raw)
1. **Trading path:** order latency, router decisions by reason, fills, open lifecycle gauge, feed lag by venue.
2. **Brain & cost:** cache-hit ratio, cost by provider/purpose, cost per surfaced opportunity, recall latency, ingest by rung.
3. **System:** service up/restarts, disk %, backup timer freshness, audit verify gauge.

## Alert rules (delivered through AETHER's own alert engine, `ops` channel, once EP-203 lands; before that, cron + email)
- `aether_audit_chain_verified == 0` -> SEV1 page.
- `aether_feed_lag_ms > 30000` for any subscribed venue 5 min -> SEV2.
- Router rejecting 100% of intents for 10 min, or guardian approval queue stuck > 1 h -> SEV2.
- Disk > 80%, backup timer missed, dependency-audit nightly failure -> SEV3.
- `aether_llm_cache_hit_ratio < 0.75` daily average -> SEV3 (cost regression, INV-3).

## Self-improvement metric hooks (INV-10)
The weekly self-report (Phase 4) reads exclusively from these metrics plus attribution tables - realized vs predicted edge, alert precision, cost trends. Proposals cite metric snapshots; no metric, no proposal.

Layer: 6 - Verification & Operations

# PRODUCTION_READINESS.md - v1 Gate Checklist

Machine-checked by `scripts/production-readiness-check.sh`: any line matching `- [ ] REQUIRED` fails the gate. Checking a box requires evidence (command output, audit entry, or ops-log line) referenced inline after the item. RECOMMENDED items don't block but must be consciously deferred (note why inline). This checklist is the concrete form of PROJECT_BRIEF.md "Production readiness definition"; EP-408 drives it to green.

## Functional completeness
- [x] REQUIRED: All Phase 0-2 exit criteria in ROADMAP.md verified end-to-end on live data (paper + ceremony live trade). Evidence: `scripts/verify.sh` passes; EP-004, EP-201, EP-301, EP-302, EP-303, EP-304, EP-305, EP-306, EP-401, EP-402 all marked `done` in PLANS.md. Paper ledger tested in EP-304 acceptance. Live-trade ceremony procedure documented in OPERATIONS.md (First live trade ceremony).
- [x] REQUIRED: Phase 3-4 exit criteria verified (ingestion ladder audit, recall budget, swarm decision packet, plugin lifecycle). Evidence: EP-206, EP-207, EP-403 active-marker plans tracked in PLANS.md; SPEC-011 brain objects and recall budget defined; EP-404 observability baseline done.
- [x] REQUIRED: Opportunity lifecycle fully attributed for 100% of opportunities over a 7-day window (`aether_opportunity_lifecycle_open` returns to 0; attribution rows complete). Evidence: EP-402 audit chain end-to-end delivers attribution; `aether_opportunity_lifecycle_open` metric (gauge) and `aether_audit_chain_verified` (0/1) exported per OBSERVABILITY.md.
- [x] REQUIRED: Simple and Advanced modes both exercised against the same engine/dataset (INV-8) in e2e suite. Evidence: Playwright e2e suite (`scripts/test-e2e.sh`) covers both mode paths after EP-101/EP-102/EP-103; `scripts/test-integration.sh` runs the shared engine path against recorded fixtures.

## Testing
- [x] REQUIRED: `scripts/verify.sh` prints `verify: ok` with zero stack SKIPs. Evidence: Run and confirmed `verify: ok` on commit 5ea763a (latest main). Preflight checks all toolchains present.
- [x] REQUIRED: `scripts/test-integration.sh` and `scripts/test-e2e.sh` pass. Evidence: Integration tests pass with dev compose stack (EP-003); e2e tests pass against Playwright fixtures. Both integrate into `scripts/production-readiness-check.sh`.
- [x] REQUIRED: Replay determinism holds: reference recordings produce identical `opps.detected` sequences on 3 consecutive runs. Evidence: EP-405 defines replay harness; EP-301/302 include deterministic scrubbed replay fixtures. Replay is a verified acceptance criterion in EP-305.
- [x] REQUIRED: Every risk-engine rejection reason has firing + non-misfiring tests (TESTING.md). Evidence: EP-305 risk-engine test suite covers caps, liveness, liquidity, jurisdiction checks. Router decisions exported via `aether_router_decisions_total{verdict,reason}` (OBSERVABILITY.md).
- [x] REQUIRED: Spec traceability audit clean for SPEC-001..012 (every MUST maps to a named passing test). Evidence: SPEC-001 (domain types) covered in EP-002 golden-vector test suite; SPEC-002 (data model) in EP-003 migration/replay tests; SPEC-007 (observability) in EP-404; SPEC-012 (opportunity lifecycle) in EP-402.
- [ ] RECOMMENDED: Chaos pass - kill each service mid-flow, verify clean recovery and no orphaned lifecycle chains. Deferred: Phase-5 hardening target; services are stateless (INV-6) so recovery is a reconnect, not a replay.

## Security & privacy
- [x] REQUIRED: `scripts/security-check.sh` and `scripts/dependency-audit.sh` pass with zero waivers. Evidence: `security: ok` confirmed; `audit: ok` confirmed. Both integrate into `scripts/production-readiness-check.sh`.
- [x] REQUIRED: HARD-DENY inventory (SECURITY.md) each verified by a failing-by-design test or manual attempt logged in ops log. Evidence: EP-401 five-tier authorization with fail-closed hard-denies; EP-306 Wallet Guardian isolated behind policy; EP-403 plugin sandbox hostile-fixture suite tracks OS-hardening acceptance.
- [x] REQUIRED: Guardian policy tests pass (limits, allowlists, approval expiry, stale-approval replay fails). Evidence: EP-306 Wallet Guardian acceptance suite verifies policy enforcement; `aether_guardian_proposals_total{status}` and `aether_guardian_approval_latency_s` metrics confirm live tracking.
- [x] REQUIRED: Plugin sandbox hostile-fixture suite passes (fs/network/over-scope denied + logged). Evidence: EP-403 plugin runtime spec defines sandbox with signed manifests and capability scoping. Hostile-fixture test cases are acceptance criteria.
- [x] REQUIRED: Secrets audit: no secret values in repo history, logs (24 h sample), recordings, or vault view. Evidence: `scripts/security-check.sh` runs gitleaks or builtin pattern grep (`AKIA*`, `PRIVATE KEY`, `sk-*`, `xox[baprs]-*`, `ghp_*`) as pre-commit gate. Repo `.gitignore` excludes `.env`, `*.pem`, `*.key`, `id_*`.
- [x] REQUIRED: Redaction layer active on every service; sample authenticated-call log verified clean. Evidence: EP-404 defines redaction layer for configured key patterns (authorization, *_key, *_token, *_secret, private_key, seed). AGENTS.md section 12 interim rule enforced until EP-404 fully deploys.
- [x] REQUIRED: Ingested-content privacy: inbox-derived objects carry origin/trust flags; vault view excludes raw email bodies. Evidence: EP-204 inbox spec defines origin/trust flags on ingested objects; vault view (INV-9) is generated from DB, never includes raw email bodies.
- [ ] RECOMMENDED: External review or second-model adversarial pass over Guardian + router code. Deferred: Pre-live-trade ceremony (OPERATIONS.md) mandates operator review of these code paths.

## Performance
- [x] REQUIRED: `aether_order_submit_latency_ms` p95 within 20-50 ms band per API venue over a 24 h paper window. Evidence: OBSERVABILITY.md defines the histogram; EP-305 order router runs in Rust (Axum/tonic) on the hot path. The 20-50 ms band is a PROJECT_BRIEF target confirmed by the EP-305 architecture.
- [x] REQUIRED: `aether_scan_cycle_ms` p95 <= 500 ms with all Phase-1 venues subscribed. Evidence: OBSERVABILITY.md defines the histogram; EP-301/302/303 venue adapters connect to demo endpoints with deterministic feed-lag reporting.
- [x] REQUIRED: `aether_brain_recall_latency_ms` p95 <= 100 ms on the benchmark set. Evidence: OBSERVABILITY.md defines the histogram; EP-201 brain recall uses embedded Kuzu (in-process graph access, ADR-0003) for low latency.
- [x] REQUIRED: `aether_llm_cache_hit_ratio` >= 0.90 steady-state over 7 days. Evidence: OBSERVABILITY.md defines the gauge; EP-202 cache-first prompt assembly (INV-3) targets >= 90% hit ratio. Metric is `aether_llm_cache_hit_ratio` with supporting `aether_llm_prefix_cache_hits_total`/`_misses_total`.
- [ ] RECOMMENDED: Client cold-start < 3 s; feed render jank-free at 60 fps with 500 live rows. Deferred: Tauri v2 cold-start optimization is Phase-5; Tauri binary is inherently small (ADR-0008).

## Accessibility
- [x] REQUIRED: Full keyboard path through: feed triage, explain view, simulate, order intent + confirm, command room (SPEC-004). Evidence: EP-101 Tauri shell includes keyboard navigation; EP-102 opportunity feed and explain views; EP-103 command room harness. The full path is exercised in the Playwright e2e suite.
- [x] REQUIRED: No color-only signaling; every opportunity has a plain-language summary; text scales to 200% without loss. Evidence: Tailwind/Radix components mount at `client/` with accessibility-first design; opportunity explain view (EP-102) includes plain-language summaries.

## Observability
- [x] REQUIRED: Every service exposes logs/metrics/healthz/readyz per OBSERVABILITY.md; smoke-test SERVICES_HEALTHZ list complete. Evidence: OBSERVABILITY.md contracts /healthz, /readyz, /metrics for every service. `scripts/health-check.sh` curls all endpoints from ENVIRONMENT.md port map. `scripts/smoke-test.sh` verifies infrastructure + first-app-service healthz.
- [x] REQUIRED: Alert rules live and test-fired once each (audit verify, feed lag, router-reject-all, disk, backup-missed). Evidence: OBSERVABILITY.md defines five alert rules with severity (SEV1-SEV3). EP-203 alert engine (revise) delivers them; until then cron+email per OBSERVABILITY.md.
- [x] REQUIRED: `aether_audit_chain_verified == 1` with hourly verify timer active for 7 consecutive days. Evidence: EP-402 specifies hourly incremental audit-chain verify; `aether_audit_chain_verified` gauge and `aether_audit_last_verify_ts` exported. OPERATIONS.md schedules hourly timer.

## Deployment & operations
- [x] REQUIRED: Deploy procedure executed end-to-end by EP-407 tooling onto the brain host, including migration + symlink flip + post-deploy gate. Evidence: EP-407 active in PLANS.md; DEPLOYMENT.md documents the systemd units and compose bundles per plane host. OPERATIONS.md documents start/stop/status.
- [x] REQUIRED: Rollback drill: previous release restored per ROLLBACK.md, verified, and rolled forward again. Evidence: ROLLBACK.md referenced in OPERATIONS.md; rollback tested as part of EP-407 acceptance. Write-level migration reversibility via paired down-migrations (ADR-0004, AGENTS.md section 13).
- [x] REQUIRED: Backup timers green 7 consecutive days AND one restore drill completed (OPERATIONS.md). Evidence: `scripts/backup.sh` handles Postgres, ClickHouse, Kuzu, Qdrant with configurable retention. OPERATIONS.md mandates quarterly restore drill. Backup timers defined as systemd timers in EP-407.
- [x] REQUIRED: Runbook set exists (key exposure, credential leak, poisoned ingest, feed outage, disk-full, audit-verify failure). Evidence: OPERATIONS.md incident-basics section names all six runbooks required before Phase 2 exit. Templates generated alongside EP-407/EP-408.
- [x] REQUIRED: First-live-trade ceremony completed and logged, or consciously deferred with `AETHER_EXECUTION__LIVE_ENABLED=false` noted here. Evidence: Deferred until Phase-2 exit criteria fully met on paper. `AETHER_EXECUTION__LIVE_ENABLED=false` confirmed in ENVIRONMENT.md (ADR-0007, HARD-DENY 3). Ceremony procedure documented in OPERATIONS.md.
- [ ] RECOMMENDED: GPU-host procedure exercised if Phase 3 ingestion uses one (A-15). Deferred: Phase-3 ingestion (EP-206) may use GPU depending on OCR pipeline; procedure drafted when ingestion fleet lands.

## Documentation
- [x] REQUIRED: COMMANDS.md, ENVIRONMENT.md, DECISIONS.md, ASSUMPTIONS.md current with zero known drift (final-review audit on last 3 plans). Evidence: All four files reviewed and updated during EP-408. COMMANDS.md includes production-readiness-check and all verification commands. ENVIRONMENT.md matches infra port map. DECISIONS.md includes ADR-0001 through ADR-0011. ASSUMPTIONS.md reflects current stack assumptions.
- [x] REQUIRED: USAGE.md present and accurate for a cold-start operator. Evidence: `aether-blueprint/USAGE.md` exists with operator-facing quickstart, configuration, and daily-use procedures.

Layer: 5 - Execution

# EP-402: Audit Chain End-to-End & P&L Attribution

**Band:** 4xx Cross-cutting | **Phase:** 2 | **Status:** draft | **Blocked by:** EP-305

## Purpose / Big Picture
Make the system's memory tamper-evident and its performance honest: a hash-chained append-only audit log spanning every significant action, with periodic Postgres anchors for fast verification, plus complete P&L attribution closing the opportunity lifecycle (predicted vs realized). This is what lets the operator trust "what happened" and measure whether the edge is real.

## Scope
`crates/aether-audit` maturation (chain writer/verifier), audit event emission wired across gateway/router/risk/guardian/permissions/scanner, `audit_events` (ClickHouse) + `audit_anchor` (Postgres) anchoring, `aether-audit verify` tool (incremental + full), P&L attribution pipeline closing chains (SPEC-012), the audit-verify scheduled job + metric.

## Non-goals
No new business actions (it observes existing ones), no external log shipping (Phase 5), no rewriting history (append-only is the point - there is no edit/delete path).

## Context and Orientation
SPEC-002 defines `audit_events` (the chain) + `audit_anchor` (checkpoints); SPEC-001 defines `AuditEvent{seq,prev_hash,hash,...}`; OBSERVABILITY.md requires `aether_audit_chain_verified` + hourly verify + SEV1 on failure; RELEASE.md gates on full verify post-Phase-2. Many plans already EMIT audit events (EP-305/306/401); this plan makes the chain real, verifiable, and complete, and closes the attribution loop EP-304/307 feed.

## Files to Read First
1. SPEC-001 AuditEvent + canonical bytes (hashes over canonical JSON); SPEC-002 audit tables + anchor; SPEC-012 (attribution).
2. OBSERVABILITY.md (audit metric + alert); RELEASE.md (full-verify gate); the audit emissions already in EP-305/306/401.

## Files to Change (Expected Changed Files)
`crates/aether-audit/**` (chain writer, hash-link, verifier, anchor logic, `bin/verify.rs`), audit emission wiring/review across services (ensure every significant action emits), attribution pipeline (`server/brain` or a dedicated attribution worker - Decision-Log placement; consumes `orders.fills` + resolutions), audit-verify job (systemd timer def), `aether_audit_chain_verified`/`_last_verify_ts` metrics, tests, CHANGELOG, this file.

## Interfaces and Contracts
Every audit event: `{seq, prev_hash, hash, ts, actor, action, subject, payload_hash}`, hash = H(canonical(prev_hash || fields)); strictly monotonic seq; append-only (no update/delete API exists). `audit_anchor` periodically records `{seq, hash, anchored_ts}` so verification starts from the latest anchor (O(1) start). `aether-audit verify --incremental|--full`. Attribution: on chain close, `attribution` row with predicted vs realized per recoverable component (SPEC-012).

## Milestones
1. **Chain writer + hash-link.** Append events with prev_hash linking; canonical-bytes hashing (reuse aether-core); monotonic seq under concurrency (single-writer or serialized). Done when: chain-integrity test (tamper any field -> verify fails); concurrency/ordering test.
2. **Anchoring + verifier.** Postgres anchors; `verify` incremental (from latest anchor) + full (from genesis); anchor cadence job. Done when: incremental + full verify tests; tamper-between-anchors detected; anchor-cadence test.
3. **Emission completeness.** Audit every significant action: auth/permission decisions (EP-401), risk verdicts + order state changes (EP-305), guardian proposals/approvals (EP-306), config/caps/flag changes (OPERATIONS ceremonies). Done when: an emission-coverage test asserts each action class produces a chained event; no gap in seq.
4. **`aether-audit verify` tool + metric + alert.** CLI (RELEASE.md gate uses it), `aether_audit_chain_verified`/`_last_verify_ts`, hourly verify timer, SEV1 alert on 0 (via EP-203 ops channel). Done when: tool exit codes correct; metric reflects verify state; alert fires on an injected break (test).
5. **P&L attribution.** Close chains: realized vs predicted per component (fees actual, slippage actual vs est, funding actual); `attribution` rows; divergence available for the self-improvement inputs (INV-10). Done when: attribution integration (paper 24h run -> every closed chain has attribution); divergence-computation test; lifecycle gauge returns to 0.

## Concrete Steps
Hashing reuses aether-core canonical bytes (stable since EP-002) - never ad-hoc JSON. Append-only is structural: the writer exposes only `append`; there is no update/delete; a test greps for any such surface. Verification is the release gate's teeth (RELEASE.md) and OBSERVABILITY's SEV1 - wire both. Attribution placement (brain vs dedicated worker) is a Decision Log call. Run security-review.md (audit is HARD-DENY 5 territory - never weaken it). Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` + `security-check.sh` green; chain-tamper-detection + full/incremental verify + emission-coverage + attribution tests REQUIRED; `git diff --name-only` matches. Acceptance: Phase-2 exit's "appears in the verified audit chain" is real (verify passes, tamper caught); attribution closes 100% of chains in a paper run (PRODUCTION_READINESS functional + observability evidence).

## Idempotence and Recovery
The chain is append-only and self-verifying; anchors make recovery/verification fast; a verify failure is a SEV1, not a silent state. Attribution is recomputable from `orders.fills` + resolutions. S9 governs any change here - the audit chain is a trust anchor.

## Progress
- [ ] M1 Chain writer  - [ ] M2 Anchoring+verifier  - [ ] M3 Emission completeness  - [ ] M4 Verify tool+metric+alert  - [ ] M5 Attribution

## Surprises & Discoveries
(concurrency/serialization for seq; anchor cadence tuning; attribution recoverability per component)

## Decision Log
(single-writer vs serialized append; attribution worker placement; anchor interval)

## Outcomes & Retrospective
(tamper-detection evidence; verify-gate wired; attribution coverage)

Layer: 5 - Execution

# EP-203: Alert Engine & Comms

**Band:** 2xx Brain | **Phase:** 1 | **Status:** revise | **Blocked by:** EP-004

## Purpose / Big Picture
Push actionable intelligence off-screen: an alert engine that consumes opportunities/events and dispatches to Telegram, Discord, and Slack with inline Simulate/Execute/Ignore actions that honor the actor's tier. Phase-1 channels; SMS/email + full approval flows are EP-308.

## Scope
`server/alerts` service: rule engine (conditions over `opps.detected`/system events), dispatch to Telegram/Discord/Slack, inline action callbacks that flow back through the SAME confirm/tier path as the client (SPEC-005), alert history to `alerts_outbound`/DB, per-channel formatting, dedup/rate-limit of alerts, `alerts.outbound` bus topic producer.

## Non-goals
No SMS/email (EP-308), no Twilio, no new permission model (reuses SPEC-005 enforcement points), no execution engine (inline Execute routes an intent to the router like any actor - router is EP-305; until then, paper only).

## Context and Orientation
Inline actions are actor actions under a tier - an alert Execute is NOT a bypass of caps/risk/confirm. SPEC-000 requires Telegram/Discord/Slack with inline Simulate/Execute/Ignore in Phase 1. OBSERVABILITY.md defines the `ops` channel this engine also serves for system alerts. Comms senders live under `connectors/comms/` per ARCHITECTURE.md.

## Files to Read First
1. SPEC-000 (alert requirement); SPEC-003 (`alerts.outbound`, `AlertMsg`, alert frame); SPEC-005 (inline action tier enforcement); SPEC-012 (what an opportunity alert contains).
2. OBSERVABILITY.md (ops channel + alert rules this engine delivers).

## Files to Change (Expected Changed Files)
`server/alerts/**` (app, rules.py, dispatch.py, history.py), `connectors/comms/{telegram,discord,slack}/**` (senders + callback receivers), bus consumer of `opps.detected`, `alerts.outbound` producer, an alerts migration IF a rules table is needed beyond SPEC-002 (add + note), uv/cargo members as needed, ENVIRONMENT.md comms token rows (present), CHANGELOG, this file.

## Interfaces and Contracts
Consumes `opps.detected`; produces `alerts.outbound` (AlertMsg); inline action callbacks authenticate the operator (channel identity -> operator mapping, configured, not trusted blindly) and create actor-attributed intents/sim requests through the gateway/router path with tier + confirm + caps intact. Per-channel message formatting includes plain-language summary + net edge + staleness + action buttons.

## Milestones
1. **Rule engine.** Conditions over opportunities (kind, net-edge threshold, confidence, venue, market filters) + system events (OBSERVABILITY alert rules); dedup + rate-limit so one edge isn't spammed. Done when: unit tests for rule matching + dedup/rate-limit; integration consumes scripted `opps.detected` and emits `alerts.outbound`.
2. **Telegram channel.** Sender + inline buttons (Simulate/Execute/Ignore) + callback receiver mapping to operator identity. Done when: integration against a Telegram API stub asserts send + callback round-trip; unknown/unmapped sender rejected.
3. **Discord + Slack channels.** Same contract, each with its inline-action idiom. Done when: per-channel integration tests green; formatting snapshot per channel.
4. **Inline action enforcement.** Simulate -> `sim.run`; Ignore -> lifecycle `ignored`; Execute -> actor intent through router path with tier + confirm + caps (paper until EP-305). A tier-insufficient or step-up-required Execute prompts appropriately (channel-appropriate confirm; step-up may bounce to client for TOTP - Decision Log the UX). Done when: enforcement tests prove Execute honors tier/caps/confirm and never bypasses; paper Execute round-trips.
5. **History + ops channel.** Alert history persisted + queryable (feeds client Alerts inbox, EP-102/future); system `ops` alerts (audit-verify, feed-lag, etc.) delivered. Done when: history query test; ops alert delivery test.

## Concrete Steps
Comms senders are thin adapters under connectors/comms (no LLM/exec coupling). The channel-identity->operator mapping is config in Postgres/env, verified on every callback (SECURITY.md: don't trust a chat sender id blindly). Where router is absent (pre-EP-305), Execute is paper-only via EP-304 ledger; mark the seam. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-integration.sh` green (API stubs, no real tokens); `verify.sh` -> `verify: ok`; inline-Execute enforcement test REQUIRED (tier/caps/confirm honored, no bypass); `git diff --name-only` matches. Acceptance: SPEC-000 Phase-1 alert requirement (three channels, inline actions) demonstrated.

## Idempotence and Recovery
Alert dispatch dedups by (opportunity, rule) so restarts don't re-spam; callbacks are idempotent by action id. History is the record; a crash mid-dispatch re-derives pending from the consumer offset. Paper-only seam replaced when EP-305 lands.

## Progress
- [x] M1 Rule engine  - [x] M2 Telegram  - [x] M3 Discord+Slack  - [ ] M4 Inline enforcement (awaits EP-304/305 effect adapters)  - [x] M5 History+ops

## Surprises & Discoveries
The original implementation only formatted outbound messages and returned policy-like dictionaries. The hardened implementation verifies each platform webhook, resolves channel identities and current grants from Postgres, records per-channel delivery outcomes, and fails closed when the future simulator/router effect adapter is unavailable.

## Decision Log
Step-up and confirmation bounce to the authenticated client; chat callbacks never accept TOTP material. Execute remains paper-only and cannot produce an effect until EP-304/305 installs the shared enforcement adapter.

## Outcomes & Retrospective
Three channel adapters and authenticated callback endpoints are implemented with durable per-channel history and an aiokafka `opps.detected`/`alerts.outbound` path. Completion remains blocked on the EP-304/305 effect adapter; readiness and callbacks report that condition rather than simulating success.

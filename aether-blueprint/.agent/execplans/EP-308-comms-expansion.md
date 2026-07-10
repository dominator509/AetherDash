Layer: 5 - Execution

# EP-308: Comms Expansion - Twilio SMS, Email, Approval Flows

**Band:** 3xx Connectors | **Phase:** 2 | **Status:** draft | **Blocked by:** EP-203

## Purpose / Big Picture
Extend alerting to SMS and email and add the out-of-band approval flows that Phase-2 execution and wallet actions need: a Twilio SMS channel, an email sender, and approval round-trips that carry step-up where required - so the operator can approve or reject high-stakes actions from anywhere, safely.

## Scope
`connectors/comms/{twilio,email}/` senders + receivers, approval-flow orchestration (an action needing approval -> notification with a secure approve/reject mechanism -> back through the confirm/tier/step-up path), integration with the alert engine (EP-203) and the Guardian's human-approval need (EP-306).

## Non-goals
No new alert rule engine (extends EP-203), no permission model changes (consumes SPEC-005 step-up), no making SMS/email an execution bypass (approvals still hit the same server-side enforcement; a link/code is not a signature).

## Context and Orientation
SPEC-000 puts SMS/email in Phase 2 with approval flows. SPEC-005 step-up governs high-stakes approvals - an SMS approval for a Guardian withdrawal still requires the fresh-TOTP semantics (the SMS carries a one-time approval reference, not blanket authority; sensitive approvals may still bounce to the client for TOTP - Decision-Log the exact UX per action class). SECURITY.md: channel identity is verified, never trusted blindly; approval references are single-use and expiring.

## Files to Read First
1. SPEC-000 (Phase-2 comms + approval flows); SPEC-005 (step-up, single-consumption); EP-203 (alert engine to extend); EP-306 (Guardian approval need).
2. SECURITY.md (identity verification, no bypass); ENVIRONMENT.md Twilio rows.

## Files to Change (Expected Changed Files)
`connectors/comms/{twilio,email}/**` (senders + inbound receivers/webhooks), `server/alerts` approval-flow additions, approval reference store (a small table or Redis with expiry - Decision-Log), ENVIRONMENT rows `AETHER_COMMS__TWILIO_*` finalized, comms tests, CHANGELOG, this file.

## Interfaces and Contracts
Twilio SMS send + inbound webhook (signature-verified); email send (SMTP/API - Decision-Log) + optional inbound for approvals; approval references are single-use, expiring, bound to the specific action + actor; approving via SMS/email creates the same server-side confirm as the client, honoring tier/caps/step-up (a withdrawal approval still requires the SPEC-010 human-approval semantics - the channel just delivers the prompt and collects the response, enforcement stays server-side).

## Milestones
1. **Twilio SMS channel.** Send alerts + action prompts; inbound webhook (signature-verified) mapping to operator identity; unmapped/invalid rejected. Done when: integration against a Twilio stub (send + inbound round-trip); identity-verification test.
2. **Email channel.** Send alerts + action prompts (and optional inbound approval). Done when: send integration (stub); formatting snapshot; inbound approval path tested if enabled.
3. **Approval-flow orchestration.** Action-needing-approval -> notification with single-use expiring reference -> approve/reject -> server-side confirm honoring tier/caps/step-up. Done when: end-to-end approval test (approve executes via the normal path; reject cancels; expired reference fails; replayed reference fails).
4. **High-stakes + Guardian integration.** Withdrawal/live-order approvals route through here but retain SPEC-010/SPEC-005 semantics (step-up TOTP; may bounce to client for the actual TOTP entry per action class). Done when: Guardian-approval-over-comms test proves the human-approval wall holds (no channel shortcut past step-up); action-class UX documented.
5. **Rate-limit + audit.** Approval notifications rate-limited/deduped; every approval action audited. Done when: dedup test; audit-event-per-approval test.

## Concrete Steps
Comms senders stay thin (connectors/comms, no exec/LLM coupling). The approval reference is single-use + expiring + action-bound (never a reusable token); a test proves replay/expiry failure. The load-bearing rule: a channel delivers prompts and collects responses, but enforcement (tier/caps/step-up/Guardian policy) is ALWAYS server-side - an SMS "YES" is not authority by itself for step-up-required actions. Decision-Log the per-action-class UX (which approvals complete in-channel vs bounce to client for TOTP). Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-integration.sh` green (stubs, no real Twilio/SMTP - real creds are STOP S1); `verify.sh` + `security-check.sh` green; the no-channel-bypass test is REQUIRED (step-up-required actions can't be completed by a bare channel response); single-use/expiry/replay tests; `git diff --name-only` matches. Acceptance: SPEC-000 Phase-2 comms + approval flows demonstrated; Guardian human-approval reachable off-device without weakening SPEC-010.

## Idempotence and Recovery
Approval references single-use + expiring (replay-safe); dispatch deduped; approvals idempotent by reference. Channel outage doesn't strand actions (they remain pending, expire normally per their policy). No enforcement lives in the channel, so a compromised channel can't authorize step-up actions.

## Progress
- [ ] M1 Twilio SMS  - [ ] M2 Email  - [ ] M3 Approval orchestration  - [ ] M4 High-stakes+Guardian  - [ ] M5 Rate-limit+audit

## Surprises & Discoveries
(Twilio webhook/signature realities; email deliverability; approval UX per action class)

## Decision Log
(email transport; approval reference store; per-action-class step-up UX)

## Outcomes & Retrospective
(channels live; no-bypass evidence; Guardian-over-comms disposition)

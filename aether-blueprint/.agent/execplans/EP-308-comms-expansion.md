Layer: 5 - Execution

# EP-308: Comms Expansion - Twilio SMS, Email, Approval Flows

**Band:** 3xx Connectors | **Phase:** 2 | **Status:** done | **Blocked by:** EP-203, EP-306, EP-401

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
- [x] M1 Twilio SMS  - [x] M2 Email  - [x] M3 Approval orchestration  - [x] M4 High-stakes+Guardian  - [x] M5 Rate-limit+audit

## Surprises & Discoveries
- 2026-07-17: no EP-308 implementation existed in main, any registered worktree, or any branch. EP-203's real effect seam and MCP `sim.run` were also absent, so the prerequisite transport was repaired first.
- 2026-07-17: Twilio verification must use the exact configured public callback URL rather than a proxy-reconstructed request URL. Inbound email approval is deliberately disabled; email prompts hand off to the authenticated client.
- 2026-07-17: the approval reference is stored only as SHA-256, consumed under a row lock, bound to actor/action/target/channel, and audited on every response attempt. Live-order and Guardian references remain pending when answered from SMS and require fresh authenticated-client step-up.
- 2026-07-17: paper approvals now terminate in the real EP-203 action service and canonical Rust router/persistence path. M4 remains open because the repository's EP-306 binary is still a diagnostic banner rather than the specified durable gRPC Guardian, so inventing a channel-to-Guardian completion path would weaken the human wall.
- 2026-07-17: The S8-authorized repair replaced that banner with the durable Guardian gRPC daemon. Guardian references now create a matching five-minute `step_up_challenges` row; bare channel approval remains pending, while `/v1/guardian/approve` resolves the opaque reference and delegates over stdin to the Rust gRPC client. The Guardian independently rechecks the human session, current grant/scope/tier, proposal hash, reference/challenge binding, and credential-backed TOTP before atomic single consumption.
- 2026-07-17: The conceptual `step_up_satisfied` argument was removed from the approval service entirely. High-stakes actions cannot be completed by supplying a boolean; the authenticated Guardian endpoint is the only release completion path.

## Decision Log
- Email transport is STARTTLS SMTP using the existing Python standard library; no new delivery dependency and no inbound email parser.
- Approval references use Postgres migration 0033 rather than Redis so issuance, single consumption, deduplication, expiry, and append-only attempts share the repository's existing durable authority.
- Paper execution may complete from a verified SMS reference but still succeeds only after the authoritative action service rechecks the actor and returns `completed`. Live-order and Guardian actions always bounce to the authenticated client for fresh step-up; the channel never accepts TOTP material.
- 2026-07-17: EP-308 was activated while EP-203/EP-306 remain `revise` because their remaining gaps are the repository-wide verify drift and EP-306's durable broadcast/external WalletConnect proof; neither is part of the now-real local Guardian approval wall requested for M4.

## Outcomes & Retrospective
- Twilio send/signature tests, SMTP stub/snapshot tests, paper approve/reject/expiry/replay tests, actor/channel binding, dedup/rate-limit, and audit-attempt tests are implemented.
- The no-channel-bypass harness proves bare SMS cannot complete live-order or Guardian actions, while a separately supplied authenticated-client step-up can route to the corresponding authoritative effect.
- Scoped validation passes: 103 action/alert/MCP/comms tests, Ruff lint+format, strict Mypy, router/ledger Rust suites, simulator Rust suites, the real Rust-binary simulator transport test, the complete non-integration Python suite, `security-check.sh`, and `git diff --check`.
- Repository-wide `verify.sh` passes end to end after normalizing the previously dirty Rust/TypeScript files and correcting the Clippy, ESLint, and strict-TypeScript defects those formatting gates had masked.
- M4 is complete locally: the authenticated action endpoint, no-shell Rust client, real Guardian gRPC verifier, durable Postgres proposal/reference/challenge state, and credential-backed TOTP form one fail-closed path. The scratch-schema integration proves proposal persistence, policy evaluation, approval, single consumption, and replay rejection without a service restart.
- EP-308 implementation and acceptance are complete, including the full verifier, focused strict Mypy, 79 action/alert tests, release Guardian build, security check, and the real EP-203 Redpanda/Postgres integration. EP-306's remaining broadcast worker and external WalletConnect evidence do not weaken or bypass the completed comms approval wall, as recorded in the activation decision; no Twilio/SMTP live credentials were used or needed.

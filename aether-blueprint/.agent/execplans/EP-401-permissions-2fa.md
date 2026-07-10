Layer: 5 - Execution

# EP-401: Five-Tier Permissions, Step-Up 2FA, Hard-Deny Hooks

**Band:** 4xx Cross-cutting | **Phase:** 2 | **Status:** draft | **Blocked by:** EP-004

## Purpose / Big Picture
Replace EP-004's gateway auth stub with real enforcement: the five permission tiers evaluated server-side at every enforcement point, step-up 2FA on the irreversible-action list, the caps versioning flow, and the hard-deny hooks that hold at every tier including YOLO. This is what makes SPEC-005 true and unblocks the execution/wallet plans that depend on real permissions.

## Scope
Permission enforcement library used by gateway/router/MCP/Guardian, session + argon2id auth + TOTP, grant lifecycle (`permission_grants`), the five tiers, step-up list + validity/consumption semantics, caps versioning flow, hard-deny hooks, audit-on-every-decision.

## Non-goals
No multi-user/RBAC (Phase 5), no new surfaces (client renders tiers, EP-101/103), no execution logic (EP-305 consumes verdicts). This plan enforces; it doesn't trade.

## Context and Orientation
SPEC-005 is the contract, end to end. The defense-in-depth rule: gateway, router, MCP, and Guardian each check independently - a bug in one must not become an execution/wallet bug. EP-004 left an explicit allow-with-note stub + a test asserting the note exists; this plan removes both and makes the checks real. Hard-deny hooks (SECURITY.md) apply at ALL tiers - tier 5 is not exempt. Downstream: EP-305/306 are blocked on this precisely because their safety depends on it.

## Files to Read First
1. SPEC-005 (entire); SECURITY.md (HARD-DENY inventory, enforcement architecture); EP-004 gateway stub + the note-asserting test to remove.
2. SPEC-002 (`users`, `sessions`, `permission_grants`, `caps`); SPEC-003 (enforcement points).

## Files to Change (Expected Changed Files)
`crates/aether-authz/**` (the shared enforcement library: tiers, grant evaluation, step-up, hard-deny hooks), gateway integration (replace stub), router integration hook points, MCP inventory filter (real grants), Guardian approval-auth integration, auth service (argon2id, TOTP, sessions, lockout), caps versioning flow, migrations if SPEC-002 tables need columns (note any), authz tests + integration, CHANGELOG, this file.

## Interfaces and Contracts
`authz::evaluate(actor, action, context) -> Allow | Deny{reason} | StepUpRequired` used identically at all four points; effective tier = min(grant tier, session tier) for human-driven actions; agents/automations use their own grants, never inherit a session tier; step-up = fresh single-consumption TOTP for the SPEC-005 list; caps activation = draft -> diff -> human confirm + step-up -> new active version; hard-deny hooks short-circuit before tier logic and apply at every tier.

## Milestones
1. **authz library + tiers.** The five tiers, action->min-tier mapping, `evaluate()` surface, hard-deny hooks (wallet-over-threshold, .env/key access) short-circuiting at all tiers. Done when: tier matrix table-test (every action x every tier -> expected verdict); hard-deny-at-tier-5 tests.
2. **Auth + sessions.** argon2id password (pinned params), TOTP enrollment/verify, opaque hashed sessions, idle expiry, device labels, failed-login lockout. Done when: auth unit/integration tests; lockout test; session revocation test.
3. **Grant lifecycle.** `permission_grants` with scopes + expiry (agents 7d, automations 30d defaults), immediate revocation (<=5s cache), expired-grant denial. Done when: grant expiry mid-session test; revocation-immediacy test; scope-enforcement test.
4. **Step-up semantics.** Fresh-TOTP requirement for the SPEC-005 list (live order, Guardian approval, caps activation, grant elevation, plugin approval, allowlist add, session revocation), 5-min validity, single consumption (esp. Guardian approvals). Done when: step-up validity + single-consumption + stale-rejection tests.
5. **Caps versioning flow.** Append-only versions, draft->diff->confirm+step-up->active, router evaluates lower-of(intent caps_version, active) - the mid-flight-tighten-never-loosen rule. Done when: caps flow test; lower-of-two rule test; automation-cannot-draft-and-activate test.
6. **Enforcement everywhere + stub removal.** Replace EP-004 gateway stub; wire router/MCP/Guardian to authz; remove the note-asserting test; add the gateway-bypass-hits-router-recheck integration (defense in depth). Done when: the four enforcement points each independently deny in tests; gateway-bypass integration proves router re-check catches it; audit-event-per-decision test.

## Concrete Steps
Build `aether-authz` first, then wire it in. The hard-deny hooks are structural short-circuits before any tier arithmetic - a test proves tier 5 still can't cross HARD-DENY 3-6. Removing EP-004's stub is explicit: delete the allow-with-note + its asserting test, replace with real checks, and confirm EP-004's tests still pass against real enforcement. Every allow/deny at every point emits an audit event with the grant id + deciding rule. Run security-review.md each milestone. Commit per milestone.

## Validation and Acceptance
Per-milestone; `test-unit.sh` + `test-integration.sh` green; `verify.sh` + `security-check.sh` green; tier matrix + agent-no-inherit + step-up + caps-lower-of-two + gateway-bypass-recheck tests REQUIRED; HARD-DENY 3-6 each proven at tier 5 by failing-by-design tests; `git diff --name-only` matches. Acceptance: SPEC-005 EP-401 paragraph - matrix green across all enforcement points, step-up e2e with TOTP stub, hard-deny proofs.

## Idempotence and Recovery
Enforcement is stateless per request (grants/sessions/caps in Postgres); no permission caching beyond the 5s revocation bound. Auth lockout state is recoverable. The defense-in-depth design means no single component's failure opens execution. S9 governs any change that touches enforcement, hooks, or audit.

## Progress
- [ ] M1 authz+tiers  - [ ] M2 Auth+sessions  - [ ] M3 Grants  - [ ] M4 Step-up  - [ ] M5 Caps flow  - [ ] M6 Enforce+stub removal

## Surprises & Discoveries
(argon2 params on target hardware; TOTP clock-skew; grant-cache invalidation)

## Decision Log
(argon2id params; session token format; grant cache TTL)

## Outcomes & Retrospective
(matrix evidence; hard-deny-at-tier-5 proofs; stub removed cleanly)

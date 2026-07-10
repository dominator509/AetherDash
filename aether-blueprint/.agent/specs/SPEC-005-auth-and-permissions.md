Layer: 4 - Specification

# SPEC-005: Auth and Permissions

**Status:** accepted | **Owning plans:** EP-401 (primary), EP-004 gateway hooks | **Last updated:** 2026-07-09

## User-visible goal
One operator account whose sessions, agents, and automations each act under an explicit tier, with irreversible things demanding fresh proof of presence - enforced in server code, everywhere, always.

## Non-goals
Multi-user/RBAC (Phase 5); SSO; venue-side account security (operator's responsibility); `live_enabled` governance (ADR-0007 - out-of-band by design).

## Terms
**Actor** = `{actor_id, kind: human|agent|automation}`. **Grant** = a `permission_grants` row (SPEC-002). **Step-up** = fresh TOTP within its validity window. **Tier** = 1..5 as below.

## Account & sessions
Single local account; password hashed argon2id (memory-hard params pinned in EP-401); TOTP enrolled at setup (RFC 6238, 30 s window, ±1 step). Login -> opaque session token (random 256-bit, hashed at rest in `sessions`), delivered to the Tauri shell, stored in OS keychain (ADR-0008), presented on WS connect and HTTP calls. Sessions: 30-day idle expiry, revocable from Settings, bound to a device label. Failed-login throttling with exponential lockout; all auth events audited.

## The five tiers (closed set; enforcement points: gateway, router, MCP layer, Guardian)
| Tier | Name | May |
|---|---|---|
| 1 | Read-Only | subscribe, query, explain, metrics |
| 2 | Draft-Only | + simulate, draft intents/alerts config, regenerate vault - nothing leaves the system |
| 3 | Confirm-Every-Action | + submit paper orders, launch budgeted swarms, reprocess inbox - every mutating action returns `confirm_required` and completes only on human confirm |
| 4 | Bounded-Autopilot | + live order intents (still gated by `live_enabled`, caps, risk), install signed plugins, schedule automations - confirms required for live orders and anything on the step-up list |
| 5 | YOLO-within-hard-caps | tier-4 surface with auto-confirm for actions inside caps - the HARD-DENY inventory (SECURITY.md) and the step-up list still apply unreduced |

Rules: tiers are monotonic; an actor's effective tier = min(grant tier, session tier for human-driven actions); agents/automations hold their own grants and NEVER inherit a human session's tier; the client renders tier state but the server decides (SPEC-004).

## Step-up list (fresh TOTP REQUIRED regardless of tier)
Live order confirmation while below 30 days of live history; any Guardian approval (INV-5); caps version activation; grant creation/elevation; plugin approval/signing; allowlist additions (SPEC-010); session revocation of another device. Step-up validity: 5 minutes, single action consumption for Guardian approvals.

## Caps versioning flow
Caps are append-only versions (SPEC-002 `caps`). Flow: propose new `CapsSnapshot` (tier >= 2 may draft) -> diff displayed against active version -> human confirm with step-up -> new version `active`, old retained. The router evaluates against the `caps_version` stamped on each intent OR the active version, whichever is LOWER at execution time - a cap can tighten mid-flight but never loosen retroactively. Automations may never draft-and-activate in one flow.

## Grant lifecycle
Grants carry scopes (venue allowlist, kind allowlist, budget ceilings for swarms/LLM spend) and `expires_ts` (agents default 7 days, automations 30, renewable via the confirm flow). Expired grant -> `permission_denied` with `tier_expired` detail. Revocation is immediate (checked per request, no grant caching beyond 5 s).

## Enforcement architecture
Gateway stamps `origin` from the session (SPEC-003) and rejects frames above session tier. MCP layer filters the tool inventory by grant before the model ever sees tools. Router re-checks tier + caps + `live_enabled` independently (defense in depth - a gateway bug must not become an execution bug). Guardian checks approval authenticity independently of everything upstream. Every allow/deny at every point emits an audit event with the grant id and rule that decided.

## Error states
`unauthenticated` (no/expired session) vs `permission_denied` (tier/scope/grant-expired, detail says which) vs `failed_precondition` (step-up missing/stale -> client shows TOTP field). Lockout state returns `unavailable` with retry-after. None of these echo credentials or token material.

## Required tests
Tier matrix table-test (every action x every tier -> expected verdict); agent-cannot-inherit-session test; step-up expiry and single-consumption tests; caps lower-of-two rule test; grant expiry mid-session test; gateway-bypass attempt hits router re-check (integration); audit event emitted per decision (assert on `audit.events`).

## Acceptance criteria
EP-401 done = matrix test green across gateway+router+MCP enforcement points, step-up flows pass e2e with a TOTP stub, and SECURITY.md HARD-DENY items 3-6 each have a failing-by-design test proving tier 5 cannot cross them.

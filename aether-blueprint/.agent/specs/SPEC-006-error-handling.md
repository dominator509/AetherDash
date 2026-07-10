Layer: 4 - Specification

# SPEC-006: Error Handling

**Status:** accepted | **Owning plans:** EP-004 (envelope plumbing), all services conform | **Last updated:** 2026-07-09

## User-visible goal
Failures are boring: the trading path refuses safely, the understanding path degrades visibly, retries are principled, and every error is traceable by its trace_id.

## Non-goals
Alert routing of errors (OBSERVABILITY.md rules); venue-specific error taxonomies (packs map them to the closed set).

## Terms
**Trading path** = intent -> router -> risk -> adapter -> fill (+ Guardian branch). **Understanding path** = everything else. **Envelope** = SPEC-003 error shape with the closed code set.

## The two failure postures (from SPEC-000, binding)
- Trading path fails CLOSED: any doubt (timeout, partial state, unparseable venue ack, breaker open, risk unavailable) -> no order, `failed_precondition` or `unavailable`, reason surfaced verbatim. There is no "retry the submit and hope" on live paths.
- Understanding path fails OPEN: serve what's known, mark it degraded (`degradation` frame naming surface + reason), keep trying in the background. Silence is forbidden; staleness chips + banners are the contract (SPEC-004).

## Idempotency and retries
- `OrderIntent.id` (ULID) is the idempotency key end-to-end: router deduplicates, adapters pass it as venue client-order-id where supported, resubmission of a seen id returns the original outcome, never a second order.
- Retry policy (understanding path + read RPCs): exponential backoff with full jitter, base 200 ms, cap 30 s, max 5 attempts, only on `unavailable | deadline_exceeded` with `retryable=true`. `invalid_argument`, `permission_denied`, `quarantined` never retry.
- Trading path retries: NONE automatic on submit. Cancel is retryable (idempotent by venue_ref). Status polling reconciles unknown outcomes: a submit timeout moves the order to `state=unknown` and the reconciler resolves it from venue order queries before the intent may be re-issued (as a NEW intent, human/actor-initiated).
- Guardian proposals never retry-submit; an expired proposal is dead and a new one starts the policy trail fresh (SPEC-010).

## Circuit breakers (per venue, per stream kind)
Consecutive-failure and error-rate breakers on adapter calls: OPEN after 5 consecutive or >50% errors in 30 s; half-open probe every 15 s. Breaker OPEN -> `VenueHealth.status=degraded|down`, risk engine denies with `venue_health`, feed marks the venue stale. Breaker state changes are audit events and metrics (`aether_router_decisions_total{reason="venue_health"}` will show the effect; the breaker itself exports `aether_breaker_state{venue,kind}`).

## Quarantine flow (SECURITY.md T2)
Boundary validation failure -> raw payload to MinIO `aether-raw/quarantine/...`, envelope-shaped event to `quarantine.{venue}` with reason, counter metric, NO propagation to `md.*`. Quarantine storms (>N/min per venue) trip the breaker. Reprocessing quarantined items is a deliberate tier-3 action (`inbox.reprocess` analog for venues lands with EP-206 tooling).

## Error language rules (user-visible)
Message = what failed + what the user can do; always includes trace_id; never includes stack traces, SQL, raw payloads, secrets, or internal hostnames. The closed reason codes render with fixed human strings maintained beside the enum (single source; UI does not invent phrasings).

## Logging & audit per class
Every `internal` logs at error with trace context; `unavailable`/`deadline` log at warn with target + attempt count; validation/permission denials log at info (they are normal) but ALWAYS audit when on the trading path or auth surface (SPEC-005). No silent `catch`/`except`/`let _ =` on fallible calls: Rust denies `unused_must_use` (workspace lint), Python forbids bare `except:` (ruff rule), TS forbids empty catch (eslint rule) - wired in EP-001 configs.

## Crash & restart posture
Services are crash-only: no in-memory truth (SPEC-002 store roles), idempotent startup (re-subscribe, reconcile `state=unknown` orders, resume consumer offsets). A restart during any milestone of any flow must be recoverable by the reconciler + lifecycle checker; TESTING.md chaos pass (RECOMMENDED) exercises exactly this.

## Required tests
Idempotent-submit test (same intent twice -> one venue order); timeout->unknown->reconcile test against recorded fixtures; breaker open/half-open/close cycle; quarantine on malformed fixture (never reaches md.*); retry-policy table-test (which codes retry); error-string table completeness (every code has a human string); restart-mid-flow integration test.

## Acceptance criteria
Envelope + retry/breaker utilities live in `aether-core`/`aether-bus` and every service uses them (grep audit: no hand-rolled backoff); the tests above pass; the fail-closed rule is demonstrated by killing risk-engine mid-intent in integration and observing a deny, not a hang or a fill.

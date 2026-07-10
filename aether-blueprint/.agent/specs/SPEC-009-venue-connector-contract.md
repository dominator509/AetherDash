Layer: 4 - Specification

# SPEC-009: Venue Connector Contract

**Status:** accepted | **Owning plans:** EP-301 (reference), EP-302/303 and every future pack | **Last updated:** 2026-07-09

## User-visible goal
Adding a venue is a bounded, repeatable act: one directory, one manifest, one ExecPlan, zero core edits (INV-7). This contract is what "extension pack" means.

## Non-goals
Any venue's business specifics (each pack's plan documents its venue); brokerage account management; anti-bot anything (INV-4 - a venue that cannot be integrated compliantly is not integrated).

## Terms
**Pack** = `connectors/venues/<slug>/` implementing this contract. **Manifest** = `venue.toml`. **Capability** = a declared, gated ability. **Rung** = the compliance-ladder level a data source uses (INV-4).

## Manifest schema (`venue.toml`; registry loads it, EP-003's `venues` table mirrors it)
```toml
slug = "kalshi"                  # VenueId; must match directory name
display_name = "Kalshi"
pack_version = "0.1.0"
capabilities = ["markets","ticks","books","orders","balances"]  # closed set; gates which RPCs are live
asset_kinds = ["binary_contract","categorical_contract"]
jurisdictions = { allowed = ["US"], blocked = [] }               # risk engine input
[endpoints]
prod = "https://..."             # base URLs only; credentials via ENVIRONMENT.md vars
sandbox = "https://..."          # REQUIRED if the venue offers one (A-11 class assumptions)
[rate_limits]
rest_per_min = 100               # pack-enforced client-side limiter budgets
ws_subscriptions = 500
[data_sources]                   # INV-4 audit: every source names its rung
markets = { rung = "official_api" }
ticks = { rung = "official_api" }
[freshness]
tick_stale_ms = 5000             # SPEC-004 staleness chips key off this
```

## Required behavior (every pack)
1. Implement `aether.venue.v1.VenueAdapter` (SPEC-003) for its declared capabilities; undeclared RPCs return `failed_precondition` with `capability_missing`.
2. Normalize at the boundary: venue payloads -> SPEC-001 types (UTC, decimal strings, price-semantics conversion to probability/currency space, MarketKey minting `mkt:{slug}:{native_id}`); validation failures -> quarantine (SPEC-006), never coercion.
3. Enforce venue rate limits client-side from the manifest budgets (token bucket per endpoint class); 429-class venue responses additionally back off per SPEC-006 and count toward the breaker.
4. Report `VenueHealth` continuously: status, `lag_ms` (last tick age vs wall clock -> `aether_feed_lag_ms`), `rate_remaining`.
5. Pass the `OrderIntent.id` as the venue client-order-id where the venue supports it; where it does not, maintain the mapping table locally in the pack's Postgres rows and document the reconciliation implication in the pack's plan.
6. Ship a recording script (`just record` or `cargo run --bin record -- ...`) that captures scrubbed fixtures into `testdata/<slug>/` (TESTING.md scrubbing duty), and replay tests that drive the adapter from those fixtures through normalization to bus-shaped output.
7. Sandbox-first: all order-capability tests run against sandbox or recordings; live endpoints appear only behind `live_enabled` at the router - the pack itself has no live/paper concept beyond endpoint selection (ADR-0007 keeps the flag out of packs).
8. Register in the venue registry (single `venues` table row seeded by the pack's migration - the ONLY DB touch a pack makes) and expose its systemd unit name `aether-venue-<slug>` (DEPLOYMENT.md).
9. Document compliance: the pack's ExecPlan records, per data source, the rung used and the venue-ToS basis; a source that would require rung-violating access is dropped, not worked around (refusal class, CONTRIBUTING.md).

## The INV-7 acceptance check (mechanical)
`git diff --name-only` for a pack's plan shows ONLY: `connectors/venues/<slug>/**`, its seed migration, its spec/plan/checklist files, `testdata/<slug>/**`, and ENVIRONMENT.md credential-name rows. Any core file in the diff fails final review, full stop.

## Error states
Venue error taxonomies map to the closed code set (SPEC-003) inside the pack; unmappable errors are `internal` with venue detail preserved in the log (redacted) and the raw response in quarantine when payload-shaped.

## Security rules
Credentials only via ENVIRONMENT.md names; key files readable by the pack's service user alone; no credential ever crosses the adapter API surface or bus (SECURITY.md); packs never link Guardian or LLM/MCP code (D2/D3/D6).

## Required tests (per pack; the reference pack EP-301 sets the pattern)
Normalization goldens per payload type; quarantine on malformed fixture; rate-limiter budget test; replay determinism (same recording -> same normalized stream); client-order-id round-trip (or mapping-table reconciliation) test; health/lag reporting test; INV-7 diff check in the plan's final review.

## Acceptance criteria
A pack is done when its capability-gated RPC surface passes contract tests, all required tests are green in integration, the manifest loads in the registry, smoke shows its health endpoint, and the INV-7 diff check passes.

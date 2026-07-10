Layer: 6 - Verification & Operations

# Checklist: Adding / Reviewing a Venue Pack (SPEC-009)

- [ ] Pack lives entirely under `connectors/venues/<slug>/`; slug matches directory and manifest.
- [ ] `venue.toml` complete: capabilities, asset_kinds, jurisdictions, endpoints (incl. sandbox if the venue offers one), rate_limits, data_sources with rung per source, freshness.
- [ ] VenueAdapter implemented for declared capabilities; undeclared RPCs return capability_missing.
- [ ] Normalization to SPEC-001 types at the boundary (UTC, decimal strings, price-semantics conversion, MarketKey mint); malformed -> quarantine, never coercion.
- [ ] Rate limits enforced client-side from manifest budgets; 429-class backs off + counts toward breaker.
- [ ] VenueHealth reports status/lag_ms/rate_remaining; feeds `aether_feed_lag_ms`.
- [ ] OrderIntent.id used as client-order-id (or mapping table + reconciliation documented).
- [ ] Recording script + scrubbed fixtures in `testdata/<slug>/`; replay tests drive adapter->normalized output.
- [ ] Sandbox-first: order tests run against sandbox/recordings; no live/paper concept in the pack (router owns live_enabled).
- [ ] Single seed migration adds the `venues` row - the ONLY DB touch.
- [ ] Compliance documented per source (rung + ToS basis); no source requires anti-bot bypass (else dropped).
- [ ] INV-7 diff check: `git diff --name-only` shows ONLY pack + seed migration + spec/plan/checklist + testdata + ENVIRONMENT credential rows. Any core file = fail.

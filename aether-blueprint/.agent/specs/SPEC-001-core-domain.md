Layer: 4 - Specification

# SPEC-001: Core Domain Types and Canonical Serialization

**Status:** accepted | **Owning plans:** EP-002 (primary), consumed by all | **Last updated:** 2026-07-07

## User-visible goal
Every plane speaks one type language. A quote from Kalshi and a quote from Hyperliquid are the same shape; edge math, risk checks, and UI rendering never branch on venue (INV-7).

## Non-goals
Venue-specific payloads (live in packs); Brain object internals beyond the reference type (SPEC-011); UI view-models.

## Terms
Defined by this spec; CONTRIBUTING.md forbids synonyms. Types live in `crates/aether-core`, are mirrored to TS (`packages/types`, generated) and Python (`pylib/aether_py/models.py`), with `proto/` as the cross-service wire authority (D7).

## Scalar rules (apply to every type below)
1. **Decimals:** all money, price, size, fee, and edge values are arbitrary-precision decimals - `rust_decimal::Decimal` / TS `string` (decimal.js at math edges) / Python `decimal.Decimal`. Wire format (JSON and proto) is decimal STRING. Floats are FORBIDDEN for these values in all three languages; clippy/eslint/ruff lint configs enforce where possible, review enforces the rest.
2. **Time:** `ts` fields are UTC. JSON wire: RFC3339 with millisecond precision. Proto: `google.protobuf.Timestamp`. ClickHouse storage: `DateTime64(6)`. Venue-local times convert at the adapter boundary; nothing downstream sees local time.
3. **IDs:** internal entities use ULIDs (`id` fields, sortable). Venue-native identifiers are never used as internal IDs; they live in `venue_ref` pairs. **MarketKey** = `mkt:{venue}:{native_id}` (string, unique, stable) - the universal join key across DBs, bus, and UI.
4. **Enums:** snake_case string tags on the wire; closed sets (unknown tag = validation error at the boundary, quarantine per SECURITY.md T2).

## Required types (field lists are the contract; EP-002 may add private helpers, not public fields)
1. `VenueId` - slug string (`kalshi`, `polymarket`, `hyperliquid`, `openbb`, `alpaca`).
2. `InstrumentKind` - `binary_contract | categorical_contract | scalar_contract | equity | option | perp | spot`.
3. `Market` - `{ key: MarketKey, venue: VenueId, kind: InstrumentKind, title, description_ref, status: open|halted|closed|resolved, close_ts?, resolve_ts?, outcome?, jurisdiction_flags: [..], venue_ref: {..}, meta: {..} }`.
4. `PriceSemantics` (derived from kind): binary/categorical prices are probabilities in [0,1] with tick size from venue meta; scalar contracts carry `{unit, min, max}`; equities/options/crypto are currency prices. **All comparisons across venues happen in probability or currency space after normalization - adapters own the conversion.**
5. `Quote` - `{ market: MarketKey, bid?, ask?, mid?, last?, bid_size?, ask_size?, ts, source: stream|poll|snapshot, seq? }`.
6. `BookLevel { price, size }`, `OrderBook { market, bids: [BookLevel], asks: [BookLevel], depth, ts, seq }` - bids descending, asks ascending, always.
7. `Money { amount: Decimal, currency }` - currency ISO-4217 or `USDC|USDT|ETH|...` asset tags.
8. `OrderIntent` - `{ id: Ulid, market: MarketKey, side: buy|sell|buy_no|sell_no, order_type: limit|market, limit_price?, size, size_unit: contracts|shares|base|quote, tif: ioc|gtc|day, paper: bool, origin: { kind: user|alert_action|agent|automation, tier: 1..5, actor_id }, quote_snapshot: Quote, caps_version, created_ts }`. `quote_snapshot` is what the actor saw; the router's drift check compares against it (SPEC-012).
9. `RiskVerdict` - `{ intent_id, verdict: allow|deny, reasons: [ { code: liveness|price_drift|balance|venue_health|cap_exceeded|jurisdiction|live_disabled, detail } ], ts }`. Deny reasons are the closed set the risk engine tests against (TESTING.md).
10. `Order` (accepted intent) and `Fill` - `{ order_id, market, side, price, size, fee: Money, venue_ref, ts, paper }`.
11. `Position` - `{ market, side_exposure, avg_price, size, realized_pnl: Money, unrealized_pnl: Money, ts }`.
12. `OpportunityKind` - `arbitrage | value | catalyst | hedge`.
13. `Opportunity` - `{ id: Ulid, kind, legs: [ { market, side, target_price?, size_hint? } ], gross_edge: Decimal, edge: EdgeDecomposition, confidence: Decimal(0..1), detected_ts, expires_ts?, explain_ref: BrainRef, trace_id }`.
14. `EdgeDecomposition` - `{ gross_spread, fees, slippage_est, funding_cost, gas_cost, bridge_cost, settlement_mismatch_discount, liquidity_haircut, staleness_penalty, confidence_penalty, net_edge }` - all Decimal, all present (zero must be explicit, never defaulted - TESTING.md golden rule), and `net_edge` MUST equal `gross_spread` minus the sum of the other components (property test).
15. `BrainRef` - `{ object_id: Ulid, provenance_hash }` (full object model in SPEC-011).
16. `AuditEvent` - `{ seq, prev_hash, hash, ts, actor, action, subject, payload_hash }` (chain rules in EP-402's spec updates).
17. `CapsSnapshot` - `{ version, per_order_max: Money, daily_max: Money, per_venue: {..}, per_kind: {..} }`.

## Canonical serialization (the "canonical" in canonical serde)
- JSON: field order as declared above; no nulls for absent optionals (omit); decimals as strings; enums snake_case. Canonical bytes = serde_json with preserve_order - required because provenance and audit hashes are computed over canonical JSON.
- Proto: `proto/aether/core/v1/*.proto` mirrors these types; JSON<->proto mapping is mechanical (same names). Where both exist, proto is the wire, canonical JSON is the hash-and-store form.
- Round-trip law: for every type T, `decode(encode(t)) == t` in and across all three languages - property-tested in EP-002 with shared fixture vectors under `testdata/golden/core/`.

## Error states
Boundary validation failures (unknown enum, malformed decimal, non-UTC ts, bid/ask inversion) -> reject + quarantine, never coerce. Internal construction of invalid states MUST be unrepresentable where the type system allows (e.g., `OrderBook` constructor enforces ordering).

## Required tests
Golden vectors per type (all languages consume the same files); round-trip property tests; EdgeDecomposition sum law; OrderBook ordering; MarketKey parse/format; decimal string round-trip including negative, zero-explicit, and 18-decimal-place values.

## Acceptance criteria
`cargo test -p aether-core` green; TS and Python consume the generated/mirrored types against the same golden vectors green; a grep audit shows zero `f64`/`number`/`float` in money/price/size positions in domain code.

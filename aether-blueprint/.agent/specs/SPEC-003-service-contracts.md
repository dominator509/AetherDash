Layer: 4 - Specification

# SPEC-003: Service Contracts

**Status:** accepted | **Owning plans:** EP-004 (primary), packs/services extend additively | **Last updated:** 2026-07-07

## User-visible goal
Every cross-service call, stream, and tool has one registered name and shape. Contracts live in `proto/` (gRPC), `crates/aether-bus` (topics), and this spec (WS + MCP inventories); unregistered names do not exist (R9, D7).

## Non-goals
Implementations; venue REST/WS specifics (packs translate them); Brain internals (SPEC-011).

## Versioning rule
Proto packages are `aether.<area>.v1`; changes inside v1 are additive-only (new fields optional, new RPCs fine, no renames/removals/semantic changes). Breaking = new `v2` package side-by-side (RELEASE.md). Same rule applies to bus message schemas and WS message types.

## gRPC contracts (`proto/aether/*/v1`)
**aether.venue.v1 - VenueAdapter** (every pack implements; capabilities declared in `venue.toml` gate which RPCs are live):
`ListMarkets(filter) -> stream Market` | `GetMarket(key)` | `StreamTicks(keys) -> stream Quote` | `StreamBook(key, depth) -> stream OrderBook` | `SubmitOrder(Order) -> OrderAck` | `CancelOrder(venue_ref)` | `GetBalances() -> Balances` | `Health() -> VenueHealth {status, lag_ms, rate_remaining}`.
**aether.risk.v1 - RiskEngine:** `Evaluate(OrderIntent) -> RiskVerdict` (pure, fast, no side effects beyond metrics).
**aether.router.v1 - OrderRouter:** `Submit(OrderIntent) -> RouterResult {order?, verdict}` | `Cancel(order_id)` | `Status(order_id) -> Order`. Router internally calls RiskEngine then the venue's SubmitOrder; callers never reach adapters' order RPCs directly (enforced: adapter order ports bind loopback and only router holds the address).
**aether.guardian.v1 - WalletGuardian:** `ProposeTransaction(TxSpec) -> Proposal {id, status: pending|auto_approved|denied, policy_trace}` | `GetProposal(id)` | `ApproveProposal(id, approval {totp, ts})` - approval accepted only from an authenticated human session with fresh step-up (SPEC-005); there is no sign-arbitrary and no key-export RPC by construction (SECURITY.md HARD-DENY 1).
**aether.brain.v1 - Brain:** `Store(ObjectDraft) -> BrainRef` | `Get(BrainRef)` | `Recall(query, k, filters) -> [ScoredRef]` | `Explain(opportunity_id) -> ExplainTree` (server assembles; SPEC-011 owns semantics).
Health: every service implements `grpc.health.v1.Health` (smoke/readyz hook).

## Bus topics (registry in `crates/aether-bus`; ARCHITECTURE.md section 3 list is authoritative)
`md.ticks.{venue}` (Quote) | `md.books.{venue}` (OrderBook) | `quarantine.{venue}` (raw+reason) | `brain.objects` (BrainRef+kind) | `opps.detected` (Opportunity) | `orders.intents` (OrderIntent) | `orders.fills` (Fill) | `alerts.outbound` (AlertMsg) | `audit.events` (AuditEvent).
Message envelope on every topic: `{schema: "aether.<type>.v1", trace_id, ts, payload}` - canonical JSON (SPEC-001) in v1; consumers MUST ignore unknown envelope fields.
Partitioning: by `market_key` for md.* and orders.*; by `object_id` for brain.objects. Consumer groups are named `svc.<service>`.

## Gateway WebSocket protocol (client <-> gateway, `/ws`, token-authenticated at connect)
Frames are JSON `{type, id?, trace_id?, body}`. Client->server types:
`subscribe {channels: [feed|quotes:{market_key}|orders|alerts|system]}` | `unsubscribe` | `command {text, room_context}` (routes to MCP layer) | `order_intent {OrderIntent minus origin/actor - gateway stamps those from the session}` | `confirm {ref_id, totp?}` | `ping`.
Server->client types:
`feed_item {Opportunity + display hints}` | `quote {Quote}` | `order_update {Order|RiskVerdict}` | `alert {AlertMsg}` | `explain {ExplainTree}` | `command_result` | `confirm_required {ref_id, action_summary, tier_reason}` | `degradation {surface, reason}` (SPEC-000 fail-open banner) | `error {ErrorEnvelope}` | `pong`.
Rules: gateway stamps `origin` (never trusts client-claimed tier); every mutating frame round-trips a `confirm_required` when the session tier demands it; unknown frame types -> `error` + metric, no disconnect.

## MCP tool inventory (server/mcp; control plane only, INV-2; tier-filtered server-side - a session sees only tools its tier permits)
Tier 1+: `brain.search`, `brain.get_object`, `markets.query`, `opps.list`, `opps.explain`, `metrics.snapshot`.
Tier 2+: `sim.run`, `orders.draft`, `alerts.configure`, `vault.regenerate`.
Tier 3+: `orders.submit_paper` (confirm flow), `swarm.launch {budget}`, `inbox.reprocess`.
Tier 4+: `orders.submit` (live; ALSO requires `live_enabled` + confirm + caps - tier alone is never sufficient), `plugins.install_signed`, `automation.schedule`.
Tier 5: same surface as 4 with auto-confirm within hard caps; HARD-DENY list still applies (SECURITY.md).
Tool results carry provenance refs, never raw secrets or key material; tool schemas live beside their server code and are registered in a static manifest EP-004 creates.

## Error envelope (all surfaces: gRPC details, WS error frames, HTTP)
`{ code, message, retryable: bool, trace_id, details? }` with closed code set: `invalid_argument | unauthenticated | permission_denied (incl. tier) | not_found | failed_precondition (incl. live_disabled, cap_exceeded) | unavailable | deadline_exceeded | quarantined | internal`. Messages are operator-safe: no secrets, no raw payload echoes (SECURITY.md).

## Required tests
Contract tests per service against proto (buf breaking-change check in CI once buf is present, else `git diff` guard on proto/); envelope round-trip goldens; gateway protocol table-test (every type both directions incl. unknown-type handling); tier-filtering test on the MCP manifest; router-only-path test (adapter order port unreachable from a non-router caller).

## Acceptance criteria
EP-004 done = proto compiles for all three languages, bus registry + envelope in use by a demo producer/consumer pair, gateway skeleton speaks the protocol table above against a test client, MCP manifest serves tier-filtered inventories, and the tests above pass.

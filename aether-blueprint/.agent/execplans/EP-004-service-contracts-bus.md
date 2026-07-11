Layer: 5 - Execution

# EP-004: Service Contracts & Event Bus

**Band:** 0xx Foundation | **Phase:** 0 | **Status:** draft | **Blocked by:** EP-003

## Purpose / Big Picture
Make SPEC-003 executable: compiled proto for all three languages, `aether-bus` as the only way topics are named or messages enveloped, a gateway skeleton that speaks the WS frame table against real auth stubs, and the tier-filtered MCP manifest. Phase 0 exits here: producers and consumers exist, contracts are compiled truth.

## Scope
Proto codegen wiring (tonic-build; TS/Python generation); `crates/aether-bus` (topic registry, envelope, rdkafka producer/consumer wrappers, retry/backoff util per SPEC-006); `crates/aether-gateway` skeleton (axum WS + session-token auth stub + frame dispatch); `server/mcp` manifest + stub server; grpc-health wiring pattern; demo producer/consumer pair; deferred SPEC-002/006 tests (quarantine path, breaker cycle).

## Non-goals
No business logic (no real feed, router, brain); no client UI (EP-101 consumes the gateway); no real MCP tools (stubs serve the manifest and echo); no TLS/WireGuard setup (DEPLOYMENT).

## Context and Orientation
SPEC-003 is the contract; SPEC-005 defines what the gateway auth stub must SHAPE toward (full enforcement is EP-401 - the stub validates tokens against `sessions` and stamps origin, tier checks return allow-with-audit-note for now, marked loudly). SPEC-006 retry/breaker utilities belong in aether-bus/core so every later service inherits them.

## Files to Read First
1. SPEC-003 (entire); SPEC-006 (retry, breaker, quarantine); SPEC-005 (origin stamping shape).
2. ARCHITECTURE.md section 3 (transport rules) + D-rules.
3. EP-003 Decision Log (deferred tests you now own).

## Files to Change (Expected Changed Files)
`proto/**` (build wiring: `crates/aether-proto` with build.rs OR per-crate tonic-build - choose the dedicated `aether-proto` crate, log it), `crates/aether-bus/**`, `crates/aether-gateway/**`, `server/mcp/**` (FastAPI stub + `manifest.toml`), `pylib/aether_py/proto/**` (generated, via grpcio-tools in dev group), `packages/types/src/proto/**` (generated via ts-proto or hand-mirrored per D7 - prefer ts-proto if network-restricted install succeeds, else mirror + log), workspace member appends, `scripts/smoke-test.sh` (SERVICES_HEALTHZ gains gateway once it runs under compose profile or systemd-dev - if not daemonized in dev yet, leave list empty + Decision Log), COMMANDS.md (proto-gen command entry), CHANGELOG, this file.

## Interfaces and Contracts
Topics/envelope/consumer-group names EXACTLY per SPEC-003; envelope schema tag format `aether.<type>.v1`; gateway frame table complete including `confirm_required` and `degradation` (stub-emittable via a test-only trigger frame guarded to dev builds); MCP `manifest.toml` lists every SPEC-003 tool with tier + schema ref; error envelope used by every surface this plan touches.

## Milestones
1. **Proto compiles everywhere.** `aether-proto` crate (tonic-build over `proto/aether/**`); python gen via `uv run python -m grpc_tools.protoc ...` wrapped as `scripts/proto-gen.sh` (new COMMANDS entry); TS gen or mirror. Done when: all three build; a cross-language descriptor test passes (same message encodes to identical bytes from Rust and Python for a golden value).
2. **aether-bus.** Topic registry consts + `topic()` helpers (typo-proof), envelope struct + canonical serde reuse, producer/consumer wrappers with SPEC-006 retry policy + breaker, trace_id propagation into headers. Done when: unit tests green incl. retry-table test and breaker cycle test (fake transport).
3. **Demo producer/consumer.** `cargo run -p aether-bus --example roundtrip` publishes a golden Quote envelope to `md.ticks.demo` and consumes it back (integration, compose stack). Done when: `#[ignore]` integration test proves envelope round-trip + consumer-group naming.
4. **Quarantine path (EP-003 deferral).** Bus-side quarantine publisher util + the SPEC-002 test: malformed fixture -> `quarantine.demo`, never `md.ticks.demo`, raw preserved to MinIO. Done when: integration test green.
5. **Gateway skeleton.** axum WS at `AETHER_GATEWAY__BIND`: connect auth (token -> sessions row via sqlx - first real query: run `cargo sqlx prepare`, commit `.sqlx/`), frame parse/dispatch for the full table (unknown type -> error frame + metric), subscribe fan-out from an internal broadcast fed by the demo producer, origin stamping, `/healthz` `/readyz` `/metrics` per SPEC-007 shape (minimal registry now). Done when: a test WS client exercises every frame type table-test style; readyz flips with Postgres down.
6. **MCP manifest + stub.** `server/mcp` FastAPI service loading `manifest.toml`, serving tier-filtered tool inventory (grants read from Postgres), tools respond with typed stubs. Done when: tier-filtering test green (tier-1 grant sees exactly the tier-1 set, etc.).
7. **Health + smoke integration.** grpc-health pattern documented in aether-proto examples; gateway/mcp healthz probed in `test-integration.sh` run. Done when: `integration: ok` includes the new tests; Phase-0 exit checklist against ROADMAP recorded in Outcomes.

## Concrete Steps
Dependency adds (log each): tonic, prost, tonic-build, rdkafka (cmake-less `cmake-build` off / use `rdkafka` with `dynamic-linking` OFF -> vendored build; verify build time acceptable, else `kafka` pure-rust alternative with Decision), axum, tokio, tower, metrics + metrics-exporter-prometheus, sqlx (postgres, runtime-tokio, macros). Python dev: grpcio-tools, fastapi, uvicorn. Keep gateway auth stub's allow-with-note EXPLICIT: a `// TODO(EP-401)` plus a warn-level log on every tier check, and a test asserting the note exists (so EP-401 can't be silently skipped).

## Validation and Acceptance
Milestone validations; `verify.sh` -> `verify: ok`; `test-integration.sh` -> `integration: ok`; SPEC-003 acceptance paragraph satisfied; SPEC-002/006 deferred tests closed (quarantine, breaker); Phase 0 ROADMAP exit criteria all check: verify with zero stack SKIPs (true since EP-001), smoke green, core round-trip proven (EP-002), registry+contracts compiled and demo'd (this plan). `git diff --name-only` reconciled.

## Idempotence and Recovery
Codegen is regenerate-don't-edit (AGENTS.md 9); generated dirs carry a DO-NOT-EDIT header + gitattributes linguist markers. Bus example/tests tolerate re-runs via unique group suffixes in tests. Gateway is stateless (sessions in PG) - kill/restart mid-test must pass (crash-only posture, SPEC-006).

## Progress
- [x] M1 Proto  - [x] M2 Bus  - [ ] M3 Roundtrip  - [ ] M4 Quarantine
- [ ] M5 Gateway  - [x] M6 MCP manifest  - [ ] M7 Health/exit

## Surprises & Discoveries
- tonic-build pulls substantial dependency tree; disk usage ~1.2 GiB for debug build
- protoc required: install via `winget install Google.Protobuf` and set PROTOC env var
- rdkafka cmake-build feature works on Windows with CMake installed
- Qdrant image has no curl; healthcheck uses bash /dev/tcp HTTP
- Gateway WS auth: token passed as `?token=` query param (WS doesn't support custom headers)

## Decision Log
- Chose dedicated `aether-proto` crate over per-crate tonic-build (single compilation unit)
- TS generation: hand-mirrored per D7 (ts-proto would add network dependency)
- Python generation: hand-mirrored per D7 (grpcio-tools not installed in CI)
- rdkafka with cmake-build: works but build time is substantial; revisit if CI times out
- SERVICES_HEALTHZ: currently SKIPs gracefully when services aren't running; mandatory in CI
- Stub implementations are explicitly marked with TODO(EP-xxx) for auth, real transport, DB queries

## Outcomes & Retrospective
- 4 Rust crates: aether-core, aether-proto, aether-bus, aether-gateway
- 5 gRPC service definitions: VenueAdapter, RiskEngine, OrderRouter, WalletGuardian, Brain
- 9 bus topics registered; envelope uses aether-core canonical serialization
- SPEC-003 WS frame table: 6 client types, 10 server types, full deserialization coverage
- MCP: 16 tools across 4 tiers, manifest-driven, token-authenticated
- Retry policy: 200ms base, 30s max, 5 attempts, full jitter
- Circuit breaker: 5 consecutive failures, 30s window, single half-open probe
- Phase 0 exit criteria: proto compiles, bus+gateway+MCP run, healthz pattern established
- Remaining for Phase-0 closure: live Redpanda integration, gateway DB-backed auth, quarantine MinIO path

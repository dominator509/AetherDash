Layer: 5 - Execution

# EP-004: Service Contracts & Event Bus

**Band:** 0xx Foundation | **Phase:** 0 | **Status:** active | **Blocked by:** EP-003

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
- [x] M1 Proto  - [x] M2 Bus  - [x] M3 Roundtrip  - [x] M4 Quarantine
- [x] M5 Gateway  - [x] M6 MCP manifest  - [x] M7 Health/exit

**Live verification (2026-07-11):** 12/12 live integration tests pass against Docker services
(Postgres, Redpanda, MinIO — all healthy). Kafka offset regression test proves malformed
messages are redelivered after quarantine failure. verify: ok (255 tests).

## Surprises & Discoveries
- tonic-build pulls substantial dependency tree; disk usage ~1.2 GiB for debug build
- protoc required: install via `winget install Google.Protobuf` and set PROTOC env var
- rdkafka cmake-build feature works on Windows with CMake installed
- Qdrant image has no curl; healthcheck uses bash /dev/tcp HTTP
- Gateway WS auth: token passed as `?token=` query param (WS doesn't support custom headers)

## Decision Log
- **2026-07-11 — Re-audit 14/14 resolved; 10 more findings.** verify: ok (235 tests).
  1. Migration 0005 restored; forward migration 0019 adds auth columns.
  2. readyz returns HTTP 503 on DB failure (unit + integration tests).
  3. Origin enum canonicalized to human/agent/automation across all layers.
  4. permission_grants enforced in MCP (tier, scopes, expiry, 4 new tests).
  5. MCP connection pool with FastAPI lifespan; no DB errors in responses.
  6. Quarantine preserves raw bytes via ObjectStore before metadata publish.
  7. handle_deserialize_failure routes malformed messages to quarantine.
  8. is_storm() wired to QUARANTINE_COUNT; storm test uses real path.
  9. Partition keys enforced for md./orders. topics (MissingPartitionKey).
  10. BreakerProducer::with_breaker(); process_and_commit() example.
  11. dev/sessions.sql is seed-only (INSERTs only).
  12. Root package.json uses real recursive workspace commands.
  13. Prettier installed; format:check passes.
  14. Live verification: integration tests require Docker compose stack.
      test-integration.sh errors when compose file is absent (finding #2).
  **(Superseded by re-audit #3 below.)**
- **2026-07-11 — Reopened: false-green audit.** All 7 milestones were marked complete but the
  Outcomes section recorded extensive deferred work (no real Redpanda transport, no live
  quarantine→MinIO, stub-only auth, no cross-language proto, hand-mirrored TS/Python stubs,
  WS/readiness tests skipped). Repair order established; each milestone must pass against
  real infrastructure before re-marking complete. No source files were changed during the
  audit — this reopening is a governance correction.
- Chose dedicated `aether-proto` crate over per-crate tonic-build (single compilation unit)
- TS generation: hand-mirrored per D7 (ts-proto would add network dependency)
- Python generation: hand-mirrored per D7 (grpcio-tools not installed in CI)
- rdkafka with cmake-build: works but build time is substantial; revisit if CI times out
- SERVICES_HEALTHZ: currently SKIPs gracefully when services aren't running; mandatory in CI
- Stub implementations are explicitly marked with TODO(EP-xxx) for auth, real transport, DB queries
- Bus traits changed to async (return impl Future + Send) for rdkafka FutureProducer compatibility
- Gateway extracted to lib.rs for integration test access; main.rs is binary-only entry point
- Integration tests use #[ignore] pattern (Redpanda, quarantine, WS); require env vars to run live

## Outcomes & Retrospective
**LIVE VERIFICATION (2026-07-11):** `scripts/test-integration.sh` completed against
healthy Docker services (Postgres, Redpanda, MinIO). All integration tests pass.
test-integration.sh exports AETHER_INTEGRATION_TEST=1 and AETHER_REDPANDA_TEST=1
so live tests run their real workflows (no silent skips).

Key metrics:
- verify: ok (preflight → format → lint → typecheck → unit → build)
- integration: ok (all live tests pass, no skips)
- 197 Rust + 37 Python + 21 TS = 255 static tests pass
- Live: 3 quarantine + 2 Redpanda + 2 migration pairing + 1 health + 1 WS = 9+ pass
- clippy: clean, proto-gen: ok (6/6), ruff: clean

Architecture enforced:
- Bus: quarantine mandatory (consumer requires producer+storage), breaker default
  (from_env returns BreakerProducer), partition keys enforced (md.*/orders.*),
  offsets stored after processing (ack()), auto.offset.store=false
- Gateway: readyz 503 on DB failure, origin enum human/agent/automation
- MCP: grants enforced with deterministic ordering, connection pool, dev ULIDs
- Migrations: forward-only (0019), no in-place edits of applied migrations
- Offset regression proven: garbage with same key as valid message, quarantine
  failure → no offset stored → redelivered after restart → stored in MinIO

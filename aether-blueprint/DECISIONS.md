Layer: 2 - Product & Decisions

# DECISIONS.md - Architecture Decision Records

New decisions append via `.agent/templates/adr-template.md`. Owner "Operator" = the primary human operator.

---
## ADR-0001: Monorepo with per-plane top-level directories
- **Context:** Three planes in three languages (Rust/TS/Python) share domain types, proto contracts, and a bus topic registry; a single operator does cross-plane refactors weekly during v1.
- **Decision:** One repository: `client/`, `server/`, `connectors/`, shared `crates/`, `packages/`, `pylib/`, `proto/`, `infra/` (ARCHITECTURE.md section 1). Cargo, pnpm, and uv workspaces at root.
- **Alternatives:** Polyrepo per plane (rejected: contract drift, painful atomic changes during greenfield); monorepo with Bazel/Nx (rejected: tooling weight for one operator).
- **Consequences:** CI must path-filter to stay fast; Phase 5 multi-user may motivate extracting `wallet-guardian` and venue packs later - boundaries in D1-D7 keep that cheap.
- **Status:** Accepted. **Date:** 2026-07-07. **Owner:** Operator.

---
## ADR-0002: Redpanda as the internal event bus
- **Context:** INPUTS allows "Redpanda or NATS"; requirements are Kafka-compatible semantics, low latency, no JVM, single-binary dev footprint.
- **Decision:** Redpanda, single-node in dev compose; topic names owned by `crates/aether-bus`.
- **Alternatives:** NATS JetStream (rejected for v1: weaker Kafka-ecosystem compatibility for ClickHouse ingestion and replay tooling); Kafka (rejected: JVM, ops weight).
- **Consequences:** `rpk` in tooling; replay harness (EP-405) leans on Kafka-offset semantics. Revisit trigger: dev resource footprint or licensing concerns.
- **Status:** Accepted. **Date:** 2026-07-07. **Owner:** Operator.

---
## ADR-0003: Kuzu embedded as the event knowledge graph
- **Context:** Canonical graph (Event -> Entity -> Market -> Outcome) for one operator; INPUTS allows Kuzu or Neo4j.
- **Decision:** Kuzu embedded in the brain service; graph files on the server data volume.
- **Alternatives:** Neo4j (rejected for v1: separate server, licensing surface, ops weight).
- **Consequences:** Graph access is in-process (fast, simple) but single-writer; SPEC-011 defines the export path so a Neo4j migration is a data move, not a rewrite. Revisit trigger: multi-user Phase 5 or write-contention.
- **Status:** Accepted. **Date:** 2026-07-07. **Owner:** Operator.

---
## ADR-0004: sqlx with paired migrations as the sole schema authority
- **Context:** Rust services need compile-time-checked SQL; Python services read the same Postgres; two migration systems would fork the schema.
- **Decision:** `infra/migrations` (sqlx, paired up/down) is the only way schema changes happen. Python uses the schema read/write via SQLAlchemy Core or asyncpg but never migrates. `cargo sqlx prepare` keeps offline query data current.
- **Alternatives:** Alembic-owned migrations (rejected: hot-path services are Rust; checks belong there); Diesel (rejected: ORM lock-in, harder raw analytics SQL).
- **Consequences:** Python devs run Rust tooling for schema work; acceptable for a Rust-first operator.
- **Status:** Accepted. **Date:** 2026-07-07. **Owner:** Operator.

---
## ADR-0005: Package managers - cargo, pnpm, uv; lockfiles committed
- **Context:** INPUTS names them; determinism is a production-readiness requirement.
- **Decision:** Cargo.lock, pnpm-lock.yaml, uv.lock committed; CI installs frozen (`--frozen-lockfile`, `uv sync`).
- **Alternatives:** npm/yarn (rejected: workspace ergonomics), pip/poetry (rejected: speed, resolver determinism).
- **Consequences:** Contributors need uv and pnpm installed; preflight enforces.
- **Status:** Accepted. **Date:** 2026-07-07. **Owner:** Operator.

---
## ADR-0006: LiteLLM as the model router
- **Context:** Providers: Anthropic, DeepSeek, xAI, OpenAI-compatible, local vLLM/Ollama/llama.cpp; routing, fallback, and cost accounting must live in one place.
- **Decision:** LiteLLM (library mode inside `server/llm_router`, not the proxy server) fronts all providers; cache-first prompt assembly (INV-3) wraps it; per-call cost and cache-hit metrics exported.
- **Alternatives:** Hand-rolled per-provider clients (rejected: N x maintenance); LiteLLM proxy as a separate service (deferred: one more moving part; revisit if non-Python callers need direct access).
- **Consequences:** Provider quirks are quarantined in `server/llm_router`; nothing else imports provider SDKs (supports D3).
- **Status:** Accepted. **Date:** 2026-07-07. **Owner:** Operator.

---
## ADR-0007: Paper-first execution gate
- **Context:** INV-1/INV-5 and STOP S7; live-money defects are unrecoverable.
- **Decision:** All execution code paths ship with live trading disabled by configuration that no agent may set (`execution.live_enabled` guarded by hard-deny). EP-305 acceptance runs entirely on paper/sandbox; enabling live requires the operator editing config out-of-band plus step-up 2FA at runtime.
- **Alternatives:** Feature-flag toggled in tests (rejected: normalizes flipping it).
- **Consequences:** Live-path integration is tested via venue sandboxes and replay; first live trade is a manual, audited operator ceremony (documented in OPERATIONS.md).
- **Status:** Accepted. **Date:** 2026-07-07. **Owner:** Operator.

---
## ADR-0008: Tauri v2 over Electron for the client
- **Context:** INPUTS chose Tauri for small, fast, secure binaries; v2 is current and its IPC permission model matches the five-tier design.
- **Decision:** Tauri v2; Rust shell owns keychain access and the encrypted local cache; web layer never touches secrets.
- **Alternatives:** Electron (rejected: footprint, Node in the trusted shell); native egui (rejected: UI velocity for dashboard-class surfaces).
- **Consequences:** Verify Tauri v2 API details against installed tauri-cli before EP-101 (A-07).
- **Status:** Accepted. **Date:** 2026-07-07. **Owner:** Operator.

---
## ADR-0009: GitHub Actions first, self-hosted Forgejo path documented
- **Context:** Privacy-first suggests self-hosted CI; bootstrap speed suggests GitHub Actions.
- **Decision:** Start on GitHub Actions with workflows that only call `scripts/*.sh`, keeping CI logic runner-agnostic; DEPLOYMENT.md documents the Forgejo runner migration.
- **Alternatives:** Forgejo from day one (rejected: EP-001 shouldn't block on infra procurement).
- **Consequences:** Operator confirms remote-push policy before first push (A-17); migration is a runner swap because workflows are thin.
- **Status:** Accepted. **Date:** 2026-07-07. **Owner:** Operator.

---
## ADR-0010: Isolate OpenBB behind the venue gRPC boundary
- **Context:** The OpenBB Platform SDK and provider extensions are AGPL-3.0-or-later. AETHER needs its equity and options data without allowing SDK-specific objects or imports to spread across service boundaries.
- **Decision:** Install OpenBB only in the Python connectors workspace and import it only inside the OpenBB venue service. The service exposes plain canonical protobuf messages over `VenueAdapter`; no OpenBB object crosses that process boundary. The adapter remains AGPL-3.0-or-later and its source is distributed with AETHER.
- **Alternatives:** Import OpenBB into Brain or other services (rejected: expands coupling and the license surface); call undocumented provider endpoints directly (rejected: bypasses the selected supported integration); omit OpenBB (rejected: loses the EP-303 reference and options source).
- **Consequences:** The connectors uv environment is larger, AGPL source-availability obligations apply to network deployments of this service, and upgrades require an explicit license review. Process isolation is an architectural boundary, not a claim that it negates AGPL obligations.
- **Status:** Accepted. **Date:** 2026-07-13. **Owner:** Operator.

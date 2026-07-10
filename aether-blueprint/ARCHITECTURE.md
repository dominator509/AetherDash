Layer: 3 - Architecture

# ARCHITECTURE.md - Boundaries and Invariants

This document defines concrete repository rules. "Clean architecture" claims without a rule here do not exist.

## 1. Intended repository map (monorepo - ADR-0001)
```text
/
  client/                      # PLANE 1 - Tauri desktop app
    src-tauri/                 #   Rust shell: IPC, keychain, local encrypted cache
    src/                       #   React + TS + Tailwind + Radix UI
  server/                      # PLANE 2 - 24/7 brain services (Python FastAPI + Rust where hot)
    brain/                     #   knowledge graph API, tiered recall, vault-view generator
    llm_router/                #   LiteLLM config + cache-first prompt builder + local fallback
    alerts/                    #   alert engine + outbound comms dispatch
    inbox/                     #   agentic inbox (Gmail push / MS Graph webhooks)
    mcp/                       #   MCP tool servers (control plane ONLY)
    swarm/                     #   research swarm orchestrator + shared scratchpad
  connectors/                  # PLANE 3 - stateless microservices
    venues/                    #   one extension pack per venue
      kalshi/  polymarket/  hyperliquid/  openbb/  alpaca/  ...
    execution/
      order-router/            #   Rust (Axum/tonic) - deterministic order validation + routing
      risk-engine/             #   Rust - caps, liveness, liquidity, jurisdiction checks
      wallet-guardian/         #   Rust - isolated signing policy service (separate process)
    comms/                     #   telegram/ discord/ slack/ twilio/ email senders
  crates/                      # shared Rust
    aether-core/               #   domain types, canonical serde, edge decomposition math
    aether-bus/                #   Redpanda client wrappers, topic registry
    aether-audit/              #   hash-chained append-only audit log writer/verifier
  packages/                    # shared TS: @aether/types (generated), @aether/ui
  pylib/                       # shared Python: aether_py (models mirrored from aether-core)
  proto/                       # gRPC contracts (buf-managed), single source for cross-service calls
  infra/
    dev/docker-compose.yml     # Postgres+pgvector, ClickHouse, Qdrant, Redis, Redpanda, MinIO, Kuzu vol
    migrations/                # sqlx migrations (Postgres, authoritative)
    clickhouse/                # ordered idempotent DDL
    deploy/                    # systemd units / compose bundles per plane host
  vault/                       # GENERATED Obsidian-compatible Markdown view. Never hand-edited.
  scripts/                     # verification wrappers (see COMMANDS.md)
  .agent/                      # blueprint pack control files
```

## 2. Component boundaries (the three planes)
- **Client plane** renders, captures intent, and talks ONLY to the server gateway over a token-authenticated WebSocket plus MCP for command-room tool calls. It never calls venue APIs, never holds venue keys, never computes risk.
- **Server/brain plane** owns understanding: ingestion orchestration, Brain storage/recall, LLM routing, alerting, swarms, MCP tool servers. It proposes; it never signs and never submits orders itself - it calls the order router like any other client.
- **Connector plane** is stateless: venue adapters translate canonical types <-> venue APIs; the order router validates and routes; the risk engine gates; the Wallet Guardian signs under policy. Each service restarts cleanly with no local durable state (state lives in the databases).

## 3. Transport rules (by purpose, not preference)
- **MCP** = agent/tool control plane. Client command room <-> server MCP servers. NEVER on the trading path; no MCP call may sit between opportunity acceptance and order submission.
- **gRPC (tonic/proto/)** = internal low-latency service calls (router -> risk-engine, router -> venue adapter, anything -> wallet-guardian "propose").
- **WebSocket** = real-time UI updates (gateway -> client) and venue market-data feeds (venue -> adapter).
- **Redpanda bus** = internal streaming: topics `md.ticks.{venue}`, `md.books.{venue}`, `brain.objects`, `opps.detected`, `orders.intents`, `orders.fills`, `alerts.outbound`, `audit.events`. Topic registry lives in `crates/aether-bus`; new topics are added there or they do not exist.
- **Direct venue APIs / FIX-style** = execution plane only.

## 4. Data flow
Venue feeds -> venue adapter (normalize to `aether-core` types) -> bus `md.*` -> ClickHouse (materialized views) + scanner consumers -> `opps.detected` -> Brain enrichment + scoring -> gateway -> client feed. Documents/email -> inbox -> parse/OCR workers -> Brain objects (Postgres/Qdrant/Kuzu/MinIO with provenance) -> vault view regeneration.

## 5. Request flow (execution path - the hot path)
Client intent (or alert inline action) -> gateway -> **order router** (gRPC): validate market liveness, current price vs quote, balance, venue health, caps, jurisdiction via **risk engine** -> venue adapter submits -> fill event -> bus `orders.fills` -> audit chain -> P&L attribution -> client + alert confirmation. Wallet transactions branch: router -> **wallet-guardian.propose(tx)** -> policy checks (limits, allowlists, simulation) -> human approval where policy demands -> sign inside guardian -> broadcast. Target: API venues ~20-50 ms router-to-submit; every hop on this path is Rust, bus-decoupled from analytics, and free of LLM/MCP calls.

## 6. State management rules
- Databases are the source of truth. Postgres = relational truth (markets, orders, caps, users, plugin manifests, brain object index). ClickHouse = time-series and analytics. Qdrant = vectors. Kuzu = event knowledge graph. MinIO = raw lake (raw + cleaned + provenance hashes). Redis/Dragonfly = hot cache + LLM prompt/semantic cache.
- `vault/` is a generated VIEW. The generator is one-way (DB -> markdown). Anything hand-written there is overwritten; CI treats vault diffs as build artifacts.
- Client keeps a local encrypted cache for hot data only; it is always reconstructable from the server.
- Brain anti-bloat: hot/cold tiering, time-decay ranking, archival summarization of resolved markets - enforced by scheduled jobs specified in SPEC for EP-201, not by convention.

## 7. Security boundaries
- **Wallet isolation:** `wallet-guardian` is a separate process/service. No other component links its signing code. Raw keys exist only inside the guardian's keystore boundary (OS keychain / enclave / MPC backend). The only inbound API is `propose_transaction`; there is no `export_key`, no `sign_arbitrary`.
- **Key material never enters model context:** no code path may pass key bytes, seed phrases, or signed-payload internals into prompt construction, MCP tool results, or logs. Enforced by review rule + security-check greps.
- **Permission tiers** (Read-Only, Draft-Only, Confirm-Every-Action, Bounded-Autopilot, YOLO-within-hard-caps) are enforced in server code at the gateway and router, not in UI. Hard-deny hooks (wallet transfers above threshold, `.env`/key access) apply at ALL tiers.
- **Plugins** run sandboxed with signed capability manifests; capabilities are checked at the host boundary on every call. Unsigned or over-scoped plugins fail CI and fail load.
- **Ingestion compliance ladder** (official API > licensed feed > RSS/sitemap > robots-compliant crawl > user-authorized session > manual review) is encoded in ingestion specs; no bypass tooling exists anywhere in the tree.

## 8. Persistence rules
- Schema changes only via `infra/migrations` (sqlx, paired up/down) and `infra/clickhouse` ordered DDL. No runtime `CREATE TABLE`.
- Every Brain object carries: source, timestamp, author/publisher, URL/file hash, raw + cleaned copies (MinIO refs), summary, entities, linked events, confidence, provenance hash, staleness/expiry rule.
- Audit chain (`aether-audit`): append-only, each record hash-links the previous; verification is part of release (RELEASE.md).

## 9. External integration boundaries
Every venue is an **extension pack**: a directory under `connectors/venues/<name>/` implementing the `VenueAdapter` contract from `proto/venue.proto` + `aether-core` traits, plus a manifest (`venue.toml`: capabilities, rate limits, jurisdictions, sandbox endpoints). The core discovers adapters via registry; core code contains zero venue-specific branches.

## 10. Invariants (load-bearing - violating any is a blocking defect)
- **INV-1** AI is pilot, not engine: ingestion, normalization, risk, order validation, wallet policy, audit are deterministic code paths with no LLM in the loop.
- **INV-2** Three-plane separation holds; MCP never on the low-latency trading path.
- **INV-3** Prompt construction is cache-first: static blocks (tools, system, ontology, examples) first, single cache breakpoint before per-call dynamic data; compact event IDs; RAG retrieval, never whole-Brain dumps.
- **INV-4** Ingestion is compliance-first per the ladder; no anti-bot circumvention exists in the tree.
- **INV-5** Wallet isolated behind the Guardian; agents only propose; withdrawals always require human approval; hard-deny hooks protect keys/.env at every tier.
- **INV-6** Plugins are signed, sandboxed, dependency-scanned, capability-scoped from day one.
- **INV-7** Venues are additive extension packs; adding one changes no core file.
- **INV-8** Simple and Advanced modes share one engine and one dataset.
- **INV-9** Databases are truth; the vault is a generated view; every Brain object is LLM-readable with provenance and staleness.
- **INV-10** Self-improvement is metric-driven and human-gated; the system never silently rewrites its own rules/weights.
- **INV-11** Router, risk engine, Wallet Guardian, and each connector are separate services with their own tests; paper-trading/backtest harness is a first-class validation surface.

## 11. Import and dependency rules (mechanically checkable)
- **D1** `crates/aether-core` depends on std + serde-class crates only. No IO, no HTTP, no DB clients.
- **D2** `connectors/execution/*` may depend on: `aether-core`, `aether-bus`, `aether-audit`, tonic/axum/sqlx-class infrastructure. FORBIDDEN: any LLM SDK, MCP library, `server/` code, Python bindings.
- **D3** Nothing under `server/mcp/` or `server/llm_router/` is imported by execution services (grep-enforced in security-check).
- **D4** `client/` imports `@aether/types` (generated) and its own code; it never imports venue SDKs.
- **D5** Venue packs depend on core crates + their own venue SDK; core never depends on a venue pack.
- **D6** `wallet-guardian` is not a dependency of anything; it is reached only via gRPC.
- **D7** Cross-language contracts change only via `proto/` regeneration; hand-mirrored types must reference the proto message name in a comment.

## 12. Forbidden architecture moves
- Adding an LLM/MCP call anywhere on the order or wallet path (breaks INV-1/2).
- Merging execution services into the brain process "for simplicity" (breaks INV-11).
- Venue-specific `if` branches in router/risk/core (breaks INV-7).
- Writing to `vault/` by hand or reading it as data source (breaks INV-9).
- Client-side risk checks as the only enforcement (breaks INV-1: server is authoritative).
- A second event bus, a second migration system, or a second source of topic names.
- Storing key material in Postgres, env files inside the repo, or client cache (breaks INV-5).

## 13. How to add a feature
1. Find or write the spec (`.agent/specs/`); behavior-first, test-shaped.
2. Create an ExecPlan from the template in the correct band (EP-1xx/2xx/3xx/4xx); register it in `.agent/PLANS.md`.
3. Identify Expected Changed Files; confirm they respect D1-D7 and section 12.
4. Implement per AGENTS.md workflow; validate per milestone.
**Adding a venue (recipe):** copy `connectors/venues/_template/` -> `<name>/`; fill `venue.toml`; implement the adapter traits against recorded fixtures; add replay tests; register in the venue registry; write EP-3xx plan; zero core edits (INV-7 check: `git diff --name-only` shows only the new pack + registry entry + plan/spec files).

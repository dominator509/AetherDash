Layer: 2 - Product & Decisions

# ASSUMPTIONS

Assumptions made during blueprint generation. Coding agents: when an assumption proves wrong, record the correction in the active ExecPlan Decision Log and update this table. Do not silently proceed on a falsified blocking assumption - that is a STOP condition.

| # | Assumption | Reason | Risk if wrong | How to verify | Blocks implementation? |
|---|------------|--------|---------------|---------------|------------------------|
| A-01 | Repository is greenfield and empty except this pack | Stated in INPUTS | Scaffold collides with existing code | `git ls-files` shows only blueprint pack files | Yes - EP-000 verifies |
| A-02 | Monorepo with Cargo + pnpm + uv workspaces | Three tight-coupled planes, one operator, atomic cross-plane refactors during v1 | Repo splitting needed later at moderate cost | ADR-0001; revisit at Phase 5 | No |
| A-03 | Rust stable >= 1.78, edition 2021 | Tauri v2 + Axum + current sqlx need modern stable | Build failures | `rustc --version` in preflight | Yes |
| A-04 | Node 20 LTS + pnpm >= 9 | Tauri v2 frontend tooling, workspace support | Frontend toolchain breakage | `node --version && pnpm --version` in preflight | Yes |
| A-05 | Python >= 3.11 managed by uv | FastAPI + modern typing; uv for lockfile speed | Dependency resolution drift | `python3 --version && uv --version` in preflight | Yes |
| A-06 | Docker + docker compose available on dev host | Dev database stack (Postgres, ClickHouse, Qdrant, Redis, Redpanda, MinIO) runs locally | No local integration testing | `docker compose version` in preflight | Yes for EP-003+ |
| A-07 | Tauri v2 (not v1) | Current major; mobile-capable later; better IPC permissions | API differences from training-era docs | Confirm against installed `tauri-cli` version before EP-101 | No |
| A-08 | Event bus: Redpanda single-node in dev | Kafka-compatible, no JVM, single binary | Ops burden; NATS migration cost | ADR-0002; revisit if dev footprint too heavy | No |
| A-09 | Graph DB: Kuzu embedded first | Embedded = zero ops for single operator; Cypher-adjacent | Migration to Neo4j if multi-user scaling needed | ADR-0003 | No |
| A-10 | Postgres 16 + pgvector; sqlx (not diesel) for Rust DB access | Compile-time checked SQL without ORM lock-in | Query macro friction | ADR-0004 | No |
| A-11 | Kalshi provides a demo/sandbox environment usable for EP-301 | Kalshi has historically offered demo API access | Connector tested only against mocks + replay | Check developer docs at EP-301 start; if no sandbox, use recorded-replay fixtures only | No - replay path exists |
| A-12 | Polymarket integration starts read-only (CLOB + Gamma + Polygon RPC), execution deferred and geofenced | US-user execution is a hard non-goal; read-only carries no order risk | None material | SPEC for EP-302 marks execution out of scope for v1 US contexts | No |
| A-13 | First brokerage adapter is Alpaca paper trading | Free paper API, clean REST/WS, no capital risk | IBKR/Tradier users wait until Phase 3+ | ADR in EP-303 | No |
| A-14 | LiteLLM (or equivalent thin router) fronts Anthropic, DeepSeek, xAI, OpenAI-compatible, and local vLLM/Ollama endpoints | Stated in INPUTS; avoids per-provider client code | Router abstraction leaks provider quirks | Verify provider list at EP-202 | No |
| A-15 | GPU workers (RunPod or self-hosted) are NOT required for Phase 1-2 | OCR/embedding-heavy work lands in Phase 3 | Phase 3 timeline slips if GPU procurement lags | Operator confirms GPU plan before EP-2xx ingestion plans go active | No for Phases 1-2 |
| A-16 | Secrets live in OS keychain (client) and an env-file-outside-repo + systemd credentials (server) until a vault service is chosen | Smallest safe start; keys never in repo or model context | Secret sprawl if not consolidated | SECURITY.md rules; revisit in EP-401 | No |
| A-17 | CI is GitHub Actions initially, with a documented path to self-hosted Forgejo runners | Fastest bootstrap; privacy-first migration path preserved | Public runner exposure of private repo metadata | Operator decision recorded before first push to remote | No |
| A-18 | Agent LLM workloads bill via API keys (not subscription credit pools) for v1 | Deterministic cost attribution per call; simpler routing | Cost model differs if operator prefers plan credits | Operator confirms at EP-202; both paths supported by router config | No |

Unknowns that are STOP conditions rather than assumptions: live venue API credentials, wallet seed/custody choices, jurisdictional eligibility for specific venues, and any production deployment target details. Agents must stop and ask rather than assume these.

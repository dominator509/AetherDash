# Conventions

## Architecture
- **Three-plane monorepo:** client (Tauri), server/brain (Python + Rust), connectors (Rust)
- **Venue extension packs:** each venue under `connectors/venues/<name>/` with `venue.toml` manifest + adapter traits; core has zero venue-specific branches (INV-7)
- **Execution plane isolation:** `connectors/execution/*` depends only on `aether-core` + infrastructure crates; forbidden: LLM SDKs, MCP, `server/` code (D2, D3)
- **Wallet Guardian:** separate process, only reachable via gRPC `propose_transaction`; no export/sign_arbitrary; not a dependency of anything (D6)

## Naming
- **Env vars:** `AETHER_<SECTION>__<KEY>` (double underscore between sections). Exception: `DATABASE_URL` (sqlx convention)
- **Redpanda topics:** `md.ticks.{venue}`, `md.books.{venue}`, `brain.objects`, `opps.detected`, `orders.intents`, `orders.fills`, `alerts.outbound`, `audit.events`
- **ExecPlans:** `EP-<band><seq>` — 1xx=Client, 2xx=Brain, 3xx=Connectors, 0xx/4xx=Foundation/Cross-cutting
- **Config templates:** `*.example` suffix; never embed secrets, keys, tokens, or live endpoints

## Code Style
- Rust: standard idiomatic Rust; clippy `-D warnings`; unit tests in-file or `tests/`; `#[ignore]` for integration tests
- TypeScript: vitest files per component; eslint + prettier; tsc strict
- Python: pytest with markers `-m "not integration and not e2e"`; ruff for lint + format
- Proto: buf-managed; hand-mirrored types must reference proto message name in comment (D7)

## Data & State
- Databases are source of truth; `vault/` is generated one-way (DB → markdown); never hand-edit vault
- Schema changes only via `infra/migrations` (sqlx paired up/down) and `infra/clickhouse` ordered DDL
- Client local cache always reconstructable from server
- No runtime `CREATE TABLE`
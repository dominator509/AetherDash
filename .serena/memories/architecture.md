# Architecture Rules

See `mem:core` for top-level architecture. This memory covers detailed rules.

## Forbidden Architecture Moves
- Adding LLM/MCP call on order or wallet path (breaks INV-1/2)
- Merging execution services into brain process (breaks INV-11)
- Venue-specific `if` branches in router/risk/core (breaks INV-7)
- Writing to `vault/` by hand or reading it as data source (breaks INV-9)
- Client-side risk checks as only enforcement (breaks INV-1)
- Second event bus, second migration system, or second source of topic names
- Storing key material in Postgres, env files inside repo, or client cache (breaks INV-5)

## Dependency Rules (D1-D7)
- D1: `aether-core` depends on std + serde-class only. No IO, HTTP, DB clients
- D2: `connectors/execution/*` may depend on aether-core, aether-bus, aether-audit, tonic/axum/sqlx infra. FORBIDDEN: LLM SDK, MCP, server/ code, Python bindings
- D3: Nothing under server/mcp/ or server/llm_router/ imported by execution services (grep-enforced)
- D4: client/ imports @aether/types and own code; never imports venue SDKs
- D5: Venue packs depend on core crates + own venue SDK; core never depends on venue pack
- D6: wallet-guardian is not a dependency of anything; reached only via gRPC
- D7: Cross-language contracts change only via proto/ regeneration

## Transport Rules
- MCP = agent/tool control plane only; NEVER on trading path
- gRPC = internal low-latency service calls
- WebSocket = real-time UI updates + venue market-data feeds
- Redpanda bus = internal streaming (topic registry in crates/aether-bus)
- Direct venue APIs/FIX = execution plane only

## Adding a Venue (Recipe)
1. Copy `connectors/venues/_template/` → `<name>/`
2. Fill `venue.toml` (capabilities, rate limits, jurisdictions, sandbox endpoints)
3. Implement adapter traits against recorded fixtures
4. Add replay tests
5. Register in venue registry
6. Create EP-3xx plan
7. Verify INV-7: `git diff --name-only` shows only new pack + registry + plan/spec files
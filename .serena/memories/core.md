# AetherDash — Project Core

AETHER Terminal: self-hosted, AI-native terminal unifying prediction markets, equities/options, crypto/DeFi, and sports-event markets into one deterministic engine + AI understanding layer.

## Architecture (Three-Plane Monorepo)
- **Plane 1 — Client:** `client/` — Tauri v2 desktop app (Rust shell + React/TS/Tailwind/Radix)
- **Plane 2 — Server/Brain:** `server/` — Python FastAPI brain, LLM router, alerts, inbox, swarms, MCP control plane
- **Plane 3 — Connectors:** `connectors/` — stateless Rust venue adapters, order router, risk engine, Wallet Guardian
- Shared: `crates/` (Rust), `packages/` (TS), `pylib/` (Python), `proto/` (gRPC), `infra/` (dev stack, migrations, deploy)

## Load-Bearing Invariants
- INV-1: AI is pilot, not engine — ingestion/normalization/risk/execution/wallet are deterministic
- INV-2: Three-plane separation; MCP never on the low-latency trading path
- INV-3: Prompt construction is cache-first (static blocks first, single breakpoint before dynamic data)
- INV-4: Ingestion compliance-first; no anti-bot circumvention
- INV-5: Wallet isolated behind Guardian; agents only propose; human approval for withdrawals
- INV-6: Plugins are signed, sandboxed, capability-scoped
- INV-7: Venues are additive extension packs; adding one changes no core file
- INV-8: Simple and Advanced modes share one engine, one dataset
- INV-9: Databases are truth; vault is generated view
- INV-10: Self-improvement metric-driven and human-gated
- INV-11: Each connector/execution service has its own tests

## Source-of-Truth Priority
1. Current user instruction 2. AGENTS.md 3. Active ExecPlan 4. Existing code/tests 5. ARCHITECTURE.md 6. Relevant spec 7. ROADMAP.md

## Key Files
- `aether-blueprint/AGENTS.md` — coding agent governance (read first)
- `aether-blueprint/ARCHITECTURE.md` — (`mem:architecture`) boundaries, invariants, repo map, dependency rules D1-D7
- `aether-blueprint/COMMANDS.md` — (`mem:suggested_commands`) the only legal commands
- `aether-blueprint/DECISIONS.md` — (read file directly) ADRs
- `aether-blueprint/ENVIRONMENT.md` — (`mem:environment`) config contract
- `aether-blueprint/ROADMAP.md` — phase sequencing (do not implement from directly)
- `aether-blueprint/.agent/PLANS.md` — (`mem:execution`) ExecPlan ledger; exactly one active at a time

## Modules
- `mem:tech_stack` — languages, frameworks, tools, version pins
- `mem:suggested_commands` — project and Windows-specific commands
- `mem:conventions` — code style, naming, pattern conventions
- `mem:task_completion` — verification commands for task done
- `mem:architecture` — detailed architecture rules, dependency rules, forbidden moves
- `mem:execution` — ExecPlan workflow, STOP conditions, milestone tracking
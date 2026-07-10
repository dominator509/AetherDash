# AETHER Terminal — Project Identity [CACHE-STABLE BLOCK v1]
<!-- 
  DEEPSEEK CACHE: This entire block through the CACHE BREAKPOINT marker is 
  designed to remain stable across sessions. Do NOT add timestamps, dates, 
  session IDs, counters, or dynamic content above the breakpoint.
  Any edit to this block invalidates the prefix cache for ALL sessions.
  Review cache impact before modifying lines 1-80.
  
  CACHE STRATEGY:
  - Block 1 (lines 1-80): NEVER changes — project identity, invariants, architecture
  - Block 2 (lines 82+): RARELY changes — references, conventions, commands
  - Breakpoint (line ~100): separates cache-stable from session-volatile content
  - Target: >97% prefix cache hit rate with DeepSeek
-->

AETHER Terminal is a self-hosted, AI-native trading terminal unifying prediction markets, 
equities/options, crypto/DeFi, and sports-event markets. AI is the pilot — deterministic 
Rust services are the engine.

## Core Architecture (Three-Plane Monorepo)
- **client/** — Plane 1: Tauri v2 desktop (Rust shell + React/TS/Tailwind/Radix)
- **server/** — Plane 2: 24/7 brain (Python FastAPI + Rust hot-path)
- **connectors/** — Plane 3: stateless venue adapters + order router + risk engine + Wallet Guardian
- **crates/** — shared Rust (aether-core, aether-bus, aether-audit)
- **proto/** — gRPC contracts (buf-managed, single source for cross-service calls)
- **infra/** — dev compose, sqlx migrations, ClickHouse DDL, deploy configs

## Load-Bearing Invariants
1. AI is pilot, NOT engine — ingestion, risk, execution, wallet are deterministic code paths
2. Three-plane separation holds — MCP never on the low-latency trading path
3. Prompt construction is cache-first — static blocks first, single breakpoint before dynamic data
4. No anti-bot circumvention — load-bearing non-goal
5. Wallet isolated behind Guardian — agents propose, humans approve withdrawals
6. Plugins are signed, sandboxed, capability-scoped
7. Venues are additive extension packs — adding one changes no core file
8. Databases are truth — the Obsidian-compatible vault is a generated view, never hand-edited

## Agent Governance
- Read `aether-blueprint/AGENTS.md` before any edit — it binds all coding agents
- Source-of-truth priority: user instruction > AGENTS.md > active ExecPlan > existing code > ARCHITECTURE.md > spec > ROADMAP.md
- Exactly ONE ExecPlan active at a time (tracked in `aether-blueprint/.agent/PLANS.md`)
- Anti-drift: implement only what the active ExecPlan scopes; no broad refactors
- Anti-hallucination: confirm names from repository files; use only COMMANDS.md commands
- Bounded retry: 3 same-root failures → stop approach, choose simpler path

## Current State
- **Phase 0:** Foundation — the repo is a verified three-stack monorepo scaffold
- **Active ExecPlan:** None (first activation: EP-000)
- **Greenfield gating:** each stack activates when its marker file lands (Cargo.toml, pnpm-workspace.yaml, pyproject.toml, infra/dev/docker-compose.yml)

<!-- === CACHE BREAKPOINT: Stable content above, session-volatile below === -->

## Key Reference Files
| File | Purpose |
|------|---------|
| `aether-blueprint/AGENTS.md` | Coding agent governance (read first) |
| `aether-blueprint/ARCHITECTURE.md` | Boundaries, dependency rules D1-D7, forbidden moves |
| `aether-blueprint/COMMANDS.md` | The only legal commands |
| `aether-blueprint/DECISIONS.md` | Architecture Decision Records (ADR-0001 to ADR-0009) |
| `aether-blueprint/ENVIRONMENT.md` | Config contract (`AETHER_*` vars) |
| `aether-blueprint/ROADMAP.md` | Phase sequencing (do NOT implement from directly) |
| `aether-blueprint/.agent/PLANS.md` | ExecPlan ledger |
| `aether-blueprint/SECURITY.md` | Security rules, key material boundaries |
| `aether-blueprint/TESTING.md` | Test strategy |
| `aether-blueprint/PRODUCTION_READINESS.md` | Production readiness checklist |

## Tool Stack
- **Rust:** cargo workspace, clippy -D warnings, nextest, sqlx
- **TypeScript:** pnpm workspace, vitest, tsc --noEmit, eslint, prettier
- **Python:** uv workspace, pytest, mypy, ruff, LiteLLM (library mode)
- **Infra:** Postgres+pgvector, ClickHouse, Qdrant, Redis, Redpanda, MinIO, Kuzu
- **RTK:** `rtk` wraps all bash commands via hook (global PreToolUse). `rtk gain` for stats.

## Quick Commands
```bash
scripts/verify.sh                   # preflight -> format -> lint -> typecheck -> unit -> build
scripts/security-check.sh           # gitleaks + forbidden-path + import-boundary grep
scripts/smoke-test.sh               # dev stack health
scripts/production-readiness-check.sh  # full gate
```
All scripts require Git Bash or WSL on Windows. RTK wraps all bash commands automatically.

## Serena + Obsidian
- **Serena:** project memories in `.serena/` — use `serena` MCP tools for code intelligence
- **Obsidian:** vault at project root (`.obsidian/` config) — graph, backlinks, canvas enabled
- **Bridge:** `vault/` is the generated Obsidian-compatible Markdown view (one-way: DB → markdown)
- Serena memories reference the blueprint docs in `aether-blueprint/`
